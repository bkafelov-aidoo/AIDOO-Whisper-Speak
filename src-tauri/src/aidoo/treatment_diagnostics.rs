use super::client::{AidooError, AidooErrorKind};
use super::types::{VerificationOutcome, VerificationResult};

pub(super) const STAGE_BEGIN_VISIT: &str = "begin_visit";
pub(super) const STAGE_ACTIVE_VISIT: &str = "active_visit";
pub(super) const STAGE_READ_ROWS: &str = "read_rows";
pub(super) const STAGE_DIAGNOSIS_CATALOG: &str = "diagnosis_catalog";
pub(super) const STAGE_PROCEDURE_CATALOG: &str = "procedure_catalog";
pub(super) const STAGE_APPLY_CHANGE: &str = "apply_change";

pub(super) fn failure(stage: &'static str, error: AidooError) -> String {
    let message = error.message;
    crate::storage::append_diagnostic(&failure_line(stage, &error.kind));
    message
}

pub(super) fn verification(stage: &'static str, result: &VerificationResult) {
    crate::storage::append_diagnostic(&verification_line(stage, &result.outcome));
}

pub(super) fn presentation(result: &Result<(), String>) {
    crate::storage::append_diagnostic(&presentation_line(result.is_ok()));
}

fn failure_line(stage: &'static str, kind: &AidooErrorKind) -> String {
    format!(
        "AIDOO Treatment diagnostic: stage={} failure={}",
        whitelisted_stage(stage),
        error_kind_label(kind)
    )
}

fn verification_line(stage: &'static str, outcome: &VerificationOutcome) -> String {
    format!(
        "AIDOO Treatment diagnostic: stage={} outcome={}",
        whitelisted_stage(stage),
        verification_outcome_label(outcome)
    )
}

fn presentation_line(visible: bool) -> String {
    format!("AIDOO Treatment diagnostic: stage=presentation visible={visible}")
}

fn whitelisted_stage(stage: &'static str) -> &'static str {
    match stage {
        STAGE_BEGIN_VISIT => STAGE_BEGIN_VISIT,
        STAGE_ACTIVE_VISIT => STAGE_ACTIVE_VISIT,
        STAGE_READ_ROWS => STAGE_READ_ROWS,
        STAGE_DIAGNOSIS_CATALOG => STAGE_DIAGNOSIS_CATALOG,
        STAGE_PROCEDURE_CATALOG => STAGE_PROCEDURE_CATALOG,
        STAGE_APPLY_CHANGE => STAGE_APPLY_CHANGE,
        _ => "unknown",
    }
}

fn error_kind_label(kind: &AidooErrorKind) -> &'static str {
    match kind {
        AidooErrorKind::Authentication => "authentication",
        AidooErrorKind::Validation => "validation",
        AidooErrorKind::NotFound => "not_found",
        AidooErrorKind::Http => "http",
        AidooErrorKind::Transport => "transport",
        AidooErrorKind::Protocol => "protocol",
    }
}

fn verification_outcome_label(outcome: &VerificationOutcome) -> &'static str {
    match outcome {
        VerificationOutcome::Verified => "verified",
        VerificationOutcome::VerifiedAfterAmbiguousWrite => "verified_after_ambiguous_write",
        VerificationOutcome::Rejected => "rejected",
        VerificationOutcome::Uncertain => "uncertain",
        VerificationOutcome::StaleDraft => "stale_draft",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_diagnostic_log(test: impl FnOnce(&std::path::Path)) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "aidoo-treatment-diagnostics-{}-{nonce}",
            std::process::id()
        ));
        crate::storage::with_test_data_dir(directory.clone(), || test(&directory));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn failure_returns_original_message_but_persists_only_whitelisted_stage_and_kind() {
        with_diagnostic_log(|directory| {
            let private_message = "patient=<private> note=<private> token=<private>";
            let returned = failure(
                STAGE_PROCEDURE_CATALOG,
                AidooError {
                    kind: AidooErrorKind::Transport,
                    message: private_message.into(),
                },
            );
            assert_eq!(returned, private_message);
            let log = fs::read_to_string(directory.join("diagnostics.log")).unwrap();
            assert!(log.contains("stage=procedure_catalog failure=transport"));
            assert!(!log.contains(private_message));
            assert!(!log.contains("patient="));
            assert!(!log.contains("token="));
        });
    }

    #[test]
    fn unknown_stage_content_is_never_persisted() {
        with_diagnostic_log(|directory| {
            let _ = failure(
                "patient/private-note/token",
                AidooError {
                    kind: AidooErrorKind::Validation,
                    message: "private error message".into(),
                },
            );
            let log = fs::read_to_string(directory.join("diagnostics.log")).unwrap();
            assert!(log.contains("stage=unknown failure=validation"));
            assert!(!log.contains("patient/private-note/token"));
            assert!(!log.contains("private error message"));
        });
    }

    #[test]
    fn verification_persists_outcome_without_result_message() {
        with_diagnostic_log(|directory| {
            verification(
                STAGE_APPLY_CHANGE,
                &VerificationResult {
                    outcome: VerificationOutcome::Uncertain,
                    message: "patient note and HTTP body must stay private".into(),
                },
            );
            let log = fs::read_to_string(directory.join("diagnostics.log")).unwrap();
            assert!(log.contains("stage=apply_change outcome=uncertain"));
            assert!(!log.contains("patient note"));
            assert!(!log.contains("HTTP body"));
        });
    }

    #[test]
    fn presentation_persists_only_visibility() {
        with_diagnostic_log(|directory| {
            presentation(&Err(
                "private patient URL and selected tooth must not be logged".into(),
            ));
            let log = fs::read_to_string(directory.join("diagnostics.log")).unwrap();
            assert!(log.contains("stage=presentation visible=false"));
            assert!(!log.contains("patient URL"));
            assert!(!log.contains("selected tooth"));
        });
    }
}
