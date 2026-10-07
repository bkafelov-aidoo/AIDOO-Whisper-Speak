use super::*;

#[test]
fn live_request_uses_reviewed_models_and_webrtc_transport() {
    let request = create_request("v=0\r\ns=test\r\n", "cedar").unwrap();
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["session"]["model"], LIVE_MODEL);
    assert_eq!(value["session"]["audio"]["output"]["voice"], "cedar");
    assert_eq!(value["session"]["store"], false);
    assert_eq!(
        value["session"]["client"]["data_channel"]["allowed_client_events"],
        serde_json::json!([
            "session.close",
            "session.instructions.append",
            "session.commentary.append",
            "response.item.create",
            "response.create"
        ])
    );
    let mut expected_server_events = vec![
        serde_json::json!({"type": "session.started"}),
        serde_json::json!({"type": "session.input_transcript.delta"}),
        serde_json::json!({"type": "session.instructions.appended"}),
        serde_json::json!({"type": "session.commentary.appended"}),
        serde_json::json!({"type": "session.closed"}),
        serde_json::json!({"type": "error"}),
        serde_json::json!({"type": "response.event", "response_event": "response.output_item.done"}),
        serde_json::json!({"type": "response.event", "response_event": "response.completed"}),
        serde_json::json!({"type": "response.event", "response_event": "response.incomplete"}),
        serde_json::json!({"type": "response.event", "response_event": "response.failed"}),
    ];
    if cfg!(debug_assertions) {
        expected_server_events.insert(
            2,
            serde_json::json!({"type": "session.output_transcript.delta"}),
        );
        expected_server_events.insert(
            8,
            serde_json::json!({"type": "response.event", "response_event": "response.output_text.delta"}),
        );
    }
    assert_eq!(
        value["session"]["client"]["data_channel"]["allowed_server_events"],
        serde_json::Value::Array(expected_server_events)
    );
    assert_eq!(value["session"]["delegation"]["type"], "responses");
    assert_eq!(
        value["session"]["delegation"]["responses"]["model"],
        LIVE_BACKEND_MODEL
    );
    let tools = value["session"]["delegation"]["responses"]["tools"]
        .as_array()
        .unwrap();
    assert_eq!(tools.len(), 20);
    assert!(tools.iter().all(|tool| tool["strict"] == true));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "begin_aidoo_status"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "apply_aidoo_statuses"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "start_aidoo_status_visit"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "create_aidoo_treatment"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "add_aidoo_procedure"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "get_aidoo_active_treatments"));
    assert!(tools.iter().any(|tool| tool["name"] == "read_aidoo_status"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "read_aidoo_treatments"));
    assert!(tools.iter().any(|tool| tool["name"] == "read_aidoo_visits"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "read_aidoo_patient_data"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "write_aidoo_official_note"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "find_aidoo_schedule_slot"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "book_aidoo_schedule_slot"));
    assert_eq!(value["transport"]["type"], "webrtc");
    assert_eq!(value["transport"]["sdp"], "v=0\r\ns=test\r\n");
}

#[test]
fn live_instructions_normalize_spoken_fdi_tooth_numbers() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    assert!(value["session"]["instructions"]
        .as_str()
        .unwrap()
        .contains("18 е „едно осем“"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("„едно шест“ е 16"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("apply_aidoo_statuses"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("Не искай потвърждение"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("18–11, 21–28, 38–31, 41–48"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("А другите зъби в 1ви квадрант?"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("групирай всички добавяния и замени за един зъб"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("replaceStatus=стария статус"));
    assert!(value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap()
        .contains("не прави междинен разговорен отговор"));
}

#[test]
fn patient_selection_response_uses_only_the_patient_name() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    assert!(instructions.contains("Кажи само „Намерих пациента {пълно име}.“"));
    assert!(instructions.contains("Не казвай датата на раждане"));
}

#[test]
fn official_notes_support_an_explicit_end_or_the_ten_second_confirmation_flow() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend_instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    assert!(live_instructions.contains("10 секунди без продължение"));
    assert!(live_instructions.contains("Да завършвам ли забележката?"));
    assert!(live_instructions.contains("Край на забележката"));
    assert!(live_instructions.contains("Самостоятелно „Край“"));
    assert!(live_instructions.contains("завършват само диктовката, без да затварят сесията"));
    assert!(!live_instructions.contains("Когато каже „Край“,"));
    assert!(backend_instructions.contains("confirmationRequired=true"));
    assert!(backend_instructions.contains("readyToSave=true"));
    assert!(backend_instructions.contains("Само след изрично потвърждение"));
}

#[test]
fn status_writes_are_immediate_and_quadrant_questions_wait_for_a_transition() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    assert!(instructions.contains("кажи дословно spokenSummary"));
    assert!(instructions.contains("кажи точно „Повтори.“"));
    assert!(instructions.contains("всяко отделно изказване за един зъб веднага"));
    assert!(instructions.contains("не отлагай записа"));
    assert!(instructions.contains("Едва когато потребителят назове зъб от следващ квадрант"));
    assert!(!instructions.contains("не извиквай инструмент още"));
}

#[test]
fn status_editing_replaces_the_exact_old_status_without_deleting_the_visit() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend = &value["session"]["delegation"]["responses"];
    let backend_instructions = backend["instructions"].as_str().unwrap();

    for instructions in [live_instructions, backend_instructions] {
        assert!(instructions.contains("„редактирай“, „коригирай“ или „замени“"));
        assert!(instructions.contains("replaceStatus=стария статус"));
        assert!(instructions.contains("status=новия статус"));
        assert!(instructions.contains("Никога не добавяй новия статус като корекция"));
        assert!(instructions.contains("никога не изтривай целия статус или посещение"));
        assert!(instructions.contains("едно apply_aidoo_statuses и едно опресняване"));
    }

    assert!(backend_instructions.contains("първо извикай read_aidoo_status"));
    assert!(backend_instructions.contains("старият статус или повърхността са неясни"));
    assert!(backend_instructions.contains("задай само един кратък въпрос"));
    assert!(backend_instructions.contains("За статус не казвай съобщение преди инструмента"));
    assert!(backend_instructions.contains("кажи дословно spokenSummary"));

    let status_tool = backend["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "apply_aidoo_statuses")
        .unwrap();
    let description = status_tool["description"].as_str().unwrap();
    assert!(description.contains("replaceStatus=стария статус"));
    assert!(description.contains("status=новия статус"));
    assert!(description.contains("не добавя корекцията като нов статус"));
    assert!(description.contains("не изтрива целия статус или посещение"));
}

#[test]
fn spoken_actions_are_one_short_sentence_without_filler_or_generic_confirmation() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend_instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    for instructions in [live_instructions, backend_instructions] {
        assert!(instructions.contains("най-много едно кратко изречение"));
        assert!(instructions.contains("Не искай потвърждение преди действие като обща стъпка"));
        assert!(instructions.contains("без обяснение, списък или следващо предложение"));
        for filler in [
            "Разбрах, проверявам",
            "Само момент",
            "Нека проверя",
            "С удоволствие",
        ] {
            assert!(!instructions.contains(filler), "forbidden filler: {filler}");
        }
    }

    assert!(backend_instructions.contains("„Отварям лечението.“"));
    assert!(backend_instructions.contains("„Създавам новото лечение.“"));
    assert!(backend_instructions.contains("„Добавям процедурата.“"));
    assert!(backend_instructions.contains("„Записвам диагнозата.“"));
    assert!(backend_instructions.contains("„Записвам забележката.“"));
}

#[test]
fn concise_action_announcements_preserve_immediate_status_and_special_questions() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend_instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    assert!(backend_instructions.contains("За статус не казвай съобщение преди инструмента"));
    assert!(backend_instructions.contains("всяко отделно изказване за един зъб веднага"));
    assert!(backend_instructions.contains("кажи дословно spokenSummary"));
    assert!(backend_instructions.contains("По НЗОК или частно?"));
    assert!(live_instructions.contains("Да завършвам ли забележката?"));
    assert!(backend_instructions.contains("Да завършвам ли забележката?"));
}

#[test]
fn treatment_note_phrases_target_the_procedure_row_without_truncating_requested_content() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend = &value["session"]["delegation"]["responses"];
    let backend_instructions = backend["instructions"].as_str().unwrap();

    for instructions in [live_instructions, backend_instructions] {
        assert!(instructions
            .contains("„Диктувай забележка“, „Добави забележка“ или „Запиши забележка“"));
        assert!(instructions.contains("реда в Лечение до Процедури"));
        assert!(instructions.contains("не е вътрешна бележка към посещението"));
        assert!(instructions.contains("дословно, без преразказ или обобщение"));
        assert!(instructions.contains("Ограничението до едно кратко изречение не важи"));
        assert!(instructions.contains("изрично поискано изчитане"));
        assert!(instructions.contains("продиктувания текст"));
    }

    let note_tool = backend["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "write_aidoo_official_note")
        .unwrap();
    let description = note_tool["description"].as_str().unwrap();
    assert!(description.contains("реда в Лечение до Процедури"));
    assert!(description.contains("не е вътрешна бележка към посещението"));
    assert!(description.contains("дословно, без преразказ или обобщение"));
}

#[test]
fn held_note_capture_stays_silent_until_the_actual_final_save() {
    let request = create_request("v=0\r\ns=test\r\n", "marin").unwrap();
    let value = serde_json::to_value(request).unwrap();
    let live_instructions = value["session"]["instructions"].as_str().unwrap();
    let backend_instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();

    for instructions in [live_instructions, backend_instructions] {
        assert!(instructions.contains("captureInProgress=true"));
        assert!(instructions.contains("не повтаряй write_aidoo_official_note"));
        assert!(instructions.contains("не казвай въпрос, потвърждение или filler"));
        assert!(instructions.contains("остани да слушаш"));
        assert!(instructions
            .contains("„Записвам забележката.“ само непосредствено преди окончателното записване"));
    }

    assert!(backend_instructions.contains("първото capture извикване без съобщение"));
    assert!(backend_instructions.contains("„Диктувайте забележката.“"));
}

#[test]
fn new_treatment_visit_is_distinct_from_a_tooth_row_and_session_end() {
    let value =
        serde_json::to_value(create_request("v=0\r\ns=test\r\n", "marin").unwrap()).unwrap();
    let backend = &value["session"]["delegation"]["responses"];
    let begin = backend["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "begin_aidoo_treatment")
        .unwrap();
    assert_eq!(
        begin["parameters"]["required"],
        serde_json::json!(["patientId"])
    );
    assert!(begin["parameters"]["properties"]["tooth"].is_null());
    assert!(begin["parameters"]["properties"]["doctorId"].is_null());
    assert!(backend["instructions"]
        .as_str()
        .unwrap()
        .contains("Посещение и ред за зъб са различни действия"));
    let live = value["session"]["instructions"].as_str().unwrap();
    assert!(!live.contains("Самостоятелното „Край“ никога"));
    assert!(live.contains("„Приключихме“"));
    assert!(live.contains("завършват само диктовката, без да затварят сесията"));
}

#[test]
fn natural_treatment_save_wording_has_a_direct_tool_workflow() {
    let value =
        serde_json::to_value(create_request("v=0\r\ns=test\r\n", "marin").unwrap()).unwrap();
    let instructions = value["session"]["delegation"]["responses"]["instructions"]
        .as_str()
        .unwrap();
    assert!(instructions.contains("Можем ли да запишем лечение?"));
    assert!(instructions.contains("Запиши зъб Х и процедура У"));
    assert!(instructions.contains("visibleInBrowser=true"));
    assert!(instructions.contains("не отговаряй само „Да“"));
    assert!(instructions.contains("не създавай втори ред за същата процедура"));
}

#[test]
fn treatment_tooth_selection_is_a_distinct_visible_action() {
    let value =
        serde_json::to_value(create_request("v=0\r\ns=test\r\n", "marin").unwrap()).unwrap();
    let backend = &value["session"]["delegation"]["responses"];
    let select = backend["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "select_aidoo_treatment_tooth")
        .unwrap();

    assert_eq!(
        select["parameters"]["required"],
        serde_json::json!(["patientId", "tooth"])
    );
    assert!(select["parameters"]["properties"]["isMilkTooth"].is_null());
    assert!(backend["instructions"]
        .as_str()
        .unwrap()
        .contains("select_aidoo_treatment_tooth"));
}

#[test]
fn live_request_rejects_empty_invalid_and_oversized_sdp() {
    assert!(create_request("", "marin").is_err());
    assert!(create_request("not-sdp", "marin").is_err());
    let oversized = format!("v=0{}", "x".repeat(MAX_SDP_BYTES));
    assert!(create_request(&oversized, "marin").is_err());
    assert!(create_request("v=0\r\ns=test\r\n", "random").is_err());
}

#[test]
fn live_response_requires_session_id_webrtc_and_sdp() {
    let response: OpenAiLiveCreateResponse = serde_json::from_value(serde_json::json!({
        "session": { "id": "live_123" },
        "transport": { "type": "webrtc", "sdp": "v=0\\r\\n" }
    }))
    .unwrap();
    assert_eq!(response.session.id, "live_123");
    assert_eq!(response.transport.r#type, "webrtc");
    assert!(!response.transport.sdp.is_empty());
}
