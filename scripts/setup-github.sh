#!/usr/bin/env bash
# Creates the private GitHub repo, pushes, and enables protections that the account/plan allows.
# Usage: scripts/setup-github.sh [repo-name]   (default: kynexos)
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

name="${1:-kynexos}"
git rev-parse --git-dir >/dev/null 2>&1 || { echo "Run 'git init' and make the first commit first." >&2; exit 1; }
git rev-parse HEAD >/dev/null 2>&1 || { echo "Make the first commit first." >&2; exit 1; }

if gh repo view >/dev/null 2>&1; then
  echo "Repository already linked."
else
  gh repo create "$name" --private --source=. --remote=origin --push
fi
slug="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
echo "Repository: $slug"

echo "Enabling Dependabot alerts and security updates..."
gh api -X PUT "repos/$slug/vulnerability-alerts" >/dev/null 2>&1 || echo "  (could not enable alerts)"
gh api -X PUT "repos/$slug/automated-security-fixes" >/dev/null 2>&1 || echo "  (could not enable security updates)"

echo "Enabling secret scanning + push protection (needs public repo or paid plan)..."
gh api -X PATCH "repos/$slug" \
  -f 'security_and_analysis[secret_scanning][status]=enabled' \
  -f 'security_and_analysis[secret_scanning_push_protection][status]=enabled' >/dev/null 2>&1 \
  || echo "  (not available for this repo/plan; local ripsecrets hook + CI gitleaks still protect you)"

echo "Protecting main..."
if gh api -X PUT "repos/$slug/branches/main/protection" --input - >/dev/null 2>&1 <<'JSON'
{
  "required_status_checks": {"strict": true, "contexts": ["verify", "secret-scan"]},
  "enforce_admins": true,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
then
  echo "  branch protection enabled"
  gh api -X POST "repos/$slug/branches/main/protection/required_signatures" >/dev/null 2>&1 \
    && echo "  signed commits required" || echo "  (could not require signed commits)"
else
  echo "  Branch protection is not available for private repos on a free plan."
  echo "  Options: upgrade, make the repo public, or rely on the local hooks (they block direct commits to main)."
fi
echo "Done."
