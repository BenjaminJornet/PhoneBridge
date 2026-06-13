use crate::adb;

use super::{AdapterDefinition, AdapterError, BackupAdapter, BackupSource};

#[derive(Default)]
pub struct AdbGenericAdapter;

impl BackupAdapter for AdbGenericAdapter {
    fn definition(&self) -> AdapterDefinition {
        AdapterDefinition {
            id: "adb-generic",
            label: "Android device (ADB)",
            description: "Pull media directly from an authorized Android phone over USB debugging.",
        }
    }

    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError> {
        match adb::detect_devices() {
            Ok(sources) => Ok(sources),
            Err(AdapterError::CommandUnavailable(_)) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }
}
