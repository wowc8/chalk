// ConnectorDispatcher — manages all active connector instances and routes calls.

use std::collections::HashMap;

use super::{AuthStatus, ConnectorInfo, LessonPlanConnector};

pub struct ConnectorDispatcher {
    registry: HashMap<String, Box<dyn LessonPlanConnector>>,
}

impl ConnectorDispatcher {
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    /// Register a connector instance. Uses the connector's info().id as the key.
    pub fn register(&mut self, connector: Box<dyn LessonPlanConnector>) {
        let id = connector.info().id.clone();
        tracing::info!(connector_id = id.as_str(), "Registering connector");
        self.registry.insert(id, connector);
    }

    /// Get a reference to a connector by its ID.
    pub fn get(&self, id: &str) -> Option<&dyn LessonPlanConnector> {
        self.registry.get(id).map(|c| c.as_ref())
    }

    /// Remove a connector by its ID.
    pub fn remove(&mut self, id: &str) -> Option<Box<dyn LessonPlanConnector>> {
        tracing::info!(connector_id = id, "Removing connector");
        self.registry.remove(id)
    }

    /// List info for all registered connectors.
    pub fn list_available(&self) -> Vec<ConnectorInfo> {
        self.registry.values().map(|c| c.info()).collect()
    }

    /// List info for only authenticated (connected) connectors.
    pub fn list_connected(&self) -> Vec<ConnectorInfo> {
        self.registry
            .values()
            .filter(|c| c.auth_status() == AuthStatus::Connected)
            .map(|c| c.info())
            .collect()
    }

    /// Get the number of registered connectors.
    pub fn count(&self) -> usize {
        self.registry.len()
    }

    /// Check if a connector with the given ID is registered.
    pub fn has(&self, id: &str) -> bool {
        self.registry.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{
        ConnectorConfig, ConnectorError, Document, FreshnessStatus, Source,
    };
    use std::sync::Mutex;

    /// A simple mock connector for testing the dispatcher.
    struct MockConnector {
        info: ConnectorInfo,
        connected: Mutex<bool>,
    }

    impl MockConnector {
        fn new(id: &str, name: &str, connected: bool) -> Self {
            Self {
                info: ConnectorInfo {
                    id: id.to_string(),
                    connector_type: "mock".to_string(),
                    display_name: name.to_string(),
                    icon: "mock".to_string(),
                    description: "Mock connector for testing".to_string(),
                },
                connected: Mutex::new(connected),
            }
        }
    }

    impl LessonPlanConnector for MockConnector {
        fn info(&self) -> ConnectorInfo {
            self.info.clone()
        }

        fn auth_status(&self) -> AuthStatus {
            if *self.connected.lock().unwrap() {
                AuthStatus::Connected
            } else {
                AuthStatus::Disconnected
            }
        }

        fn authenticate(&self) -> Result<AuthStatus, ConnectorError> {
            *self.connected.lock().unwrap() = true;
            Ok(AuthStatus::Connected)
        }

        fn disconnect(&self) -> Result<(), ConnectorError> {
            *self.connected.lock().unwrap() = false;
            Ok(())
        }

        fn list_sources(
            &self,
            _parent_id: Option<&str>,
        ) -> Result<Vec<Source>, ConnectorError> {
            Ok(vec![])
        }

        fn fetch_document(&self, _id: &str) -> Result<Document, ConnectorError> {
            Err(ConnectorError::Other("Not implemented".into()))
        }

        fn check_freshness(&self, _id: &str) -> Result<FreshnessStatus, ConnectorError> {
            Err(ConnectorError::Other("Not implemented".into()))
        }
    }

    #[test]
    fn test_dispatcher_register_and_get() {
        let mut dispatcher = ConnectorDispatcher::new();
        let mock = MockConnector::new("mock-1", "Mock Drive", true);
        dispatcher.register(Box::new(mock));

        assert!(dispatcher.has("mock-1"));
        assert_eq!(dispatcher.count(), 1);

        let connector = dispatcher.get("mock-1").unwrap();
        assert_eq!(connector.info().display_name, "Mock Drive");
    }

    #[test]
    fn test_dispatcher_remove() {
        let mut dispatcher = ConnectorDispatcher::new();
        dispatcher.register(Box::new(MockConnector::new("mock-1", "Mock 1", true)));

        assert!(dispatcher.has("mock-1"));
        let removed = dispatcher.remove("mock-1");
        assert!(removed.is_some());
        assert!(!dispatcher.has("mock-1"));
        assert_eq!(dispatcher.count(), 0);
    }

    #[test]
    fn test_dispatcher_list_available() {
        let mut dispatcher = ConnectorDispatcher::new();
        dispatcher.register(Box::new(MockConnector::new("m-1", "Drive 1", true)));
        dispatcher.register(Box::new(MockConnector::new("m-2", "Drive 2", false)));

        let available = dispatcher.list_available();
        assert_eq!(available.len(), 2);
    }

    #[test]
    fn test_dispatcher_list_connected() {
        let mut dispatcher = ConnectorDispatcher::new();
        dispatcher.register(Box::new(MockConnector::new("m-1", "Connected", true)));
        dispatcher.register(Box::new(MockConnector::new("m-2", "Disconnected", false)));
        dispatcher.register(Box::new(MockConnector::new("m-3", "Also Connected", true)));

        let connected = dispatcher.list_connected();
        assert_eq!(connected.len(), 2);
    }

    #[test]
    fn test_dispatcher_get_nonexistent() {
        let dispatcher = ConnectorDispatcher::new();
        assert!(dispatcher.get("nope").is_none());
    }

    #[test]
    fn test_dispatcher_empty() {
        let dispatcher = ConnectorDispatcher::new();
        assert_eq!(dispatcher.count(), 0);
        assert!(dispatcher.list_available().is_empty());
        assert!(dispatcher.list_connected().is_empty());
    }
}
