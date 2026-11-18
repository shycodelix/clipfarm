use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::time::Duration;

use crate::config::is_loopback;

/// A deliberately tiny Ollama client. JSON in, JSON out, localhost only.
///
/// The zero-cost rule is enforced here rather than in the UI: if the endpoint is
/// not loopback, the call is refused before a single byte leaves the machine.
pub fn complete_json(
    endpoint: &str,
    model: &str,
    prompt: &str,
    timeout: Duration,
) -> Result<Value> {
    if !is_loopback(endpoint) {
        return Err(anyhow!(
            "Refusing to call `{endpoint}`: clipfarm only talks to a local model."
        ));
    }
    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "format": "json",
        "options": { "temperature": 0.4 }
    });
    let response = ureq::post(&url)
        .timeout(timeout)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|error| match error {
            ureq::Error::Status(code, _) => {
                anyhow!("Local model returned HTTP {code}. Is `ollama serve` running?")
            }
            other => anyhow!("Could not reach local model at {url}: {other}"),
        })?;
    let payload: Value = response
        .into_json()
        .context("Local model returned something that was not JSON")?;
    let text = payload
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Local model response had no `response` field"))?;
    extract_json(text).ok_or_else(|| anyhow!("Local model did not return a JSON object"))
}

/// Models sometimes wrap JSON in prose or fences. Dig out the first object.
pub fn extract_json(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        return Some(value);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_chatty_models() {
        let raw = "Sure! Here you go:\n```json\n{\"title\":\"the bit\"}\n```\nCheers";
        let value = extract_json(raw).expect("json");
        assert_eq!(value["title"], "the bit");
    }

    #[test]
    fn refuses_non_loopback_endpoints() {
        let error = complete_json(
            "https://api.example.com",
            "model",
            "hi",
            Duration::from_millis(10),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("local model"));
    }
}
