use super::{valid_aidoo_url, ChromeWindow, NavigationAction};
use std::collections::HashSet;

pub(super) fn patient_id_from_url(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !name.eq_ignore_ascii_case("patientid") || value.is_empty() {
            return None;
        }
        Some(uuid::Uuid::parse_str(value).map_or_else(|_| value.to_string(), |id| id.to_string()))
    })
}

pub(super) fn navigation_action(
    current_patient: Option<&str>,
    next_patient: Option<&str>,
) -> NavigationAction {
    if next_patient.is_none() || current_patient == next_patient {
        NavigationAction::RefreshCurrentTab
    } else {
        NavigationAction::OpenNewTab
    }
}

pub(super) fn live_address_action(expected: &str, observed: &str) -> NavigationAction {
    let observed = if observed.trim().starts_with("https://") {
        observed.trim().to_string()
    } else {
        ["https:", "//", observed.trim()].concat()
    };
    if valid_aidoo_url(&observed) {
        navigation_action(
            patient_id_from_url(&observed).as_deref(),
            patient_id_from_url(expected).as_deref(),
        )
    } else {
        NavigationAction::OpenNewTab
    }
}

pub(super) fn may_open_dedicated_window(had_existing_aidoo_surface: bool) -> bool {
    !had_existing_aidoo_surface
}

pub(super) fn recovery_candidates(
    failed_window: Option<ChromeWindow>,
    previous_patient_id: Option<&str>,
    next_patient_id: Option<&str>,
    aidoo_windows: &[ChromeWindow],
    chrome_windows: &[ChromeWindow],
) -> Vec<(ChromeWindow, NavigationAction)> {
    let mut seen = HashSet::new();
    if let Some(window) = failed_window {
        seen.insert((window.pid, window.number));
    }
    let mut candidates = Vec::new();
    let action = navigation_action(previous_patient_id, next_patient_id);
    let windows = if aidoo_windows.is_empty() {
        chrome_windows
    } else {
        aidoo_windows
    };
    for window in windows {
        if seen.insert((window.pid, window.number)) {
            candidates.push((*window, action));
        }
    }
    candidates
}
