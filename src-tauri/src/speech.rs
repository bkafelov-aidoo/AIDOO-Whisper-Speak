use futures_util::StreamExt;
use serde::Serialize;
use std::time::Duration;

use crate::models::LIVE_VOICES;
use crate::{acquire_operation, api_key_from_state, stop_wake_word_listener, AppState};
use tauri::{AppHandle, State};

const SPEECH_ENDPOINT: &str = "https://api.openai.com/v1/audio/speech";
const MAX_PREVIEW_BYTES: usize = 2_000_000;
const MAX_ERROR_BYTES: usize = 64_000;

#[derive(Serialize)]
struct SpeechRequest<'a> {
    model: &'static str,
    voice: &'a str,
    input: &'a str,
    instructions: &'static str,
    response_format: &'static str,
}

pub(crate) async fn generate_voice_preview(
    api_key: &str,
    voice: &str,
    language: &str,
) -> Result<Vec<u8>, String> {
    if !LIVE_VOICES.contains(&voice) {
        return Err("Избраният глас не се поддържа.".into());
    }
    let input = if language == "en" {
        "Hello. I am your AIDOO assistant."
    } else {
        "Здравейте. Аз съм вашият AIDOO асистент."
    };
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("OpenAI връзката не можа да бъде подготвена: {error}"))?;
    let response = client
        .post(SPEECH_ENDPOINT)
        .bearer_auth(api_key.trim())
        .json(&SpeechRequest {
            model: "gpt-4o-mini-tts",
            voice,
            input,
            instructions: "Speak naturally, warmly, clearly, and at a calm conversational pace.",
            response_format: "mp3",
        })
        .send()
        .await
        .map_err(|error| format!("Няма връзка с OpenAI: {error}"))?;
    if !response.status().is_success() {
        return Err(api_error(response).await);
    }
    read_bounded(response, MAX_PREVIEW_BYTES)
        .await
        .map_err(|error| format!("OpenAI върна невалидна гласова проба: {error}"))
}

#[tauri::command]
pub(crate) async fn preview_live_voice(
    voice: String,
    language: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let _operation = acquire_operation(&app, &state)?;
    stop_wake_word_listener(&state);
    let api_key = api_key_from_state(&state)?;
    generate_voice_preview(&api_key, &voice, &language).await
}

async fn api_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = read_bounded(response, MAX_ERROR_BYTES)
        .await
        .unwrap_or_default();
    let message = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| value.pointer("/error/message")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("HTTP {status}"));
    match status.as_u16() {
        401 => "Гласовата проба не успя: невалиден или изтрит API ключ.".into(),
        429 if message.to_lowercase().contains("quota") => {
            "Гласовата проба не успя: няма наличен API баланс или е достигнат лимитът.".into()
        }
        _ => format!("Гласовата проба не успя: {message}"),
    }
}

async fn read_bounded(
    response: reqwest::Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err("отговорът надвишава безопасния лимит".into());
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if body.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err("отговорът надвишава безопасния лимит".into());
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err("получен е празен аудио файл".into());
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_voice_is_rejected_before_network_access() {
        let result = generate_voice_preview("sk-test", "random", "bg").await;
        assert_eq!(result.unwrap_err(), "Избраният глас не се поддържа.");
    }
}
