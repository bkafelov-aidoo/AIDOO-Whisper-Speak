use super::{
    commands::show_patient_view,
    dental::spoken_tooth,
    draft,
    presentation::{self, PatientView},
    treatment, treatment_diagnostics, types, workflow,
};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn aidoo_select_patient(
    patient_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::PatientSummary, String> {
    let patient = state.aidoo.select_patient(&patient_id)?;
    let _ = show_patient_view(&app, &state, &patient.id, PatientView::Treatment);
    Ok(patient)
}

#[tauri::command]
pub(crate) fn aidoo_next_patient(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::PatientSummary, String> {
    let patient = state.aidoo.select_next_patient()?;
    let _ = show_patient_view(&app, &state, &patient.id, PatientView::Treatment);
    Ok(patient)
}

#[tauri::command]
pub(crate) async fn aidoo_begin_status(
    patient_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::StatusEntryState, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let result = client
        .active_visit(&session.token, &session.clinic_id, &patient_id)
        .await;
    let entry = match result {
        Ok(visit) if !visit.is_finished && !visit.cancelled => {
            state.aidoo.clear_nzok_choice();
            types::StatusEntryState {
                ready: true,
                needs_visit: false,
                funding_choice_required: false,
                nzok_available: false,
                message: "Статусът е готов за попълване.".into(),
            }
        }
        Err(error) if error.is_not_found() => {
            if session.works_with_nzok
                && nzok_status_available(&client, &session, &patient_id).await?
            {
                state.aidoo.remember_nzok_choice(&patient_id)?;
                types::StatusEntryState {
                    ready: false,
                    needs_visit: true,
                    funding_choice_required: true,
                    nzok_available: true,
                    message: "Няма активно посещение. Попитайте само: „По НЗОК или частно?“".into(),
                }
            } else {
                state.aidoo.clear_nzok_choice();
                workflow::create_status_visit(
                    &client,
                    &session.token,
                    &session.clinic_id,
                    &patient_id,
                    &session.doctor_id,
                    false,
                )
                .await
                .map_err(|error| error.message)?;
                types::StatusEntryState {
                    ready: true,
                    needs_visit: false,
                    funding_choice_required: false,
                    nzok_available: false,
                    message: "Започнах частно посещение. Статусът е готов за попълване.".into(),
                }
            }
        }
        Ok(_) => return Err("Няма активно посещение за попълване на статус.".into()),
        Err(error) => return Err(error.message),
    };
    let _ = show_patient_view(&app, &state, &patient_id, PatientView::Status);
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn aidoo_begin_treatment(
    patient_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    state.aidoo.selected_patient(&patient_id)?;
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let result = workflow::create_treatment_visit(
        &client,
        &session.token,
        &session.clinic_id,
        &patient_id,
        &session.doctor_id,
    )
    .await
    .map_err(|error| treatment_diagnostics::failure("begin_visit", error))?;
    treatment_diagnostics::verification("begin_visit", &result.verification);
    let verified = result.visit.is_some()
        && matches!(
            result.verification.outcome,
            types::VerificationOutcome::Verified
                | types::VerificationOutcome::VerifiedAfterAmbiguousWrite
        );
    if !verified {
        let spoken_summary = result.verification.message.clone();
        return Ok(types::ClinicalWriteResult {
            spoken_summary,
            verification: result.verification,
            visible_in_browser: false,
            presentation_error: None,
        });
    }

    let presentation = show_patient_view(&app, &state, &patient_id, PatientView::Treatment);
    treatment_diagnostics::presentation(&presentation);
    let presentation_error = presentation.err();
    let spoken_summary = if presentation_error.is_some() {
        "Посещението е готово, но картонът не се опресни на екрана.".into()
    } else if result.created {
        "Създадох новото посещение. Отварям Лечение.".into()
    } else {
        "Отварям активното Лечение.".into()
    };
    Ok(types::ClinicalWriteResult {
        spoken_summary,
        verification: result.verification,
        visible_in_browser: presentation_error.is_none(),
        presentation_error,
    })
}

#[tauri::command]
pub(crate) fn aidoo_select_treatment_tooth(
    patient_id: String,
    tooth: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::TreatmentToothSelectionResult, String> {
    let tooth = selected_treatment_tooth(&state.aidoo, &patient_id, &tooth)?;
    presentation::present_treatment_tooth(&app, &state, patient_id, tooth.clone())?;
    Ok(treatment_tooth_selection_result(tooth))
}

#[tauri::command]
pub(crate) async fn aidoo_start_status_visit(
    patient_id: String,
    is_nzok: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::StatusVisitResult, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    if is_nzok {
        if !session.works_with_nzok {
            return Err("Тази клиника не работи с НЗОК. Започнете частно посещение.".into());
        }
        let checked_by_begin = state.aidoo.take_nzok_choice(&patient_id)?;
        if !checked_by_begin && !nzok_status_available(&client, &session, &patient_id).await? {
            return Err(
                "Не може да се започне статус по НЗОК: за годината вече има отчетена 101.".into(),
            );
        }
    } else {
        state.aidoo.clear_nzok_choice();
    }
    let result = workflow::create_status_visit(
        &client,
        &session.token,
        &session.clinic_id,
        &patient_id,
        &session.doctor_id,
        is_nzok,
    )
    .await;
    let _ = show_patient_view(&app, &state, &patient_id, PatientView::Status);
    result.map_err(|error| error.message)
}

#[tauri::command]
pub(crate) async fn aidoo_apply_status(
    patient_id: String,
    is_nzok: bool,
    change: types::SpokenStatusChange,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    apply_status_changes(patient_id, is_nzok, vec![change], &app, &state).await
}

#[tauri::command]
pub(crate) async fn aidoo_apply_statuses(
    patient_id: String,
    is_nzok: bool,
    changes: Vec<types::SpokenStatusChange>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    apply_status_changes(patient_id, is_nzok, changes, &app, &state).await
}

async fn apply_status_changes(
    patient_id: String,
    is_nzok: bool,
    changes: Vec<types::SpokenStatusChange>,
    app: &AppHandle,
    state: &AppState,
) -> Result<types::ClinicalWriteResult, String> {
    if changes.is_empty() {
        return Err("Липсват статусни промени за запис.".into());
    }
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let visit = client
        .active_visit(&session.token, &session.clinic_id, &patient_id)
        .await
        .map_err(|error| error.message)?;
    if visit.is_finished || visit.cancelled {
        return Err("Няма активно посещение за промяна на статуса.".into());
    }
    let baseline = client
        .editable_status(
            &session.token,
            &session.clinic_id,
            &patient_id,
            &visit.id,
            is_nzok,
        )
        .await
        .map_err(|error| error.message)?
        .teeth_status;
    let catalog = client
        .status_catalog(&session.token)
        .await
        .map(draft::editable_status_catalog)
        .map_err(|error| error.message)?;
    let resolved_changes = changes
        .into_iter()
        .map(|change| {
            let status_id = resolve_status(&catalog, &change.status)?.id.clone();
            let existing_status_id = change
                .replace_status
                .as_deref()
                .map(|query| resolve_status(&catalog, query).map(|entry| entry.id.clone()))
                .transpose()?;
            let operation = if existing_status_id.is_some() {
                types::StatusOperation::Replace
            } else {
                types::StatusOperation::Add
            };
            Ok(types::StatusChange {
                operation,
                tooth: change.tooth,
                status_id,
                regions: change.regions,
                existing_status_id,
                is_milk_tooth: change.is_milk_tooth,
                for_observation: change.for_observation,
                note: change.note,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let draft = draft::build_draft(
        patient_id.clone(),
        &visit,
        is_nzok,
        baseline,
        &catalog,
        &resolved_changes,
    )?;
    let result =
        workflow::apply_confirmed_draft(&client, &session.token, &session.clinic_id, &draft).await;
    let verification = result.map_err(|error| error.message)?;
    let presentation = show_patient_view(app, state, &patient_id, PatientView::Status);
    Ok(status_clinical_result(
        &draft.spoken_summary,
        verification,
        presentation,
    ))
}

#[tauri::command]
pub(crate) async fn aidoo_create_treatment(
    patient_id: String,
    change: types::SpokenTreatmentChange,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let visit = active_treatment_visit(&state.aidoo, &client, &session, &patient_id).await?;
    let baseline = client
        .visit_treatments(&session.token, &session.clinic_id, &patient_id, &visit.id)
        .await
        .map_err(|error| treatment_diagnostics::failure("read_rows", error))?;
    let diagnoses = if change.diagnosis.is_some() {
        client
            .diagnosis_catalog(&session.token, &session.clinic_id)
            .await
            .map_err(|error| treatment_diagnostics::failure("diagnosis_catalog", error))?
    } else {
        Vec::new()
    };
    let procedures = if change.procedures.is_empty() {
        Vec::new()
    } else {
        client
            .procedure_catalog(
                &session.token,
                &session.clinic_id,
                session.current_currency.as_deref(),
            )
            .await
            .map_err(|error| treatment_diagnostics::failure("procedure_catalog", error))?
    };
    let diagnosis_id = change
        .diagnosis
        .as_deref()
        .map(|query| resolve_diagnosis(&diagnoses, query).map(|entry| entry.id.clone()))
        .transpose()?;
    let procedure_ids = change
        .procedures
        .iter()
        .map(|query| resolve_procedure(&procedures, query).map(|entry| entry.id.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    apply_treatment_change(
        &app,
        &state,
        &session,
        &client,
        patient_id,
        visit,
        baseline,
        diagnoses,
        procedures,
        types::TreatmentChange {
            tooth: spoken_treatment_tooth(&change.tooth, change.is_milk_tooth)?,
            existing_treatment_id: None,
            diagnosis_id,
            treatment_id: None,
            note: change.note,
            procedure_ids,
        },
    )
    .await
}

#[tauri::command]
pub(crate) fn aidoo_finish_status(
    patient_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> String {
    let _ = show_patient_view(&app, &state, &patient_id, PatientView::Treatment);
    "Статусът е записан. Отварям Лечение.".into()
}

#[tauri::command]
pub(crate) async fn aidoo_add_procedure(
    patient_id: String,
    tooth: String,
    procedure: String,
    existing_treatment_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let visit = active_treatment_visit(&state.aidoo, &client, &session, &patient_id).await?;
    let baseline = client
        .visit_treatments(&session.token, &session.clinic_id, &patient_id, &visit.id)
        .await
        .map_err(|error| treatment_diagnostics::failure("read_rows", error))?;
    let procedures = client
        .procedure_catalog(
            &session.token,
            &session.clinic_id,
            session.current_currency.as_deref(),
        )
        .await
        .map_err(|error| treatment_diagnostics::failure("procedure_catalog", error))?;
    let procedure_id = resolve_procedure(&procedures, &procedure)?.id.clone();
    let row = resolve_treatment_row(&baseline, &tooth, existing_treatment_id.as_deref())?;
    apply_treatment_change(
        &app,
        &state,
        &session,
        &client,
        patient_id,
        visit,
        baseline,
        Vec::new(),
        procedures,
        types::TreatmentChange {
            tooth,
            existing_treatment_id: row,
            diagnosis_id: None,
            treatment_id: None,
            note: None,
            procedure_ids: vec![procedure_id],
        },
    )
    .await
}

#[tauri::command]
pub(crate) async fn aidoo_write_diagnosis(
    patient_id: String,
    tooth: String,
    diagnosis: String,
    existing_treatment_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let visit = active_treatment_visit(&state.aidoo, &client, &session, &patient_id).await?;
    let baseline = client
        .visit_treatments(&session.token, &session.clinic_id, &patient_id, &visit.id)
        .await
        .map_err(|error| treatment_diagnostics::failure("read_rows", error))?;
    let diagnoses = client
        .diagnosis_catalog(&session.token, &session.clinic_id)
        .await
        .map_err(|error| treatment_diagnostics::failure("diagnosis_catalog", error))?;
    let diagnosis_id = resolve_diagnosis(&diagnoses, &diagnosis)?.id.clone();
    let row = resolve_treatment_row(&baseline, &tooth, existing_treatment_id.as_deref())?;
    apply_treatment_change(
        &app,
        &state,
        &session,
        &client,
        patient_id,
        visit,
        baseline,
        diagnoses,
        Vec::new(),
        types::TreatmentChange {
            tooth,
            existing_treatment_id: row,
            diagnosis_id: Some(diagnosis_id),
            treatment_id: None,
            note: None,
            procedure_ids: Vec::new(),
        },
    )
    .await
}

#[tauri::command]
pub(crate) async fn aidoo_write_official_note(
    patient_id: String,
    tooth: String,
    note: String,
    existing_treatment_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<types::ClinicalWriteResult, String> {
    let session = state.aidoo.session()?;
    let client = state.aidoo.client()?;
    let visit = active_treatment_visit(&state.aidoo, &client, &session, &patient_id).await?;
    let baseline = client
        .visit_treatments(&session.token, &session.clinic_id, &patient_id, &visit.id)
        .await
        .map_err(|error| treatment_diagnostics::failure("read_rows", error))?;
    let row = resolve_treatment_row(&baseline, &tooth, existing_treatment_id.as_deref())?;
    apply_treatment_change(
        &app,
        &state,
        &session,
        &client,
        patient_id,
        visit,
        baseline,
        Vec::new(),
        Vec::new(),
        types::TreatmentChange {
            tooth,
            existing_treatment_id: row,
            diagnosis_id: None,
            treatment_id: None,
            note: Some(note),
            procedure_ids: Vec::new(),
        },
    )
    .await
}

async fn nzok_status_available(
    client: &super::client::AidooClient,
    session: &super::runtime::AidooSessionSnapshot,
    patient_id: &str,
) -> Result<bool, String> {
    let xml = client
        .nzis_status_search_xml(&session.token, &session.clinic_id, patient_id)
        .await
        .map_err(|error| error.message)?;
    let signed_xml = client
        .sign_nzis_xml(&xml)
        .await
        .map_err(|error| error.message)?;
    let patient = client
        .patient_details(&session.token, &session.clinic_id, patient_id)
        .await
        .map_err(|error| error.message)?;
    let identifier = patient
        .identifier
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Пациентът няма идентификатор за НЗОК проверка.".to_string())?;
    let person_type = patient
        .identifier_type
        .as_deref()
        .and_then(nzok_person_type)
        .ok_or_else(|| {
            "Пациентът няма поддържан тип идентификатор за НЗОК проверка.".to_string()
        })?;
    let nzok_data = client
        .nhif_four_year_data(identifier, person_type)
        .await
        .map_err(|error| error.message)?;
    client
        .has_available_status_check(&session.token, &session.clinic_id, &signed_xml, &nzok_data)
        .await
        .map_err(|error| error.message)
}

fn nzok_person_type(value: &str) -> Option<&'static str> {
    match value.trim() {
        "1" | "ЕГН" => Some("1"),
        "2" | "ЛНЧ" => Some("2"),
        "3" | "Социален номер - за чужди граждани" => Some("3"),
        "4" | "Номер на паспорт" => Some("4"),
        _ => None,
    }
}

fn spoken_treatment_tooth(value: &str, is_milk_tooth: bool) -> Result<String, String> {
    if !is_milk_tooth {
        return Ok(value.into());
    }
    let number = value
        .parse::<u8>()
        .map_err(|_| "Млечният зъб трябва да има валиден FDI номер.".to_string())?;
    let quadrant = number / 10;
    let position = number % 10;
    if matches!(quadrant, 5..=8) && matches!(position, 1..=5) {
        return Ok(value.into());
    }
    if matches!(quadrant, 1..=4) && matches!(position, 1..=5) {
        return Ok((number + 40).to_string());
    }
    Err("Този номер няма млечен зъб в AIDOO.".into())
}

fn selected_treatment_tooth(
    runtime: &super::runtime::AidooRuntime,
    patient_id: &str,
    tooth: &str,
) -> Result<String, String> {
    runtime.selected_patient(patient_id)?;
    treatment::canonical_treatment_tooth(tooth)
}

async fn active_treatment_visit(
    runtime: &super::runtime::AidooRuntime,
    client: &super::client::AidooClient,
    session: &super::runtime::AidooSessionSnapshot,
    patient_id: &str,
) -> Result<types::Visit, String> {
    runtime.selected_patient(patient_id)?;
    let visit = client
        .active_visit(&session.token, &session.clinic_id, patient_id)
        .await
        .map_err(|error| treatment_diagnostics::failure("active_visit", error))?;
    if visit.is_finished || visit.cancelled {
        return Err("Няма активно посещение за запис в Лечение.".into());
    }
    Ok(visit)
}

#[allow(clippy::too_many_arguments)]
async fn apply_treatment_change(
    app: &AppHandle,
    state: &AppState,
    session: &super::runtime::AidooSessionSnapshot,
    client: &super::client::AidooClient,
    patient_id: String,
    visit: types::Visit,
    baseline: Vec<types::VisitTreatment>,
    diagnoses: Vec<types::DiagnosisCatalogEntry>,
    procedures: Vec<types::ProcedureCatalogEntry>,
    change: types::TreatmentChange,
) -> Result<types::ClinicalWriteResult, String> {
    let draft = treatment::build_treatment_draft(
        patient_id.clone(),
        &visit,
        baseline,
        &diagnoses,
        &procedures,
        change,
    )?;
    let result = workflow::apply_confirmed_treatment_draft(
        client,
        &session.token,
        &session.clinic_id,
        &draft,
    )
    .await;
    let verification =
        result.map_err(|error| treatment_diagnostics::failure("apply_change", error))?;
    treatment_diagnostics::verification("apply_change", &verification);
    let presentation = presentation::present_treatment_tooth(
        app,
        state,
        patient_id,
        draft.treatment.tooth.clone(),
    );
    treatment_diagnostics::presentation(&presentation);
    Ok(clinical_result(
        &draft.spoken_summary,
        verification,
        presentation,
    ))
}

fn clinical_result(
    draft_summary: &str,
    verification: types::VerificationResult,
    presentation: Result<(), String>,
) -> types::ClinicalWriteResult {
    let write_verified = matches!(
        verification.outcome,
        types::VerificationOutcome::Verified
            | types::VerificationOutcome::VerifiedAfterAmbiguousWrite
    );
    let presentation_error = presentation.err();
    let spoken_summary = if write_verified && presentation_error.is_none() {
        completed_summary(draft_summary)
    } else if write_verified {
        "Записано е, но картонът не се опресни на екрана.".into()
    } else {
        verification.message.clone()
    };
    types::ClinicalWriteResult {
        spoken_summary,
        verification,
        visible_in_browser: presentation_error.is_none(),
        presentation_error,
    }
}

fn treatment_tooth_selection_result(tooth: String) -> types::TreatmentToothSelectionResult {
    let spoken_summary = if tooth == "*" {
        "Показвам общите процедури.".into()
    } else {
        format!("Показвам зъб {}.", spoken_tooth(&tooth))
    };
    types::TreatmentToothSelectionResult {
        tooth,
        spoken_summary,
        visible_in_browser: true,
    }
}

fn status_clinical_result(
    draft_summary: &str,
    verification: types::VerificationResult,
    presentation: Result<(), String>,
) -> types::ClinicalWriteResult {
    let write_verified = matches!(
        verification.outcome,
        types::VerificationOutcome::Verified
            | types::VerificationOutcome::VerifiedAfterAmbiguousWrite
    );
    let mut result = clinical_result(draft_summary, verification, presentation);
    if write_verified && result.visible_in_browser {
        result.spoken_summary = completed_status_summary(draft_summary);
    } else if !write_verified {
        result.spoken_summary = "Повтори.".into();
    }
    result
}

fn completed_status_summary(value: &str) -> String {
    let value = value
        .strip_prefix("Ще ")
        .unwrap_or(value)
        .strip_suffix(". Да го запиша ли?")
        .unwrap_or(value)
        .replace("добавя ", "")
        .replace("заменя със ", "");
    format!("Записах: {value}.")
}

fn completed_summary(value: &str) -> String {
    let value = value
        .strip_prefix("Ще запиша ")
        .unwrap_or(value)
        .strip_suffix(". Да го запиша ли?")
        .unwrap_or(value);
    format!("Записано: {value}.")
}

fn normalized(value: &str) -> String {
    value
        .to_lowercase()
        .replace(|character: char| !character.is_alphanumeric(), " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn unique_catalog_match<'a, T>(
    entries: &'a [T],
    query: &str,
    name: impl Fn(&T) -> &str,
    key: impl Fn(&T) -> &str,
    label: &str,
) -> Result<&'a T, String> {
    let query = normalized(query);
    if query.is_empty() {
        return Err(format!("Липсва {label} за търсене."));
    }
    let exact = entries
        .iter()
        .filter(|entry| normalized(name(entry)) == query || normalized(key(entry)) == query)
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let partial = entries
        .iter()
        .filter(|entry| {
            normalized(name(entry)).contains(&query) || normalized(key(entry)).contains(&query)
        })
        .collect::<Vec<_>>();
    if partial.len() == 1 {
        return Ok(partial[0]);
    }
    Err(if exact.len() > 1 || partial.len() > 1 {
        format!("{label} е двусмислена. Уточнете пълното име.")
    } else {
        format!("{label} не е намерена в актуалния AIDOO каталог.")
    })
}

fn resolve_status<'a>(
    entries: &'a [types::StatusCatalogEntry],
    query: &str,
) -> Result<&'a types::StatusCatalogEntry, String> {
    unique_catalog_match(
        entries,
        query,
        |entry| &entry.name,
        |entry| &entry.code,
        "Статусът",
    )
}

fn resolve_procedure<'a>(
    entries: &'a [types::ProcedureCatalogEntry],
    query: &str,
) -> Result<&'a types::ProcedureCatalogEntry, String> {
    unique_catalog_match(
        entries,
        query,
        |entry| &entry.name,
        |entry| &entry.key,
        "Процедурата",
    )
}

fn resolve_diagnosis<'a>(
    entries: &'a [types::DiagnosisCatalogEntry],
    query: &str,
) -> Result<&'a types::DiagnosisCatalogEntry, String> {
    unique_catalog_match(
        entries,
        query,
        |entry| &entry.name,
        |entry| &entry.key,
        "Диагнозата",
    )
}

fn resolve_treatment_row(
    entries: &[types::VisitTreatment],
    tooth: &str,
    explicit_id: Option<&str>,
) -> Result<Option<String>, String> {
    let body = treatment::canonical_treatment_tooth(tooth)?;
    let explicitly_milk = tooth
        .trim()
        .as_bytes()
        .first()
        .is_some_and(|quadrant| (b'5'..=b'8').contains(quadrant));
    let matches_tooth =
        |row: &&types::VisitTreatment| row.tooth == body && row.is_milk_tooth == explicitly_milk;
    if let Some(id) = explicit_id {
        let row = entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| "Избраният ред за лечение вече не съществува.".to_string())?;
        if !matches_tooth(&row) {
            return Err("Избраният ред за лечение е за друг зъб или не е млечен.".into());
        }
        return Ok(Some(id.to_string()));
    }
    let matches = entries.iter().filter(matches_tooth).collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [entry] => Ok(Some(entry.id.clone())),
        _ => Err(format!(
            "Има {} реда в Лечение за {}. Уточнете кой ред да се промени.",
            matches.len(),
            if tooth == "*" {
                "звездичката".into()
            } else {
                format!("зъб {}", spoken_tooth(tooth))
            }
        )),
    }
}

#[cfg(test)]
#[path = "tests/treatment_selection.rs"]
mod treatment_selection_tests;

#[cfg(test)]
#[path = "tests/protocol.rs"]
mod tests;
