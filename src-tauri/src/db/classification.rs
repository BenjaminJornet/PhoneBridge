use std::path::Path;

pub(super) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn category_from_relative_path(path: &str) -> String {
    path.split(std::path::MAIN_SEPARATOR)
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("Other")
        .to_ascii_lowercase()
}

/// Map a file extension to one of the gallery's media buckets.
fn media_category_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "tif" | "tiff"
        | "dng" | "raw" | "cr2" | "nef" | "arw" | "rw2" | "orf" | "svg" => Some("photo"),
        "mp4" | "mov" | "m4v" | "mkv" | "avi" | "webm" | "3gp" | "3gpp" | "mpg" | "mpeg"
        | "wmv" | "flv" | "ts" | "m2ts" => Some("video"),
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "oga" | "opus" | "wma" | "amr"
        | "aiff" | "aif" | "mid" | "midi" => Some("music"),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt"
        | "ods" | "odp" | "csv" | "md" | "epub" | "pages" | "numbers" | "key" => Some("documents"),
        _ => None,
    }
}

/// Classify an indexed file into a gallery bucket (`photo`/`video`/`music`/`documents`),
/// extension first, then falling back to a leading `Photo/`, `Video/`, ... folder
/// (the layout produced by the SmartSwitch category sync). Anything else is `other`.
pub fn classify_media_category(relative_path: &str, extension: Option<&str>) -> String {
    if let Some(extension) = extension {
        if let Some(category) = media_category_for_extension(&extension.to_ascii_lowercase()) {
            return category.to_string();
        }
    }

    match category_from_relative_path(relative_path).as_str() {
        leading @ ("photo" | "video" | "music" | "documents") => leading.to_string(),
        _ => "other".to_string(),
    }
}

pub(super) fn source_from_relative_path(path: &str) -> String {
    let mut parts = path.split(std::path::MAIN_SEPARATOR);
    let _category = parts.next();
    let Some(source) = parts.next() else {
        return "local".to_string();
    };

    if parts.next().is_none() {
        return "local".to_string();
    }

    source.to_string()
}
