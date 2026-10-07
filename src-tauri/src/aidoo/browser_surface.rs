use super::{
    ax_string_attribute, cf_string, AXUIElementCopyAttributeValue, AXUIElementCreateApplication,
    AXUIElementPerformAction, AXUIElementRef, AXUIElementSetAttributeValue, CFArrayGetCount,
    CFArrayGetValueAtIndex, CFArrayRef, CFNumberGetValue, CFRelease, CFStringRef, CFTypeRef,
    AX_SUCCESS,
};
use std::ffi::c_void;
use std::ptr;

const MAX_TREE_DEPTH: usize = 9;
const MAX_TREE_NODES: usize = 320;
const CF_NUMBER_SINT64_TYPE: i64 = 4;
const AX_ERROR_INVALID_UI_ELEMENT: i32 = -25202;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(value: CFTypeRef) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CFTypeRef) -> u8;
    fn CFNumberGetTypeID() -> usize;
}

#[derive(Debug)]
pub(super) struct RetainedAxElement(AXUIElementRef);

impl RetainedAxElement {
    unsafe fn from_owned(element: AXUIElementRef) -> Self {
        Self(element)
    }

    unsafe fn retain(element: AXUIElementRef) -> Self {
        // SAFETY: The caller supplies a live AXUIElement borrowed from another retained AX tree.
        unsafe { CFRetain(element.cast()) };
        Self(element)
    }

    pub(super) fn as_ref(&self) -> AXUIElementRef {
        self.0
    }
}

impl Clone for RetainedAxElement {
    fn clone(&self) -> Self {
        // SAFETY: `self.0` remains valid because this wrapper owns one retain count.
        unsafe { Self::retain(self.0) }
    }
}

impl Drop for RetainedAxElement {
    fn drop(&mut self) {
        // SAFETY: Every wrapper is created from an owned or explicitly retained AX reference.
        unsafe { CFRelease(self.0.cast()) };
    }
}

// SAFETY: AXUIElementRef is an immutable Core Foundation proxy. Retain/release are thread-safe,
// and the synchronous Accessibility calls do not expose Rust-owned interior state.
unsafe impl Send for RetainedAxElement {}
// SAFETY: Shared references only expose synchronous Accessibility operations on the immutable
// proxy; Core Foundation owns and synchronizes the remote element state.
unsafe impl Sync for RetainedAxElement {}

struct RetainedValue(CFTypeRef);

impl RetainedValue {
    fn as_ref(&self) -> CFTypeRef {
        self.0
    }
}

impl Drop for RetainedValue {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: Values in this wrapper come from a successful Copy/Create-rule function.
        unsafe { CFRelease(self.0) };
    }
}

#[derive(Clone, Debug)]
pub(super) struct ChromeSurface {
    pub(super) window: RetainedAxElement,
    pub(super) selected_tab: RetainedAxElement,
}

impl ChromeSurface {
    pub(super) fn capture(pid: i32) -> Option<Self> {
        // SAFETY: The pid is supplied by the caller from a running Chrome application.
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return None;
        }
        // SAFETY: AXUIElementCreateApplication follows the Create rule.
        let application = unsafe { RetainedAxElement::from_owned(application) };
        let window = copy_ax_element(application.as_ref(), "AXFocusedWindow")?;
        let mut visited = 0;
        let selected_tab = find_selected_tab(window.as_ref(), false, 0, &mut visited)?;
        Some(Self {
            window,
            selected_tab,
        })
    }

    pub(super) fn raise(&self, pid: i32) -> Result<bool, String> {
        // SAFETY: The pid is supplied by the caller from the Chrome process that owns the surface.
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return Ok(false);
        }
        // SAFETY: AXUIElementCreateApplication follows the Create rule.
        let application = unsafe { RetainedAxElement::from_owned(application) };
        let focused_window = owned_cf_string("AXFocusedWindow")?;
        let raise_action = owned_cf_string("AXRaise")?;
        // SAFETY: The retained window belongs to this Chrome process and both strings remain live.
        let _ = unsafe {
            AXUIElementSetAttributeValue(
                application.as_ref(),
                focused_window.as_ref().cast(),
                self.window.as_ref().cast(),
            )
        };
        // SAFETY: The retained element is a Chrome window and AXRaise is a standard AX action.
        let raised =
            unsafe { AXUIElementPerformAction(self.window.as_ref(), raise_action.as_ref().cast()) }
                == AX_SUCCESS;
        Ok(raised)
    }

    pub(super) fn select_tab(&self) -> Result<(), String> {
        let press_action = owned_cf_string("AXPress")?;
        // SAFETY: Capture accepts only a selected Chrome tab element retained by this surface.
        let pressed = unsafe {
            AXUIElementPerformAction(self.selected_tab.as_ref(), press_action.as_ref().cast())
        } == AX_SUCCESS;
        if pressed {
            Ok(())
        } else {
            Err("Запазеният Chrome таб вече не е наличен.".into())
        }
    }

    pub(super) fn is_stale(&self) -> bool {
        ax_element_is_invalid(self.window.as_ref())
            || ax_element_is_invalid(self.selected_tab.as_ref())
    }
}

fn find_selected_tab(
    element: AXUIElementRef,
    inside_tab_group: bool,
    depth: usize,
    visited: &mut usize,
) -> Option<RetainedAxElement> {
    if depth > MAX_TREE_DEPTH || *visited >= MAX_TREE_NODES {
        return None;
    }
    *visited += 1;

    let role = ax_string_attribute(element, "AXRole").unwrap_or_default();
    if !should_descend_into(&role) {
        return None;
    }
    let inside_tab_group = inside_tab_group || role == "AXTabGroup";
    if is_tab_candidate(&role, inside_tab_group) && selected_value(element) == Some(true) {
        // SAFETY: `element` is borrowed from a retained Accessibility tree during traversal.
        return Some(unsafe { RetainedAxElement::retain(element) });
    }

    let children = copy_attribute(element, "AXChildren")?;
    // SAFETY: AXChildren is documented as a CFArray of borrowed AXUIElement references.
    let count = unsafe { CFArrayGetCount(children.as_ref().cast::<c_void>() as CFArrayRef) };
    for index in 0..count {
        if *visited >= MAX_TREE_NODES {
            break;
        }
        // SAFETY: `index` is bounded by `count` and `children` remains retained for this loop.
        let child = unsafe {
            CFArrayGetValueAtIndex(children.as_ref().cast::<c_void>() as CFArrayRef, index)
        };
        if child.is_null() {
            continue;
        }
        if let Some(tab) = find_selected_tab(child.cast(), inside_tab_group, depth + 1, visited) {
            return Some(tab);
        }
    }
    None
}

fn should_descend_into(role: &str) -> bool {
    role != "AXWebArea"
}

fn is_tab_candidate(role: &str, inside_tab_group: bool) -> bool {
    inside_tab_group && matches!(role, "AXTab" | "AXRadioButton")
}

fn selected_value(element: AXUIElementRef) -> Option<bool> {
    classify_selected_values(["AXSelected", "AXValue"].map(|attribute| {
        copy_attribute(element, attribute).and_then(|value| typed_boolean(value.as_ref()))
    }))
}

fn classify_selected_values(values: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut observed = false;
    for value in values.into_iter().flatten() {
        observed = true;
        if value {
            return Some(true);
        }
    }
    observed.then_some(false)
}

fn typed_boolean(value: CFTypeRef) -> Option<bool> {
    if value.is_null() {
        return None;
    }
    // SAFETY: `value` is a live Core Foundation object copied from an AX attribute.
    let type_id = unsafe { CFGetTypeID(value) };
    // SAFETY: Type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFBooleanGetTypeID() } {
        // SAFETY: The type id above proves that `value` is a CFBoolean.
        return Some(unsafe { CFBooleanGetValue(value) } != 0);
    }
    // SAFETY: Type-id getters do not dereference caller-owned memory.
    if type_id == unsafe { CFNumberGetTypeID() } {
        let mut number = 0_i64;
        // SAFETY: The type id proves that `value` is a CFNumber and `number` has SInt64 layout.
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

fn copy_ax_element(element: AXUIElementRef, attribute: &str) -> Option<RetainedAxElement> {
    let value = copy_attribute(element, attribute)?;
    let raw = value.0.cast();
    std::mem::forget(value);
    // SAFETY: The retained Copy-rule value is transferred into the AX wrapper.
    Some(unsafe { RetainedAxElement::from_owned(raw) })
}

fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Option<RetainedValue> {
    let attribute = owned_cf_string(attribute).ok()?;
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: The AX element and attribute name are live; successful copies return retained values.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_ref().cast(), &raw mut value)
    };
    if result == AX_SUCCESS && !value.is_null() {
        return Some(RetainedValue(value));
    }
    if !value.is_null() {
        // SAFETY: A non-null value returned through the Copy-rule out parameter is retained even
        // when the operation reports an error.
        unsafe { CFRelease(value) };
    }
    None
}

fn ax_element_is_invalid(element: AXUIElementRef) -> bool {
    let Ok(attribute) = owned_cf_string("AXRole") else {
        return false;
    };
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: The retained AX element and attribute string are live for this synchronous probe.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_ref().cast(), &raw mut value)
    };
    if !value.is_null() {
        // SAFETY: A non-null Copy-rule out value must be released for every result code.
        unsafe { CFRelease(value) };
    }
    is_invalid_ui_element_error(result)
}

fn is_invalid_ui_element_error(result: i32) -> bool {
    result == AX_ERROR_INVALID_UI_ELEMENT
}

fn owned_cf_string(value: &str) -> Result<RetainedValue, String> {
    let value: CFStringRef = cf_string(value)?;
    Ok(RetainedValue(value.cast()))
}

#[cfg(test)]
mod tests {
    use super::{
        classify_selected_values, is_invalid_ui_element_error, ChromeSurface, RetainedValue,
    };
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    use std::ptr;

    #[test]
    fn null_retained_value_is_safe_to_drop() {
        drop(RetainedValue(ptr::null()));
    }

    #[test]
    fn any_explicit_selected_signal_wins_over_a_false_signal() {
        assert_eq!(
            classify_selected_values([Some(false), Some(true)]),
            Some(true)
        );
        assert_eq!(classify_selected_values([Some(false), None]), Some(false));
        assert_eq!(classify_selected_values([None, None]), None);
    }

    #[test]
    fn only_an_invalid_element_error_proves_that_a_surface_is_stale() {
        assert!(is_invalid_ui_element_error(-25202));
        assert!(!is_invalid_ui_element_error(-25204));
        assert!(!is_invalid_ui_element_error(-25205));
        assert!(!is_invalid_ui_element_error(0));
    }

    #[test]
    fn captures_current_chrome_surface_when_explicitly_requested() {
        if std::env::var_os("AIDOO_TEST_CHROME_SURFACE_CAPTURE").is_none() {
            return;
        }
        if !crate::accessibility_granted() {
            eprintln!("Chrome surface capture observation skipped: accessibility=false");
            return;
        }
        let identifier = NSString::from_str("com.google.Chrome");
        let captured = NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier)
            .iter()
            .any(|application| ChromeSurface::capture(application.processIdentifier()).is_some());
        eprintln!("Chrome surface capture succeeded: {captured}");
        assert!(
            captured,
            "Chrome exposed no exact selected tab through Accessibility"
        );
    }
}
