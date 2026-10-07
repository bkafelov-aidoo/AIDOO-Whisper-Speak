#[cfg(target_os = "macos")]
mod platform {
    use core_graphics::event::{CGEvent, CGEventFlags, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListOptionAll,
        kCGWindowListOptionExcludeDesktopElements, kCGWindowName, kCGWindowNumber,
        kCGWindowOwnerPID,
    };
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    use std::collections::HashSet;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::ptr;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    mod surface {
        include!("browser_surface.rs");
    }
    use surface::ChromeSurface;
    include!("browser_treatment.rs");
    mod routing {
        include!("browser_routing.rs");
    }
    use routing::{
        live_address_action, may_open_dedicated_window, navigation_action, patient_id_from_url,
        recovery_candidates,
    };

    type AXUIElementRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFNumberRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;

    const AX_SUCCESS: i32 = 0;
    const CF_NUMBER_SINT64_TYPE: i64 = 4;
    const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const RETURN_KEY: u16 = 0x24;
    const ESCAPE_KEY: u16 = 0x35;
    const L_KEY: u16 = 0x25;
    const T_KEY: u16 = 0x11;
    const NAVIGATION_ATTEMPTS: usize = 3;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ChromeWindow {
        pid: i32,
        number: u32,
    }

    #[derive(Clone)]
    struct ManagedTarget {
        window: ChromeWindow,
        patient_id: Option<String>,
        surface: Option<ChromeSurface>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum NavigationAction {
        RefreshCurrentTab,
        OpenNewTab,
    }

    static PRESENTATION_LOCK: Mutex<()> = Mutex::new(());
    static MANAGED_TARGET: Mutex<Option<ManagedTarget>> = Mutex::new(None);
    static MANAGED_PATIENTS: Mutex<Vec<ManagedTarget>> = Mutex::new(Vec::new());

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> i32;
        fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: CFArrayRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> *const c_void;
        fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(number: CFNumberRef, number_type: i64, value: *mut c_void) -> bool;
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            value: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetCString(
            string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        fn CFStringGetLength(string: CFStringRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFRelease(value: CFTypeRef);
        fn CFEqual(first: CFTypeRef, second: CFTypeRef) -> bool;
    }

    pub fn present(url: &str) -> Result<(), String> {
        let _guard = PRESENTATION_LOCK
            .lock()
            .map_err(|_| "Chrome синхронизацията е заключена.".to_string())?;
        if !valid_aidoo_url(url) {
            return Err("Отказана е навигация извън AIDOO.".into());
        }

        let patient_id = patient_id_from_url(url);
        discard_stale_targets();
        if let Some(target) = managed_patient_target(patient_id.as_deref()) {
            navigate_managed_target(&target, url)?;
            return complete_presentation(target.window, patient_id, url);
        }

        // A failed navigation must not turn a known patient into a new patient on the next
        // command. Keep the binding and report the failure instead of opening another tab.
        if let Some(target) = managed_target() {
            match navigation_action(target.patient_id.as_deref(), patient_id.as_deref()) {
                NavigationAction::RefreshCurrentTab => navigate_managed_target(&target, url)?,
                NavigationAction::OpenNewTab => open_new_tab(target.window, url)?,
            }
            return complete_presentation(target.window, patient_id, url);
        }

        let mut last_recovery_error = None;
        let aidoo_windows = existing_aidoo_windows();
        let chrome_windows = chrome_windows();
        let had_existing_aidoo_surface = !aidoo_windows.is_empty() || !chrome_windows.is_empty();
        for (window, _) in recovery_candidates(
            None,
            None,
            patient_id.as_deref(),
            &aidoo_windows,
            &chrome_windows,
        ) {
            let result = open_new_tab(window, url);
            match result {
                Ok(()) => {
                    return complete_presentation(window, patient_id, url);
                }
                Err(error) => {
                    // Cmd-T may already have created the one allowed patient tab. Its binding
                    // is recorded before URL entry, so a failed confirmation cannot create more.
                    if managed_target().is_some() {
                        return Err(error);
                    }
                    last_recovery_error = Some(error);
                }
            }
        }

        if !may_open_dedicated_window(had_existing_aidoo_surface) {
            let detail = last_recovery_error
                .as_ref()
                .map(|error| format!(" {error}"))
                .unwrap_or_default();
            return Err(format!(
                "Съществуващият AIDOO прозорец не можа да бъде опреснен.{detail} Нов прозорец няма да бъде отворен автоматично."
            ));
        }

        let window = open_dedicated_window(url).map_err(|error| match last_recovery_error {
            Some(previous) => format!(
                "{error} Последният намерен Chrome прозорец също отказа опресняване: {previous}"
            ),
            None => error,
        })?;
        activate_window(window)?;
        confirm_current_target(window, url)?;
        complete_presentation(window, patient_id, url)
    }

    pub fn forget_managed_window() {
        if let Ok(mut current) = MANAGED_TARGET.lock() {
            *current = None;
        }
        if let Ok(mut patients) = MANAGED_PATIENTS.lock() {
            patients.clear();
        }
    }

    fn valid_aidoo_url(url: &str) -> bool {
        [
            ["https:", "//app.aidoo.bg/clinics/"].concat(),
            ["https:", "//aidoo-web.on.dev-craft.tech/clinics/"].concat(),
        ]
        .iter()
        .any(|prefix| url.starts_with(prefix))
    }

    fn managed_target() -> Option<ManagedTarget> {
        MANAGED_TARGET
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    fn remember_target(window: ChromeWindow, patient_id: Option<String>) {
        let previous = managed_patient_target(patient_id.as_deref())
            .or_else(managed_target)
            .filter(|target| {
                target.window == window && (patient_id.is_none() || target.patient_id == patient_id)
            });
        let target = ManagedTarget {
            window,
            patient_id,
            surface: ChromeSurface::capture(window.pid)
                .or_else(|| previous.and_then(|target| target.surface)),
        };
        if target.patient_id.is_some() {
            if let Ok(mut patients) = MANAGED_PATIENTS.lock() {
                patients.retain(|previous| previous.patient_id != target.patient_id);
                patients.push(target.clone());
            }
        }
        if let Ok(mut current) = MANAGED_TARGET.lock() {
            *current = Some(target);
        }
    }

    fn managed_patient_target(patient_id: Option<&str>) -> Option<ManagedTarget> {
        let patient_id = patient_id?;
        MANAGED_PATIENTS
            .lock()
            .ok()?
            .iter()
            .find(|target| target.patient_id.as_deref() == Some(patient_id))
            .cloned()
    }

    fn discard_stale_targets() {
        let is_stale = |target: &ManagedTarget| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(target.window.pid)
                .is_none()
                || target.surface.as_ref().is_some_and(ChromeSurface::is_stale)
        };
        if let Ok(mut patients) = MANAGED_PATIENTS.lock() {
            patients.retain(|target| !is_stale(target));
        }
        if let Ok(mut current) = MANAGED_TARGET.lock() {
            if current.as_ref().is_some_and(is_stale) {
                *current = None;
            }
        }
    }

    fn navigate_managed_target(target: &ManagedTarget, url: &str) -> Result<(), String> {
        if let Ok(mut current) = MANAGED_TARGET.lock() {
            *current = Some(target.clone());
        }
        activate_window(target.window)?;
        if let Some(surface) = &target.surface {
            surface.select_tab()?;
        } else {
            let observed = read_address_bar(target.window.pid)?;
            if live_address_action(url, &observed) != NavigationAction::RefreshCurrentTab {
                return Err(
                    "Запазеният AIDOO таб не може да бъде избран. Нов таб няма да бъде отворен."
                        .into(),
                );
            }
        }
        crate::storage::append_diagnostic("AIDOO Chrome routing: reuse registered patient tab.");
        navigate_window(target.window, url)
    }

    fn ax_window_index(target_number: u32, candidates: &[(Option<i64>, String)]) -> Option<usize> {
        candidates
            .iter()
            .position(|(number, _)| *number == Some(i64::from(target_number)))
            .or_else(|| {
                candidates
                    .iter()
                    .position(|(_, title)| title.to_ascii_lowercase().contains("aidoo"))
            })
    }

    fn open_dedicated_window(url: &str) -> Result<ChromeWindow, String> {
        let before = chrome_windows()
            .into_iter()
            .map(|window| window.number)
            .collect::<HashSet<_>>();
        let executable = chrome_executable()
            .ok_or_else(|| "Google Chrome не е намерен в Applications.".to_string())?;
        Command::new(executable)
            .arg("--new-window")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Google Chrome не можа да бъде стартиран: {error}"))?;

        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline {
            let windows = chrome_windows();
            if let Some(window) = opened_window_candidate(&before, &windows) {
                return Ok(window);
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("Новият AIDOO прозорец в Chrome не беше открит.".into())
    }

    fn opened_window_candidate(
        before: &HashSet<u32>,
        windows: &[ChromeWindow],
    ) -> Option<ChromeWindow> {
        windows
            .iter()
            .find(|window| !before.contains(&window.number))
            .copied()
    }

    fn navigate_window(window: ChromeWindow, url: &str) -> Result<(), String> {
        enter_url_and_confirm(window, url)
    }

    fn open_new_tab(window: ChromeWindow, url: &str) -> Result<(), String> {
        activate_window(window)?;
        let observed = read_address_bar(window.pid)?;
        if live_address_action(url, &observed) == NavigationAction::OpenNewTab {
            crate::storage::append_diagnostic("AIDOO Chrome routing: create one new patient tab.");
            press_command_shortcut(window.pid, T_KEY)?;
            thread::sleep(Duration::from_millis(220));
        } else {
            crate::storage::append_diagnostic(
                "AIDOO Chrome routing: reuse patient tab from live address.",
            );
        }
        // Bind even before confirmation: a timeout after Cmd-T must retry this tab, not Cmd-T.
        remember_target(window, patient_id_from_url(url));
        enter_url_and_confirm(window, url)
    }

    fn enter_url_and_confirm(window: ChromeWindow, url: &str) -> Result<(), String> {
        let mut last_observed = String::new();
        for attempt in 0..NAVIGATION_ATTEMPTS {
            activate_window(window)?;
            focus_address_bar(window.pid)?;
            thread::sleep(Duration::from_millis(120 + attempt as u64 * 80));
            type_text(window.pid, url)?;
            thread::sleep(Duration::from_millis(70));
            press_key(window.pid, RETURN_KEY)?;
            thread::sleep(Duration::from_millis(250 + attempt as u64 * 180));

            match read_address_bar(window.pid) {
                Ok(observed) if address_matches_target(url, &observed) => return Ok(()),
                Ok(_) => last_observed = "Chrome остана на друг AIDOO екран.".into(),
                Err(error) => last_observed = error,
            }
        }
        let detail = if last_observed.is_empty() {
            String::new()
        } else {
            format!(" Последно състояние: {last_observed}")
        };
        Err(format!(
            "Chrome не потвърди навигацията до точния AIDOO екран след {NAVIGATION_ATTEMPTS} опита.{detail}"
        ))
    }

    fn read_address_bar(pid: i32) -> Result<String, String> {
        focus_address_bar(pid)?;
        thread::sleep(Duration::from_millis(120));
        let result = focused_text_value(pid);
        let _ = press_key(pid, ESCAPE_KEY);
        result
    }

    fn focused_text_value(pid: i32) -> Result<String, String> {
        // SAFETY: The pid belongs to the Chrome application targeted by the caller.
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return Err("Chrome Accessibility елементът не е наличен.".into());
        }
        let focused_attribute = cf_string("AXFocusedUIElement")?;
        let value_attribute = cf_string("AXValue")?;
        let mut focused_element: CFTypeRef = ptr::null();
        let mut value: CFTypeRef = ptr::null();

        // SAFETY: The application and retained attribute names are valid Core Foundation objects.
        let focused_result = unsafe {
            AXUIElementCopyAttributeValue(application, focused_attribute, &raw mut focused_element)
        };
        let value_result = if focused_result == AX_SUCCESS && !focused_element.is_null() {
            // SAFETY: A successful AXFocusedUIElement read returns an AXUIElement.
            unsafe {
                AXUIElementCopyAttributeValue(
                    focused_element.cast(),
                    value_attribute,
                    &raw mut value,
                )
            }
        } else {
            -1
        };
        let text = if value_result == AX_SUCCESS && !value.is_null() {
            cf_string_value(value.cast())
        } else {
            None
        };

        // SAFETY: Each non-null object was created or returned retained above.
        unsafe {
            if !value.is_null() {
                CFRelease(value);
            }
            if !focused_element.is_null() {
                CFRelease(focused_element);
            }
            CFRelease(focused_attribute);
            CFRelease(value_attribute);
            CFRelease(application);
        }
        text.filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Chrome не предостави адреса на активния таб чрез Accessibility.".into())
    }

    fn ax_string_attribute(element: AXUIElementRef, name: &str) -> Option<String> {
        let attribute = cf_string(name).ok()?;
        let mut value: CFTypeRef = ptr::null();
        // SAFETY: `element` is an AXUIElement and the output receives a retained CF object.
        let copied = unsafe { AXUIElementCopyAttributeValue(element, attribute, &raw mut value) }
            == AX_SUCCESS;
        let result = if copied && !value.is_null() {
            cf_string_value(value.cast())
        } else {
            None
        };
        // SAFETY: `attribute` was created retained and a successful AX copy returns `value`
        // retained as well.
        unsafe {
            if !value.is_null() {
                CFRelease(value);
            }
            CFRelease(attribute);
        }
        result
    }

    fn is_official_note_field(role: &str, labels: &[Option<&str>]) -> bool {
        matches!(role, "AXTextArea" | "AXTextField")
            && labels.iter().flatten().any(|label| {
                matches!(
                    label.trim().to_lowercase().as_str(),
                    "забележка" | "официална забележка"
                )
            })
    }

    fn address_matches_target(expected: &str, observed: &str) -> bool {
        route_identity(expected).is_some_and(|expected| {
            route_identity(observed).is_some_and(|observed| observed == expected)
        })
    }

    fn route_identity(url: &str) -> Option<String> {
        let normalized = url.trim().strip_prefix("https://").unwrap_or(url.trim());
        let (base, query) = normalized.split_once('?')?;
        let base = base.trim_end_matches('/').to_ascii_lowercase();
        let patient_id = query_value(query, "patientid");
        let keys: &[&str] = if patient_id.is_some() {
            &["patientid", "tab", "mode"]
        } else {
            &["mode", "active-date", "selected-doctors"]
        };
        let values = keys
            .iter()
            .map(|key| query_value(query, key).map(|value| format!("{key}={value}")))
            .collect::<Option<Vec<_>>>()?;
        Some(format!("{base}?{}", values.join("&")))
    }

    fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
        query.split('&').find_map(|parameter| {
            let (key, value) = parameter.split_once('=')?;
            (key.eq_ignore_ascii_case(name) && !value.is_empty()).then_some(value)
        })
    }

    fn activate_window(window: ChromeWindow) -> Result<(), String> {
        if !crate::accessibility_granted() {
            return Err("Accessibility разрешението липсва.".into());
        }
        if !raise_window(window)? {
            return Err("AIDOO прозорецът в Chrome вече е затворен.".into());
        }

        if let Some(application) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
        {
            application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        }
        thread::sleep(Duration::from_millis(100));
        let _ = raise_window(window);
        Ok(())
    }

    fn focus_address_bar(pid: i32) -> Result<(), String> {
        press_command_shortcut(pid, L_KEY)
    }

    fn press_command_shortcut(pid: i32, keycode: u16) -> Result<(), String> {
        let source = event_source()?;
        let command_down = key_event(source.clone(), KeyCode::COMMAND, true)?;
        let key_down = key_event(source.clone(), keycode, true)?;
        let key_up = key_event(source.clone(), keycode, false)?;
        let command_up = key_event(source, KeyCode::COMMAND, false)?;

        command_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_up.set_flags(CGEventFlags::CGEventFlagCommand);
        command_up.set_flags(CGEventFlags::CGEventFlagNull);
        for event in [&command_down, &key_down, &key_up, &command_up] {
            event.post_to_pid(pid);
            thread::sleep(Duration::from_millis(8));
        }
        Ok(())
    }

    fn type_text(pid: i32, text: &str) -> Result<(), String> {
        let source = event_source()?;
        let down = key_event(source.clone(), 0, true)?;
        let up = key_event(source, 0, false)?;
        down.set_string(text);
        up.set_string(text);
        down.post_to_pid(pid);
        up.post_to_pid(pid);
        Ok(())
    }

    fn press_key(pid: i32, keycode: u16) -> Result<(), String> {
        let source = event_source()?;
        let down = key_event(source.clone(), keycode, true)?;
        let up = key_event(source, keycode, false)?;
        down.post_to_pid(pid);
        thread::sleep(Duration::from_millis(8));
        up.post_to_pid(pid);
        Ok(())
    }

    fn event_source() -> Result<CGEventSource, String> {
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| "macOS не успя да създаде браузърно събитие.".to_string())
    }

    fn key_event(source: CGEventSource, keycode: u16, down: bool) -> Result<CGEvent, String> {
        CGEvent::new_keyboard_event(source, keycode, down)
            .map_err(|_| "macOS не успя да създаде браузърно събитие.".to_string())
    }

    fn chrome_executable() -> Option<PathBuf> {
        let system = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if system.is_file() {
            return Some(system);
        }
        dirs::home_dir()
            .map(|home| home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"))
            .filter(|path| path.is_file())
    }

    fn existing_aidoo_windows() -> Vec<ChromeWindow> {
        chrome_window_infos()
            .into_iter()
            .filter(|(_, title)| title.to_lowercase().contains("aidoo"))
            .map(|(window, _)| window)
            .collect()
    }

    fn chrome_windows() -> Vec<ChromeWindow> {
        chrome_window_infos()
            .into_iter()
            .map(|(window, _)| window)
            .collect()
    }

    fn chrome_window_infos() -> Vec<(ChromeWindow, String)> {
        let Some(info) = copy_window_info(
            kCGWindowListOptionAll | kCGWindowListOptionExcludeDesktopElements,
            kCGNullWindowID,
        ) else {
            return Vec::new();
        };
        let chrome_pids = chrome_pids();
        let mut windows = Vec::new();
        // SAFETY: CoreGraphics exports these immutable CFString key pointers for process-wide use.
        let owner_pid_key = unsafe { kCGWindowOwnerPID }.cast();
        // SAFETY: CoreGraphics exports these immutable CFString key pointers for process-wide use.
        let layer_key = unsafe { kCGWindowLayer }.cast();
        // SAFETY: CoreGraphics exports these immutable CFString key pointers for process-wide use.
        let number_key = unsafe { kCGWindowNumber }.cast();
        // SAFETY: CoreGraphics exports these immutable CFString key pointers for process-wide use.
        let name_key = unsafe { kCGWindowName }.cast();
        for raw in info.get_all_values() {
            let dictionary = raw.cast::<c_void>();
            let Some(pid) = dictionary_i64(dictionary, owner_pid_key) else {
                continue;
            };
            let Some(layer) = dictionary_i64(dictionary, layer_key) else {
                continue;
            };
            if layer != 0 || !chrome_pids.contains(&(pid as i32)) {
                continue;
            }
            if let Some(number) = dictionary_i64(dictionary, number_key) {
                let title = dictionary_string(dictionary, name_key).unwrap_or_default();
                windows.push((
                    ChromeWindow {
                        pid: pid as i32,
                        number: number as u32,
                    },
                    title,
                ));
            }
        }
        windows
    }

    fn chrome_pids() -> HashSet<i32> {
        let identifier = objc2_foundation::NSString::from_str("com.google.Chrome");
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier)
            .iter()
            .map(|application| application.processIdentifier())
            .collect()
    }

    fn dictionary_i64(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<i64> {
        // SAFETY: CGWindowListCopyWindowInfo returns CFDictionary values and the public
        // kCGWindow* keys used here point to CFNumber values for these three attributes.
        let number = unsafe { CFDictionaryGetValue(dictionary, key) };
        if number.is_null() {
            return None;
        }
        let mut value = 0_i64;
        // SAFETY: `value` has the SInt64 layout requested from CFNumberGetValue.
        unsafe {
            CFNumberGetValue(
                number.cast(),
                CF_NUMBER_SINT64_TYPE,
                (&raw mut value).cast(),
            )
        }
        .then_some(value)
    }

    fn dictionary_string(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<String> {
        // SAFETY: CGWindowListCopyWindowInfo returns a CFString for kCGWindowName.
        let string = unsafe { CFDictionaryGetValue(dictionary, key) };
        if string.is_null() {
            return None;
        }
        cf_string_value(string.cast())
    }

    fn cf_string_value(string: CFStringRef) -> Option<String> {
        // SAFETY: `string` is a valid CFString borrowed from the window dictionary.
        let length = unsafe { CFStringGetLength(string) };
        // SAFETY: The Core Foundation sizing function accepts the length read above.
        let capacity =
            unsafe { CFStringGetMaximumSizeForEncoding(length, CF_STRING_ENCODING_UTF8) }
                .checked_add(1)?;
        let mut buffer = vec![0_u8; usize::try_from(capacity).ok()?];
        // SAFETY: The buffer is writable for `capacity` bytes and the encoding is UTF-8.
        if !unsafe {
            CFStringGetCString(
                string,
                buffer.as_mut_ptr().cast(),
                capacity,
                CF_STRING_ENCODING_UTF8,
            )
        } {
            return None;
        }
        // SAFETY: CFStringGetCString writes a trailing NUL on success.
        Some(
            unsafe { CStr::from_ptr(buffer.as_ptr().cast()) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn raise_window(target: ChromeWindow) -> Result<bool, String> {
        if let Some(surface) = managed_target()
            .filter(|managed| managed.window == target)
            .and_then(|managed| managed.surface)
        {
            return surface.raise(target.pid);
        }
        // SAFETY: The process id belongs to a currently running Chrome instance.
        let application = unsafe { AXUIElementCreateApplication(target.pid) };
        if application.is_null() {
            return Ok(false);
        }
        let windows_attribute = cf_string("AXWindows")?;
        let number_attribute = cf_string("AXWindowNumber")?;
        let focused_window_attribute = cf_string("AXFocusedWindow")?;
        let raise_action = cf_string("AXRaise")?;
        let mut windows: CFTypeRef = ptr::null();

        // SAFETY: The AX application and retained CFString references are valid for this call.
        let copied = unsafe {
            AXUIElementCopyAttributeValue(application, windows_attribute, &raw mut windows)
        } == AX_SUCCESS;
        let mut found = false;
        if copied && !windows.is_null() {
            // SAFETY: A successful AXWindows read returns a CFArray of AXUIElement references.
            let count = unsafe { CFArrayGetCount(windows.cast()) };
            let mut candidates = Vec::new();
            for index in 0..count {
                // SAFETY: `index` is bounded by the CFArray count.
                let window = unsafe { CFArrayGetValueAtIndex(windows.cast(), index) };
                candidates.push((
                    ax_number(window, number_attribute),
                    ax_string_attribute(window.cast(), "AXTitle").unwrap_or_default(),
                ));
            }
            let mut focused_window: CFTypeRef = ptr::null();
            // SAFETY: The output receives the retained focused AX window of this Chrome process.
            let has_focused = unsafe {
                AXUIElementCopyAttributeValue(
                    application,
                    focused_window_attribute,
                    &raw mut focused_window,
                )
            } == AX_SUCCESS;
            let index = ax_window_index(target.number, &candidates).or_else(|| {
                if !has_focused || focused_window.is_null() {
                    return None;
                }
                (0..count).position(|index| {
                    // SAFETY: Both references are live AX elements from this same application.
                    unsafe {
                        CFEqual(
                            CFArrayGetValueAtIndex(windows.cast(), index),
                            focused_window,
                        )
                    }
                })
            });
            if !focused_window.is_null() {
                // SAFETY: AXUIElementCopyAttributeValue returned this value retained.
                unsafe { CFRelease(focused_window) };
            }
            if let Some(index) = index {
                let index = isize::try_from(index)
                    .map_err(|_| "Chrome прозорецът има невалиден индекс.".to_string())?;
                // SAFETY: `index` comes from the candidate vector built from this same array.
                let window = unsafe { CFArrayGetValueAtIndex(windows.cast(), index) };
                // SAFETY: The selected element belongs to this application and supports AXRaise.
                let raised = unsafe {
                    AXUIElementSetAttributeValue(
                        application,
                        focused_window_attribute,
                        window.cast(),
                    );
                    AXUIElementPerformAction(window.cast(), raise_action)
                } == AX_SUCCESS;
                found = raised;
            }
        }

        // SAFETY: Each non-null Core Foundation object was returned retained or created here.
        unsafe {
            if !windows.is_null() {
                CFRelease(windows);
            }
            CFRelease(windows_attribute);
            CFRelease(number_attribute);
            CFRelease(focused_window_attribute);
            CFRelease(raise_action);
            CFRelease(application);
        }
        Ok(found)
    }

    fn ax_number(element: *const c_void, attribute: CFStringRef) -> Option<i64> {
        let mut number: CFTypeRef = ptr::null();
        // SAFETY: `element` comes from AXWindows and the output receives a retained CF object.
        if unsafe { AXUIElementCopyAttributeValue(element.cast(), attribute, &raw mut number) }
            != AX_SUCCESS
            || number.is_null()
        {
            return None;
        }
        let mut value = 0_i64;
        // SAFETY: `number` is the CFNumber returned by AXWindowNumber.
        let converted = unsafe {
            CFNumberGetValue(
                number.cast(),
                CF_NUMBER_SINT64_TYPE,
                (&raw mut value).cast(),
            )
        };
        // SAFETY: AXUIElementCopyAttributeValue returned `number` retained.
        unsafe { CFRelease(number) };
        converted.then_some(value)
    }

    fn cf_string(value: &str) -> Result<CFStringRef, String> {
        let value = CString::new(value).map_err(|_| "Невалиден macOS атрибут.".to_string())?;
        // SAFETY: The CString is NUL-terminated and valid UTF-8 for the duration of the call.
        let string = unsafe {
            CFStringCreateWithCString(ptr::null(), value.as_ptr(), CF_STRING_ENCODING_UTF8)
        };
        if string.is_null() {
            Err("macOS не успя да създаде Accessibility атрибут.".into())
        } else {
            Ok(string)
        }
    }

    #[cfg(test)]
    mod tests {
        include!("browser_tests.rs");
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn present(_url: &str) -> Result<(), String> {
        Err("Chrome presentation is available only on macOS.".into())
    }

    pub fn forget_managed_window() {}

    pub fn preview_official_note(_text: &str, _patient_id: &str) -> Result<bool, String> {
        Ok(false)
    }
}

pub use platform::{forget_managed_window, present, preview_official_note};
