//! Integration tests for the AI Admin Agent.
//!
//! These tests verify the end-to-end flow of the admin onboarding process:
//! OAuth configuration, token persistence, onboarding status tracking,
//! and the interaction between admin and database modules.

use chalk_lib::admin::oauth::{
    get_valid_access_token, OAuthClient, OAuthConfig, OnboardingStatus, TokenStorage,
};
use chalk_lib::database::{Database, NewLessonPlan, NewSubject};
use chrono::Utc;
use std::fs;
use tempfile::TempDir;

// ── OAuth Client Integration ────────────────────────────────────

#[test]
fn test_full_oauth_config_lifecycle() {
    let dir = TempDir::new().unwrap();
    let mut client = OAuthClient::new(dir.path());

    // Initially no config loaded.
    assert!(!client.load_config().unwrap());
    assert!(client.config.client_id.is_empty());

    // Save a config.
    let config = OAuthConfig {
        client_id: "integration-test-id".into(),
        client_secret: "integration-test-secret".into(),
        redirect_uri: "http://localhost:1420/oauth/callback".into(),
        scopes: vec![
            "https://www.googleapis.com/auth/drive.readonly".into(),
            "https://www.googleapis.com/auth/documents.readonly".into(),
        ],
    };
    client.save_config(&config).unwrap();

    // Create a new client and load the config from disk.
    let mut client2 = OAuthClient::new(dir.path());
    assert!(client2.load_config().unwrap());
    assert_eq!(client2.config.client_id, "integration-test-id");
    assert_eq!(client2.config.client_secret, "integration-test-secret");
    assert_eq!(client2.config.scopes.len(), 2);

    // Authorization URL contains the saved client_id.
    let url = client2.get_authorization_url();
    assert!(url.contains("integration-test-id"));
    assert!(url.contains("access_type=offline"));
}

#[test]
fn test_full_token_lifecycle() {
    let dir = TempDir::new().unwrap();
    let client = OAuthClient::new(dir.path());

    // No tokens initially.
    assert!(client.load_tokens().unwrap().is_none());

    // Save tokens.
    let tokens = TokenStorage {
        access_token: "integration-access-token".into(),
        refresh_token: Some("integration-refresh-token".into()),
        expires_at: Utc::now() + chrono::Duration::seconds(3600),
        token_type: "Bearer".into(),
    };
    client.save_tokens(&tokens).unwrap();

    // Load and verify.
    let loaded = client.load_tokens().unwrap().unwrap();
    assert_eq!(loaded.access_token, "integration-access-token");
    assert_eq!(
        loaded.refresh_token,
        Some("integration-refresh-token".into())
    );
    assert!(!loaded.is_expired());

    // Overwrite with expired tokens.
    let expired_tokens = TokenStorage {
        access_token: "expired-token".into(),
        refresh_token: None,
        expires_at: Utc::now() - chrono::Duration::seconds(100),
        token_type: "Bearer".into(),
    };
    client.save_tokens(&expired_tokens).unwrap();

    let loaded2 = client.load_tokens().unwrap().unwrap();
    assert!(loaded2.is_expired());
    assert!(loaded2.refresh_token.is_none());
}

// ── Onboarding Status Integration ───────────────────────────────

#[test]
fn test_full_onboarding_flow() {
    let dir = TempDir::new().unwrap();
    let client = OAuthClient::new(dir.path());

    // Step 1: Fresh start — nothing configured.
    let status = client.load_onboarding_status();
    assert!(!status.oauth_configured);
    assert!(!status.tokens_stored);
    assert!(!status.folder_selected);
    assert!(!status.initial_shred_complete);

    // Step 2: OAuth configured.
    let mut status = OnboardingStatus {
        oauth_configured: true,
        ..Default::default()
    };
    client.save_onboarding_status(&status).unwrap();
    let loaded = client.load_onboarding_status();
    assert!(loaded.oauth_configured);
    assert!(!loaded.tokens_stored);

    // Step 3: Tokens stored.
    status.tokens_stored = true;
    client.save_onboarding_status(&status).unwrap();
    let loaded = client.load_onboarding_status();
    assert!(loaded.tokens_stored);

    // Step 4: Folder selected and accessible.
    status.folder_selected = true;
    status.folder_accessible = true;
    status.selected_folder_id = Some("folder_123".into());
    status.selected_folder_name = Some("Lesson Plans 2026".into());
    client.save_onboarding_status(&status).unwrap();
    let loaded = client.load_onboarding_status();
    assert!(loaded.folder_selected);
    assert!(loaded.folder_accessible);
    assert_eq!(loaded.selected_folder_id, Some("folder_123".into()));
    assert_eq!(
        loaded.selected_folder_name,
        Some("Lesson Plans 2026".into())
    );

    // Step 5: Initial shred complete.
    status.initial_shred_complete = true;
    client.save_onboarding_status(&status).unwrap();
    let loaded = client.load_onboarding_status();
    assert!(loaded.initial_shred_complete);

    // All steps complete.
    assert!(loaded.oauth_configured);
    assert!(loaded.tokens_stored);
    assert!(loaded.folder_selected);
    assert!(loaded.folder_accessible);
    assert!(loaded.initial_shred_complete);
}

#[test]
fn test_onboarding_persistence_across_clients() {
    let dir = TempDir::new().unwrap();

    // Client 1 saves status.
    let client1 = OAuthClient::new(dir.path());
    let status = OnboardingStatus {
        oauth_configured: true,
        tokens_stored: true,
        folder_selected: true,
        folder_accessible: true,
        initial_shred_complete: true,
        selected_folder_id: Some("abc".into()),
        selected_folder_name: Some("Plans".into()),
    };
    client1.save_onboarding_status(&status).unwrap();

    // Client 2 (new instance) reads the same status.
    let client2 = OAuthClient::new(dir.path());
    let loaded = client2.load_onboarding_status();
    assert!(loaded.initial_shred_complete);
    assert_eq!(loaded.selected_folder_id, Some("abc".into()));
}

// ── Admin + Database Integration ────────────────────────────────

#[test]
fn test_admin_with_database_post_shred_flow() {
    // Simulate what happens after the admin agent's initial shred:
    // lesson plans get inserted into the database.
    let db = Database::open_in_memory().unwrap();

    // Create a subject (discovered from folder structure).
    let subject = db
        .create_subject(&NewSubject {
            name: "World History".into(),
            grade_level: Some("10th".into()),
            description: Some("AP World History lessons".into()),
        })
        .unwrap();

    // Create lesson plans (shredded from Google Doc tables).
    let plan1 = db
        .create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Ancient Civilizations".into(),
            content: Some("Mesopotamia, Egypt, Indus Valley...".into()),
            source_doc_id: Some("google-doc-id-001".into()),
            source_table_index: Some(0),
            learning_objectives: Some("Identify key ancient civilizations".into()),
        })
        .unwrap();

    let plan2 = db
        .create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Medieval Europe".into(),
            content: Some("Feudalism, Black Death, Crusades...".into()),
            source_doc_id: Some("google-doc-id-001".into()),
            source_table_index: Some(1),
            learning_objectives: Some("Analyze medieval European society".into()),
        })
        .unwrap();

    // Verify plans are queryable.
    let plans = db.list_lesson_plans_by_subject(&subject.id).unwrap();
    assert_eq!(plans.len(), 2);

    // Verify plan details.
    let fetched = db.get_lesson_plan(&plan1.id).unwrap();
    assert_eq!(fetched.title, "Ancient Civilizations");
    assert_eq!(fetched.source_doc_id, Some("google-doc-id-001".into()));
    assert_eq!(fetched.source_table_index, Some(0));
    assert_eq!(fetched.status, "draft");

    // After shredding, plans get published.
    db.update_lesson_plan_status(&plan1.id, "published").unwrap();
    db.update_lesson_plan_status(&plan2.id, "published").unwrap();

    let published = db.get_lesson_plan(&plan1.id).unwrap();
    assert_eq!(published.status, "published");
}

#[test]
fn test_admin_database_with_vectors() {
    // Simulate post-shred vector embedding storage.
    let db = Database::open_in_memory().unwrap();

    // Recreate vec table with smaller dimension for testing.
    db.with_conn(|conn| {
        conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
        conn.execute_batch(
            "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
        )?;
        Ok(())
    })
    .unwrap();

    let subject = db
        .create_subject(&NewSubject {
            name: "Science".into(),
            grade_level: None,
            description: None,
        })
        .unwrap();

    let plan1 = db
        .create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Photosynthesis".into(),
            content: Some("Plants convert sunlight...".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

    let plan2 = db
        .create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Cellular Respiration".into(),
            content: Some("Cells break down glucose...".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

    // Store embeddings (simulating what the admin shredder would do).
    db.upsert_embedding(&plan1.id, &[1.0, 0.0, 0.0, 0.0])
        .unwrap();
    db.upsert_embedding(&plan2.id, &[0.0, 1.0, 0.0, 0.0])
        .unwrap();

    // Semantic search for similar plans.
    let results = db.search_similar(&[0.9, 0.1, 0.0, 0.0], 2).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].lesson_plan_id, plan1.id);
}

// ── Token Access Async Integration ──────────────────────────────

#[tokio::test]
async fn test_get_valid_access_token_with_valid_file() {
    let dir = TempDir::new().unwrap();
    let token_file = dir.path().join("tokens.json");

    let tokens = TokenStorage {
        access_token: "integration-valid-token".into(),
        refresh_token: Some("refresh".into()),
        expires_at: Utc::now() + chrono::Duration::seconds(7200),
        token_type: "Bearer".into(),
    };
    fs::write(&token_file, serde_json::to_string(&tokens).unwrap()).unwrap();

    let config = OAuthConfig::default();
    let result = get_valid_access_token(&config, &token_file).await.unwrap();
    assert_eq!(result, "integration-valid-token");
}

#[tokio::test]
async fn test_get_valid_access_token_no_tokens_file() {
    let dir = TempDir::new().unwrap();
    let token_file = dir.path().join("missing.json");
    let config = OAuthConfig::default();
    let result = get_valid_access_token(&config, &token_file).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_valid_access_token_expired_no_refresh() {
    let dir = TempDir::new().unwrap();
    let token_file = dir.path().join("tokens.json");

    let tokens = TokenStorage {
        access_token: "old".into(),
        refresh_token: None,
        expires_at: Utc::now() - chrono::Duration::seconds(60),
        token_type: "Bearer".into(),
    };
    fs::write(&token_file, serde_json::to_string(&tokens).unwrap()).unwrap();

    let config = OAuthConfig::default();
    let result = get_valid_access_token(&config, &token_file).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expired"));
}

// ── Exchange Params Integration ─────────────────────────────────

#[test]
fn test_exchange_params_roundtrip() {
    let dir = TempDir::new().unwrap();
    let mut client = OAuthClient::new(dir.path());

    let config = OAuthConfig {
        client_id: "param-id".into(),
        client_secret: "param-secret".into(),
        redirect_uri: "http://localhost/cb".into(),
        scopes: vec!["scope1".into()],
    };
    client.save_config(&config).unwrap();
    client.load_config().unwrap();

    let (extracted_config, token_file, _pkce_verifier) = client.exchange_params();
    assert_eq!(extracted_config.client_id, "param-id");
    assert_eq!(extracted_config.client_secret, "param-secret");
    assert!(token_file.to_str().unwrap().contains("oauth_tokens.json"));
}

// ── Concurrent Client Access ────────────────────────────────────

#[test]
fn test_concurrent_status_access() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let dir = TempDir::new().unwrap();
    let client = Arc::new(Mutex::new(OAuthClient::new(dir.path())));

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let client = Arc::clone(&client);
            thread::spawn(move || {
                let c = client.lock().unwrap();
                let mut status = c.load_onboarding_status();
                status.oauth_configured = true;
                status.selected_folder_id = Some(format!("folder_{}", i));
                c.save_onboarding_status(&status).unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Verify final state is consistent.
    let c = client.lock().unwrap();
    let final_status = c.load_onboarding_status();
    assert!(final_status.oauth_configured);
    assert!(final_status.selected_folder_id.is_some());
}
