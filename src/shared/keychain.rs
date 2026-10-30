use crate::shared::errors::{AppError, AppResult};

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
pub fn read(service: &str, account: &str) -> AppResult<Option<String>> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .map_err(|_| AppError::InternalError("failed to run macOS Keychain command".into()))?;

    if output.status.success() {
        let value = String::from_utf8(output.stdout).map_err(|_| AppError::InternalError("Keychain value is not valid UTF-8".into()))?;
        return Ok(Some(value.trim_end_matches(['\r', '\n']).to_string()));
    }

    if output.status.code() == Some(44) {
        return Ok(None);
    }

    Err(AppError::InternalError("failed to read macOS Keychain item".into()))
}

#[cfg(not(target_os = "macos"))]
pub fn read(_service: &str, _account: &str) -> AppResult<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
pub fn write(service: &str, account: &str, value: &str) -> AppResult<()> {
    let status = Command::new("/usr/bin/security")
        .args(["add-generic-password", "-U", "-s", service, "-a", account, "-w", value])
        .status()
        .map_err(|_| AppError::InternalError("failed to run macOS Keychain command".into()))?;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::InternalError("failed to write macOS Keychain item".into()))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn write(_service: &str, _account: &str, _value: &str) -> AppResult<()> {
    Err(AppError::InternalError("macOS Keychain is unavailable on this platform".into()))
}
