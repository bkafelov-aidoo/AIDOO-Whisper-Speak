use super::*;
use crate::aidoo::{client::AidooClient, runtime::AidooRuntime};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use zeroize::Zeroizing;

fn patient(id: &str) -> types::PatientSearchResult {
    types::PatientSearchResult {
        patient: types::PatientSummary {
            id: id.into(),
            first_name: "Тест".into(),
            middle_name: None,
            last_name: "Пациент".into(),
            mobile_phone: None,
            birthdate: None,
            identifier: None,
            identifier_type: None,
        },
    }
}

fn session() -> crate::aidoo::runtime::AidooSessionSnapshot {
    crate::aidoo::runtime::AidooSessionSnapshot {
        token: Zeroizing::new("session-token".into()),
        clinic_id: "clinic-id".into(),
        doctor_id: "doctor-id".into(),
        current_currency: None,
        works_with_nzok: false,
    }
}

fn active_visit_server() -> (AidooClient, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
        let mut bytes = [0_u8; 4096];
        let read = stream.read(&mut bytes).unwrap_or(0);
        sender
            .send(String::from_utf8_lossy(&bytes[..read]).into_owned())
            .ok();
        let body =
            r#"{"id":"visit-id","createdStatusUpdate":false,"isFinished":false,"cancelled":false}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).ok();
    });
    (
        AidooClient::for_test(format!("http://{address}"), Duration::from_secs(1)),
        receiver,
    )
}

#[tokio::test]
async fn ambiguous_patient_is_rejected_before_the_active_visit_get() {
    let runtime = AidooRuntime::new();
    runtime
        .remember_patient_search(&[patient("one"), patient("two")])
        .unwrap();
    let (client, requests) = active_visit_server();

    let error = active_treatment_visit(&runtime, &client, &session(), "one")
        .await
        .unwrap_err();

    assert!(error.contains("не е избран"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn wrong_patient_is_rejected_before_the_active_visit_get() {
    let runtime = AidooRuntime::new();
    runtime.remember_patient_search(&[patient("one")]).unwrap();
    let (client, requests) = active_visit_server();

    let error = active_treatment_visit(&runtime, &client, &session(), "two")
        .await
        .unwrap_err();

    assert!(error.contains("не е избран"));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn exact_selected_patient_reaches_only_the_expected_active_visit_get() {
    let runtime = AidooRuntime::new();
    runtime.remember_patient_search(&[patient("one")]).unwrap();
    let (client, requests) = active_visit_server();

    let visit = active_treatment_visit(&runtime, &client, &session(), "one")
        .await
        .unwrap();

    assert_eq!(visit.id, "visit-id");
    let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(request.starts_with("GET /clinics/clinic-id/patients/one/visits/active HTTP/1.1"));
    assert!(requests.try_recv().is_err());
}

#[test]
fn selected_treatment_tooth_keeps_permanent_and_general_and_canonicalizes_milk() {
    let runtime = AidooRuntime::new();
    runtime.remember_patient_search(&[patient("one")]).unwrap();

    assert_eq!(
        selected_treatment_tooth(&runtime, "one", "18").unwrap(),
        "18"
    );
    assert_eq!(
        selected_treatment_tooth(&runtime, "one", "55").unwrap(),
        "15"
    );
    assert_eq!(selected_treatment_tooth(&runtime, "one", "*").unwrap(), "*");
}
