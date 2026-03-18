//! OpenAI provider implementation — GPT-4o, GPT-4o-mini via the
//! OpenAI-compatible chat completions API.
//!
//! Handles both non-streaming and SSE streaming responses.

use serde::{Deserialize, Serialize};

use crate::errors::ChalkError;

use super::provider::{
    AiProvider, AiProviderConfig, CompletionMessage, ModelInfo, ProviderInfo, TokenCallback,
};

// ── Internal API Types ──────────────────────────────────────

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<CompletionMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

#[derive(Deserialize)]
pub(crate) struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
pub(crate) struct StreamChoice {
    pub delta: StreamDelta,
}

#[derive(Deserialize)]
pub(crate) struct StreamDelta {
    pub content: Option<String>,
}

// ── Provider ────────────────────────────────────────────────

pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(config: &AiProviderConfig) -> Self {
        Self {
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
        }
    }
}

#[async_trait::async_trait]
impl AiProvider for OpenAiProvider {
    async fn complete(
        &self,
        messages: &[CompletionMessage],
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String, ChalkError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            max_tokens,
            temperature,
            stream: None,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let client = reqwest::Client::new();

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ChalkError::connector_api(format!("OpenAI request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(ChalkError::connector_api(format!(
                "OpenAI returned {status}: {body}"
            )));
        }

        let result: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ChalkError::connector_api(format!("Failed to parse OpenAI response: {e}")))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| ChalkError::connector_api("Empty response from OpenAI"))
    }

    async fn complete_stream(
        &self,
        messages: &[CompletionMessage],
        max_tokens: u32,
        temperature: f32,
        on_token: TokenCallback,
    ) -> Result<String, ChalkError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            max_tokens,
            temperature,
            stream: Some(true),
        };

        let url = format!("{}/chat/completions", self.base_url);
        let client = reqwest::Client::new();

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ChalkError::connector_api(format!("OpenAI request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(ChalkError::connector_api(format!(
                "OpenAI returned {status}: {body}"
            )));
        }

        let mut full_content = String::new();
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                ChalkError::connector_api(format!("Stream read error: {e}"))
            })?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim_end_matches('\r').to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                        for choice in &chunk.choices {
                            if let Some(content) = &choice.delta.content {
                                full_content.push_str(content);
                                on_token(content);
                            }
                        }
                    }
                }
            }
        }

        if full_content.is_empty() {
            return Err(ChalkError::connector_api("Empty streaming response from OpenAI"));
        }

        Ok(full_content)
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "openai".into(),
            display_name: "OpenAI".into(),
            models: vec![
                ModelInfo {
                    id: "gpt-4o-mini".into(),
                    display_name: "GPT-4o Mini".into(),
                    description: "Fast & affordable".into(),
                },
                ModelInfo {
                    id: "gpt-4o".into(),
                    display_name: "GPT-4o".into(),
                    description: "Most capable".into(),
                },
            ],
        }
    }

    fn model(&self) -> &str {
        &self.model
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let config = AiProviderConfig {
            provider_type: "openai".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
        };
        let provider = OpenAiProvider::new(&config);
        assert_eq!(provider.model(), "gpt-4o-mini");
        assert_eq!(provider.info().id, "openai");
        assert_eq!(provider.info().models.len(), 2);
    }

    #[test]
    fn test_stream_chunk_deserialization() {
        let json = r#"{"id":"chatcmpl-abc","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_stream_chunk_empty_delta() {
        let json = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn test_request_serialization_no_stream() {
        let req = ChatCompletionRequest {
            model: "gpt-4o-mini".into(),
            messages: vec![CompletionMessage {
                role: "user".into(),
                content: "Hello".into(),
            }],
            max_tokens: 2048,
            temperature: 0.7,
            stream: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn test_request_serialization_with_stream() {
        let req = ChatCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            max_tokens: 2048,
            temperature: 0.7,
            stream: Some(true),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], true);
    }
}
