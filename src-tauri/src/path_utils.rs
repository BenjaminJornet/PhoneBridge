use std::env;
use std::path::PathBuf;

pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::expand_home;
    use std::path::PathBuf;

    #[test]
    fn leaves_plain_paths_unchanged() {
        assert_eq!(expand_home("/tmp/example"), PathBuf::from("/tmp/example"));
        assert_eq!(expand_home("relative/path"), PathBuf::from("relative/path"));
    }

    #[test]
    fn expands_tilde_to_home() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };

        assert_eq!(expand_home("~"), PathBuf::from(&home));
        assert_eq!(
            expand_home("~/library"),
            PathBuf::from(&home).join("library")
        );
    }
}
