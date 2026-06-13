use super::{AdapterDefinition, AdapterError, BackupAdapter, BackupSource};

#[derive(Default)]
pub struct FolderAdapter;

impl BackupAdapter for FolderAdapter {
    fn definition(&self) -> AdapterDefinition {
        AdapterDefinition {
            id: "generic-folder",
            label: "Generic folder",
            description: "Use any user-selected folder as a local backup source.",
        }
    }

    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
fn classify_media_category(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "avif" | "gif" | "heic" | "jpeg" | "jpg" | "png" | "webp" => "photo",
        "3gp" | "avi" | "m4v" | "mkv" | "mov" | "mp4" | "webm" => "video",
        "aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav" => "music",
        _ => "documents",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_media_extensions() {
        assert_eq!(classify_media_category("JPG"), "photo");
        assert_eq!(classify_media_category("mp4"), "video");
        assert_eq!(classify_media_category("opus"), "music");
        assert_eq!(classify_media_category("pdf"), "documents");
    }
}
