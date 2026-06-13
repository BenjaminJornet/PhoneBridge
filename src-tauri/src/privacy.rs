pub fn redact_identifier(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 4 {
        return "[redacted]".to_string();
    }
    format!("[redacted:{}]", &trimmed[trimmed.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_last_four_characters() {
        assert_eq!(redact_identifier("RFCT816LGQN"), "[redacted:LGQN]");
        assert_eq!(redact_identifier("abc"), "[redacted]");
    }
}
