use super::*;

#[tokio::test]
async fn client_reads_the_visit_list_used_for_historical_status_fallback() {
    let (base, requests) = scripted_server(vec![ResponseScript::json(
        200,
        r#"[{"id":"visit-id","createdStatusUpdate":true,"cancelled":false,"timestamp":"2026-09-17T10:00:00Z"}]"#,
    )]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let visits = client
        .visits("session-token", "clinic-id", "patient-id")
        .await
        .unwrap();
    assert_eq!(visits.len(), 1);
    assert!(visits[0].created_status_update);
    assert_eq!(visits[0].timestamp.as_deref(), Some("2026-09-17T10:00:00Z"));
    let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(request.starts_with("GET /clinics/clinic-id/patients/patient-id/visits HTTP/1.1"));
}

#[test]
fn milk_tooth_and_observation_follow_the_aidoo_base_tooth_contract() {
    let healthy = StatusCatalogEntry {
        id: "healthy-id".into(),
        name: "Здрав".into(),
        code: "H".into(),
        order: 1,
        diagnosis_id: None,
        can_have_regions: false,
        regions: vec![],
        incompatible_statuses: vec![],
        nzis_tooth_diagnosis_id: None,
    };
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![],
        &[healthy],
        &[StatusChange {
            operation: StatusOperation::Add,
            tooth: "55".into(),
            status_id: "healthy-id".into(),
            regions: vec![],
            existing_status_id: None,
            is_milk_tooth: false,
            for_observation: true,
            note: None,
        }],
    )
    .unwrap();

    assert_eq!(draft.writes[0].tooth, "15");
    assert!(draft.writes[0].is_milk_tooth);
    assert!(draft.writes[0].for_observation);
    assert!(draft.spoken_summary.contains("зъб пет пет за наблюдение"));
}

#[test]
fn several_spoken_statuses_become_one_multi_write_draft() {
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![],
        &catalog(),
        &[
            StatusChange {
                operation: StatusOperation::Add,
                tooth: "16".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: None,
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
            StatusChange {
                operation: StatusOperation::Add,
                tooth: "26".into(),
                status_id: "restoration-id".into(),
                regions: vec!["MESIAL".into()],
                existing_status_id: None,
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
        ],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 2);
    assert!(draft.spoken_summary.contains("зъб едно шест"));
    assert!(draft.spoken_summary.contains("зъб две шест"));
}

#[tokio::test]
async fn grouped_statuses_use_one_put_and_one_independent_readback() {
    let desired_16 = status_json("16", &["caries-id"], &["OCCLUSAL"]);
    let desired_26 = status_json("26", &["restoration-id"], &["MESIAL"]);
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, r#"{"teethStatus":[]}"#),
        ResponseScript::json(
            200,
            &format!(r#"{{"teethStatus":[{desired_16},{desired_26}]}}"#),
        ),
        ResponseScript::json(
            200,
            &format!(
                r#"{{"visitTeethStatus":[{{"currentToothStatus":{desired_16},"previousToothStatus":null}},{{"currentToothStatus":{desired_26},"previousToothStatus":null}}]}}"#
            ),
        ),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![],
        &catalog(),
        &[
            StatusChange {
                operation: StatusOperation::Add,
                tooth: "16".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: None,
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
            StatusChange {
                operation: StatusOperation::Add,
                tooth: "26".into(),
                status_id: "restoration-id".into(),
                regions: vec!["MESIAL".into()],
                existing_status_id: None,
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
        ],
    )
    .unwrap();

    let result = apply_confirmed_draft(&client, "token", "clinic-id", &draft)
        .await
        .unwrap();
    assert_eq!(result.outcome, VerificationOutcome::Verified);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("PUT "))
            .count(),
        1
    );
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.contains("/teeth-status/visits/"))
            .count(),
        1
    );
    assert!(captured[1].contains("\"tooth\":\"16\""));
    assert!(captured[1].contains("\"tooth\":\"26\""));
}

#[test]
fn new_milk_treatment_combines_diagnosis_and_procedure_in_one_draft() {
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment("existing-row", None, None, None, &[])],
        &diagnosis_catalog(),
        &procedure_catalog(),
        TreatmentChange {
            tooth: "55".into(),
            existing_treatment_id: None,
            diagnosis_id: Some("diagnosis-id".into()),
            treatment_id: None,
            note: None,
            procedure_ids: vec!["procedure-id".into()],
        },
    )
    .unwrap();

    assert_eq!(draft.existing_treatment_id, None);
    assert_eq!(draft.treatment.tooth, "15");
    assert!(draft.treatment.is_milk_tooth);
    assert_eq!(
        draft.treatment.diagnosis_id.as_deref(),
        Some("diagnosis-id")
    );
    assert_eq!(draft.procedures.len(), 1);
    assert!(draft.spoken_summary.contains("зъб пет пет"));
}

#[tokio::test]
async fn client_uses_the_observed_nzok_status_check_contract() {
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, r#""<xml>check</xml>""#),
        ResponseScript::json(200, "true"),
        ResponseScript::json(
            200,
            r#"{"id":"patient-id","firstName":"Тест","lastName":"Пациент","identifier":"0000000000","identifierType":"ЕГН"}"#,
        ),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let xml = client
        .nzis_status_search_xml("token", "clinic-id", "patient-id")
        .await
        .unwrap();
    assert_eq!(xml, "<xml>check</xml>");
    assert!(client
        .has_available_status_check(
            "token",
            "clinic-id",
            "<signed />",
            &serde_json::json!({"history": []}),
        )
        .await
        .unwrap());
    let patient = client
        .patient_details("token", "clinic-id", "patient-id")
        .await
        .unwrap();
    assert_eq!(patient.identifier.as_deref(), Some("0000000000"));
    assert_eq!(patient.identifier_type.as_deref(), Some("ЕГН"));

    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert!(captured[0].starts_with(
        "GET /clinics/clinic-id/nzok-checks/patients/patient-id/nzis-search-xml HTTP/1.1"
    ));
    assert!(
        captured[1].starts_with("POST /clinics/clinic-id/nzok-checks/has-status-checks HTTP/1.1")
    );
    assert!(captured[1].contains("\"signedNzisXml\":\"<signed />\""));
    assert!(captured[1].contains("\"nzokData\":{\"history\":[]}"));
    assert!(captured[2].starts_with("GET /clinics/clinic-id/patients/patient-id HTTP/1.1"));
}

#[tokio::test]
async fn active_visit_maps_the_observed_no_visit_400_to_missing() {
    let (base, requests) = scripted_server(vec![ResponseScript::json(
        400,
        r#"{"error":"No active visit found for patient with id: patient-id","details":null}"#,
    )]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let error = client
        .active_visit("token", "clinic-id", "patient-id")
        .await
        .unwrap_err();

    assert!(error.is_not_found());
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        request.starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1")
    );
}

#[test]
fn treatment_draft_targets_one_of_multiple_rows_and_allows_note_only() {
    let baseline = vec![
        treatment("row-a", None, Some("Първи ред"), None, &[]),
        treatment("row-b", None, Some("Втори ред"), None, &[]),
    ];
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        baseline,
        &[],
        &[],
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("row-b".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: Some("  Само официална забележка.  ".into()),
            procedure_ids: vec![],
        },
    )
    .unwrap();

    assert_eq!(draft.existing_treatment_id.as_deref(), Some("row-b"));
    assert_eq!(
        draft.treatment.note.as_deref(),
        Some("Само официална забележка.")
    );
    assert!(draft.procedures.is_empty());
}

#[test]
fn treatment_draft_rejects_existing_procedure_and_unverified_treatment_pair() {
    let duplicate = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment(
            "treatment-row",
            None,
            None,
            None,
            &["procedure-id"],
        )],
        &[],
        &procedure_catalog(),
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            procedure_ids: vec!["procedure-id".into()],
        },
    )
    .unwrap_err();
    assert!(duplicate.contains("вече съществува"));

    let incompatible = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment(
            "treatment-row",
            Some("old-diagnosis"),
            None,
            Some("linked-treatment"),
            &[],
        )],
        &diagnosis_catalog(),
        &[],
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: Some("diagnosis-id".into()),
            treatment_id: Some("linked-treatment".into()),
            note: None,
            procedure_ids: vec![],
        },
    )
    .unwrap_err();
    assert!(incompatible.contains("свързано лечение"));
}

#[test]
fn treatment_draft_uses_the_catalog_price_for_the_clinic_currency() {
    let procedures = vec![ProcedureCatalogEntry {
        id: "procedure-bgn".into(),
        name: "Процедура в лева".into(),
        key: "BG1".into(),
        price: serde_json::json!("95.00"),
        price_currency: Some("BGN".into()),
    }];
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment("treatment-row", None, None, None, &[])],
        &[],
        &procedures,
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            procedure_ids: vec!["procedure-bgn".into()],
        },
    )
    .unwrap();

    assert_eq!(draft.procedures[0].price, "95.00");
}

#[test]
fn treatment_snapshot_ignores_order_but_detects_row_changes() {
    let first = treatment("row-a", None, None, None, &["procedure-a", "procedure-b"]);
    let second = treatment("row-b", None, Some("Бележка"), None, &[]);
    let mut reordered_first = first.clone();
    reordered_first.procedures.reverse();
    assert!(same_treatment_snapshot(
        &[first.clone(), second.clone()],
        &[second.clone(), reordered_first]
    ));

    let unchanged_first = first.clone();
    let mut changed = second;
    changed.note = Some("Променено от браузъра".into());
    assert!(!same_treatment_snapshot(
        &[first, treatment("row-b", None, Some("Бележка"), None, &[])],
        &[unchanged_first, changed]
    ));
}

#[tokio::test]
async fn procedure_catalog_uses_the_clinic_currency_without_rewriting_prices() {
    let (base, requests) = scripted_server(vec![ResponseScript::json(
        200,
        r#"[{"id":"procedure-bgn","name":"Процедура","key":"BG1","price":"95.00","priceCurrency":"BGN"}]"#,
    )]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let catalog = client
        .procedure_catalog("token", "clinic-id", Some("BGN"))
        .await
        .unwrap();

    assert_eq!(catalog[0].price_text(), "95.00");
    assert_eq!(catalog[0].price_currency.as_deref(), Some("BGN"));
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(request.starts_with(
        "GET /clinics/clinic-id/procedures/prices?sortBy=name&direction=ASC&currency=BGN HTTP/1.1"
    ));
}

#[tokio::test]
async fn treatment_workflow_rejects_a_row_changed_in_the_browser_before_writing() {
    let before = treatment("treatment-row", None, Some("Преди"), None, &[]);
    let changed = treatment_json("treatment-row", None, Some("Променено в браузъра"), &[]);
    let (base, requests) =
        scripted_server(vec![ResponseScript::json(200, &format!("[{changed}]"))]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![before],
        &[],
        &[],
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: Some("Нова забележка".into()),
            procedure_ids: vec![],
        },
    )
    .unwrap();

    let result = apply_confirmed_treatment_draft(&client, "token", "clinic-id", &draft)
        .await
        .unwrap();
    assert_eq!(result.outcome, VerificationOutcome::StaleDraft);
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(request.starts_with("GET "));
    assert!(requests.recv_timeout(Duration::from_millis(150)).is_err());
}

#[tokio::test]
async fn incompatible_first_procedure_is_rejected_without_an_earlier_write() {
    let before = treatment_json("treatment-row", None, None, &[]);
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, &format!("[{before}]")),
        ResponseScript::json(422, r#"{"error":"incompatible procedure"}"#),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment("treatment-row", None, None, None, &[])],
        &[],
        &procedure_catalog(),
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            procedure_ids: vec!["procedure-id".into()],
        },
    )
    .unwrap();

    let result = apply_confirmed_treatment_draft(&client, "token", "clinic-id", &draft)
        .await
        .unwrap();
    assert_eq!(result.outcome, VerificationOutcome::Rejected);
    let captured = (0..2)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert!(captured[0].starts_with("GET "));
    assert!(captured[1].starts_with("POST "));
    assert!(captured.iter().all(|request| !request.starts_with("PUT ")));
}

#[tokio::test]
async fn partial_procedure_success_is_read_back_and_never_retried() {
    let before = treatment_json("treatment-row", None, None, &[]);
    let after_first = treatment_json("treatment-row", None, None, &["procedure-a"]);
    let procedures = vec![
        ProcedureCatalogEntry {
            id: "procedure-a".into(),
            name: "Първа процедура".into(),
            key: "P1".into(),
            price: serde_json::json!(10),
            price_currency: Some("BGN".into()),
        },
        ProcedureCatalogEntry {
            id: "procedure-b".into(),
            name: "Несъвместима процедура".into(),
            key: "P2".into(),
            price: serde_json::json!(20),
            price_currency: Some("BGN".into()),
        },
    ];
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, &format!("[{before}]")),
        ResponseScript::json(
            200,
            r#"{"procedure":{"id":"joined-a","procedureId":"procedure-a","price":"10","discount":"0"},"treatmentId":"treatment-row"}"#,
        ),
        ResponseScript::json(422, r#"{"error":"incompatible procedures"}"#),
        ResponseScript::json(200, &format!("[{after_first}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_treatment_draft(
        "patient-id".into(),
        &visit(true),
        vec![treatment("treatment-row", None, None, None, &[])],
        &[],
        &procedures,
        TreatmentChange {
            tooth: "26".into(),
            existing_treatment_id: Some("treatment-row".into()),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            procedure_ids: vec!["procedure-a".into(), "procedure-b".into()],
        },
    )
    .unwrap();

    let result = apply_confirmed_treatment_draft(&client, "token", "clinic-id", &draft)
        .await
        .unwrap();
    assert_eq!(result.outcome, VerificationOutcome::Uncertain);
    assert!(result.message.contains("Част от промените"));
    assert!(result.message.contains("Не повтаряйте автоматично"));
    let captured = (0..4)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        2
    );
    assert!(captured.last().unwrap().starts_with("GET "));
}
