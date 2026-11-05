#!/usr/bin/env bash
# Configures GitHub settings, protections, security alerts, and branch policies for kynex-shell.
# Usage: scripts/setup-github.sh [repo-name]   (default: kynex-shell)
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

name="${1:-kynex-shell}"
git rev-parse --git-dir >/dev/null 2>&1 || { echo "Run 'git init' and make the first commit first." >&2; exit 1; }
git rev-parse HEAD >/dev/null 2>&1 || { echo "Make the first commit first." >&2; exit 1; }

if gh repo view >/dev/null 2>&1; then
  echo "Repository already linked."
else
  gh repo create "$name" --public --source=. --remote=origin --push
fi
slug="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
echo "Repository: $slug"

echo "Enabling Dependabot alerts..."
gh api -X PUT "repos/$slug/vulnerability-alerts" \
  && echo "  Dependabot vulnerability alerts enabled." || echo "  (could not enable alerts)"

echo "Enabling automated security fixes..."
gh api -X PUT "repos/$slug/automated-security-fixes" \
  && echo "  Automated security fixes enabled." || echo "  (could not enable security updates)"

echo "Enabling secret scanning and push protection..."
gh api -X PATCH "repos/$slug" \
  -f 'security_and_analysis[secret_scanning][status]=enabled' \
  -f 'security_and_analysis[secret_scanning_push_protection][status]=enabled' \
  && echo "  Secret scanning and push protection enabled." || echo "  (could not enable secret scanning)"

echo "Applying branch protection on main..."
gh api -X PUT "repos/$slug/branches/main/protection" --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["verify", "secret-scan"],
    "checks": [
      {
        "context": "verify",
        "app_id": 15368
      },
      {
        "context": "secret-scan",
        "app_id": 15368
      }
    ]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": false,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
echo "  Branch protection configured on main: PR required (0 approvals), strict status checks (verify [15368], secret-scan [15368]), enforce_admins true, linear history, no force push, no deletion."

echo "Requiring signed commits on main..."
gh api -X POST "repos/$slug/branches/main/protection/required_signatures" \
  && echo "  Signed commits required." || echo "  (could not require signed commits)"

echo "GitHub setup and protections complete."
