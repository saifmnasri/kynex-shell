#!/usr/bin/env python3
"""Unit tests verifying PR-scoped GUARD CHANGES parity and edge cases (F4)."""

import os
import shutil
import subprocess
import tempfile
import unittest

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CHECK_REPORT_PY = os.path.join(ROOT_DIR, "scripts/check-report.py")
GUARD_DIFF_SH = os.path.join(ROOT_DIR, "scripts/guard-diff.sh")


class TestGuardDiffParity(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp(prefix="test_parity_")
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "config", "user.name", "Test Runner"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=self.test_dir, check=True)

        # Set up scripts and dummy files
        os.makedirs(os.path.join(self.test_dir, "scripts"), exist_ok=True)
        os.makedirs(os.path.join(self.test_dir, "docs/work/evidence"), exist_ok=True)
        os.makedirs(os.path.join(self.test_dir, "docs/work/claims"), exist_ok=True)
        shutil.copy(CHECK_REPORT_PY, os.path.join(self.test_dir, "scripts/check-report.py"))
        shutil.copy(GUARD_DIFF_SH, os.path.join(self.test_dir, "scripts/guard-diff.sh"))

        with open(os.path.join(self.test_dir, "_typos.toml"), "w") as f:
            f.write("# dummy typos config\n")
        with open(os.path.join(self.test_dir, "deny.toml"), "w") as f:
            f.write("# dummy deny config\n")

        subprocess.run(["git", "add", "."], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "initial commit"], cwd=self.test_dir, check=True)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _create_report(self, filename, guard_changes):
        bullets = "\n".join(f"- `{f}`: justification" for f in guard_changes)
        content = (
            "# Test Report\n"
            "## 1. Executive Summary\n"
            "- Test executive summary bullet {claim:claim-dummy}\n\n"
            "## 8. GUARD CHANGES\n"
            f"{bullets}\n"
        )
        path = os.path.join(self.test_dir, f"docs/work/{filename}")
        with open(path, "w") as f:
            f.write(content)

        claim_file = os.path.join(self.test_dir, "docs/work/claims/stage-test.toml")
        with open(claim_file, "w") as f:
            f.write('[claims.claim-dummy]\nuntested = "verified dummy claim"\n')

        return path

    def test_a_pr_scoped_mismatch_fails(self):
        """(a) PR-scoped case with report in range and mismatching GUARD CHANGES fails."""
        subprocess.run(["git", "checkout", "-q", "-b", "feat/test-a"], cwd=self.test_dir, check=True)
        with open(os.path.join(self.test_dir, "_typos.toml"), "a") as f:
            f.write("# changed in PR\n")
        with open(os.path.join(self.test_dir, "deny.toml"), "a") as f:
            f.write("# also changed in PR\n")

        # Report lists ONLY _typos.toml, omitting deny.toml and stage-test.toml
        report_path = self._create_report("stage-test-report.md", ["_typos.toml"])
        subprocess.run(["git", "add", "."], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: changes"], cwd=self.test_dir, check=True)

        script = os.path.join(self.test_dir, "scripts/check-report.py")
        res = subprocess.run([script, report_path], cwd=self.test_dir, capture_output=True, text=True)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("missing from GUARD CHANGES", res.stderr)
        self.assertIn("deny.toml", res.stderr)

    def test_b_matching_list_passes(self):
        """(b) PR-scoped case with matching GUARD CHANGES passes."""
        subprocess.run(["git", "checkout", "-q", "-b", "feat/test-b"], cwd=self.test_dir, check=True)
        with open(os.path.join(self.test_dir, "_typos.toml"), "a") as f:
            f.write("# changed in PR\n")

        report_path = self._create_report("stage-test-report.md", ["_typos.toml", "docs/work/claims/stage-test.toml"])
        subprocess.run(["git", "add", "."], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: changes"], cwd=self.test_dir, check=True)

        script = os.path.join(self.test_dir, "scripts/check-report.py")
        res = subprocess.run([script, report_path], cwd=self.test_dir, capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Expected 0, got {res.returncode}:\nstdout: {res.stdout}\nstderr: {res.stderr}")
        self.assertIn("match scripts/guard-diff.sh exactly", res.stdout)

    def test_c_squash_merged_main_skips_and_passes(self):
        """(c) Squash-merged main with no report in range prints SKIPPED and passes."""
        subprocess.run(["git", "checkout", "-q", "-b", "feat/test-c"], cwd=self.test_dir, check=True)
        with open(os.path.join(self.test_dir, "_typos.toml"), "a") as f:
            f.write("# changed in PR\n")
        report_path = self._create_report("stage-test-report.md", ["_typos.toml", "docs/work/claims/stage-test.toml"])
        subprocess.run(["git", "add", "."], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: changes"], cwd=self.test_dir, check=True)

        # Merge to main
        subprocess.run(["git", "checkout", "-q", "main"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "merge", "--squash", "feat/test-c"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: squash merge"], cwd=self.test_dir, check=True)

        # Subsequent commit on main touching something else
        with open(os.path.join(self.test_dir, "unrelated.txt"), "w") as f:
            f.write("unrelated commit\n")
        subprocess.run(["git", "add", "unrelated.txt"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "chore: unrelated commit on main"], cwd=self.test_dir, check=True)

        script = os.path.join(self.test_dir, "scripts/check-report.py")
        res = subprocess.run([script, report_path], cwd=self.test_dir, capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Failed on main: {res.stderr}")
        self.assertIn("SKIPPED (parity: no report in diff range)", res.stdout)

    def test_d_no_git_directory_handles_gracefully(self):
        """(d) Missing .git directory gives a clear error, never an unhandled exception."""
        no_git_dir = tempfile.mkdtemp(prefix="test_no_git_")
        try:
            os.makedirs(os.path.join(no_git_dir, "scripts"), exist_ok=True)
            copied_guard = os.path.join(no_git_dir, "scripts/guard-diff.sh")
            shutil.copy(GUARD_DIFF_SH, copied_guard)

            res = subprocess.run([copied_guard], cwd=no_git_dir, capture_output=True, text=True)
            self.assertNotEqual(res.returncode, 0)
            self.assertIn("ERROR: Not a git repository", res.stderr)
        finally:
            shutil.rmtree(no_git_dir, ignore_errors=True)

    def test_e_pr28_scenario_passes_on_main(self):
        """(e) The PR #28 scenario: on main, running check-report.py on an earlier report."""
        subprocess.run(["git", "checkout", "-q", "-b", "feat/pr27"], cwd=self.test_dir, check=True)
        with open(os.path.join(self.test_dir, "_typos.toml"), "a") as f:
            f.write("# changed in pr27\n")
        report_path = self._create_report("stage-test-report.md", ["_typos.toml", "docs/work/claims/stage-test.toml"])
        subprocess.run(["git", "add", "."], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: pr27"], cwd=self.test_dir, check=True)

        subprocess.run(["git", "checkout", "-q", "main"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "merge", "--squash", "feat/pr27"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "feat: pr27 squash merge (#27)"], cwd=self.test_dir, check=True)

        # PR 28 only touched scripts/check-report.py
        with open(os.path.join(self.test_dir, "scripts/check-report.py"), "a") as f:
            f.write("\n# pr28 touch\n")
        subprocess.run(["git", "add", "scripts/check-report.py"], cwd=self.test_dir, check=True)
        subprocess.run(["git", "commit", "-qm", "fix(ci): pr28 commit (#28)"], cwd=self.test_dir, check=True)

        script = os.path.join(self.test_dir, "scripts/check-report.py")
        res = subprocess.run([script, report_path], cwd=self.test_dir, capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"PR 28 scenario failed: {res.stderr}")
        self.assertIn("SKIPPED (parity: no report in diff range)", res.stdout)


if __name__ == "__main__":
    unittest.main()
