#!/usr/bin/env python3
"""
Checks that all test-hook environment variables (KYNEX_TEST_*, KYNEX_ALLOW_*, KYNEX_BENCH_*)
in crates/ production code are strictly gated behind #[cfg(feature = "test-hooks")],
#[cfg(feature = "bench")], or #[cfg(test)].
"""

import sys
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HOOK_PATTERN = re.compile(r'["\'](KYNEX_TEST_[A-Z0-9_]+|KYNEX_ALLOW_[A-Z0-9_]+|KYNEX_BENCH_[A-Z0-9_]+)["\']')
CFG_GATE_PATTERN = re.compile(r'#\[cfg\((feature\s*=\s*"(test-hooks|bench)"|test)\)\]')

def check_file(file_path: Path) -> list[str]:
    errors = []
    lines = file_path.read_text(encoding="utf-8").splitlines()
    
    for i, line in enumerate(lines):
        if line.strip().startswith("//"):
            continue
        match = HOOK_PATTERN.search(line)
        if match:
            var_name = match.group(1)
            # Look backwards up to 8 lines for a gating cfg attribute
            start_idx = max(0, i - 8)
            preceding = "\n".join(lines[start_idx:i+1])
            if not CFG_GATE_PATTERN.search(preceding):
                errors.append(f"{file_path}:{i+1}: Non-gated read of test hook '{var_name}'")
    return errors

def main():
    errors = []
    for rs_file in (ROOT / "crates").rglob("*.rs"):
        # Ignore tests directory and target directory
        if "target" in rs_file.parts or "tests" in rs_file.parts or "fuzz" in rs_file.parts:
            continue
        errors.extend(check_file(rs_file))
        
    if errors:
        print("ERROR: Found non-gated test hook environment variables in production source files:", file=sys.stderr)
        for err in errors:
            print(f"  {err}", file=sys.stderr)
        print("All KYNEX_TEST_*, KYNEX_ALLOW_*, and KYNEX_BENCH_* reads must be gated with #[cfg(feature = \"...\")] or #[cfg(test)].", file=sys.stderr)
        sys.exit(1)
    
    print("✓ All test-hook environment variables in production code paths are strictly gated.")
    sys.exit(0)

if __name__ == "__main__":
    main()
