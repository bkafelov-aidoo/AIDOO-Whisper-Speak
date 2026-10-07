use super::{
    classify_treatment_signature_gate, is_treatment_url, treatment_gate_decision,
    treatment_page_without_gate_is_ready, walk_treatment_nodes, TreatmentGateDecision,
    TreatmentGateMatch, TreatmentGateObservation, TreatmentGateState, TreatmentNodeSnapshot,
    TreatmentFilterChild, TreatmentFilterMatch, TreatmentFilterScan, TreatmentRouteExpectation,
    TREATMENT_SIGNATURE_GATE_LABEL,
};
use std::collections::HashSet;

#[derive(Clone)]
struct FixtureNode {
    gate: TreatmentGateMatch,
    procedures: bool,
    remarks: bool,
    children: Vec<usize>,
}

fn scan_fixture(nodes: &[FixtureNode]) -> super::TreatmentPageScan<usize> {
    walk_treatment_nodes(0, 2_400, |index| {
        let node = &nodes[*index];
        TreatmentNodeSnapshot {
            gate: node.gate,
            procedures: node.procedures,
            remarks: node.remarks,
            children: node.children.clone(),
        }
    })
}

#[test]
fn exact_enabled_button_is_the_only_pressable_signature_gate() {
    for labels in [
        [Some(TREATMENT_SIGNATURE_GATE_LABEL), None, None],
        [None, Some(TREATMENT_SIGNATURE_GATE_LABEL), None],
        [None, None, Some(TREATMENT_SIGNATURE_GATE_LABEL)],
    ] {
        assert_eq!(
            classify_treatment_signature_gate("AXButton", Some(true), &labels),
            TreatmentGateMatch::Enabled
        );
    }
    for (role, label) in [
        ("AXButton", "Продължи"),
        ("AXButton", "Продължи без подписване"),
        ("AXButton", "Подпиши"),
        ("AXStaticText", TREATMENT_SIGNATURE_GATE_LABEL),
    ] {
        assert_eq!(
            classify_treatment_signature_gate(role, Some(true), &[Some(label), None, None]),
            TreatmentGateMatch::Other
        );
    }
}

#[test]
fn exact_but_disabled_button_is_never_pressable() {
    for enabled in [Some(false), None] {
        assert_eq!(
            classify_treatment_signature_gate(
                "AXButton",
                enabled,
                &[Some(TREATMENT_SIGNATURE_GATE_LABEL), None, None],
            ),
            TreatmentGateMatch::Disabled
        );
    }
}

#[test]
fn gate_state_machine_waits_for_ready_ui_and_presses_only_once() {
    use TreatmentGateDecision::{Complete, FailNotReady, FailStillPresent, Press, Wait};
    use TreatmentGateObservation::{Enabled, NotReady, Ready};
    use TreatmentGateState::{NotPressed, Pressed};
    for (state, observation, deadline, confirmed, expected) in [
        (NotPressed, NotReady, false, false, Wait),
        (NotPressed, NotReady, true, false, FailNotReady),
        (NotPressed, Ready, false, false, Wait),
        (NotPressed, Ready, false, true, Complete),
        (NotPressed, Ready, true, true, Complete),
        (NotPressed, Ready, true, false, FailNotReady),
        (NotPressed, Enabled, false, false, Press),
        (Pressed, Enabled, false, false, Wait),
        (Pressed, Ready, false, false, Wait),
        (Pressed, Ready, false, true, Complete),
        (Pressed, Enabled, true, false, FailStillPresent),
    ] {
        assert_eq!(
            treatment_gate_decision(state, observation, deadline, confirmed),
            expected
        );
    }
}

#[test]
fn a_stable_ready_treatment_page_completes_before_the_deadline() {
    assert_eq!(
        treatment_gate_decision(
            TreatmentGateState::NotPressed,
            TreatmentGateObservation::Ready,
            false,
            true,
        ),
        TreatmentGateDecision::Complete,
        "a verified patient route, selection and stable visible headers must not wait for timeout"
    );
}

#[test]
fn a_transient_ready_page_cannot_complete_at_the_deadline() {
    assert_eq!(
        treatment_gate_decision(
            TreatmentGateState::NotPressed,
            TreatmentGateObservation::Ready,
            true,
            false,
        ),
        TreatmentGateDecision::FailNotReady,
    );
}

#[test]
fn treatment_readiness_diagnostics_only_contain_predicates_and_counts() {
    let diagnostic = super::TreatmentReadinessDiagnostics {
        web_area: true,
        page_nodes: 908,
        procedures: true,
        remarks: true,
        expected_selection: 1,
        filter_groups: 0,
        ..Default::default()
    }
    .summary();
    assert!(diagnostic.contains("page_nodes=908"));
    assert!(diagnostic.contains("selection_visible=false"));
    assert!(!diagnostic.contains("patient"));
    assert!(!diagnostic.contains("https"));
    assert!(!diagnostic.contains("token"));
    assert!(!diagnostic.contains("note"));
}

#[test]
fn route_guard_requires_exact_patient_and_treatment_mode() {
    let expected = TreatmentRouteExpectation::Patient("patient-a");
    let treatment = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=treatment",
    ]
    .concat();
    assert!(expected.matches(&treatment));
    assert!(!expected.matches(&treatment.replace("patient-a", "patient-b")));
    assert!(!expected.matches(&treatment.replace("mode=treatment", "mode=status")));
    assert!(!expected.matches(&treatment.replace("app.aidoo.bg", "example.com")));
    assert!(is_treatment_url(&treatment));
}

#[test]
fn exact_route_guard_requires_the_same_selected_tooth_set_after_aidoo_reorders_it() {
    let base = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=treatment&selectedTeeth=18,17,16,15,14,13,12,11&triggerNzokChecksProp=true",
    ]
    .concat();
    let expected = TreatmentRouteExpectation::ExactUrl(&base);
    assert!(expected.matches(&base.replace(
        "18,17,16,15,14,13,12,11",
        "11,12,13,14,15,16,17,18"
    )));
    assert!(!expected.matches(&base.replace(
        "18,17,16,15,14,13,12,11",
        "11,12,13,14,15,16,17"
    )));
    assert!(!expected.matches(&base.replace("selectedTeeth=", "selectedTeeth=26,")));
}

#[test]
fn empty_selection_tolerates_aidoo_removing_the_empty_query_parameter() {
    let expected_url = [
        "https:",
        "//app.aidoo.bg/clinics/demo/medical-record?patientid=patient-a&tab=record&mode=treatment&selectedTeeth=&triggerNzokChecksProp=true",
    ]
    .concat();
    let observed_url = expected_url.replace("&selectedTeeth=", "");
    assert!(TreatmentRouteExpectation::ExactUrl(&expected_url).matches(&observed_url));
}

#[test]
fn exact_filter_group_proves_the_visible_tooth_selection() {
    use TreatmentFilterChild::{RemoveButton, Text};
    let parsed = super::classify_treatment_filter(&[
        Text("Филтри: 26".into()),
        RemoveButton,
    ]);
    assert_eq!(
        parsed,
        TreatmentFilterMatch::Teeth(HashSet::from(["26".to_string()]))
    );
    let observed = TreatmentFilterScan {
        teeth: vec![HashSet::from(["26".to_string()])],
        invalid: false,
        truncated: false,
    };
    assert!(super::treatment_filter_is_visible(
        &HashSet::from(["26".to_string()]),
        &observed
    ));
}

#[test]
fn general_filter_accepts_all_teeth_as_a_set_but_rejects_partial_extra_or_stale_groups() {
    use TreatmentFilterChild::{RemoveButton, Text};
    let mut children = Vec::new();
    let mut expected = HashSet::new();
    for (index, tooth) in [
        "11", "12", "13", "14", "15", "16", "17", "18", "21", "22", "23", "24", "25",
        "26", "27", "28", "31", "32", "33", "34", "35", "36", "37", "38", "41", "42",
        "43", "44", "45", "46", "47", "48",
    ]
    .into_iter()
    .enumerate()
    {
        children.push(Text(if index == 0 {
            format!("Филтри: {tooth}")
        } else {
            tooth.to_string()
        }));
        children.push(RemoveButton);
        expected.insert(tooth.to_string());
    }
    let TreatmentFilterMatch::Teeth(observed) = super::classify_treatment_filter(&children) else {
        panic!("the deployed 32-tooth filter shape should be recognized");
    };
    assert_eq!(observed, expected);
    assert!(super::treatment_filter_is_visible(
        &expected,
        &TreatmentFilterScan {
            teeth: vec![observed.clone()],
            invalid: false,
            truncated: false,
        }
    ));
    assert!(!super::treatment_filter_is_visible(
        &expected,
        &TreatmentFilterScan {
            teeth: vec![observed.clone(), HashSet::from(["26".to_string()])],
            invalid: false,
            truncated: false,
        }
    ));
    let mut partial = observed;
    partial.remove("48");
    assert!(!super::treatment_filter_is_visible(
        &expected,
        &TreatmentFilterScan {
            teeth: vec![partial],
            invalid: false,
            truncated: false,
        }
    ));
}

#[test]
fn malformed_filter_group_and_truncated_scan_fail_closed() {
    use TreatmentFilterChild::{Other, RemoveButton, Text};
    assert_eq!(
        super::classify_treatment_filter(&[Text("Филтри: 26".into()), Other]),
        TreatmentFilterMatch::Invalid
    );
    assert_eq!(
        super::classify_treatment_filter(&[Text("Филтри: 26".into()), RemoveButton, Other]),
        TreatmentFilterMatch::Invalid
    );
    assert!(!super::treatment_filter_is_visible(
        &HashSet::from(["26".to_string()]),
        &TreatmentFilterScan {
            teeth: vec![HashSet::from(["26".to_string()])],
            invalid: false,
            truncated: true,
        }
    ));
}

#[test]
fn ready_headers_are_not_proof_when_the_ax_traversal_was_truncated() {
    assert!(treatment_page_without_gate_is_ready(true, true, false));
    assert!(!treatment_page_without_gate_is_ready(true, true, true));
    assert!(!treatment_page_without_gate_is_ready(true, false, false));
    assert!(!treatment_page_without_gate_is_ready(false, true, false));
}

#[test]
fn bounded_deep_treatment_table_is_ready_without_a_signature_gate() {
    let mut nodes = vec![FixtureNode {
        gate: TreatmentGateMatch::Other,
        procedures: true,
        remarks: true,
        children: vec![1],
    }];
    for index in 1..=24 {
        nodes.push(FixtureNode {
            gate: TreatmentGateMatch::Other,
            procedures: false,
            remarks: false,
            children: (index < 24).then_some(index + 1).into_iter().collect(),
        });
    }

    let scan = scan_fixture(&nodes);
    assert!(treatment_page_without_gate_is_ready(
        scan.procedures,
        scan.remarks,
        scan.truncated
    ));
}

#[test]
fn separate_late_signature_dialog_wins_over_ready_table_headers() {
    let mut nodes = vec![FixtureNode {
        gate: TreatmentGateMatch::Other,
        procedures: true,
        remarks: true,
        children: vec![1],
    }];
    for index in 1..=24 {
        nodes.push(FixtureNode {
            gate: TreatmentGateMatch::Other,
            procedures: false,
            remarks: false,
            children: (index < 24).then_some(index + 1).into_iter().collect(),
        });
    }
    let dialog_root = nodes.len();
    nodes[0].children.push(dialog_root);
    for _ in 0..18 {
        let next = nodes.len() + 1;
        nodes.push(FixtureNode {
            gate: TreatmentGateMatch::Other,
            procedures: false,
            remarks: false,
            children: vec![next],
        });
    }
    let gate = nodes.len();
    nodes.push(FixtureNode {
        gate: TreatmentGateMatch::Enabled,
        procedures: false,
        remarks: false,
        children: Vec::new(),
    });

    let scan = scan_fixture(&nodes);
    assert_eq!(scan.enabled_gate, Some(gate));
    assert!(!scan.truncated);
}

#[test]
fn node_budget_truncation_cannot_be_mistaken_for_a_ready_page() {
    let nodes = [
        FixtureNode {
            gate: TreatmentGateMatch::Other,
            procedures: true,
            remarks: true,
            children: vec![1],
        },
        FixtureNode {
            gate: TreatmentGateMatch::Other,
            procedures: false,
            remarks: false,
            children: vec![2],
        },
        FixtureNode {
            gate: TreatmentGateMatch::Enabled,
            procedures: false,
            remarks: false,
            children: Vec::new(),
        },
    ];
    let scan = walk_treatment_nodes(0, 2, |index| {
        let node = &nodes[*index];
        TreatmentNodeSnapshot {
            gate: node.gate,
            procedures: node.procedures,
            remarks: node.remarks,
            children: node.children.clone(),
        }
    });

    assert!(scan.truncated);
    assert_eq!(scan.enabled_gate, None);
    assert!(!treatment_page_without_gate_is_ready(
        scan.procedures,
        scan.remarks,
        scan.truncated
    ));
}

#[test]
fn focused_note_container_is_refused_even_when_it_is_named_like_a_note() {
    assert!(!super::is_official_note_field(
        "AXGroup",
        &[Some("Забележка"), None, None]
    ));
    assert!(super::is_official_note_field(
        "AXTextArea",
        &[Some("Забележка"), None, None]
    ));
}

#[test]
fn disabled_gate_waits_only_until_the_bound_is_reached() {
    assert_eq!(
        treatment_gate_decision(
            TreatmentGateState::NotPressed,
            TreatmentGateObservation::Disabled,
            false,
            false
        ),
        TreatmentGateDecision::Wait
    );
    assert_eq!(
        treatment_gate_decision(
            TreatmentGateState::NotPressed,
            TreatmentGateObservation::Disabled,
            true,
            false
        ),
        TreatmentGateDecision::FailDisabled
    );
}
