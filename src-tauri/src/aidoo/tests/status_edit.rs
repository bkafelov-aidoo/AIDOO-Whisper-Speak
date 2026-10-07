use super::*;
use crate::aidoo::draft::same_snapshot;

#[tokio::test]
async fn status_edit_replacement_removes_only_the_old_status_and_preserves_the_status_row() {
    let mut status_catalog = catalog();
    status_catalog.push(StatusCatalogEntry {
        id: "sealant-id".into(),
        name: "Силанизиране".into(),
        code: "S".into(),
        order: 3,
        diagnosis_id: None,
        can_have_regions: true,
        regions: vec!["OCCLUSAL".into()],
        incompatible_statuses: vec![],
        nzis_tooth_diagnosis_id: None,
    });
    let baseline = ToothStatus {
        id: Some("record-32".into()),
        tooth: "32".into(),
        statuses: vec!["restoration-id".into(), "sealant-id".into()],
        is_milk_tooth: false,
        for_observation: false,
        regions: vec!["OCCLUSAL".into()],
        timestamp: Some("2026-09-16T00:00:00Z".into()),
        note: Some("Запази тази бележка".into()),
        generated_by_procedure: false,
    };
    let desired = ToothStatus {
        statuses: vec!["caries-id".into(), "sealant-id".into()],
        ..baseline.clone()
    };
    let baseline_response = serde_json::json!({ "teethStatus": [baseline.clone()] }).to_string();
    let desired_response = serde_json::json!({ "teethStatus": [desired.clone()] }).to_string();
    let readback_response = serde_json::json!({
        "visitTeethStatus": [{
            "currentToothStatus": desired,
            "previousToothStatus": baseline.clone(),
        }],
    })
    .to_string();
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, &baseline_response),
        ResponseScript::json(200, &desired_response),
        ResponseScript::json(200, &readback_response),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![baseline],
        &status_catalog,
        &[StatusChange {
            operation: StatusOperation::Replace,
            tooth: "32".into(),
            status_id: "caries-id".into(),
            regions: vec!["OCCLUSAL".into()],
            existing_status_id: Some("restoration-id".into()),
            is_milk_tooth: false,
            for_observation: false,
            note: None,
        }],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 1);
    assert_eq!(
        draft.writes[0],
        ToothStatusWrite {
            tooth: "32".into(),
            statuses: vec!["caries-id".into(), "sealant-id".into()],
            is_milk_tooth: false,
            for_observation: false,
            regions: vec!["OCCLUSAL".into()],
            note: Some("Запази тази бележка".into()),
        }
    );

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
    let put = captured
        .iter()
        .find(|request| request.starts_with("PUT "))
        .unwrap();
    let body = put.split_once("\r\n\r\n").unwrap().1;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap(),
        serde_json::json!({
            "teethStatus": [{
                "tooth": "32",
                "statuses": ["caries-id", "sealant-id"],
                "isMilkTooth": false,
                "forObservation": false,
                "regions": ["OCCLUSAL"],
                "note": "Запази тази бележка",
            }],
        })
    );
    assert!(!body.contains("restoration-id"));
}
#[test]
fn status_edit_merges_duplicate_baseline_rows_before_replacement() {
    let mut status_catalog = catalog();
    status_catalog.push(surface_status_entry("sealant-id", "Силанизиране", 3));
    let old = status_row(
        "32",
        &["restoration-id"],
        &["OCCLUSAL"],
        Some("Запази бележката"),
    );
    let keep = status_row(
        "32",
        &["sealant-id"],
        &["OCCLUSAL"],
        Some("Запази бележката"),
    );
    let change = StatusChange {
        operation: StatusOperation::Replace,
        tooth: "32".into(),
        status_id: "caries-id".into(),
        regions: vec!["OCCLUSAL".into()],
        existing_status_id: Some("restoration-id".into()),
        is_milk_tooth: false,
        for_observation: false,
        note: None,
    };

    for baseline in [
        vec![old.clone(), keep.clone()],
        vec![keep.clone(), old.clone()],
    ] {
        let draft = build_draft(
            "patient-id".into(),
            &visit(true),
            false,
            baseline,
            &status_catalog,
            std::slice::from_ref(&change),
        )
        .unwrap();
        assert_eq!(draft.writes.len(), 1);
        assert_eq!(
            draft.writes[0].statuses,
            ["caries-id".to_string(), "sealant-id".to_string()]
        );
        assert_eq!(draft.writes[0].note.as_deref(), Some("Запази бележката"));
    }
}

#[test]
fn status_edit_rejects_duplicate_baseline_rows_with_conflicting_metadata() {
    let first = status_row(
        "32",
        &["restoration-id"],
        &["OCCLUSAL"],
        Some("Първа бележка"),
    );
    let mut conflicts = Vec::new();
    let mut note_conflict = first.clone();
    note_conflict.statuses = vec!["caries-id".into()];
    note_conflict.note = Some("Различна бележка".into());
    conflicts.push(note_conflict);
    let mut milk_conflict = first.clone();
    milk_conflict.statuses = vec!["caries-id".into()];
    milk_conflict.is_milk_tooth = true;
    conflicts.push(milk_conflict);
    let mut observation_conflict = first.clone();
    observation_conflict.statuses = vec!["caries-id".into()];
    observation_conflict.for_observation = true;
    conflicts.push(observation_conflict);

    for conflicting in conflicts {
        let malformed = vec![first.clone(), conflicting];
        let error = build_draft(
            "patient-id".into(),
            &visit(true),
            false,
            malformed.clone(),
            &catalog(),
            &[StatusChange {
                operation: StatusOperation::Add,
                tooth: "32".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: None,
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            }],
        )
        .unwrap_err();
        assert!(error.contains("противоречиви"));
        assert!(!same_snapshot(&malformed, &malformed));
        assert!(!verifies(&[ToothStatusWrite::from(&first)], &malformed));
    }
}

#[tokio::test]
async fn status_edit_moves_a_unique_old_surface_to_new_regions_in_one_put() {
    let mut status_catalog = catalog();
    status_catalog.push(surface_status_entry("sealant-id", "Силанизиране", 3));
    let source = status_row(
        "32",
        &["restoration-id"],
        &["MESIAL"],
        Some("Изходна бележка"),
    );
    let destination = status_row("32", &["sealant-id"], &["OCCLUSAL"], Some("Целева бележка"));
    let desired_source = ToothStatus {
        statuses: vec![],
        ..source.clone()
    };
    let desired_destination = ToothStatus {
        statuses: vec!["caries-id".into(), "sealant-id".into()],
        ..destination.clone()
    };
    let baseline_response = serde_json::json!({
        "teethStatus": [source.clone(), destination.clone()],
    })
    .to_string();
    let desired_response = serde_json::json!({
        "teethStatus": [desired_source.clone(), desired_destination.clone()],
    })
    .to_string();
    let readback_response = serde_json::json!({
        "visitTeethStatus": [
            {
                "currentToothStatus": desired_source,
                "previousToothStatus": source.clone(),
            },
            {
                "currentToothStatus": desired_destination,
                "previousToothStatus": destination.clone(),
            },
        ],
    })
    .to_string();
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, &baseline_response),
        ResponseScript::json(200, &desired_response),
        ResponseScript::json(200, &readback_response),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![source, destination],
        &status_catalog,
        &[StatusChange {
            operation: StatusOperation::Replace,
            tooth: "32".into(),
            status_id: "caries-id".into(),
            regions: vec!["OCCLUSAL".into()],
            existing_status_id: Some("restoration-id".into()),
            is_milk_tooth: false,
            for_observation: false,
            note: None,
        }],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 2);
    assert!(draft.writes.iter().any(|write| {
        write.regions == ["MESIAL"]
            && write.statuses.is_empty()
            && write.note.as_deref() == Some("Изходна бележка")
    }));
    assert!(draft.writes.iter().any(|write| {
        write.regions == ["OCCLUSAL"]
            && write.statuses == ["caries-id", "sealant-id"]
            && write.note.as_deref() == Some("Целева бележка")
    }));

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
    assert!(!captured
        .iter()
        .any(|request| request.starts_with("DELETE ")));
    let put = captured
        .iter()
        .find(|request| request.starts_with("PUT "))
        .unwrap();
    let body = put.split_once("\r\n\r\n").unwrap().1;
    assert!(!body.contains("restoration-id"));
    assert!(body.contains("\"statuses\":[]"));
    assert!(body.contains("\"statuses\":[\"caries-id\",\"sealant-id\"]"));
}

#[test]
fn status_edit_rejects_an_ambiguous_old_status_on_multiple_surfaces() {
    let baseline = vec![
        status_row("32", &["restoration-id"], &["MESIAL"], None),
        status_row("32", &["restoration-id"], &["DISTAL"], None),
    ];
    let error = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        baseline,
        &catalog(),
        &[StatusChange {
            operation: StatusOperation::Replace,
            tooth: "32".into(),
            status_id: "caries-id".into(),
            regions: vec!["OCCLUSAL".into()],
            existing_status_id: Some("restoration-id".into()),
            is_milk_tooth: false,
            for_observation: false,
            note: None,
        }],
    )
    .unwrap_err();

    assert!(error.contains("повече от една повърхност"));
}

#[test]
fn status_edit_prefers_the_matching_regions_when_old_status_has_multiple_sources() {
    let baseline = vec![
        status_row("32", &["restoration-id"], &["MESIAL"], Some("M")),
        status_row("32", &["restoration-id"], &["DISTAL"], Some("D")),
    ];
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        baseline,
        &catalog(),
        &[StatusChange {
            operation: StatusOperation::Replace,
            tooth: "32".into(),
            status_id: "caries-id".into(),
            regions: vec!["MESIAL".into()],
            existing_status_id: Some("restoration-id".into()),
            is_milk_tooth: false,
            for_observation: false,
            note: None,
        }],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 1);
    assert_eq!(draft.writes[0].regions, ["MESIAL"]);
    assert_eq!(draft.writes[0].statuses, ["caries-id"]);
    assert_eq!(draft.writes[0].note.as_deref(), Some("M"));
}

#[test]
fn status_edit_grouped_swap_resolves_all_sources_before_mutation() {
    let baseline = vec![status_row(
        "32",
        &["caries-id", "restoration-id"],
        &["OCCLUSAL"],
        Some("Запази"),
    )];
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        baseline,
        &catalog(),
        &[
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "restoration-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("caries-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("restoration-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
        ],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 1);
    assert_eq!(
        draft.writes[0].statuses,
        ["caries-id".to_string(), "restoration-id".to_string()]
    );
    assert_eq!(draft.writes[0].note.as_deref(), Some("Запази"));
}

#[test]
fn status_edit_grouped_replacements_remove_both_old_statuses_before_adding_new() {
    let mut status_catalog = catalog();
    status_catalog.push(surface_status_entry("sealant-id", "Силанизиране", 3));
    status_catalog.push(surface_status_entry("crown-id", "Корона", 4));
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![status_row(
            "32",
            &["restoration-id", "sealant-id"],
            &["OCCLUSAL"],
            Some("Запази"),
        )],
        &status_catalog,
        &[
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("restoration-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "crown-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("sealant-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
        ],
    )
    .unwrap();

    assert_eq!(draft.writes.len(), 1);
    assert_eq!(
        draft.writes[0].statuses,
        ["caries-id".to_string(), "crown-id".to_string()]
    );
    assert_eq!(draft.writes[0].note.as_deref(), Some("Запази"));
}

#[test]
fn status_edit_rejects_consuming_the_same_old_status_twice() {
    let mut status_catalog = catalog();
    status_catalog.push(surface_status_entry("sealant-id", "Силанизиране", 3));
    let error = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![status_row(
            "32",
            &["restoration-id"],
            &["OCCLUSAL"],
            Some("Запази"),
        )],
        &status_catalog,
        &[
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "caries-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("restoration-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
            StatusChange {
                operation: StatusOperation::Replace,
                tooth: "32".into(),
                status_id: "sealant-id".into(),
                regions: vec!["OCCLUSAL".into()],
                existing_status_id: Some("restoration-id".into()),
                is_milk_tooth: false,
                for_observation: false,
                note: None,
            },
        ],
    )
    .unwrap_err();

    assert!(error.contains("повече от една замяна"));
}

#[test]
fn status_edit_whole_milk_tooth_preserves_metadata_and_other_statuses() {
    let status_catalog = vec![
        whole_tooth_status_entry("old-id", "Стар статус", 1),
        whole_tooth_status_entry("new-id", "Нов статус", 2),
        whole_tooth_status_entry("keep-id", "Запазен статус", 3),
    ];
    let baseline = ToothStatus {
        id: Some("record-15".into()),
        tooth: "15".into(),
        statuses: vec!["old-id".into(), "keep-id".into()],
        is_milk_tooth: true,
        for_observation: true,
        regions: vec![],
        timestamp: Some("2026-09-16T00:00:00Z".into()),
        note: Some("Запази бележката".into()),
        generated_by_procedure: false,
    };
    let draft = build_draft(
        "patient-id".into(),
        &visit(true),
        false,
        vec![baseline],
        &status_catalog,
        &[StatusChange {
            operation: StatusOperation::Replace,
            tooth: "15".into(),
            status_id: "new-id".into(),
            regions: vec![],
            existing_status_id: Some("old-id".into()),
            is_milk_tooth: true,
            for_observation: true,
            note: None,
        }],
    )
    .unwrap();

    assert_eq!(
        draft.writes,
        [ToothStatusWrite {
            tooth: "15".into(),
            statuses: vec!["keep-id".into(), "new-id".into()],
            is_milk_tooth: true,
            for_observation: true,
            regions: vec![],
            note: Some("Запази бележката".into()),
        }]
    );
}

fn surface_status_entry(id: &str, name: &str, order: i64) -> StatusCatalogEntry {
    StatusCatalogEntry {
        id: id.into(),
        name: name.into(),
        code: id.into(),
        order,
        diagnosis_id: None,
        can_have_regions: true,
        regions: vec!["MESIAL".into(), "DISTAL".into(), "OCCLUSAL".into()],
        incompatible_statuses: vec![],
        nzis_tooth_diagnosis_id: None,
    }
}

fn whole_tooth_status_entry(id: &str, name: &str, order: i64) -> StatusCatalogEntry {
    StatusCatalogEntry {
        id: id.into(),
        name: name.into(),
        code: id.into(),
        order,
        diagnosis_id: None,
        can_have_regions: false,
        regions: vec![],
        incompatible_statuses: vec![],
        nzis_tooth_diagnosis_id: None,
    }
}

fn status_row(tooth: &str, statuses: &[&str], regions: &[&str], note: Option<&str>) -> ToothStatus {
    ToothStatus {
        id: Some(format!("record-{tooth}-{}", regions.join("-"))),
        tooth: tooth.into(),
        statuses: statuses.iter().map(|value| (*value).into()).collect(),
        is_milk_tooth: false,
        for_observation: false,
        regions: regions.iter().map(|value| (*value).into()).collect(),
        timestamp: Some("2026-09-16T00:00:00Z".into()),
        note: note.map(str::to_string),
        generated_by_procedure: false,
    }
}
