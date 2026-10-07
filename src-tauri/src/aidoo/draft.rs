use super::{
    dental::{display_tooth, spoken_tooth},
    types::*,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn build_draft(
    patient_id: String,
    visit: &Visit,
    is_nzok: bool,
    baseline: Vec<ToothStatus>,
    catalog: &[StatusCatalogEntry],
    changes: &[StatusChange],
) -> Result<StatusDraft, String> {
    if changes.is_empty() {
        return Err("Липсва промяна за зъбния статус.".into());
    }
    if patient_id.trim().is_empty() || visit.id.trim().is_empty() {
        return Err("Липсва пациент или посещение.".into());
    }
    let catalog_by_id = catalog
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let normalized_changes = changes
        .iter()
        .cloned()
        .map(|mut change| -> Result<StatusChange, String> {
            let (tooth, is_milk_tooth) = normalize_tooth(&change.tooth, change.is_milk_tooth)?;
            change.tooth = tooth;
            change.is_milk_tooth = is_milk_tooth;
            change.regions = normalize_regions(&change.regions);
            Ok(change)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut state = status_map(&baseline)?;
    let baseline_state = state.clone();
    let mut touched = BTreeSet::new();
    let mut replacement_sources = vec![None; normalized_changes.len()];
    let mut consumed_sources = BTreeSet::new();

    for (index, change) in normalized_changes.iter().enumerate() {
        validate_change(change, &catalog_by_id)?;
        if change.operation == StatusOperation::Replace {
            let existing = change
                .existing_status_id
                .as_ref()
                .ok_or_else(|| "При замяна трябва да е посочен текущият статус.".to_string())?;
            let source = resolve_replacement_source(&baseline_state, change, existing)?;
            if !consumed_sources.insert((source.clone(), existing.clone())) {
                return Err("Един и същ текущ статус е посочен за повече от една замяна.".into());
            }
            replacement_sources[index] = Some(source);
        }
    }

    for (change, source) in normalized_changes.iter().zip(&replacement_sources) {
        let Some(source) = source else { continue };
        let existing = change
            .existing_status_id
            .as_ref()
            .ok_or_else(|| "При замяна трябва да е посочен текущият статус.".to_string())?;
        let entry = state
            .get_mut(source)
            .ok_or_else(|| "Липсва текущ статус за замяна.".to_string())?;
        entry.statuses.retain(|status| status != existing);
        touched.insert(source.clone());
    }

    for change in &normalized_changes {
        let regions = sorted_unique(&change.regions);
        let key = (change.tooth.clone(), regions.clone());
        state
            .entry(key.clone())
            .or_insert_with(|| ToothStatusWrite {
                tooth: change.tooth.clone(),
                statuses: Vec::new(),
                is_milk_tooth: change.is_milk_tooth,
                for_observation: change.for_observation,
                regions: regions.clone(),
                note: change.note.clone(),
            });
        ensure_status_compatible(&state, change, &catalog_by_id)?;
        let entry = state
            .get_mut(&key)
            .ok_or_else(|| "Липсва ред за зъбния статус.".to_string())?;
        if !regions.is_empty() {
            entry.is_milk_tooth = false;
            entry.for_observation = false;
        }
        if let Some(note) = &change.note {
            entry.note = Some(note.clone());
        }
        entry.statuses.push(change.status_id.clone());
        entry.statuses = sorted_unique(&entry.statuses);
        touched.insert(key);
    }

    let writes = touched
        .into_iter()
        .filter_map(|key| state.remove(&key))
        .collect::<Vec<_>>();
    Ok(StatusDraft {
        id: uuid::Uuid::new_v4().to_string(),
        patient_id,
        visit_id: visit.id.clone(),
        is_nzok,
        create_status_update: !visit.created_status_update,
        baseline,
        spoken_summary: spoken_summary(&normalized_changes, catalog)?,
        writes,
    })
}

pub fn editable_status_catalog(entries: Vec<StatusCatalogEntry>) -> Vec<StatusCatalogEntry> {
    entries
        .into_iter()
        .filter(StatusCatalogEntry::is_editable_aidoo_status)
        .collect()
}

pub fn same_snapshot(left: &[ToothStatus], right: &[ToothStatus]) -> bool {
    match (normalized_snapshot(left), normalized_snapshot(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub fn verifies(writes: &[ToothStatusWrite], actual: &[ToothStatus]) -> bool {
    let Ok(actual) = status_map(actual) else {
        return false;
    };
    writes.iter().all(|expected| {
        let key = (expected.tooth.clone(), sorted_unique(&expected.regions));
        actual.get(&key).is_some_and(|found| {
            found.statuses == sorted_unique(&expected.statuses)
                && found.is_milk_tooth == expected.is_milk_tooth
                && found.for_observation == expected.for_observation
                && found.note == expected.note
        })
    })
}

fn normalized_snapshot(
    values: &[ToothStatus],
) -> Result<BTreeMap<(String, Vec<String>), ToothStatusWrite>, String> {
    status_map(values)
}

fn resolve_replacement_source(
    state: &BTreeMap<(String, Vec<String>), ToothStatusWrite>,
    change: &StatusChange,
    existing_status_id: &str,
) -> Result<(String, Vec<String>), String> {
    let destination = (change.tooth.clone(), sorted_unique(&change.regions));
    if state.get(&destination).is_some_and(|entry| {
        entry
            .statuses
            .iter()
            .any(|status| status == existing_status_id)
    }) {
        return Ok(destination);
    }
    let candidates = state
        .iter()
        .filter(|((tooth, _), entry)| {
            tooth == &change.tooth
                && entry
                    .statuses
                    .iter()
                    .any(|status| status == existing_status_id)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Err("Статусът за замяна вече не присъства в текущия запис.".into()),
        [source] => Ok(source.clone()),
        _ => Err(
            "Статусът за замяна присъства на повече от една повърхност. Посочете точната повърхност."
                .into(),
        ),
    }
}

fn validate_change(
    change: &StatusChange,
    catalog: &BTreeMap<&str, &StatusCatalogEntry>,
) -> Result<(), String> {
    if !valid_tooth(&change.tooth) {
        return Err("Невалиден номер на зъб.".into());
    }
    let status = catalog
        .get(change.status_id.as_str())
        .copied()
        .filter(|entry| entry.is_editable_aidoo_status())
        .ok_or_else(|| "Избраният статус липсва в актуалния AIDOO каталог.".to_string())?;
    if let Some(existing) = &change.existing_status_id {
        if !catalog
            .get(existing.as_str())
            .is_some_and(|entry| entry.is_editable_aidoo_status())
        {
            return Err("Статусът за замяна липсва в актуалния AIDOO каталог.".into());
        }
    }
    if change
        .regions
        .iter()
        .any(|region| !ALLOWED_REGIONS.contains(&region.as_str()))
    {
        return Err("Невалидна зъбна повърхност.".into());
    }
    if status.can_have_regions && change.regions.is_empty() {
        return Err("Избраният статус изисква поне една зъбна повърхност.".into());
    }
    if !status.can_have_regions && !change.regions.is_empty() {
        return Err("Избраният статус не се нанася по зъбни повърхности.".into());
    }
    if change
        .regions
        .iter()
        .any(|region| !status.regions.iter().any(|allowed| allowed == region))
    {
        return Err("Избраната повърхност не е разрешена за този AIDOO статус.".into());
    }
    Ok(())
}

fn ensure_status_compatible(
    state: &BTreeMap<(String, Vec<String>), ToothStatusWrite>,
    change: &StatusChange,
    catalog: &BTreeMap<&str, &StatusCatalogEntry>,
) -> Result<(), String> {
    let candidate = catalog
        .get(change.status_id.as_str())
        .copied()
        .ok_or_else(|| "Избраният статус липсва в актуалния AIDOO каталог.".to_string())?;
    for existing_id in state
        .iter()
        .filter(|((tooth, _), _)| tooth == &change.tooth)
        .flat_map(|(_, value)| value.statuses.iter())
    {
        if existing_id == &change.status_id {
            continue;
        }
        let Some(existing) = catalog.get(existing_id.as_str()).copied() else {
            continue;
        };
        if candidate
            .incompatible_statuses
            .iter()
            .any(|id| id == existing_id)
            || existing
                .incompatible_statuses
                .iter()
                .any(|id| id == &change.status_id)
        {
            return Err(format!(
                "Статусите „{}“ и „{}“ са несъвместими за зъб {}.",
                candidate.name,
                existing.name,
                spoken_tooth(&display_tooth(&change.tooth, change.is_milk_tooth))
            ));
        }
    }
    Ok(())
}

fn normalize_regions(values: &[String]) -> Vec<String> {
    sorted_unique(
        &values
            .iter()
            .map(|value| match value.as_str() {
                "PALATAL" => "LINGUAL".to_string(),
                "CERVICAL_PALATAL" => "CERVICAL_LINGUAL".to_string(),
                _ => value.clone(),
            })
            .collect::<Vec<_>>(),
    )
}

fn valid_tooth(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    let Ok(number) = value.parse::<u8>() else {
        return false;
    };
    let quadrant = number / 10;
    let position = number % 10;
    matches!(quadrant, 1..=4) && matches!(position, 1..=8)
}

fn normalize_tooth(value: &str, is_milk_tooth: bool) -> Result<(String, bool), String> {
    if value == "*" {
        return if is_milk_tooth {
            Err("Звездичката не може да бъде маркирана като млечен зъб.".into())
        } else {
            Ok((value.into(), false))
        };
    }
    let number = value
        .parse::<u8>()
        .map_err(|_| "Невалиден номер на зъб.".to_string())?;
    let quadrant = number / 10;
    let position = number % 10;
    if matches!(quadrant, 5..=8) && matches!(position, 1..=5) {
        return Ok((format!("{}{}", quadrant - 4, position), true));
    }
    if !matches!(quadrant, 1..=4) || !matches!(position, 1..=8) {
        return Err("Невалиден номер на зъб.".into());
    }
    if is_milk_tooth && position > 5 {
        return Err("Този номер няма млечен зъб в AIDOO.".into());
    }
    Ok((value.into(), is_milk_tooth))
}

fn spoken_summary(
    changes: &[StatusChange],
    catalog: &[StatusCatalogEntry],
) -> Result<String, String> {
    let names = catalog
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut parts = Vec::with_capacity(changes.len());
    for change in changes {
        let name = names
            .get(change.status_id.as_str())
            .ok_or_else(|| "Липсва име за избрания статус.".to_string())?;
        let surface = if change.regions.is_empty() {
            String::new()
        } else {
            format!(" ({})", change.regions.join(", "))
        };
        let verb = match change.operation {
            StatusOperation::Add => "добавя",
            StatusOperation::Replace => "заменя със",
        };
        let tooth = spoken_tooth(&display_tooth(&change.tooth, change.is_milk_tooth));
        let observation = if change.for_observation {
            " за наблюдение"
        } else {
            ""
        };
        parts.push(format!(
            "{verb} {name}{surface} на зъб {tooth}{observation}"
        ));
    }
    Ok(format!("Ще {}. Да го запиша ли?", parts.join(" и ")))
}
