//! AI Provider abstraction — trait + factory for swappable LLM backends.
//!
//! Follows the same Trait + Factory pattern used by data connectors.
//! Adding a new provider requires:
//! 1. Implement `AiProvider` for your backend
//! 2. Add a match arm in `AiProviderFactory::create`
//!
//! Supported providers:
//! - `openai` — OpenAI GPT models (gpt-4o, gpt-4o-mini)
//! - Future: `anthropic`, `ollama`, `azure_openai`, etc.

use serde::{Deserialize, Serialize};

use crate::errors::ChalkError;

// ── Shared Types ────────────────────────────────────────────

/// A single message in a chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionMessage {
    pub role: String,
    pub content: String,
}

/// Configuration for creating an AI provider instance.
#[derive(Debug, Clone)]
pub struct AiProviderConfig {
    pub provider_type: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// Metadata about a provider (for Settings UI display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub models: Vec<ModelInfo>,
}

/// Metadata about a model within a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
}

/// Token callback for streaming responses.
pub type TokenCallback = Box<dyn Fn(&str) + Send>;

// ── Provider Trait ──────────────────────────────────────────

/// The core trait that every AI backend must implement.
///
/// Providers handle the transport-level details (API format, auth headers,
/// SSE parsing) while the chat module handles conversation management,
/// RAG context, and message history.
#[async_trait::async_trait]
pub trait AiProvider: Send + Sync {
    /// Generate a complete response (non-streaming).
    async fn complete(
        &self,
        messages: &[CompletionMessage],
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String, ChalkError>;

    /// Generate a streaming response, calling `on_token` for each chunk.
    /// Returns the full assembled response.
    async fn complete_stream(
        &self,
        messages: &[CompletionMessage],
        max_tokens: u32,
        temperature: f32,
        on_token: TokenCallback,
    ) -> Result<String, ChalkError>;

    /// Provider info for display in Settings.
    fn info(&self) -> ProviderInfo;

    /// The model ID currently configured.
    fn model(&self) -> &str;
}

// ── Factory ─────────────────────────────────────────────────

/// Factory that creates provider instances from stored configuration.
pub struct AiProviderFactory;

impl AiProviderFactory {
    /// Create a provider from config. Returns an error for unknown types.
    pub fn create(config: &AiProviderConfig) -> Result<Box<dyn AiProvider>, ChalkError> {
        match config.provider_type.as_str() {
            "openai" => Ok(Box::new(super::openai::OpenAiProvider::new(config))),
            // Future providers:
            // "anthropic" => Ok(Box::new(super::anthropic::AnthropicProvider::new(config))),
            // "ollama"    => Ok(Box::new(super::ollama::OllamaProvider::new(config))),
            other => Err(ChalkError::connector_api(format!(
                "Unknown AI provider: {other}"
            ))),
        }
    }

    /// List all known provider types and their available models.
    pub fn available_providers() -> Vec<ProviderInfo> {
        vec![
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
            },
            // Future:
            // ProviderInfo { id: "anthropic", ... },
            // ProviderInfo { id: "ollama", ... },
        ]
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_unknown_provider() {
        let config = AiProviderConfig {
            provider_type: "nonexistent".into(),
            api_key: "key".into(),
            base_url: "http://localhost".into(),
            model: "test".into(),
        };
        let result = AiProviderFactory::create(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_factory_creates_openai() {
        let config = AiProviderConfig {
            provider_type: "openai".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
        };
        let provider = AiProviderFactory::create(&config).unwrap();
        assert_eq!(provider.model(), "gpt-4o-mini");
        assert_eq!(provider.info().id, "openai");
    }

    #[test]
    fn test_available_providers() {
        let providers = AiProviderFactory::available_providers();
        assert!(!providers.is_empty());
        assert_eq!(providers[0].id, "openai");
        assert!(providers[0].models.len() >= 2);
    }

    #[test]
    fn test_completion_message_serialization() {
        let msg = CompletionMessage {
            role: "user".into(),
            content: "Hello".into(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_provider_config_clone() {
        let config = AiProviderConfig {
            provider_type: "openai".into(),
            api_key: "key".into(),
            base_url: "url".into(),
            model: "model".into(),
        };
        let cloned = config.clone();
        assert_eq!(cloned.provider_type, "openai");
    }

    #[test]
    fn test_model_info_serialization() {
        let info = ModelInfo {
            id: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            description: "Most capable".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["id"], "gpt-4o");
    }
}
