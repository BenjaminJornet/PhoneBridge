# Security Policy

PhoneBridge handles sensitive local data: photos, contacts, call logs, messages, device identifiers, and backup keys.

## Reporting a vulnerability

Please do not open a public issue for vulnerabilities or data exposure reports.

Use GitHub's private vulnerability reporting if available on the repository, or contact the maintainer privately with:

- affected version or commit
- reproduction steps
- impact summary
- whether personal data, keys, or paths were exposed

## Security principles

- PhoneBridge is local-first and must not upload user data.
- Backup keys are user-provided only; the app must not root devices or extract keys automatically.
- Real backups, device serials, account identifiers, contacts, messages, and media must never be committed as fixtures.
- Prefer anonymized synthetic fixtures for parser tests.
