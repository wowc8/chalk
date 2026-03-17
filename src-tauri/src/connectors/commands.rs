// Tauri commands for the connector system — thin wrappers through the Dispatcher.

use tauri::State;

use super::ConnectorInfo;
use crate::AppState;

#[tauri::command]
pub async fn list_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectorInfo>, String> {
    let dispatcher = state.dispatcher.lock().map_err(|e| e.to_string())?;
    Ok(dispatcher.list_available())
}

#[tauri::command]
pub async fn list_connected_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectorInfo>, String> {
    let dispatcher = state.dispatcher.lock().map_err(|e| e.to_string())?;
    Ok(dispatcher.list_connected())
}

#[tauri::command]
pub async fn disconnect_connector(
    state: State<'_, AppState>,
    connector_id: String,
) -> Result<(), String> {
    let mut dispatcher = state.dispatcher.lock().map_err(|e| e.to_string())?;
    if let Some(connector) = dispatcher.get(&connector_id) {
        connector.disconnect().map_err(|e| e.to_string())?;
    }
    dispatcher.remove(&connector_id);
    Ok(())
}
