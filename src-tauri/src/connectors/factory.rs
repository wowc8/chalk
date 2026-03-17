// ConnectorFactory — knows how to instantiate each connector type from stored config.

use std::path::Path;

use super::google_drive::GoogleDriveConnector;
use super::{ConnectorConfig, ConnectorError, LessonPlanConnector};

pub struct ConnectorFactory;

impl ConnectorFactory {
    /// Create a single connector instance from a stored config.
    pub fn create(
        config: &ConnectorConfig,
        data_dir: &Path,
    ) -> Result<Box<dyn LessonPlanConnector>, ConnectorError> {
        match config.connector_type.as_str() {
            "google_drive" => Ok(Box::new(GoogleDriveConnector::new(config, data_dir)?)),
            // Future connectors:
            // "onedrive"    => Ok(Box::new(OneDriveConnector::new(config, data_dir)?)),
            // "local_files" => Ok(Box::new(LocalFilesConnector::new(config, data_dir)?)),
            // "canvas_lms"  => Ok(Box::new(CanvasLmsConnector::new(config, data_dir)?)),
            other => Err(ConnectorError::Other(format!(
                "Unknown connector type: {}",
                other
            ))),
        }
    }

    /// Create connector instances from all stored configs.
    pub fn create_all(
        configs: &[ConnectorConfig],
        data_dir: &Path,
    ) -> Vec<Box<dyn LessonPlanConnector>> {
        configs
            .iter()
            .filter_map(|config| {
                match Self::create(config, data_dir) {
                    Ok(connector) => Some(connector),
                    Err(e) => {
                        tracing::warn!(
                            connector_type = config.connector_type.as_str(),
                            connector_id = config.id.as_str(),
                            error = %e,
                            "Failed to create connector, skipping"
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_google_drive_connector() {
        let dir = TempDir::new().unwrap();
        let config = ConnectorConfig {
            id: "gd-factory-1".into(),
            connector_type: "google_drive".into(),
            display_name: "My Drive".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = ConnectorFactory::create(&config, dir.path()).unwrap();
        assert_eq!(connector.info().connector_type, "google_drive");
    }

    #[test]
    fn test_create_unknown_connector() {
        let dir = TempDir::new().unwrap();
        let config = ConnectorConfig {
            id: "unk-1".into(),
            connector_type: "dropbox".into(),
            display_name: "Dropbox".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let result = ConnectorFactory::create(&config, dir.path());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown connector type"));
    }

    #[test]
    fn test_create_all_skips_failures() {
        let dir = TempDir::new().unwrap();
        let configs = vec![
            ConnectorConfig {
                id: "gd-all-1".into(),
                connector_type: "google_drive".into(),
                display_name: "Drive 1".into(),
                credentials: None,
                source_id: None,
                created_at: "2026-01-01".into(),
                last_sync_at: None,
            },
            ConnectorConfig {
                id: "bad-1".into(),
                connector_type: "nonexistent".into(),
                display_name: "Bad".into(),
                credentials: None,
                source_id: None,
                created_at: "2026-01-01".into(),
                last_sync_at: None,
            },
            ConnectorConfig {
                id: "gd-all-2".into(),
                connector_type: "google_drive".into(),
                display_name: "Drive 2".into(),
                credentials: None,
                source_id: None,
                created_at: "2026-01-01".into(),
                last_sync_at: None,
            },
        ];
        let connectors = ConnectorFactory::create_all(&configs, dir.path());
        assert_eq!(connectors.len(), 2);
    }
}
