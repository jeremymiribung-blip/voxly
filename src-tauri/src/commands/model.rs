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

#[tauri::command]
pub fn pause_model_download(manager: State<Arc<ModelManager>>) {
    manager.pause_download();
}

#[tauri::command]
pub fn resume_model_download(manager: State<Arc<ModelManager>>) {
    manager.resume_download();
}

#[tauri::command]
pub fn cancel_model_download(manager: State<Arc<ModelManager>>) {
    manager.cancel_download();
}

#[tauri::command]
pub fn get_model_path(manager: State<Arc<ModelManager>>) -> String {
    manager.primary_model_path().to_string_lossy().to_string()
}

#[tauri::command]
pub async fn delete_model(manager: State<'_, Arc<ModelManager>>) -> Result<bool, String> {
    let path = manager.primary_model_path();
    if path.exists() {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| e.to_string())?;
        // also remove any partials or meta if present
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn get_model_size(manager: State<'_, Arc<ModelManager>>) -> Result<u64, String> {
    let path = manager.primary_model_path();
    if path.exists() {
        let meta = tokio::fs::metadata(&path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(meta.len())
    } else {
        Ok(0)
    }
}
