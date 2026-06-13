# Contributing

Thanks for considering a contribution to PhoneBridge.

## Project Rules

- Keep user data local.
- Do not add analytics, cloud sync, or network uploads without an explicit design discussion.
- Do not commit real backup data or personal identifiers.
- Keep adapters isolated: generic Android logic belongs in `src-tauri/src/adb` or `src-tauri/src/adapters/adb_generic.rs`; Samsung-specific logic belongs in `src-tauri/src/adapters/smartswitch.rs` or `src-tauri/src/smartswitch`.
- Prefer small, tested changes.

## Verification

Before opening a PR, run:

```bash
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

If you touch parser code, add anonymized fixtures and targeted parser tests.
