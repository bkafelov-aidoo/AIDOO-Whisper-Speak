use super::*;

fn strict_procedure_row_server(
    expected_create: serde_json::Value,
    created_row: String,
    verified_row: String,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for step in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_contract_request(&mut stream);
            sender.send(request.clone()).ok();
            let (status, body) = match step {
                0 => (200, "[]".to_string()),
                1 => {
                    let actual = request
                        .split("\r\n\r\n")
                        .nth(1)
                        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
                    if actual.as_ref() == Some(&expected_create) {
                        (200, created_row.clone())
                    } else {
                        (422, r#"{"error":"procedure placeholder required"}"#.into())
                    }
                }
                2 => (
                    200,
                    r#"{"procedure":{"id":"joined-procedure","procedureId":"procedure-id","price":"50","discount":"0"}}"#.into(),
                ),
                _ => (200, format!("[{verified_row}]")),
            };
            write_contract_response(&mut stream, status, &body);
        }
    });
    (format!("http://{address}"), receiver)
}

fn read_contract_request(stream: &mut TcpStream) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected = None;
    loop {
        let read = stream.read(&mut buffer).unwrap_or(0);
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if expected.is_none() {
            if let Some(header_end) = find_bytes(&request, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                expected = Some(header_end + 4 + content_length);
            }
        }
        if expected.is_some_and(|length| request.len() >= length) {
            break;
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn write_contract_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).ok();
}

#[tokio::test]
async fn treatment_visit_creation_posts_only_the_authenticated_doctor_and_reads_back_active_visit()
{
    let created =
        r#"{"id":"new-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(404, r#"{"message":"not found"}"#),
        ResponseScript::json(200, created),
        ResponseScript::json(200, created),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.visit.as_ref().unwrap().id, "new-visit");
    assert!(result.created);
    assert_eq!(result.verification.outcome, VerificationOutcome::Verified);
    let initial_request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    let create_request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    let readback_request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(initial_request
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(
        create_request.starts_with("POST /clinics/clinic-id/patients/patient-id/visits HTTP/1.1")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            create_request.split("\r\n\r\n").nth(1).unwrap_or_default()
        )
        .unwrap(),
        serde_json::json!({"doctorId":"doctor-id"})
    );
    assert!(readback_request
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn treatment_visit_creation_never_retries_when_the_active_visit_readback_mismatches() {
    let created =
        r#"{"id":"new-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let other =
        r#"{"id":"other-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(404, r#"{"message":"not found"}"#),
        ResponseScript::json(200, created),
        ResponseScript::json(200, other),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.verification.outcome, VerificationOutcome::Uncertain);
    assert!(result.visit.is_none());
    assert!(!result.created);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert!(captured[0]
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(captured[1].starts_with("POST /clinics/clinic-id/patients/patient-id/visits HTTP/1.1"));
    assert!(captured[2]
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn treatment_visit_reuses_a_proven_active_visit_without_a_write() {
    let active =
        r#"{"id":"active-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![ResponseScript::json(200, active)]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.visit.as_ref().unwrap().id, "active-visit");
    assert!(!result.created);
    assert_eq!(result.verification.outcome, VerificationOutcome::Verified);
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        request.starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1")
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn treatment_visit_rejects_a_finished_active_visit_without_a_write() {
    let finished = r#"{"id":"finished-visit","createdStatusUpdate":false,"isFinished":true,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![ResponseScript::json(200, finished)]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.verification.outcome, VerificationOutcome::Rejected);
    assert!(result.visit.is_none());
    assert!(!result.created);
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        request.starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1")
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn treatment_visit_does_not_retry_an_ambiguous_creation() {
    let created =
        r#"{"id":"new-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(404, r#"{"message":"not found"}"#),
        ResponseScript::delayed_json(200, created, Duration::from_millis(400)),
        ResponseScript::json(200, created),
    ]);
    let client = AidooClient::for_test(base, Duration::from_millis(100));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.verification.outcome, VerificationOutcome::Uncertain);
    assert!(result.visit.is_none());
    assert!(!result.created);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert!(captured[0]
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(captured[1].starts_with("POST /clinics/clinic-id/patients/patient-id/visits HTTP/1.1"));
    assert!(captured[2]
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn treatment_visit_does_not_retry_a_successful_unparseable_creation() {
    let created =
        r#"{"id":"new-visit","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(404, r#"{"message":"not found"}"#),
        ResponseScript::json(200, "{}"),
        ResponseScript::json(200, created),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));

    let result = create_treatment_visit(
        &client,
        "session-token",
        "clinic-id",
        "patient-id",
        "doctor-id",
    )
    .await
    .unwrap();

    assert_eq!(result.verification.outcome, VerificationOutcome::Uncertain);
    assert!(result.visit.is_none());
    assert!(!result.created);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
    assert!(captured[2]
        .starts_with("GET /clinics/clinic-id/patients/patient-id/visits/active HTTP/1.1"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_fresh_treatment_row_matches_the_production_placeholder_contract() {
    let created_row = serde_json::json!({
        "id": "treatment-row",
        "tooth": "*",
        "diagnosisId": null,
        "treatmentId": null,
        "note": null,
        "status": null,
        "isMilkTooth": false,
        "procedures": []
    })
    .to_string();
    let verified_row = serde_json::json!({
        "id": "treatment-row",
        "tooth": "*",
        "diagnosisId": null,
        "treatmentId": null,
        "note": null,
        "status": null,
        "isMilkTooth": false,
        "procedures": [{
            "id": "joined-procedure",
            "procedureId": "procedure-id",
            "price": "50",
            "discount": "0"
        }]
    })
    .to_string();
    let expected_create = serde_json::json!({
        "tooth": "*",
        "diagnosisId": null,
        "treatmentId": null,
        "note": null,
        "procedures": [{
            "procedureId": "procedure-id",
            "price": "0",
            "discount": "0"
        }],
        "nzis": false,
        "nhif": false,
        "isMilkTooth": false
    });
    let (base, requests) =
        strict_procedure_row_server(expected_create.clone(), created_row, verified_row);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "*".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: vec![ProcedureWrite {
            id: None,
            procedure_id: "procedure-id".into(),
            price: "50".into(),
            discount: "0".into(),
        }],
        spoken_summary: "Записах процедурата.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Verified);
    let captured = (0..4)
        .filter_map(|_| requests.recv_timeout(Duration::from_millis(100)).ok())
        .collect::<Vec<_>>();
    assert!(captured[0].starts_with(
        "GET /clinics/clinic-id/patients/patient-id/visits/visit-id/treatments HTTP/1.1"
    ));
    let create_request = &captured[1];
    assert!(create_request.starts_with(
        "POST /clinics/clinic-id/patients/patient-id/visits/visit-id/treatments HTTP/1.1"
    ));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            create_request.split("\r\n\r\n").nth(1).unwrap_or_default()
        )
        .unwrap(),
        expected_create
    );
    assert_eq!(captured.len(), 4);
    assert!(captured[2].starts_with(
        "POST /clinics/clinic-id/patients/patient-id/treatments/treatment-row/procedures HTTP/1.1"
    ));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            captured[2].split("\r\n\r\n").nth(1).unwrap_or_default()
        )
        .unwrap(),
        serde_json::json!({
            "procedureId": "procedure-id",
            "price": "50",
            "discount": "0"
        })
    );
    assert!(captured[3].starts_with(
        "GET /clinics/clinic-id/patients/patient-id/visits/visit-id/treatments HTTP/1.1"
    ));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn an_unfinalized_placeholder_never_verifies_a_rejected_procedure_post() {
    let placeholder = serde_json::json!({
        "id": "treatment-row",
        "tooth": "*",
        "diagnosisId": null,
        "treatmentId": null,
        "note": null,
        "status": null,
        "isMilkTooth": false,
        "procedures": [{
            "id": null,
            "procedureId": "procedure-id",
            "price": "0",
            "discount": "0"
        }]
    })
    .to_string();
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, "[]"),
        ResponseScript::json(200, &placeholder),
        ResponseScript::json(422, r#"{"error":"procedure rejected"}"#),
        ResponseScript::json(200, &format!("[{placeholder}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "*".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: vec![ProcedureWrite {
            id: None,
            procedure_id: "procedure-id".into(),
            price: "50".into(),
            discount: "0".into(),
        }],
        spoken_summary: "Записах процедурата.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Uncertain);
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
    assert!(requests.try_recv().is_err());
}

#[test]
fn a_finalized_zero_price_procedure_requires_a_joined_id_and_normalizes_amounts() {
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "*".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: vec![ProcedureWrite {
            id: None,
            procedure_id: "free-procedure".into(),
            price: "0.00".into(),
            discount: "0".into(),
        }],
        spoken_summary: "Записах процедурата.".into(),
    };
    let row = |id: Option<&str>| VisitTreatment {
        id: "treatment-row".into(),
        tooth: "*".into(),
        diagnosis_id: None,
        treatment_id: None,
        note: None,
        status: None,
        is_milk_tooth: false,
        procedures: vec![TreatmentProcedure {
            id: id.map(str::to_string),
            procedure_id: "free-procedure".into(),
            price: serde_json::json!(0),
            discount: serde_json::json!("0.00"),
        }],
    };

    assert!(!verifies_treatment_with_identity(
        &draft,
        &[row(None)],
        Some("treatment-row"),
        None,
    ));
    assert!(verifies_treatment_with_identity(
        &draft,
        &[row(Some("joined-free"))],
        Some("treatment-row"),
        None,
    ));
}

#[tokio::test]
async fn a_successful_procedure_uses_the_exact_returned_treatment_id_for_readback() {
    let created_row = treatment_json("treatment-row", None, None, &[]);
    let verified_row = serde_json::json!({
        "id": "treatment-row",
        "tooth": "*",
        "diagnosisId": null,
        "treatmentId": "clinical-treatment-id",
        "note": null,
        "status": null,
        "isMilkTooth": false,
        "procedures": [{
            "id": "joined-procedure",
            "procedureId": "procedure-id",
            "price": "50",
            "discount": "0"
        }]
    })
    .to_string();
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, "[]"),
        ResponseScript::json(200, &created_row),
        ResponseScript::json(
            200,
            r#"{"procedure":{"id":"joined-procedure","procedureId":"procedure-id","price":"50","discount":"0"},"treatmentId":"clinical-treatment-id"}"#,
        ),
        ResponseScript::json(200, &format!("[{verified_row}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "*".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: vec![ProcedureWrite {
            id: None,
            procedure_id: "procedure-id".into(),
            price: "50".into(),
            discount: "0".into(),
        }],
        spoken_summary: "Записах процедурата.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Verified);
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
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_fresh_readback_must_match_the_exact_created_row_id() {
    let created = treatment_json("created-row", None, None, &[]);
    let unrelated = treatment_json("other-new-row", None, None, &[]);
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, "[]"),
        ResponseScript::json(200, &created),
        ResponseScript::json(200, &format!("[{unrelated}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "26".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: Vec::new(),
        spoken_summary: "Записах лечението.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Uncertain);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_fresh_readback_must_preserve_the_milk_tooth_flag() {
    let created = serde_json::json!({
        "id": "created-row",
        "tooth": "34",
        "diagnosisId": null,
        "treatmentId": null,
        "note": null,
        "status": null,
        "isMilkTooth": true,
        "procedures": []
    })
    .to_string();
    let wrong_readback = created.replace("\"isMilkTooth\":true", "\"isMilkTooth\":false");
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, "[]"),
        ResponseScript::json(200, &created),
        ResponseScript::json(200, &format!("[{wrong_readback}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "34".into(),
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: true,
        },
        procedures: Vec::new(),
        spoken_summary: "Записах лечението.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Uncertain);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(captured.len(), 3);
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_successful_unparseable_fresh_row_stays_uncertain_without_a_retry() {
    let verified_row = treatment_json("treatment-row", Some("diagnosis-id"), None, &[]);
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, "[]"),
        ResponseScript::json(200, "{}"),
        ResponseScript::json(200, &format!("[{verified_row}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: None,
        baseline: Vec::new(),
        treatment: TreatmentWrite {
            tooth: "26".into(),
            diagnosis_id: Some("diagnosis-id".into()),
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: Vec::new(),
        spoken_summary: "Записах диагнозата.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(result.outcome, VerificationOutcome::Uncertain);
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_successful_unparseable_procedure_is_verified_without_a_retry() {
    let before = treatment("treatment-row", Some("diagnosis-id"), None, None, &[]);
    let after = treatment_json(
        "treatment-row",
        Some("diagnosis-id"),
        None,
        &["procedure-id"],
    );
    let (base, requests) = scripted_server(vec![
        ResponseScript::json(200, &serde_json::to_string(&vec![before.clone()]).unwrap()),
        ResponseScript::json(200, "{}"),
        ResponseScript::json(200, &format!("[{after}]")),
    ]);
    let client = AidooClient::for_test(base, Duration::from_secs(2));
    let draft = TreatmentDraft {
        id: "draft-id".into(),
        patient_id: "patient-id".into(),
        visit_id: "visit-id".into(),
        existing_treatment_id: Some("treatment-row".into()),
        baseline: vec![before],
        treatment: TreatmentWrite {
            tooth: "26".into(),
            diagnosis_id: Some("diagnosis-id".into()),
            treatment_id: None,
            note: None,
            status: None,
            is_milk_tooth: false,
        },
        procedures: vec![ProcedureWrite {
            id: None,
            procedure_id: "procedure-id".into(),
            price: "42.5".into(),
            discount: "0".into(),
        }],
        spoken_summary: "Записах процедурата.".into(),
    };

    let result = apply_confirmed_treatment_draft(&client, "session-token", "clinic-id", &draft)
        .await
        .unwrap();

    assert_eq!(
        result.outcome,
        VerificationOutcome::VerifiedAfterAmbiguousWrite
    );
    let captured = (0..3)
        .map(|_| requests.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        captured
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
    assert!(requests.try_recv().is_err());
}
