//! Embedding generation via OpenAI-compatible API.
//!
//! Uses the text-embedding-3-small model (1536 dimensions) which matches
//! our sqlite-vec table definition.

use serde::{Deserialize, Serialize};

use crate::errors::ChalkError;

/// The embedding model to use. text-embedding-3-small is cost-effective and
/// produces 1536-dimensional vectors matching our vec0 table.
const EMBEDDING_MODEL: &str = "text-embedding-3-small";
/// Expected dimension count for validation.
pub const EMBEDDING_DIMENSIONS: usize = 1536;

/// OpenAI embedding API request body.
#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

/// OpenAI embedding API response.
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Client for generating text embeddings via the OpenAI API.
#[derive(Clone)]
pub struct EmbeddingClient {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl EmbeddingClient {
    /// Create a new embedding client with the given API key.
    /// Uses the standard OpenAI endpoint by default.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Create a client with a custom base URL (for local models or proxies).
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key,
            base_url,
            http: reqwest::Client::new(),
        }
    }

    /// Generate embeddings for one or more text inputs.
    /// Returns a Vec of embedding vectors, one per input.
    pub async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ChalkError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            model: EMBEDDING_MODEL,
            input: texts.to_vec(),
        };

        let url = format!("{}/embeddings", self.base_url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ChalkError::connector_api(format!("Embedding API request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(ChalkError::connector_api(format!(
                "Embedding API returned {status}: {body}"
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| ChalkError::connector_api(format!("Failed to parse embedding response: {e}")))?;

        let embeddings: Vec<Vec<f32>> = result.data.into_iter().map(|d| d.embedding).collect();

        // Validate dimensions.
        for (i, emb) in embeddings.iter().enumerate() {
            if emb.len() != EMBEDDING_DIMENSIONS {
                return Err(ChalkError::connector_api(format!(
                    "Embedding {i} has {} dimensions, expected {EMBEDDING_DIMENSIONS}",
                    emb.len()
                )));
            }
        }

        Ok(embeddings)
    }

    /// Generate a single embedding for one text input.
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, ChalkError> {
        let mut results = self.embed(&[text]).await?;
        results
            .pop()
            .ok_or_else(|| ChalkError::connector_api("Empty embedding response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_client_creation() {
        let client = EmbeddingClient::new("test-key".to_string());
        assert_eq!(client.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_embedding_client_custom_url() {
        let client =
            EmbeddingClient::with_base_url("key".to_string(), "http://localhost:8080".to_string());
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_embed_empty_input() {
        let client = EmbeddingClient::new("fake-key".to_string());
        let result = client.embed(&[]).await.unwrap();
        assert!(result.is_empty());
    }
}
