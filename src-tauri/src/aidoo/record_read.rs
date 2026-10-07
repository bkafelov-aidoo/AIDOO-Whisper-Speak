use super::{
    dental::{display_tooth, spoken_tooth, tooth_sort_key},
    types::{
        DiagnosisCatalogEntry, PatientDataCategory, PatientDataItem, PatientDataReadResult,
        PatientRecordDetails, ProcedureCatalogEntry, TreatmentReadEntry, TreatmentReadResult,
        VisitReadEntry, VisitReadResult, VisitStatusReference, VisitTreatment,
    },
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub fn build_treatment_read_result(
    treatments: Vec<VisitTreatment>,
    diagnoses: &[DiagnosisCatalogEntry],
    procedures: &[ProcedureCatalogEntry],
) -> TreatmentReadResult {
    let diagnosis_names = diagnoses
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name.as_str()))
        .collect::<HashMap<_, _>>();
    let procedure_names = procedures
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut entries = treatments
        .into_iter()
        .map(|treatment| TreatmentReadEntry {
            tooth: display_tooth(&treatment.tooth, treatment.is_milk_tooth),
            diagnosis: treatment
                .diagnosis_id
                .as_deref()
                .map(|id| {
                    diagnosis_names
                        .get(id)
                        .copied()
                        .unwrap_or("неразпозната диагноза")
                })
                .map(str::to_string),
            procedures: treatment
                .procedures
                .iter()
                .map(|procedure| {
                    procedure_names
                        .get(procedure.procedure_id.as_str())
                        .copied()
                        .unwrap_or("неразпозната процедура")
                        .to_string()
                })
                .collect(),
            note: treatment.note.filter(|note| !note.trim().is_empty()),
            status: treatment.status.filter(|status| !status.trim().is_empty()),
            is_milk_tooth: treatment.is_milk_tooth,
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| tooth_sort_key(&entry.tooth));
    TreatmentReadResult {
        spoken_summary: treatment_summary(&entries),
        entries,
    }
}

pub fn build_visit_read_result(mut visits: Vec<VisitStatusReference>) -> VisitReadResult {
    visits.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    let entries = visits
        .into_iter()
        .filter(|visit| !visit.cancelled)
        .take(10)
        .map(|visit| VisitReadEntry {
            date: visit
                .timestamp
                .as_deref()
                .map(display_date)
                .unwrap_or_else(|| "неизвестна дата".into()),
            finished: visit.is_finished,
            has_status: visit.created_status_update,
            note: visit.note.filter(|note| !note.trim().is_empty()),
        })
        .collect::<Vec<_>>();
    VisitReadResult {
        spoken_summary: visit_summary(&entries),
        entries,
    }
}

pub fn build_patient_data_result(
    patient: PatientRecordDetails,
    category: PatientDataCategory,
) -> PatientDataReadResult {
    let mut items = Vec::new();
    if matches!(
        category,
        PatientDataCategory::Contact | PatientDataCategory::All
    ) {
        push_optional(&mut items, "Телефон", patient.mobile_phone);
        push_optional(&mut items, "Имейл", patient.email);
        let address = [
            patient.city,
            patient.street,
            patient.street_number,
            patient.neighbourhood,
            patient.block,
            patient.entrance,
            patient.floor,
            patient.apartment,
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        push_value(&mut items, "Адрес", address);
    }
    if matches!(
        category,
        PatientDataCategory::Medical | PatientDataCategory::All
    ) {
        push_json(&mut items, "Алергии", patient.allergies);
        push_json(&mut items, "Заболявания", patient.diseases);
        push_json(&mut items, "Медицинска история", patient.medical_history);
        if patient.mental_illness {
            push_value(&mut items, "Психично заболяване", "да".into());
        }
    }
    if matches!(
        category,
        PatientDataCategory::Insurance | PatientDataCategory::All
    ) {
        if let Some(insured) = patient.public_health_insured {
            push_value(
                &mut items,
                "Здравно осигурен",
                if insured { "да" } else { "не" }.into(),
            );
        }
        if patient.pensioner {
            push_value(&mut items, "Пенсионер", "да".into());
        }
        if patient.institutionalized {
            push_value(&mut items, "Институционализиран", "да".into());
        }
    }
    if matches!(
        category,
        PatientDataCategory::Identity | PatientDataCategory::All
    ) {
        let name = [
            Some(patient.first_name),
            patient.middle_name,
            Some(patient.last_name),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        push_value(&mut items, "Пациент", name);
        push_optional(&mut items, "Дата на раждане", patient.birthdate);
        push_optional(&mut items, "Пол", patient.gender);
    }
    PatientDataReadResult {
        spoken_summary: patient_data_summary(&items),
        items,
    }
}

fn treatment_summary(entries: &[TreatmentReadEntry]) -> String {
    if entries.is_empty() {
        return "В избраното посещение няма въведени лечения.".into();
    }
    let rows = entries
        .iter()
        .map(|entry| {
            let mut details = Vec::new();
            if let Some(diagnosis) = &entry.diagnosis {
                details.push(format!("диагноза {diagnosis}"));
            }
            if !entry.procedures.is_empty() {
                details.push(format!("процедури {}", entry.procedures.join(", ")));
            }
            if let Some(status) = &entry.status {
                details.push(format!("състояние {status}"));
            }
            if let Some(note) = &entry.note {
                details.push(format!("бележка: {}", note.trim()));
            }
            if details.is_empty() {
                details.push("няма допълнителни данни".into());
            }
            format!("зъб {}: {}", spoken_tooth(&entry.tooth), details.join(", "))
        })
        .collect::<Vec<_>>();
    format!("Леченията са: {}.", rows.join(". "))
}

fn visit_summary(entries: &[VisitReadEntry]) -> String {
    if entries.is_empty() {
        return "Няма записани посещения.".into();
    }
    let rows = entries
        .iter()
        .map(|entry| {
            let mut details = vec![if entry.finished {
                "приключено"
            } else {
                "активно"
            }];
            if entry.has_status {
                details.push("има статус");
            }
            let mut text = format!("{} — {}", entry.date, details.join(", "));
            if let Some(note) = &entry.note {
                text.push_str(&format!(", бележка: {}", note.trim()));
            }
            text
        })
        .collect::<Vec<_>>();
    format!("Последните посещения са: {}.", rows.join(". "))
}

fn patient_data_summary(items: &[PatientDataItem]) -> String {
    if items.is_empty() {
        return "В тази част на картона няма попълнени данни.".into();
    }
    format!(
        "Данните в картона са: {}.",
        items
            .iter()
            .map(|item| format!("{}: {}", item.label, item.value))
            .collect::<Vec<_>>()
            .join(". ")
    )
}

fn display_date(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc).format("%d.%m.%Y").to_string())
        .unwrap_or_else(|_| value.chars().take(10).collect())
}

fn push_optional(items: &mut Vec<PatientDataItem>, label: &str, value: Option<String>) {
    if let Some(value) = value {
        push_value(items, label, value);
    }
}

fn push_value(items: &mut Vec<PatientDataItem>, label: &str, value: String) {
    let value = value.trim();
    if !value.is_empty() {
        items.push(PatientDataItem {
            label: label.into(),
            value: value.chars().take(1_000).collect(),
        });
    }
}

fn push_json(items: &mut Vec<PatientDataItem>, label: &str, value: serde_json::Value) {
    if let Some(value) = clinical_value_text(&value) {
        push_value(items, label, value);
    }
}

fn clinical_value_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => (!value.trim().is_empty()).then(|| value.clone()),
        serde_json::Value::Bool(value) => Some(if *value { "да" } else { "не" }.into()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Array(values) => {
            let values = values
                .iter()
                .filter_map(clinical_value_text)
                .collect::<Vec<_>>();
            (!values.is_empty()).then(|| values.join(", "))
        }
        serde_json::Value::Object(values) => {
            let values = values
                .values()
                .filter_map(clinical_value_text)
                .collect::<Vec<_>>();
            (!values.is_empty()).then(|| values.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn treatment_read_maps_catalog_ids_and_milk_teeth() {
        let result = build_treatment_read_result(
            vec![VisitTreatment {
                id: "row".into(),
                tooth: "15".into(),
                diagnosis_id: Some("diagnosis".into()),
                treatment_id: None,
                note: Some("Контрол".into()),
                status: None,
                is_milk_tooth: true,
                procedures: vec![super::super::types::TreatmentProcedure {
                    id: None,
                    procedure_id: "procedure".into(),
                    price: json!(0),
                    discount: json!(0),
                }],
            }],
            &[DiagnosisCatalogEntry {
                id: "diagnosis".into(),
                name: "Пулпит".into(),
                key: "K04".into(),
            }],
            &[ProcedureCatalogEntry {
                id: "procedure".into(),
                name: "Обтурация".into(),
                key: "P1".into(),
                price: json!(0),
                price_currency: None,
            }],
        );
        assert_eq!(result.entries[0].tooth, "55");
        assert!(result.spoken_summary.contains("диагноза Пулпит"));
        assert!(result.spoken_summary.contains("процедури Обтурация"));
    }

    #[test]
    fn patient_medical_read_omits_identity_and_contact_data() {
        let patient = PatientRecordDetails {
            first_name: "Иван".into(),
            middle_name: None,
            last_name: "Иванов".into(),
            birthdate: Some("1980-01-01".into()),
            email: Some("patient@example.test".into()),
            mobile_phone: Some("000".into()),
            city: None,
            street: None,
            street_number: None,
            neighbourhood: None,
            block: None,
            entrance: None,
            floor: None,
            apartment: None,
            allergies: json!(["Пеницилин"]),
            diseases: json!(null),
            medical_history: json!("Хипертония"),
            public_health_insured: Some(true),
            pensioner: false,
            institutionalized: false,
            mental_illness: false,
            gender: None,
        };
        let result = build_patient_data_result(patient, PatientDataCategory::Medical);
        assert!(result.spoken_summary.contains("Пеницилин"));
        assert!(result.spoken_summary.contains("Хипертония"));
        assert!(!result.spoken_summary.contains("1980"));
        assert!(!result.spoken_summary.contains("example.test"));
    }
}
