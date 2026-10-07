use super::*;

fn milk_treatment_row(is_milk_tooth: bool) -> types::VisitTreatment {
    types::VisitTreatment {
        id: "milk-row".into(),
        tooth: "34".into(),
        diagnosis_id: None,
        treatment_id: None,
        note: None,
        status: None,
        is_milk_tooth,
        procedures: Vec::new(),
    }
}

#[test]
fn spoken_milk_fdi_reuses_the_existing_base_key_row() {
    assert_eq!(
        resolve_treatment_row(&[milk_treatment_row(true)], "74", None).unwrap(),
        Some("milk-row".into()),
        "missing this row would create a duplicate treatment"
    );
}

#[test]
fn explicit_milk_row_accepts_its_spoken_fdi_number() {
    assert_eq!(
        resolve_treatment_row(&[milk_treatment_row(true)], "74", Some("milk-row")).unwrap(),
        Some("milk-row".into()),
    );
}

#[test]
fn a_spoken_milk_tooth_never_binds_to_a_permanent_row() {
    assert_eq!(resolve_treatment_row(&[], "74", None).unwrap(), None);
    assert_eq!(
        resolve_treatment_row(&[milk_treatment_row(false)], "74", None).unwrap(),
        None,
    );
    assert!(resolve_treatment_row(&[milk_treatment_row(false)], "74", Some("milk-row")).is_err());
}

#[test]
fn a_spoken_permanent_tooth_never_binds_to_a_milk_row() {
    assert_eq!(
        resolve_treatment_row(&[milk_treatment_row(true)], "34", None).unwrap(),
        None,
    );
    assert!(resolve_treatment_row(&[milk_treatment_row(true)], "34", Some("milk-row")).is_err());
}

#[test]
fn catalog_matching_accepts_full_name_code_and_one_unique_partial() {
    let entries = vec![
        types::ProcedureCatalogEntry {
            id: "one".into(),
            name: "Професионално почистване".into(),
            key: "PROC-1".into(),
            price: serde_json::json!(10),
            price_currency: Some("BGN".into()),
        },
        types::ProcedureCatalogEntry {
            id: "two".into(),
            name: "Обтурация".into(),
            key: "PROC-2".into(),
            price: serde_json::json!(20),
            price_currency: Some("BGN".into()),
        },
    ];
    assert_eq!(resolve_procedure(&entries, "proc 1").unwrap().id, "one");
    assert_eq!(resolve_procedure(&entries, "обтурац").unwrap().id, "two");
    assert!(resolve_procedure(&entries, "липсваща").is_err());
}

#[test]
fn nzok_person_types_and_spoken_milk_teeth_match_aidoo() {
    assert_eq!(nzok_person_type("ЕГН"), Some("1"));
    assert_eq!(nzok_person_type("ЛНЧ"), Some("2"));
    assert_eq!(spoken_treatment_tooth("15", true).unwrap(), "55");
    assert_eq!(spoken_treatment_tooth("55", true).unwrap(), "55");
    assert!(spoken_treatment_tooth("16", true).is_err());
}

#[test]
fn treatment_row_is_automatic_only_when_unambiguous() {
    let row = |id: &str| types::VisitTreatment {
        id: id.into(),
        tooth: "16".into(),
        diagnosis_id: None,
        treatment_id: None,
        note: None,
        status: None,
        is_milk_tooth: false,
        procedures: Vec::new(),
    };
    assert_eq!(resolve_treatment_row(&[], "16", None).unwrap(), None);
    assert_eq!(
        resolve_treatment_row(&[row("one")], "16", None).unwrap(),
        Some("one".into())
    );
    assert!(resolve_treatment_row(&[row("one"), row("two")], "16", None).is_err());
    assert!(resolve_treatment_row(&[row("one")], "26", Some("one"))
        .unwrap_err()
        .contains("друг зъб"));
}

#[test]
fn visible_treatment_tooth_result_reports_only_verified_selection() {
    let specific = treatment_tooth_selection_result("18".into());
    assert_eq!(specific.tooth, "18");
    assert_eq!(specific.spoken_summary, "Показвам зъб едно осем.");
    assert!(specific.visible_in_browser);

    let general = treatment_tooth_selection_result("*".into());
    assert_eq!(general.tooth, "*");
    assert_eq!(general.spoken_summary, "Показвам общите процедури.");
    assert!(general.visible_in_browser);
}

#[test]
fn spoken_result_claims_saved_only_after_verified_read_back() {
    let verified = clinical_result(
        "Ще запиша кариес на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Verified,
            message: "verified".into(),
        },
        Ok(()),
    );
    assert_eq!(
        verified.spoken_summary,
        "Записано: кариес на зъб едно шест."
    );
    assert!(verified.visible_in_browser);
    assert_eq!(verified.presentation_error, None);

    let rejected = clinical_result(
        "Ще запиша кариес на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Rejected,
            message: "Записът не е потвърден.".into(),
        },
        Ok(()),
    );
    assert_eq!(rejected.spoken_summary, "Записът не е потвърден.");
}

#[test]
fn verified_write_is_not_reported_as_visibly_complete_when_chrome_does_not_refresh() {
    let result = clinical_result(
        "Ще запиша кариес на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Verified,
            message: "verified".into(),
        },
        Err("Chrome не потвърди опресняването.".into()),
    );

    assert!(!result.visible_in_browser);
    assert_eq!(
        result.presentation_error.as_deref(),
        Some("Chrome не потвърди опресняването.")
    );
    assert_eq!(
        result.spoken_summary,
        "Записано е, но картонът не се опресни на екрана."
    );
}

#[test]
fn tooth_by_tooth_status_repeats_every_verified_status() {
    let verified = status_clinical_result(
        "Ще добавя Кариес на зъб едно шест и добавя Обтурация на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Verified,
            message: "verified".into(),
        },
        Ok(()),
    );
    assert_eq!(
        verified.spoken_summary,
        "Записах: Кариес на зъб едно шест и Обтурация на зъб едно шест."
    );

    let rejected = status_clinical_result(
        "Ще запиша кариес на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Rejected,
            message: "Записът не е потвърден.".into(),
        },
        Ok(()),
    );
    assert_eq!(rejected.spoken_summary, "Повтори.");
}

#[test]
fn saved_status_with_failed_refresh_keeps_the_duplicate_write_warning() {
    let result = status_clinical_result(
        "Ще запиша кариес на зъб едно шест. Да го запиша ли?",
        types::VerificationResult {
            outcome: types::VerificationOutcome::Verified,
            message: "verified".into(),
        },
        Err("Chrome не потвърди опресняването.".into()),
    );

    assert_eq!(
        result.spoken_summary,
        "Записано е, но картонът не се опресни на екрана."
    );
}
