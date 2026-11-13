#!/usr/bin/env python3
"""Unit tests for scripts/check-private-paths.sh enforcing D-019/D-020 privacy invariant."""

import os
import shutil
import subprocess
import tempfile
import unittest

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CHECK_SCRIPT = os.path.join(ROOT_DIR, "scripts/check-private-paths.sh")


class TestCheckPrivatePaths(unittest.TestCase):
    def test_current_repo_passes(self):
        """Current repository must pass check-private-paths.sh with zero tracked private files."""
        res = subprocess.run([CHECK_SCRIPT], cwd=ROOT_DIR, capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"check-private-paths.sh failed on current repo: {res.stderr}")
        self.assertIn("PASS: Zero private files tracked in git repository.", res.stdout)

    def test_scratch_repo_clean_passes(self):
        """Clean repository with no private files passes."""
        temp_dir = tempfile.mkdtemp(prefix="test_priv_clean_")
        try:
            subprocess.run(["git", "init", "-q"], cwd=temp_dir, check=True)
            with open(os.path.join(temp_dir, "README.md"), "w") as f:
                f.write("Hello")
            subprocess.run(["git", "add", "README.md"], cwd=temp_dir, check=True)
            res = subprocess.run([CHECK_SCRIPT], cwd=temp_dir, capture_output=True, text=True)
            self.assertEqual(res.returncode, 0)
        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)

    def test_scratch_repo_with_tracked_docs_fails(self):
        """Tracking a file under docs/ fails the check."""
        temp_dir = tempfile.mkdtemp(prefix="test_priv_docs_")
        try:
            subprocess.run(["git", "init", "-q"], cwd=temp_dir, check=True)
            os.makedirs(os.path.join(temp_dir, "docs"), exist_ok=True)
            with open(os.path.join(temp_dir, "docs/leak.md"), "w") as f:
                f.write("secret")
            subprocess.run(["git", "add", "-f", "docs/leak.md"], cwd=temp_dir, check=True)
            res = subprocess.run([CHECK_SCRIPT], cwd=temp_dir, capture_output=True, text=True)
            self.assertNotEqual(res.returncode, 0)
            self.assertIn("docs/leak.md", res.stderr)
        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)

    def test_scratch_repo_with_tracked_setup_or_agents_fails(self):
        """Tracking SETUP.md or AGENTS.md fails the check."""
        temp_dir = tempfile.mkdtemp(prefix="test_priv_files_")
        try:
            subprocess.run(["git", "init", "-q"], cwd=temp_dir, check=True)
            with open(os.path.join(temp_dir, "SETUP.md"), "w") as f:
                f.write("setup instructions")
            subprocess.run(["git", "add", "-f", "SETUP.md"], cwd=temp_dir, check=True)
            res = subprocess.run([CHECK_SCRIPT], cwd=temp_dir, capture_output=True, text=True)
            self.assertNotEqual(res.returncode, 0)
            self.assertIn("SETUP.md", res.stderr)
        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()

