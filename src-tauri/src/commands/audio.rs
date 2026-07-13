//! Audio device and permission related commands.

use cpal::traits::{DeviceTrait, HostTrait};
use tauri::command;

#[command]
pub fn list_input_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .filter_map(|d| d.name().ok())
        .collect();
    Ok(devices)
}

#[command]
pub fn get_default_input_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|d| d.name().ok())
}
