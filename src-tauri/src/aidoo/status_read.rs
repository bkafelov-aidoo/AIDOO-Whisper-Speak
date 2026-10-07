use super::{
    dental::{display_tooth, spoken_tooth, tooth_sort_key},
    types::{StatusCatalogEntry, StatusReadEntry, StatusReadResult, ToothStatus},
};
use std::collections::{BTreeMap, HashMap};

pub fn build_status_read_result(
    teeth: Vec<ToothStatus>,
    catalog: &[StatusCatalogEntry],
) -> StatusReadResult {
    let names = catalog
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut entries = teeth
        .into_iter()
        .filter(status_has_content)
        .map(|status| StatusReadEntry {
            tooth: display_tooth(&status.tooth, status.is_milk_tooth),
            statuses: status
                .statuses
                .iter()
                .map(|id| {
                    names
                        .get(id.as_str())
                        .copied()
                        .unwrap_or("неразпознат статус")
                })
                .map(str::to_string)
                .collect(),
            regions: status
                .regions
                .iter()
                .map(|value| region_name(value))
                .collect(),
            is_milk_tooth: status.is_milk_tooth,
            for_observation: status.for_observation,
            note: status.note.filter(|note| !note.trim().is_empty()),
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| tooth_sort_key(&entry.tooth));

    StatusReadResult {
        spoken_summary: spoken_summary(&entries),
        entries,
    }
}

fn status_has_content(status: &ToothStatus) -> bool {
    !status.statuses.is_empty()
        || !status.regions.is_empty()
        || status.for_observation
        || status
            .note
            .as_ref()
            .is_some_and(|note| !note.trim().is_empty())
}

fn region_name(value: &str) -> String {
    match value {
        "MESIAL" => "медиално",
        "DISTAL" => "дистално",
        "OCCLUSAL" => "оклузално / инцизално",
        "VESTIBULAR" => "букално / лабиално",
        "LINGUAL" => "лингвално / палатинално",
        "CERVICAL_LINGUAL" => "цервикално лингвално",
        "CERVICAL_VESTIBULAR" => "цервикално вестибуларно",
        _ => "неразпозната повърхност",
    }
    .to_string()
}

fn spoken_summary(entries: &[StatusReadEntry]) -> String {
    if entries.is_empty() {
        return "В текущия статус няма отбелязани находки.".into();
    }
    let mut by_tooth = BTreeMap::<String, Vec<String>>::new();
    for entry in entries {
        let mut details = Vec::new();
        if !entry.statuses.is_empty() {
            details.push(entry.statuses.join(", "));
        }
        if !entry.regions.is_empty() {
            details.push(entry.regions.join(", "));
        }
        if entry.for_observation {
            details.push("за наблюдение".into());
        }
        if let Some(note) = entry.note.as_deref() {
            details.push(format!("бележка: {}", note.trim()));
        }
        by_tooth
            .entry(entry.tooth.clone())
            .or_default()
            .push(details.join(", "));
    }
    let mut by_tooth = by_tooth.into_iter().collect::<Vec<_>>();
    by_tooth.sort_by_key(|(tooth, _)| tooth_sort_key(tooth));
    let details = by_tooth
        .into_iter()
        .map(|(tooth, rows)| format!("зъб {}: {}", spoken_tooth(&tooth), rows.join("; ")))
        .collect::<Vec<_>>();
    format!("Текущият статус е: {}.", details.join(". "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<StatusCatalogEntry> {
        vec![StatusCatalogEntry {
            id: "caries-id".into(),
            name: "Кариес".into(),
            code: "CARIES".into(),
            order: 1,
            diagnosis_id: None,
            can_have_regions: true,
            regions: vec!["OCCLUSAL".into()],
            incompatible_statuses: vec![],
            nzis_tooth_diagnosis_id: None,
        }]
    }

    #[test]
    fn reads_only_meaningful_status_rows_with_localized_names() {
        let result = build_status_read_result(
            vec![
                ToothStatus {
                    id: None,
                    tooth: "16".into(),
                    statuses: vec![],
                    is_milk_tooth: false,
                    for_observation: false,
                    regions: vec![],
                    timestamp: None,
                    note: None,
                    generated_by_procedure: false,
                },
                ToothStatus {
                    id: Some("row".into()),
                    tooth: "15".into(),
                    statuses: vec!["caries-id".into()],
                    is_milk_tooth: true,
                    for_observation: true,
                    regions: vec!["OCCLUSAL".into()],
                    timestamp: None,
                    note: Some("Контрол след шест месеца".into()),
                    generated_by_procedure: false,
                },
            ],
            &catalog(),
        );

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].tooth, "55");
        assert_eq!(result.entries[0].statuses, ["Кариес"]);
        assert_eq!(result.entries[0].regions, ["оклузално / инцизално"]);
        assert!(result.spoken_summary.contains("зъб пет пет"));
        assert!(result.spoken_summary.contains("за наблюдение"));
    }

    #[test]
    fn reads_status_in_the_clinical_quadrant_sequence() {
        let teeth = ["41", "11", "28", "38", "18", "21", "31", "48"]
            .into_iter()
            .map(|tooth| ToothStatus {
                id: Some(format!("row-{tooth}")),
                tooth: tooth.into(),
                statuses: vec!["caries-id".into()],
                is_milk_tooth: false,
                for_observation: false,
                regions: vec!["OCCLUSAL".into()],
                timestamp: None,
                note: None,
                generated_by_procedure: false,
            })
            .collect();

        let result = build_status_read_result(teeth, &catalog());
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.tooth.as_str())
                .collect::<Vec<_>>(),
            ["18", "11", "21", "28", "38", "31", "41", "48"]
        );
        let spoken_positions = [
            "зъб едно осем",
            "зъб едно едно",
            "зъб две едно",
            "зъб две осем",
            "зъб три осем",
            "зъб три едно",
            "зъб четири едно",
            "зъб четири осем",
        ]
        .map(|label| result.spoken_summary.find(label).unwrap());
        assert!(spoken_positions.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn empty_status_has_a_clear_spoken_result() {
        let result = build_status_read_result(Vec::new(), &catalog());
        assert!(result.entries.is_empty());
        assert_eq!(
            result.spoken_summary,
            "В текущия статус няма отбелязани находки."
        );
    }
}
