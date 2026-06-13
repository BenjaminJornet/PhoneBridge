use super::{AdapterDefinition, AdapterError, BackupAdapter, BackupSource};
#[cfg(test)]
use std::path::Path;

#[derive(Default)]
pub struct TakeoutAdapter;

impl BackupAdapter for TakeoutAdapter {
    fn definition(&self) -> AdapterDefinition {
        AdapterDefinition {
            id: "google-takeout",
            label: "Google Takeout",
            description: "Recognize Google Takeout exports selected from the filesystem.",
        }
    }

    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
fn looks_like_takeout_root(path: &Path) -> bool {
    path.join("Takeout").is_dir()
        || path.join("Google Photos").is_dir()
        || path.join("Archive Browser.html").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_common_takeout_layouts() {
        let temp = tempdir().unwrap();
        assert!(!looks_like_takeout_root(temp.path()));
        fs::create_dir_all(temp.path().join("Takeout/Google Photos")).unwrap();
        assert!(looks_like_takeout_root(temp.path()));
    }
}
