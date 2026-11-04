# Contributing to Kynex Shell

Thank you for your interest in contributing to Kynex Shell!

## Development Workflow

1. **Branching**: Branch from `main` using descriptive branch names (`feat/<feature>`, `fix/<bug>`, `chore/<task>`).
2. **Conventional Commits**: All commit messages must follow the [Conventional Commits](https://www.conventionalcommits.org/) specification (e.g., `feat(shell): add battery indicator`, `fix(style): prevent path traversal`).
3. **Signed Commits**: All commits must be cryptographically signed (`git config commit.gpgsign true`).
4. **Verification Gate**: Before pushing or submitting a pull request, run:
   ```bash
   ./scripts/verify.sh
   ```
   All 10 verification gates (rustfmt, clippy with zero warnings, unit tests, cargo-deny, security audit, secret scan, typos, shellcheck, desktop-file-validate) must pass.
5. **Dependency Policy**: We strictly prefer permissive licenses (**MIT**, **BSD-2/3-Clause**, **Apache-2.0**, **ISC**) for third-party dependencies and libraries. Any dependency requiring an exception must be justified.

---

## Contributor License Agreement (CLA) & Grant of Rights

To protect Kynex Shell against proprietary forks by third-party companies, ensure long-term architectural stewardship, and preserve the author's ability to maintain and evolve the project, all contributions are submitted under the following agreement:

1. **Inbound License**: Your contributions are submitted under the terms of the **GNU General Public License v3.0 or later (GPL-3.0-or-later)**.
2. **Grant of Rights**: By submitting a pull request, patch, or code contribution to this repository, you grant **Saif Mnasri** (the project author and copyright holder) an irrevocable, perpetual, worldwide, non-exclusive, royalty-free, transferable, and sublicensable license to use, reproduce, modify, display, distribute, and re-license your contributions (in whole or in part, standalone or incorporated into Kynex Shell and derivative works) under any license terms chosen by the project author, including without limitation:
   - Permissive open-source licenses (such as MIT, BSD, or Apache-2.0);
   - Copyleft open-source licenses (such as GPL, AGPL, or LGPL);
   - Commercial, enterprise, proprietary, or closed-source license terms.
3. **Original Work & Authority**: You represent that your contribution is your original work, or that you have the necessary rights and permissions from the copyright owner to submit the contribution under these terms.
4. **Attribution**: The project author agrees that your authorship and public git commit history will be permanently preserved as attribution for your contributions.
