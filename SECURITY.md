# Security policy

Report vulnerabilities privately through GitHub: the repository's Security tab -> "Report a vulnerability" (private advisories). Do not open public issues for security problems.

We aim to acknowledge reports within 72 hours and to publish fixes with an advisory. Supported versions: the latest release and `main`.

## Security Architecture & Verification

Kynex Shell follows defense-in-depth principles:
- **Rust logic**: Memory-safe implementation for daemons, services, and core shell logic.
- **D-Bus sandboxing**: Systemd user service hardening with restricted system calls and address families.
- **Automated security gates**: Every commit is verified against `cargo-deny`, `cargo-audit` (RustSec advisory database), secret detection, and strict static analysis.
