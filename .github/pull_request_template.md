## Summary

<!-- Brief description of the changes introduced in this pull request -->

## Related Issues / Decisions

- Closes #
- Follows Owner Decisions: D-019..D-021

## Verification Checklist

- [ ] `./scripts/verify.sh` passed locally with zero errors and zero warnings
- [ ] `qmllint` passed on styles and core types with zero warnings
- [ ] Style token linter verified no hardcoded colors or pixel sizes in styles
- [ ] Headless smoke test (`./scripts/smoke-test.sh`) passed
- [ ] Nested session tested in `./dev/run-nested.sh` (if UI-related)
- [ ] No secrets or sensitive files tracked
- [ ] Commits are cryptographically signed and adhere to Conventional Commits

## Notice

Per project policy, this PR must not be merged until CI is green AND the project owner explicitly approves with "merge" in chat.
