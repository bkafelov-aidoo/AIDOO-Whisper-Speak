use super::{
    address_matches_target, ax_window_index, is_official_note_field, live_address_action,
    is_treatment_url, may_open_dedicated_window, navigation_action, opened_window_candidate,
    patient_id_from_url, recovery_candidates, valid_aidoo_url, ChromeWindow, NavigationAction,
};
use std::collections::HashSet;

#[test]
fn new_window_discovery_waits_instead_of_reusing_an_old_chrome_window() {
    let old_window = ChromeWindow { pid: 10, number: 1 };
    let before = HashSet::from([old_window.number]);

    assert_eq!(opened_window_candidate(&before, &[old_window]), None);
    assert_eq!(
        opened_window_candidate(&before, &[old_window, ChromeWindow { pid: 10, number: 2 }]),
        Some(ChromeWindow { pid: 10, number: 2 })
    );
}

#[test]
fn navigation_confirmation_requires_the_exact_patient_and_view() {
    let expected = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=status&selectedTeeth=&aidooControlSync=42",
    ]
    .concat();
    assert!(address_matches_target(
        &expected,
        "app.aidoo.bg/clinics/demo/medical-record?mode=status&tab=record&patientid=patient-a"
    ));
    assert!(!address_matches_target(
        &expected,
        &[
            "https:",
            "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-b&tab=record&mode=status"
        ]
        .concat()
    ));
    assert!(!address_matches_target(
        &expected,
        &["https:", "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=treatment"].concat()
    ));
}

#[test]
fn same_patient_refreshes_the_current_tab_and_a_new_patient_opens_a_tab() {
    assert_eq!(
        navigation_action(Some("patient-a"), Some("patient-a")),
        NavigationAction::RefreshCurrentTab
    );
    assert_eq!(
        navigation_action(Some("patient-a"), Some("patient-b")),
        NavigationAction::OpenNewTab
    );
    assert_eq!(
        navigation_action(None, Some("patient-a")),
        NavigationAction::OpenNewTab
    );
}

#[test]
fn stale_same_patient_window_recovers_through_a_live_aidoo_window() {
    let stale = ChromeWindow { pid: 10, number: 1 };
    let live_aidoo = ChromeWindow { pid: 10, number: 2 };
    let other_chrome = ChromeWindow { pid: 10, number: 3 };

    assert_eq!(
        recovery_candidates(
            Some(stale),
            Some("patient-a"),
            Some("patient-a"),
            &[stale, live_aidoo],
            &[stale, live_aidoo, other_chrome],
        ),
        vec![(live_aidoo, NavigationAction::RefreshCurrentTab)]
    );
}

#[test]
fn regular_chrome_is_used_only_when_no_aidoo_window_exists() {
    let other_chrome = ChromeWindow { pid: 10, number: 3 };

    assert_eq!(
        recovery_candidates(None, None, Some("patient-a"), &[], &[other_chrome]),
        vec![(other_chrome, NavigationAction::OpenNewTab)]
    );
}

#[test]
fn same_patient_recovery_never_opens_a_tab_when_chrome_omits_aidoo_titles() {
    let stale = ChromeWindow { pid: 10, number: 1 };
    let live = ChromeWindow { pid: 10, number: 2 };
    let mut opened_tabs = 0;
    for _ in 0..20 {
        let candidates = recovery_candidates(
            Some(stale),
            Some("patient-a"),
            Some("patient-a"),
            &[],
            &[live],
        );
        assert_eq!(candidates.len(), 1);
        opened_tabs += usize::from(candidates[0].1 == NavigationAction::OpenNewTab);
    }
    assert_eq!(
        opened_tabs, 0,
        "same-patient commands must reuse the existing tab"
    );
}

#[test]
fn consecutive_schedule_commands_reuse_the_current_tab() {
    assert_eq!(
        navigation_action(None, None),
        NavigationAction::RefreshCurrentTab
    );
}

#[test]
fn an_existing_patient_tab_is_reused_after_context_loss_or_an_app_restart() {
    let status = ["https:", "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=status&aidooControlSync=42"].concat();
    let treatment =
        "app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=treatment";
    for _ in 0..20 {
        assert_eq!(
            live_address_action(&status, treatment),
            NavigationAction::RefreshCurrentTab
        );
    }
    let other_patient = treatment.replace("patient-a", "patient-b");
    assert_eq!(
        live_address_action(&status, &other_patient),
        NavigationAction::OpenNewTab
    );
    assert_eq!(
        live_address_action(&status, "google.com/"),
        NavigationAction::OpenNewTab
    );
}

#[test]
fn uuid_letter_case_does_not_create_another_patient_tab() {
    let upper = ["https:", "//app.aidoo.bg/clinics/demo/medical-record?patientid=117332E5-B9AE-416D-B612-0E2634C95178&tab=record&mode=status"].concat();
    let lower = upper.to_ascii_lowercase();
    assert_eq!(
        live_address_action(&lower, &upper),
        NavigationAction::RefreshCurrentTab
    );
}

#[test]
fn patient_context_is_read_from_the_medical_record_url() {
    let url = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=status",
    ]
    .concat();
    assert_eq!(patient_id_from_url(&url).as_deref(), Some("patient-a"));
    assert_eq!(
        patient_id_from_url(&["https:", "//app.aidoo.bg/clinics/demo/schedule"].concat()),
        None
    );
}

#[test]
fn browser_navigation_is_limited_to_aidoo_clinics() {
    assert!(valid_aidoo_url(
        &["https:", "//app.aidoo.bg/clinics/demo/medical-record"].concat()
    ));
    assert!(valid_aidoo_url(
        &[
            "https:",
            "//aidoo-web.on.dev-craft.tech/clinics/demo/medical-record"
        ]
        .concat()
    ));
    assert!(!valid_aidoo_url(
        &["https:", "//example.com/clinics/demo"].concat()
    ));
    assert!(!valid_aidoo_url(
        &["https:", "//app.aidoo.bg.evil.test/clinics/demo"].concat()
    ));
}

#[test]
fn signature_gate_handling_is_limited_to_the_exact_treatment_mode() {
    let base = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record",
    ]
    .concat();
    assert!(is_treatment_url(&format!("{base}&mode=treatment")));
    assert!(is_treatment_url(&format!("{base}&mode=TREATMENT")));
    assert!(!is_treatment_url(&format!("{base}&mode=status")));
    assert!(!is_treatment_url(&format!("{base}&mode=treatment-preview")));
    assert!(!is_treatment_url(&base));
}

#[test]
fn official_note_preview_targets_only_the_named_aidoo_text_area() {
    assert!(is_official_note_field(
        "AXTextArea",
        &[Some("Забележка"), None, None]
    ));
    assert!(is_official_note_field(
        "AXTextField",
        &[None, Some("Официална забележка"), None]
    ));
    assert!(!is_official_note_field(
        "AXTextArea",
        &[Some("Диагноза"), None, None]
    ));
    assert!(!is_official_note_field(
        "AXTextArea",
        &[Some("Забележка за посещението"), None, None]
    ));
    assert!(!is_official_note_field(
        "AXButton",
        &[Some("Забележка"), None, None]
    ));
}

#[test]
fn chrome_window_without_ax_number_reuses_the_existing_aidoo_window() {
    let candidates = [
        (None, "Мила Горбунова | AIDOO".into()),
        (None, "Downloads".into()),
    ];

    assert_eq!(ax_window_index(42, &candidates), Some(0));
}

#[test]
fn twenty_refreshes_cannot_create_twenty_windows() {
    let opened_windows = (0..20).filter(|_| may_open_dedicated_window(true)).count();

    assert_eq!(opened_windows, 0);
    assert!(may_open_dedicated_window(false));
}
