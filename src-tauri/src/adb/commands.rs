use crate::adapters::AdapterError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn adb_pull(
    serial: &str,
    remote_path: &str,
    local_path: &Path,
) -> Result<(), AdapterError> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = Command::new(resolve_adb_command()?)
        .args(["-s", serial, "pull", remote_path])
        .arg(local_path)
        .output();
    let Ok(output) = output else {
        return Err(AdapterError::CommandUnavailable("adb".to_string()));
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AdapterError::CommandFailed(format!(
            "adb pull {remote_path}: {stderr}"
        )));
    }
    Ok(())
}

pub(super) fn adb_getprop(serial: &str, property: &str) -> Option<String> {
    adb_output(&["-s", serial, "shell", "getprop", property])
        .ok()
        .map(|value| value.trim().to_string())
}

pub(super) fn adb_output(args: &[&str]) -> Result<String, AdapterError> {
    let output = Command::new(resolve_adb_command()?).args(args).output();
    let Ok(output) = output else {
        return Err(AdapterError::CommandUnavailable("adb".to_string()));
    };
    if !output.status.success() {
        return Err(AdapterError::CommandFailed(format!(
            "adb {}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run `adb -s <serial> shell <command>` and return stdout, tolerating non-zero exits
/// (find/du report failures for unreadable subtrees we deliberately skip).
pub(super) fn adb_shell_capture(serial: &str, command: &str) -> String {
    let Ok(adb) = resolve_adb_command() else {
        return String::new();
    };
    Command::new(adb)
        .args(["-s", serial, "shell", command])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

pub(super) fn resolve_adb_command() -> Result<PathBuf, AdapterError> {
    let candidates = adb_command_candidates();

    for candidate in candidates {
        if adb_candidate_works(&candidate) {
            return Ok(candidate);
        }
    }

    Err(AdapterError::CommandUnavailable("adb".to_string()))
}

pub(super) fn adb_command_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("ADB_PATH").filter(|value| !value.is_empty()) {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(PathBuf::from("adb"));
    for env_name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = env::var_os(env_name).filter(|value| !value.is_empty()) {
            candidates.push(PathBuf::from(root).join("platform-tools/adb"));
        }
    }

    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            candidates.push(PathBuf::from("/opt/homebrew/bin/adb"));
        }
        candidates.push(PathBuf::from("/usr/local/bin/adb"));
    }

    candidates
}

fn adb_candidate_works(candidate: &Path) -> bool {
    Command::new(candidate)
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_paths_are_checked_before_homebrew_fallbacks() {
        unsafe {
            env::set_var("ANDROID_HOME", "/android-sdk");
            env::remove_var("ANDROID_SDK_ROOT");
            env::remove_var("ADB_PATH");
        }

        let candidates = adb_command_candidates();
        let sdk_index = candidates
            .iter()
            .position(|candidate| candidate == &PathBuf::from("/android-sdk/platform-tools/adb"))
            .unwrap();
        let usr_local_index = candidates
            .iter()
            .position(|candidate| candidate == &PathBuf::from("/usr/local/bin/adb"));

        if let Some(usr_local_index) = usr_local_index {
            assert!(sdk_index < usr_local_index);
        }
    }
}
