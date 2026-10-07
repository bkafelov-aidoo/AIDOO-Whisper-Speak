use super::browser;
use super::clinic::parse_clinic_reference;
use crate::AppState;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const ALL_TREATMENT_TEETH: [&str; 32] = [
    "18", "17", "16", "15", "14", "13", "12", "11", "21", "22", "23", "24", "25", "26", "27", "28",
    "48", "47", "46", "45", "44", "43", "42", "41", "31", "32", "33", "34", "35", "36", "37", "38",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatientView {
    Status,
    Treatment,
}

impl PatientView {
    fn mode(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Treatment => "treatment",
        }
    }
}

pub fn present_patient(
    app: &AppHandle,
    state: &AppState,
    patient_id: String,
    view: PatientView,
) -> Result<(), String> {
    let clinic_link = clinic_link(state)?;
    let target = match patient_view_url(&clinic_link, &patient_id, view, sync_nonce()) {
        Ok(target) => target,
        Err(error) => {
            let _ = app.emit("toast", error.clone());
            return Err(error);
        }
    };
    present_target(
        app,
        target,
        "AIDOO промяната е запазена, но пациентският екран в Chrome не можа да бъде показан.",
    )
}

pub fn present_treatment_tooth(
    app: &AppHandle,
    state: &AppState,
    patient_id: String,
    tooth: String,
) -> Result<(), String> {
    let clinic_link = clinic_link(state)?;
    let target = match treatment_tooth_view_url(&clinic_link, &patient_id, &tooth, sync_nonce()) {
        Ok(target) => target,
        Err(error) => {
            let _ = app.emit("toast", error.clone());
            return Err(error);
        }
    };
    present_target(
        app,
        target,
        "Зъбът не можа да бъде показан в Treatment екрана на AIDOO.",
    )
}

pub fn present_schedule(
    app: &AppHandle,
    state: &AppState,
    date: String,
    doctor_id: String,
) -> Result<(), String> {
    let clinic_link = clinic_link(state)?;
    let target = match schedule_view_url(&clinic_link, &date, &doctor_id, sync_nonce()) {
        Ok(target) => target,
        Err(error) => {
            let _ = app.emit("toast", error.clone());
            return Err(error);
        }
    };
    present_target(
        app,
        target,
        "Графикът е обработен, но страницата му в Chrome не можа да бъде показана.",
    )
}

pub fn close() {
    browser::forget_managed_window();
}

fn clinic_link(state: &AppState) -> Result<String, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Настройките са заключени.".to_string())?;
    if !settings.aidoo_browser_sync_enabled {
        return Err("Браузърната синхронизация с AIDOO е изключена.".into());
    }
    settings
        .aidoo_clinic_url
        .clone()
        .or_else(|| settings.aidoo_clinic_slug.clone())
        .ok_or_else(|| "Липсва линк към AIDOO клиниката.".to_string())
}

fn present_target(
    app: &AppHandle,
    target: PatientViewTarget,
    failure_message: &'static str,
) -> Result<(), String> {
    match browser::present(&target.url) {
        Ok(()) => {
            crate::storage::append_diagnostic("AIDOO Chrome presentation confirmed.");
            Ok(())
        }
        Err(error) => {
            crate::storage::append_diagnostic(&format!(
                "AIDOO Chrome presentation failed: {error}"
            ));
            let _ = app.emit("toast", format!("{failure_message} {error}"));
            Err(error)
        }
    }
}

struct PatientViewTarget {
    url: String,
}

fn patient_view_url(
    clinic_link: &str,
    patient_id: &str,
    view: PatientView,
    nonce: u128,
) -> Result<PatientViewTarget, String> {
    patient_view_url_with_selected_teeth(clinic_link, patient_id, view, "", nonce)
}

fn treatment_tooth_view_url(
    clinic_link: &str,
    patient_id: &str,
    tooth: &str,
    nonce: u128,
) -> Result<PatientViewTarget, String> {
    let selected_teeth = treatment_selected_teeth(tooth)?;
    patient_view_url_with_selected_teeth(
        clinic_link,
        patient_id,
        PatientView::Treatment,
        &selected_teeth,
        nonce,
    )
}

fn patient_view_url_with_selected_teeth(
    clinic_link: &str,
    patient_id: &str,
    view: PatientView,
    selected_teeth: &str,
    nonce: u128,
) -> Result<PatientViewTarget, String> {
    if patient_id.is_empty()
        || patient_id.len() > 128
        || !patient_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(
            "Пациентският екран не може да бъде показан за невалиден идентификатор.".into(),
        );
    }
    let clinic = parse_clinic_reference(clinic_link)?;
    let record_base = clinic.url.strip_suffix("/login").ok_or_else(|| {
        "Линкът към AIDOO клиниката не съдържа валиден входен маршрут.".to_string()
    })?;
    Ok(PatientViewTarget {
        url: format!(
            "{record_base}/medical-record?patientid={patient_id}&tab=record&mode={}&selectedTeeth={selected_teeth}&triggerNzokChecksProp=true&aidooControlSync={nonce}",
            view.mode(),
        ),
    })
}

fn treatment_selected_teeth(tooth: &str) -> Result<String, String> {
    let tooth = tooth.trim();
    if tooth == "*" {
        return Ok(ALL_TREATMENT_TEETH.join(","));
    }
    if tooth.len() != 2 || !tooth.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Невалиден номер на зъб за Treatment екрана.".into());
    }
    let bytes = tooth.as_bytes();
    let quadrant = bytes[0] - b'0';
    let position = bytes[1] - b'0';
    if (1..=4).contains(&quadrant) && (1..=8).contains(&position) {
        return Ok(tooth.to_string());
    }
    if (5..=8).contains(&quadrant) && (1..=5).contains(&position) {
        return Ok(format!("{}{}", quadrant - 4, position));
    }
    Err("Невалиден номер на зъб за Treatment екрана.".into())
}

fn schedule_view_url(
    clinic_link: &str,
    date: &str,
    doctor_id: &str,
    nonce: u128,
) -> Result<PatientViewTarget, String> {
    if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
        || doctor_id.is_empty()
        || doctor_id.len() > 128
        || !doctor_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Графикът не може да бъде показан за невалидна дата или лекар.".into());
    }
    let clinic = parse_clinic_reference(clinic_link)?;
    let record_base = clinic.url.strip_suffix("/login").ok_or_else(|| {
        "Линкът към AIDOO клиниката не съдържа валиден входен маршрут.".to_string()
    })?;
    Ok(PatientViewTarget {
        url: format!(
            "{record_base}/schedule?mode=doctors&active-date={date}&selected-doctors=%5B%22{doctor_id}%22%5D&aidooControlSync={nonce}"
        ),
    })
}

fn sync_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_observed_status_and_treatment_routes() {
        let scheme = ["https:", "//"].concat();
        let clinic = format!("{scheme}aidoo-web.on.dev-craft.tech/clinics/demo/login");
        let status = patient_view_url(&clinic, "patient-id", PatientView::Status, 42).unwrap();
        let treatment =
            patient_view_url(&clinic, "patient-id", PatientView::Treatment, 43).unwrap();

        assert_eq!(
            status.url,
            format!(
                "{scheme}aidoo-web.on.dev-craft.tech/clinics/demo/medical-record?patientid=patient-id&tab=record&mode=status&selectedTeeth=&triggerNzokChecksProp=true&aidooControlSync=42"
            )
        );
        assert!(treatment.url.contains("mode=treatment"));
        assert!(treatment.url.ends_with("aidooControlSync=43"));
    }

    #[test]
    fn treatment_route_can_present_requested_tooth() {
        let scheme = ["https:", "//"].concat();
        let clinic = format!("{scheme}app.aidoo.bg/clinics/demo/login");
        let treatment = treatment_tooth_view_url(&clinic, "patient-id", "26", 43).unwrap();

        assert!(treatment.url.contains("&selectedTeeth=26&"));
    }

    #[test]
    fn milk_teeth_use_the_deployed_permanent_body_key_without_clicking_the_number() {
        assert_eq!(treatment_selected_teeth("51").unwrap(), "11");
        assert_eq!(treatment_selected_teeth("65").unwrap(), "25");
        assert_eq!(treatment_selected_teeth("75").unwrap(), "35");
        assert_eq!(treatment_selected_teeth("85").unwrap(), "45");
    }

    #[test]
    fn general_treatment_expands_to_the_exact_deployed_chart_order() {
        assert_eq!(
            treatment_selected_teeth("*").unwrap(),
            ALL_TREATMENT_TEETH.join(",")
        );
    }

    #[test]
    fn refuses_invalid_permanent_and_milk_tooth_numbers() {
        for tooth in ["", "0", "19", "50", "56", "86", "99", "2&mode=status"] {
            assert!(treatment_selected_teeth(tooth).is_err(), "accepted {tooth}");
        }
    }

    #[test]
    fn refuses_patient_ids_that_could_escape_the_query_value() {
        let clinic = ["https:", "//app.aidoo.bg/clinics/demo/login"].concat();
        assert!(
            patient_view_url(&clinic, "patient&mode=treatment", PatientView::Status, 1).is_err()
        );
    }

    #[test]
    fn builds_the_observed_schedule_route_for_the_exact_date_and_doctor() {
        let scheme = ["https:", "//"].concat();
        let clinic = format!("{scheme}app.aidoo.bg/clinics/demo/login");
        let target = schedule_view_url(&clinic, "2026-09-21", "doctor-id", 44).unwrap();
        assert_eq!(
            target.url,
            format!("{scheme}app.aidoo.bg/clinics/demo/schedule?mode=doctors&active-date=2026-09-21&selected-doctors=%5B%22doctor-id%22%5D&aidooControlSync=44")
        );
    }

    #[test]
    fn refuses_schedule_values_that_can_escape_the_query() {
        let clinic = ["https:", "//app.aidoo.bg/clinics/demo/login"].concat();
        assert!(schedule_view_url(&clinic, "2026-09-21&mode=x", "doctor-id", 1).is_err());
        assert!(schedule_view_url(&clinic, "2026-09-21", "doctor&id", 1).is_err());
    }
}
