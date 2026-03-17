// Connectors module — external API handlers (Google Drive, OneDrive, LMS).
//
// Implements the Factory + Dispatcher + Trait pattern from the spec.
// Phase 1 Polish: ConnectorDispatcher as Tauri managed state with token cache.

pub mod dispatcher;
pub mod types;

pub use dispatcher::ConnectorDispatcher;
pub use types::{AuthStatus, ConnectorInfo, ConnectionDetails};
