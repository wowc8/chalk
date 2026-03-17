//! Integration tests for the AI Admin Agent.
//!
//! These tests verify the end-to-end flow of the admin onboarding process:
//! OAuth configuration, token persistence, onboarding status tracking,
//! and the interaction between admin and database modules.
//!
//! After the connector architecture refactor, OAuth types live in
//! connectors::google_drive and onboarding status is managed by admin::oauth
//! using standalone functions (no longer OAuthClient struct).

use chalk_lib::connectors::google_drive::{
    get_valid_access_token, GoogleDriveConnector, OAuthConfig, TokenStorage,
};
use chalk_lib::connectors::{AuthStatus, ConnectorConfig, LessonPlanConnector};
use chalk_lib::database::{Database, NewLessonPlan, NewSubject};
use chrono::Utc;
use std::fs;
use tempfile::TempDir;

// ── GoogleDriveConnector Integration ────────────────────────────

#[test]
fn test_full_oauth_config_lifecycle() {
    let dir = TempDir::new().unwrap();
    let chalk_dir = dir.path().join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).unwrap();

    let config = ConnectorConfig {
        id: "gd-int-1".into(),
        connector_type: "google_drive".into(),
        display_name: "Integration Test Drive".into(),
        credentials: None,
        source_id: None,
        created_at: "2026-01-01".into(),
        last_sync_at: None,
    };

    let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();

    // Initially no config → empty client_id.
    let loaded = connector.oauth_config().unwrap();
    assert!(loaded.client_id.is_empty());

    // Save a config.
    let oauth_config = OAuthConfig {
        client_id: "integration-test-id".into(),
        client_secret: "integration-test-secret".into(),
        redirect_uri: "http://localhost:1420/oauth/callback".into(),
        scopes: vec![
            "https://www.googleapis.com/auth/drive.readonly".into(),
            "https://www.googleapis.com/auth/documents.readonly".into(),
        ],
    };
    connector.save_oauth_config(&oauth_config).unwrap();

    // Create a new connector and verify config persisted.
    let connector2 = GoogleDriveConnector::new(&config, dir.path()).unwrap();
    connector2.load_oauth_config().unwrap();
    let loaded2 = connector2.oauth_config().unwrap();
    assert_eq!(loaded2.client_id, "integration-test-id");
    assert_eq!(loaded2.client_secret, "integration-test-secret");
    assert_eq!(loaded2.scopes.len(), 2);

    // Authorization URL contains the saved client_id.
    let url = connector2.get_authorization_url().unwrap();
    assert!(url.contains("integration-test-id"));
    assert!(url.contains("access_type=offline"));
}

#[test]
fn test_connector_auth_status_lifecycle() {
    let dir = TempDir::new().unwrap();
    let chalk_dir = dir.path().join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).unwrap();

    let config = ConnectorConfig {
        id: "gd-int-2".into(),
        connector_type: "google_drive".into(),
        display_name: "Test".into(),
        credentials: None,
        source_id: None,
        created_at: "2026-01-01".into(),
        last_sync_at: None,
    };

    // No tokens → disconnected.
    let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
    assert_eq!(connector.auth_status(), AuthStatus::Disconnected);

    // Write valid tokens.
    let tokens = TokenStorage {
        access_token: "integration-access-token".into(),
        refresh_token: Some("integration-refresh-token".into()),
        expires_at: Utc::now() + chrono::Duration::seconds(3600),
        token_type: "Bearer".into(),
    };
    fs::write(
        chalk_dir.join("oauth_tokens.json"),
        serde_json::to_string(&tokens).unwrap(),
    )
    .unwrap();

    // Re-create connector — should detect tokens.
    let connector2 = GoogleDriveConnector::new(&config, dir.path()).unwrap();
    assert_eq!(connector2.auth_status(), AuthStatus::Connected);

    // Disconnect.
    connector2.disconnect().unwrap();
    assert_eq!(connector2.auth_status(), AuthStatus::Disconnected);
    assert!(!chalk_dir.join("oauth_tokens.json").exists());
}

#[test]
fn test_connector_expired_tokens() {
    let dir = TempDir::new().unwrap();
    let chalk_dir = dir.path().join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).unwrap();

    let tokens = TokenStorage {
        access_token: "expired-token".into(),
        refresh_token: None,
        expires_at: Utc::now() - chrono::Duration::seconds(100),
        token_type: "Bearer".into(),
    };
    fs::write(
        chalk_dir.join("oauth_tokens.json"),
        serde_json::to_string(&tokens).unwrap(),
    )
    .unwrap();

    let config = ConnectorConfig {
        id: "gd-int-3".into(),
        connector_type: "google_drive".into(),
        display_name: "Test".into(),
        credentials: None,
        source_id: None,
        created_at: "2026-01-01".into(),
        last_sync_at: None,
    };
    let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
    assert_eq!(connector.auth_status(), AuthStatus::Expired);
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
    let chalk_dir = dir.path().join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).unwrap();

    let config = ConnectorConfig {
        id: "gd-int-ep".into(),
        connector_type: "google_drive".into(),
        display_name: "Test".into(),
        credentials: None,
        source_id: None,
        created_at: "2026-01-01".into(),
        last_sync_at: None,
    };
    let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();

    let oauth_cfg = OAuthConfig {
        client_id: "param-id".into(),
        client_secret: "param-secret".into(),
        redirect_uri: "http://localhost/cb".into(),
        scopes: vec!["scope1".into()],
    };
    connector.save_oauth_config(&oauth_cfg).unwrap();
    connector.load_oauth_config().unwrap();

    let (extracted_config, token_file, _pkce_verifier) = connector.exchange_params().unwrap();
    assert_eq!(extracted_config.client_id, "param-id");
    assert_eq!(extracted_config.client_secret, "param-secret");
    assert!(token_file.to_str().unwrap().contains("oauth_tokens.json"));
}

// ── Connector Trait Integration ─────────────────────────────────

#[test]
fn test_connector_info_from_config() {
    let dir = TempDir::new().unwrap();
    let config = ConnectorConfig {
        id: "gd-info-test".into(),
        connector_type: "google_drive".into(),
        display_name: "Teacher's Drive".into(),
        credentials: None,
        source_id: None,
        created_at: "2026-01-01".into(),
        last_sync_at: None,
    };
    let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
    let info = connector.info();
    assert_eq!(info.id, "gd-info-test");
    assert_eq!(info.connector_type, "google_drive");
    assert_eq!(info.display_name, "Teacher's Drive");
    assert_eq!(info.icon, "google-drive");
}
