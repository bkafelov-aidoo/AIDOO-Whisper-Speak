use std::{sync::OnceLock, time::Duration};

static LIVE_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

#[cfg(debug_assertions)]
const MAX_RECORDED_LIVE_STAGE_MS: u128 = 60_000;

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
pub(super) enum LiveStartupStage {
    RequestBuild,
    SessionPost,
    ResponseDecode,
}

#[cfg(debug_assertions)]
impl LiveStartupStage {
    fn label(self) -> &'static str {
        match self {
            Self::RequestBuild => "request_build",
            Self::SessionPost => "session_post",
            Self::ResponseDecode => "response_decode",
        }
    }
}

#[cfg(debug_assertions)]
fn startup_stage_message(stage: LiveStartupStage, elapsed: Duration) -> String {
    let elapsed_ms = elapsed.as_millis().min(MAX_RECORDED_LIVE_STAGE_MS);
    format!(
        "GPT-Live startup stage={} elapsed_ms={elapsed_ms}",
        stage.label()
    )
}

#[cfg(debug_assertions)]
pub(super) fn record_startup_stage(stage: LiveStartupStage, elapsed: Duration) {
    crate::storage::append_diagnostic(&startup_stage_message(stage, elapsed));
}

pub(super) fn client() -> Result<&'static reqwest::Client, String> {
    if let Some(client) = LIVE_HTTP_CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("GPT-Live връзката не можа да бъде подготвена: {error}"))?;
    Ok(LIVE_HTTP_CLIENT.get_or_init(|| client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_sessions_share_one_production_http_client() {
        let first = client().unwrap();
        let second = client().unwrap();

        assert!(std::ptr::eq(first, second));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn development_startup_timing_is_bounded_and_contains_only_stage_and_milliseconds() {
        assert_eq!(
            startup_stage_message(LiveStartupStage::RequestBuild, Duration::ZERO),
            "GPT-Live startup stage=request_build elapsed_ms=0"
        );
        assert_eq!(
            startup_stage_message(LiveStartupStage::SessionPost, Duration::from_secs(90)),
            "GPT-Live startup stage=session_post elapsed_ms=60000"
        );
        assert_eq!(
            startup_stage_message(LiveStartupStage::ResponseDecode, Duration::from_millis(17),),
            "GPT-Live startup stage=response_decode elapsed_ms=17"
        );
    }
}
