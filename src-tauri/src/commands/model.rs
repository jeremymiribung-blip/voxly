//! Model management commands (download, status, etc.).

use crate::model::ModelManager;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ensure_model_downloaded(
    manager: State<'_, Arc<ModelManager>>,
) -> Result<String, String> {
    let path = manager
        .ensure_primary_model()
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn is_model_downloaded(manager: State<Arc<ModelManager>>) -> bool {
    manager.is_primary_model_downloaded()
}
