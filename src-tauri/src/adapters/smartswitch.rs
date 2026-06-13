use super::{AdapterDefinition, AdapterError, BackupAdapter, BackupSource};

#[derive(Default)]
pub struct SmartSwitchAdapter;

impl BackupAdapter for SmartSwitchAdapter {
    fn definition(&self) -> AdapterDefinition {
        AdapterDefinition {
            id: "samsung-smartswitch",
            label: "Samsung SmartSwitch",
            description:
                "Import media and structured inventories from a user-selected SmartSwitch backup.",
        }
    }

    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
fn timestamp_from_name(name: &str) -> Option<String> {
    let raw = name.rsplit_once('_')?.1;
    if raw.len() != 14 || !raw.chars().all(|item| item.is_ascii_digit()) {
        return None;
    }

    Some(format!(
        "{}-{}-{}T{}:{}:{}",
        &raw[0..4],
        &raw[4..6],
        &raw[6..8],
        &raw[8..10],
        &raw[10..12],
        &raw[12..14]
    ))
}

#[cfg(test)]
mod tests {
    use super::timestamp_from_name;

    #[test]
    fn parses_samsung_backup_timestamp() {
        assert_eq!(
            timestamp_from_name("SM-X000A_20250102030405"),
            Some("2025-01-02T03:04:05".to_string())
        );
    }
}
