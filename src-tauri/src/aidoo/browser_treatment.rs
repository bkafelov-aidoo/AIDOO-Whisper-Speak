const TREATMENT_SIGNATURE_GATE_LABEL: &str = "Продължи без подпис";
const TREATMENT_PROCEDURES_LABEL: &str = "Процедури";
const TREATMENT_REMARKS_LABEL: &str = "Забележки";
const TREATMENT_FILTER_PREFIX: &str = "Филтри:";
const TREATMENT_FILTER_REMOVE_LABEL: &str = "Remove";
const TREATMENT_GATE_MAX_DEPTH: usize = 16;
const TREATMENT_GATE_MAX_NODES: usize = 2_400;
const TREATMENT_GATE_POLL_MILLIS: u64 = 75;
// A slow AIDOO hydration gets a bounded grace period; stable pages finish immediately.
const TREATMENT_GATE_TIMEOUT_MILLIS: u64 = 8_000;
const TREATMENT_READY_STABLE_POLLS: usize = 3;
const FOCUSED_ANCESTOR_LIMIT: usize = 32;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(value: CFTypeRef) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CFTypeRef) -> u8;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFURLGetTypeID() -> usize;
    fn CFURLGetString(url: CFTypeRef) -> CFStringRef;
}

struct TreatmentRetainedValue(CFTypeRef);

impl TreatmentRetainedValue {
    fn as_ref(&self) -> CFTypeRef {
        self.0
    }
}

impl Drop for TreatmentRetainedValue {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Values in this wrapper follow the Core Foundation Create/Copy rule.
            unsafe { CFRelease(self.0) };
        }
    }
}

struct TreatmentRetainedAx(AXUIElementRef);

impl TreatmentRetainedAx {
    unsafe fn from_owned(element: AXUIElementRef) -> Self {
        Self(element)
    }

    unsafe fn retain(element: AXUIElementRef) -> Self {
        // SAFETY: The caller supplies a live element borrowed from a retained AX tree.
        unsafe { CFRetain(element.cast()) };
        Self(element)
    }

    fn as_ref(&self) -> AXUIElementRef {
        self.0
    }
}

impl Clone for TreatmentRetainedAx {
    fn clone(&self) -> Self {
        // SAFETY: This wrapper owns a retain count for the live AX element.
        unsafe { Self::retain(self.0) }
    }
}

impl Drop for TreatmentRetainedAx {
    fn drop(&mut self) {
        // SAFETY: Every wrapper owns exactly one retain count.
        unsafe { CFRelease(self.0.cast()) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreatmentGateMatch {
    Other,
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreatmentGateState {
    NotPressed,
    Pressed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreatmentGateObservation {
    NotReady,
    Ready,
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreatmentGateDecision {
    Complete,
    Press,
    Wait,
    FailDisabled,
    FailNotReady,
    FailStillPresent,
}

fn classify_treatment_signature_gate(
    role: &str,
    enabled: Option<bool>,
    labels: &[Option<&str>],
) -> TreatmentGateMatch {
    if role != "AXButton"
        || !labels
            .iter()
            .flatten()
            .any(|label| label.trim() == TREATMENT_SIGNATURE_GATE_LABEL)
    {
        return TreatmentGateMatch::Other;
    }
    if enabled == Some(true) {
        TreatmentGateMatch::Enabled
    } else {
        TreatmentGateMatch::Disabled
    }
}

fn treatment_gate_decision(
    state: TreatmentGateState,
    observation: TreatmentGateObservation,
    deadline_reached: bool,
    ready_confirmed: bool,
) -> TreatmentGateDecision {
    match (state, observation, deadline_reached, ready_confirmed) {
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Ready, _, true) => {
            TreatmentGateDecision::Complete
        }
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Ready, false, false) => {
            TreatmentGateDecision::Wait
        }
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Ready, true, false) => {
            TreatmentGateDecision::FailNotReady
        }
        (TreatmentGateState::Pressed, TreatmentGateObservation::Ready, _, true) => {
            TreatmentGateDecision::Complete
        }
        (TreatmentGateState::Pressed, TreatmentGateObservation::Ready, false, false) => {
            TreatmentGateDecision::Wait
        }
        (TreatmentGateState::Pressed, TreatmentGateObservation::Ready, true, false) => {
            TreatmentGateDecision::FailStillPresent
        }
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Enabled, _, _) => {
            TreatmentGateDecision::Press
        }
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Disabled, false, _)
        | (TreatmentGateState::NotPressed, TreatmentGateObservation::NotReady, false, _)
        | (TreatmentGateState::Pressed, _, false, _) => TreatmentGateDecision::Wait,
        (TreatmentGateState::NotPressed, TreatmentGateObservation::Disabled, true, _) => {
            TreatmentGateDecision::FailDisabled
        }
        (TreatmentGateState::NotPressed, TreatmentGateObservation::NotReady, true, _) => {
            TreatmentGateDecision::FailNotReady
        }
        (TreatmentGateState::Pressed, _, true, _) => TreatmentGateDecision::FailStillPresent,
    }
}

enum ObservedTreatmentGate {
    NotReady,
    Ready,
    Enabled(TreatmentRetainedAx),
    Disabled,
}

#[derive(Default)]
struct TreatmentReadinessDiagnostics {
    web_area: bool,
    route_truncated: bool,
    page_nodes: usize,
    page_truncated: bool,
    procedures: bool,
    remarks: bool,
    expected_selection: usize,
    filter_groups: usize,
    filter_invalid: bool,
    filter_truncated: bool,
    selection_visible: bool,
}

impl TreatmentReadinessDiagnostics {
    fn summary(&self) -> String {
        format!(
            "web_area={} route_truncated={} page_nodes={} page_truncated={} procedures={} remarks={} expected_selection={} filter_groups={} filter_invalid={} filter_truncated={} selection_visible={}",
            self.web_area, self.route_truncated, self.page_nodes, self.page_truncated,
            self.procedures, self.remarks, self.expected_selection, self.filter_groups,
            self.filter_invalid, self.filter_truncated, self.selection_visible,
        )
    }
}

impl ObservedTreatmentGate {
    fn observation(&self) -> TreatmentGateObservation {
        match self {
            Self::NotReady => TreatmentGateObservation::NotReady,
            Self::Ready => TreatmentGateObservation::Ready,
            Self::Enabled(_) => TreatmentGateObservation::Enabled,
            Self::Disabled => TreatmentGateObservation::Disabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TreatmentFilterChild {
    Text(String),
    RemoveButton,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TreatmentFilterMatch {
    Other,
    Invalid,
    Teeth(HashSet<String>),
}

fn classify_treatment_filter(children: &[TreatmentFilterChild]) -> TreatmentFilterMatch {
    let Some(TreatmentFilterChild::Text(first)) = children.first() else {
        return TreatmentFilterMatch::Other;
    };
    let Some(first_tooth) = first.trim().strip_prefix(TREATMENT_FILTER_PREFIX) else {
        return TreatmentFilterMatch::Other;
    };
    if !children.len().is_multiple_of(2) {
        return TreatmentFilterMatch::Invalid;
    }

    let mut teeth = HashSet::new();
    for (index, pair) in children.chunks_exact(2).enumerate() {
        let TreatmentFilterChild::Text(label) = &pair[0] else {
            return TreatmentFilterMatch::Invalid;
        };
        if pair[1] != TreatmentFilterChild::RemoveButton {
            return TreatmentFilterMatch::Invalid;
        }
        let tooth = if index == 0 {
            first_tooth.trim()
        } else {
            label.trim()
        };
        if !valid_treatment_body(tooth) || !teeth.insert(tooth.to_string()) {
            return TreatmentFilterMatch::Invalid;
        }
    }
    TreatmentFilterMatch::Teeth(teeth)
}

fn valid_treatment_body(tooth: &str) -> bool {
    let bytes = tooth.as_bytes();
    bytes.len() == 2
        && (b'1'..=b'4').contains(&bytes[0])
        && (b'1'..=b'8').contains(&bytes[1])
}

fn treatment_selection_from_url(url: &str) -> Option<HashSet<String>> {
    let query = url.split_once('?')?.1;
    let raw = query
        .split('&')
        .find_map(|parameter| {
        let (key, value) = parameter.split_once('=')?;
        key.eq_ignore_ascii_case("selectedTeeth").then_some(value)
        })
        .unwrap_or_default();
    if raw.is_empty() {
        return Some(HashSet::new());
    }
    let decoded = raw.replace("%2C", ",").replace("%2c", ",");
    let mut teeth = HashSet::new();
    for tooth in decoded.split(',') {
        if !valid_treatment_body(tooth) || !teeth.insert(tooth.to_string()) {
            return None;
        }
    }
    Some(teeth)
}

#[derive(Default)]
struct TreatmentFilterScan {
    teeth: Vec<HashSet<String>>,
    invalid: bool,
    truncated: bool,
}

fn treatment_filter_is_visible(
    expected: &HashSet<String>,
    observed: &TreatmentFilterScan,
) -> bool {
    !observed.truncated
        && !observed.invalid
        && match observed.teeth.as_slice() {
            [] => expected.is_empty(),
            [only] => only == expected,
            _ => false,
        }
}

enum TreatmentRouteExpectation<'a> {
    ExactUrl(&'a str),
    Patient(&'a str),
}

impl TreatmentRouteExpectation<'_> {
    fn matches(&self, observed_url: &str) -> bool {
        match self {
            Self::ExactUrl(expected_url) => {
                address_matches_target(expected_url, observed_url)
                    && is_treatment_url(observed_url)
                    && treatment_selection_from_url(expected_url).is_some_and(|expected| {
                        treatment_selection_from_url(observed_url)
                            .is_some_and(|observed| observed == expected)
                    })
            }
            Self::Patient(expected_patient) => {
                valid_aidoo_url(observed_url)
                    && is_treatment_url(observed_url)
                    && patient_id_from_url(observed_url)
                        .is_some_and(|patient| patient.eq_ignore_ascii_case(expected_patient))
            }
        }
    }
}

pub fn preview_official_note(text: &str, patient_id: &str) -> Result<bool, String> {
    if text.trim().is_empty() || patient_id.trim().is_empty() {
        return Ok(false);
    }
    if text.chars().count() > 10_000 {
        return Err("Забележката е твърде дълга за преглед в AIDOO.".into());
    }
    if !crate::accessibility_granted() {
        return Ok(false);
    }
    let _guard = PRESENTATION_LOCK
        .lock()
        .map_err(|_| "Chrome синхронизацията е заключена.".to_string())?;
    discard_stale_targets();
    let Some(target) = managed_patient_target(Some(patient_id)) else {
        return Ok(false);
    };
    let Some(surface) = target.surface else {
        return Ok(false);
    };
    surface.preview_official_note(target.window.pid, patient_id, text)
}

fn complete_presentation(
    window: ChromeWindow,
    patient_id: Option<String>,
    url: &str,
) -> Result<(), String> {
    remember_target(window, patient_id.clone());
    if !is_treatment_url(url) {
        return Ok(());
    }
    let patient_id = patient_id
        .as_deref()
        .ok_or_else(|| "Лечението няма избран пациент.".to_string())?;
    let target = managed_patient_target(Some(patient_id))
        .filter(|target| target.window == window)
        .ok_or_else(|| {
            "AIDOO Treatment табът не е регистриран за избрания пациент.".to_string()
        })?;
    let surface = target.surface.ok_or_else(|| {
        "Chrome не предостави точния AIDOO Treatment таб чрез Accessibility.".to_string()
    })?;
    surface.clear_treatment_signature_gate(url)
}

fn confirm_current_target(window: ChromeWindow, url: &str) -> Result<(), String> {
    match read_address_bar(window.pid) {
        Ok(observed) if address_matches_target(url, &observed) => Ok(()),
        _ => navigate_window(window, url),
    }
}

fn is_treatment_url(url: &str) -> bool {
    url.split_once('?')
        .and_then(|(_, query)| query_value(query, "mode"))
        .is_some_and(|mode| mode.eq_ignore_ascii_case("treatment"))
}

impl ChromeSurface {
    fn clear_treatment_signature_gate(&self, expected_url: &str) -> Result<(), String> {
        self.select_tab()?;
        if self.is_stale() {
            return Err("Запазеният Chrome таб вече не е наличен.".into());
        }

        let expected = TreatmentRouteExpectation::ExactUrl(expected_url);
        let started = std::time::Instant::now();
        let mut polls = 0;
        let mut state = TreatmentGateState::NotPressed;
        let mut ready_streak = 0;
        let mut deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(TREATMENT_GATE_TIMEOUT_MILLIS);
        loop {
            let mut diagnostics = TreatmentReadinessDiagnostics::default();
            let observed = observe_treatment_page(self.window.as_ref(), &expected, &mut diagnostics)?;
            polls += 1;
            if matches!(observed, ObservedTreatmentGate::Ready) {
                ready_streak += 1;
            } else {
                ready_streak = 0;
            }
            let decision = treatment_gate_decision(
                state,
                observed.observation(),
                std::time::Instant::now() >= deadline,
                ready_streak >= TREATMENT_READY_STABLE_POLLS,
            );
            match decision {
                TreatmentGateDecision::Complete => return Ok(()),
                TreatmentGateDecision::Press => {
                    let ObservedTreatmentGate::Enabled(button) = observed else {
                        return Err(
                            "AIDOO модалът без подпис се промени преди натискането.".into()
                        );
                    };
                    let action = treatment_cf_string("AXPress")?;
                    // SAFETY: The exact enabled AXButton is retained for this synchronous action.
                    let pressed = unsafe {
                        AXUIElementPerformAction(button.as_ref(), action.as_ref().cast())
                    } == AX_SUCCESS;
                    if !pressed {
                        return Err(
                            "AIDOO показа „Продължи без подпис“, но Chrome не прие натискането."
                                .into(),
                        );
                    }
                    crate::storage::append_diagnostic(
                        "AIDOO Treatment: continued without signature in the bound patient tab.",
                    );
                    state = TreatmentGateState::Pressed;
                    deadline = std::time::Instant::now()
                        + std::time::Duration::from_millis(TREATMENT_GATE_TIMEOUT_MILLIS);
                }
                TreatmentGateDecision::Wait => std::thread::sleep(
                    std::time::Duration::from_millis(TREATMENT_GATE_POLL_MILLIS),
                ),
                TreatmentGateDecision::FailDisabled => {
                    log_treatment_readiness_failure(started, polls, observed.observation(), &diagnostics);
                    return Err(
                        "AIDOO показа „Продължи без подпис“, но бутонът остана неактивен."
                            .into(),
                    );
                }
                TreatmentGateDecision::FailNotReady => {
                    log_treatment_readiness_failure(started, polls, observed.observation(), &diagnostics);
                    return Err(
                        "AIDOO Treatment екранът не се зареди в точния пациентски таб.".into(),
                    );
                }
                TreatmentGateDecision::FailStillPresent => {
                    log_treatment_readiness_failure(started, polls, observed.observation(), &diagnostics);
                    return Err(
                        "AIDOO не показа готов Treatment екран след „Продължи без подпис“."
                            .into(),
                    );
                }
            }
        }
    }

    fn preview_official_note(
        &self,
        pid: i32,
        patient_id: &str,
        text: &str,
    ) -> Result<bool, String> {
        if !treatment_tab_is_selected(self.selected_tab.as_ref()) || self.is_stale() {
            return Ok(false);
        }
        let expected = TreatmentRouteExpectation::Patient(patient_id);
        let mut visited = 0;
        let mut truncated = false;
        let Some(web_area) = find_treatment_web_area(
            self.window.as_ref(),
            &expected,
            0,
            &mut visited,
            &mut truncated,
        ) else {
            return Ok(false);
        };

        // SAFETY: The pid belongs to the registered Chrome surface.
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return Ok(false);
        }
        // SAFETY: AXUIElementCreateApplication follows the Create rule.
        let application = unsafe { TreatmentRetainedAx::from_owned(application) };
        let Some(focused) = treatment_copy_ax(application.as_ref(), "AXFocusedUIElement") else {
            return Ok(false);
        };
        let Some(focused_web_area) = focused_web_area(&focused) else {
            return Ok(false);
        };
        // SAFETY: Both retained elements are live AX objects from this Chrome process.
        if !unsafe { CFEqual(web_area.as_ref().cast(), focused_web_area.as_ref().cast()) } {
            return Ok(false);
        }
        let Some(focused_url) = treatment_ax_url(focused_web_area.as_ref()) else {
            return Ok(false);
        };
        if !expected.matches(&focused_url) {
            return Ok(false);
        }

        let role = ax_string_attribute(focused.as_ref(), "AXRole").unwrap_or_default();
        let description = ax_string_attribute(focused.as_ref(), "AXDescription");
        let title = ax_string_attribute(focused.as_ref(), "AXTitle");
        let placeholder = ax_string_attribute(focused.as_ref(), "AXPlaceholderValue");
        if !is_official_note_field(
            &role,
            &[
                description.as_deref(),
                title.as_deref(),
                placeholder.as_deref(),
            ],
        ) {
            return Ok(false);
        }
        let value_attribute = treatment_cf_string("AXValue")?;
        let value = treatment_cf_string(text)?;
        // SAFETY: The exact focused named note field and both retained CFStrings are live.
        Ok(unsafe {
            AXUIElementSetAttributeValue(
                focused.as_ref(),
                value_attribute.as_ref().cast(),
                value.as_ref(),
            )
        } == AX_SUCCESS)
    }
}

fn observe_treatment_page(
    root: AXUIElementRef,
    expected: &TreatmentRouteExpectation<'_>,
    diagnostics: &mut TreatmentReadinessDiagnostics,
) -> Result<ObservedTreatmentGate, String> {
    let mut visited = 0;
    let mut truncated = false;
    let Some(web_area) = find_treatment_web_area(
        root,
        expected,
        0,
        &mut visited,
        &mut truncated,
    ) else {
        diagnostics.route_truncated = truncated;
        return Ok(ObservedTreatmentGate::NotReady);
    };
    diagnostics.web_area = true;
    diagnostics.route_truncated = truncated;
    let scan = scan_treatment_page(web_area.clone());
    diagnostics.page_nodes = scan.visited;
    diagnostics.page_truncated = scan.truncated;
    diagnostics.procedures = scan.procedures;
    diagnostics.remarks = scan.remarks;
    if let Some(button) = scan.enabled_gate {
        return Ok(ObservedTreatmentGate::Enabled(button));
    }
    if scan.disabled_gate {
        return Ok(ObservedTreatmentGate::Disabled);
    }
    let selection_visible = match expected {
        TreatmentRouteExpectation::ExactUrl(url) => {
            let Some(expected_selection) = treatment_selection_from_url(url) else {
                return Ok(ObservedTreatmentGate::NotReady);
            };
            let observed_selection = scan_treatment_filters(web_area);
            diagnostics.expected_selection = expected_selection.len();
            diagnostics.filter_groups = observed_selection.teeth.len();
            diagnostics.filter_invalid = observed_selection.invalid;
            diagnostics.filter_truncated = observed_selection.truncated;
            treatment_filter_is_visible(&expected_selection, &observed_selection)
        }
        TreatmentRouteExpectation::Patient(_) => true,
    };
    diagnostics.selection_visible = selection_visible;
    if treatment_page_without_gate_is_ready(scan.procedures, scan.remarks, scan.truncated)
        && selection_visible
    {
        return Ok(ObservedTreatmentGate::Ready);
    }
    Ok(ObservedTreatmentGate::NotReady)
}

fn log_treatment_readiness_failure(
    started: std::time::Instant,
    polls: usize,
    observation: TreatmentGateObservation,
    diagnostics: &TreatmentReadinessDiagnostics,
) {
    // Only predicates/counts: never persist patient URLs, clinical content or tooth values.
    crate::storage::append_diagnostic(&format!(
        "AIDOO Treatment readiness timeout: elapsed_ms={} polls={} state={observation:?} {}",
        started.elapsed().as_millis(), polls, diagnostics.summary(),
    ));
}

fn treatment_page_without_gate_is_ready(
    procedures: bool,
    remarks: bool,
    traversal_truncated: bool,
) -> bool {
    procedures && remarks && !traversal_truncated
}

struct TreatmentPageScan<N> {
    visited: usize,
    truncated: bool,
    enabled_gate: Option<N>,
    disabled_gate: bool,
    procedures: bool,
    remarks: bool,
}

impl<N> Default for TreatmentPageScan<N> {
    fn default() -> Self {
        Self {
            visited: 0,
            truncated: false,
            enabled_gate: None,
            disabled_gate: false,
            procedures: false,
            remarks: false,
        }
    }
}

struct TreatmentNodeSnapshot<N> {
    gate: TreatmentGateMatch,
    procedures: bool,
    remarks: bool,
    children: Vec<N>,
}

fn walk_treatment_nodes<N>(
    root: N,
    max_nodes: usize,
    mut inspect: impl FnMut(&N) -> TreatmentNodeSnapshot<N>,
) -> TreatmentPageScan<N> {
    let mut scan = TreatmentPageScan::default();
    let mut pending = std::collections::VecDeque::from([root]);
    while let Some(node) = pending.pop_front() {
        if scan.visited >= max_nodes {
            scan.truncated = true;
            break;
        }
        scan.visited += 1;
        let snapshot = inspect(&node);
        scan.procedures |= snapshot.procedures;
        scan.remarks |= snapshot.remarks;
        match snapshot.gate {
            TreatmentGateMatch::Enabled => {
                scan.enabled_gate = Some(node);
                return scan;
            }
            TreatmentGateMatch::Disabled => scan.disabled_gate = true,
            TreatmentGateMatch::Other => {}
        }
        pending.extend(snapshot.children);
    }
    scan
}

fn scan_treatment_page(
    root: TreatmentRetainedAx,
) -> TreatmentPageScan<TreatmentRetainedAx> {
    walk_treatment_nodes(
        root,
        TREATMENT_GATE_MAX_NODES,
        |element| inspect_treatment_node(element.as_ref()),
    )
}

fn scan_treatment_filters(root: TreatmentRetainedAx) -> TreatmentFilterScan {
    let mut scan = TreatmentFilterScan::default();
    let mut pending = std::collections::VecDeque::from([root]);
    let mut visited = 0;
    while let Some(element) = pending.pop_front() {
        if visited >= TREATMENT_GATE_MAX_NODES {
            scan.truncated = true;
            break;
        }
        visited += 1;
        let role = ax_string_attribute(element.as_ref(), "AXRole").unwrap_or_default();
        let children = treatment_ax_children(element.as_ref());
        if role == "AXGroup" {
            match classify_treatment_filter(
                &children
                    .iter()
                    .map(|child| treatment_filter_child(child.as_ref()))
                    .collect::<Vec<_>>(),
            ) {
                TreatmentFilterMatch::Other => {}
                TreatmentFilterMatch::Invalid => scan.invalid = true,
                TreatmentFilterMatch::Teeth(teeth) => scan.teeth.push(teeth),
            }
        }
        pending.extend(children);
    }
    scan
}

fn treatment_filter_child(element: AXUIElementRef) -> TreatmentFilterChild {
    let role = ax_string_attribute(element, "AXRole").unwrap_or_default();
    if role == "AXStaticText" {
        let label = ["AXTitle", "AXValue", "AXDescription"]
            .into_iter()
            .filter_map(|attribute| ax_string_attribute(element, attribute))
            .find(|label| !label.trim().is_empty());
        return label
            .map(TreatmentFilterChild::Text)
            .unwrap_or(TreatmentFilterChild::Other);
    }
    if role == "AXButton" {
        let remove = ["AXTitle", "AXDescription", "AXHelp"]
            .into_iter()
            .filter_map(|attribute| ax_string_attribute(element, attribute))
            .any(|label| label.trim() == TREATMENT_FILTER_REMOVE_LABEL);
        return if remove {
            TreatmentFilterChild::RemoveButton
        } else {
            TreatmentFilterChild::Other
        };
    }
    TreatmentFilterChild::Other
}

fn treatment_ax_children(element: AXUIElementRef) -> Vec<TreatmentRetainedAx> {
    let mut retained_children = Vec::new();
    if let Some(children) = treatment_copy_attribute(element, "AXChildren") {
        // SAFETY: AXChildren is a retained CFArray of borrowed AXUIElement references.
        let count = unsafe { CFArrayGetCount(children.as_ref().cast()) };
        for index in 0..count {
            // SAFETY: `index` is bounded by `count` and the array remains retained.
            let child = unsafe { CFArrayGetValueAtIndex(children.as_ref().cast(), index) };
            if !child.is_null() {
                // SAFETY: The child is borrowed from the retained AXChildren array.
                retained_children.push(unsafe { TreatmentRetainedAx::retain(child.cast()) });
            }
        }
    }
    retained_children
}

fn inspect_treatment_node(element: AXUIElementRef) -> TreatmentNodeSnapshot<TreatmentRetainedAx> {
    let role = ax_string_attribute(element, "AXRole").unwrap_or_default();
    let gate = if role == "AXButton" {
        let title = ax_string_attribute(element, "AXTitle");
        let description = ax_string_attribute(element, "AXDescription");
        let help = ax_string_attribute(element, "AXHelp");
        let enabled = treatment_copy_attribute(element, "AXEnabled")
            .and_then(|value| treatment_boolean(value.as_ref()));
        classify_treatment_signature_gate(
            &role,
            enabled,
            &[title.as_deref(), description.as_deref(), help.as_deref()],
        )
    } else {
        TreatmentGateMatch::Other
    };
    let mut procedures = false;
    let mut remarks = false;
    if matches!(role.as_str(), "AXStaticText" | "AXHeading" | "AXCell") {
        let title = ax_string_attribute(element, "AXTitle");
        let description = ax_string_attribute(element, "AXDescription");
        let value = (role == "AXStaticText")
            .then(|| ax_string_attribute(element, "AXValue"))
            .flatten();
        for label in [title.as_deref(), description.as_deref(), value.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
        {
            procedures |= label == TREATMENT_PROCEDURES_LABEL;
            remarks |= label == TREATMENT_REMARKS_LABEL;
        }
    }
    let retained_children = treatment_ax_children(element);
    TreatmentNodeSnapshot {
        gate,
        procedures,
        remarks,
        children: retained_children,
    }
}

fn find_treatment_web_area(
    element: AXUIElementRef,
    expected: &TreatmentRouteExpectation<'_>,
    depth: usize,
    visited: &mut usize,
    truncated: &mut bool,
) -> Option<TreatmentRetainedAx> {
    if depth > TREATMENT_GATE_MAX_DEPTH || *visited >= TREATMENT_GATE_MAX_NODES {
        *truncated = true;
        return None;
    }
    *visited += 1;
    let role = ax_string_attribute(element, "AXRole").unwrap_or_default();
    if role == "AXWebArea"
        && treatment_ax_url(element).is_some_and(|url| expected.matches(&url))
    {
        // SAFETY: The element is borrowed from a retained AX tree during traversal.
        return Some(unsafe { TreatmentRetainedAx::retain(element) });
    }
    let children = treatment_copy_attribute(element, "AXChildren")?;
    // SAFETY: AXChildren is a retained CFArray of borrowed AXUIElement references.
    let count = unsafe { CFArrayGetCount(children.as_ref().cast()) };
    for index in 0..count {
        // SAFETY: `index` is bounded by `count` and the array remains retained.
        let child = unsafe { CFArrayGetValueAtIndex(children.as_ref().cast(), index) };
        if child.is_null() {
            continue;
        }
        if let Some(web_area) =
            find_treatment_web_area(child.cast(), expected, depth + 1, visited, truncated)
        {
            return Some(web_area);
        }
    }
    None
}

fn focused_web_area(focused: &TreatmentRetainedAx) -> Option<TreatmentRetainedAx> {
    let mut current = focused.clone();
    for _ in 0..FOCUSED_ANCESTOR_LIMIT {
        if ax_string_attribute(current.as_ref(), "AXRole").as_deref() == Some("AXWebArea") {
            return Some(current);
        }
        current = treatment_copy_ax(current.as_ref(), "AXParent")?;
    }
    None
}

fn treatment_tab_is_selected(tab: AXUIElementRef) -> bool {
    ["AXSelected", "AXValue"]
        .into_iter()
        .filter_map(|attribute| {
            treatment_copy_attribute(tab, attribute)
                .and_then(|value| treatment_boolean(value.as_ref()))
        })
        .any(|selected| selected)
}

fn treatment_ax_url(element: AXUIElementRef) -> Option<String> {
    let value = treatment_copy_attribute(element, "AXURL")?;
    // SAFETY: The value is retained and live while its Core Foundation type is inspected.
    let type_id = unsafe { CFGetTypeID(value.as_ref()) };
    // SAFETY: Core Foundation type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFStringGetTypeID() } {
        return cf_string_value(value.as_ref().cast());
    }
    // SAFETY: Core Foundation type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFURLGetTypeID() } {
        // SAFETY: The type id proves this is a CFURL; CFURLGetString returns a borrowed CFString.
        let string = unsafe { CFURLGetString(value.as_ref()) };
        return (!string.is_null()).then(|| cf_string_value(string)).flatten();
    }
    None
}

fn treatment_boolean(value: CFTypeRef) -> Option<bool> {
    if value.is_null() {
        return None;
    }
    // SAFETY: The retained value is live while its Core Foundation type is inspected.
    let type_id = unsafe { CFGetTypeID(value) };
    // SAFETY: Core Foundation type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFBooleanGetTypeID() } {
        // SAFETY: The type id proves this value is a CFBoolean.
        return Some(unsafe { CFBooleanGetValue(value) } != 0);
    }
    // SAFETY: Core Foundation type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFNumberGetTypeID() } {
        let mut number = 0_i64;
        // SAFETY: The type id proves this value is a CFNumber and `number` has SInt64 layout.
        let converted = unsafe {
            CFNumberGetValue(
                value.cast(),
                CF_NUMBER_SINT64_TYPE,
                (&raw mut number).cast(),
            )
        };
        return converted.then_some(number != 0);
    }
    None
}

fn treatment_copy_ax(element: AXUIElementRef, attribute: &str) -> Option<TreatmentRetainedAx> {
    let value = treatment_copy_attribute(element, attribute)?;
    let raw = value.0.cast();
    std::mem::forget(value);
    // SAFETY: Ownership of the retained Copy-rule value moves into the AX wrapper.
    Some(unsafe { TreatmentRetainedAx::from_owned(raw) })
}

fn treatment_copy_attribute(
    element: AXUIElementRef,
    attribute: &str,
) -> Option<TreatmentRetainedValue> {
    let attribute = treatment_cf_string(attribute).ok()?;
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: The AX element and retained attribute are live for this synchronous copy.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_ref().cast(), &raw mut value)
    };
    if result == AX_SUCCESS && !value.is_null() {
        return Some(TreatmentRetainedValue(value));
    }
    if !value.is_null() {
        // SAFETY: A non-null Copy-rule out value is retained even on an error result.
        unsafe { CFRelease(value) };
    }
    None
}

fn treatment_cf_string(value: &str) -> Result<TreatmentRetainedValue, String> {
    Ok(TreatmentRetainedValue(cf_string(value)?.cast()))
}

#[cfg(test)]
mod treatment_gate_tests {
    include!("browser_treatment_tests.rs");

}
