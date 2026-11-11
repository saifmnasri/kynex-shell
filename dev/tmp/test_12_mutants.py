#!/usr/bin/env python3
import os
import subprocess
import sys

mutants = [
    {
        "name": "1. sender-owns-name check",
        "file": "crates/kynex-tray/src/registry.rs",
        "find": "if owner.as_deref() != Some(sender_bus_name) {",
        "replace": "if false && owner.as_deref() != Some(sender_bus_name) {",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "2. per-sender cap",
        "file": "crates/kynex-tray/src/limits.rs",
        "find": "pub const MAX_ITEMS_PER_SENDER: usize = 4;",
        "replace": "pub const MAX_ITEMS_PER_SENDER: usize = 100;",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "3. total cap",
        "file": "crates/kynex-tray/src/limits.rs",
        "find": "pub const MAX_TOTAL_ITEMS: usize = 64;",
        "replace": "pub const MAX_TOTAL_ITEMS: usize = 1000;",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "4. vanish handling",
        "file": "crates/kynex-tray/src/registry.rs",
        "find": "pub fn remove_sender(&mut self, sender_bus_name: &str) -> Vec<ItemState> {",
        "replace": "pub fn remove_sender(&mut self, sender_bus_name: &str) -> Vec<ItemState> { return Vec::new();",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "5. pixmap length check",
        "file": "crates/kynex-tray/src/pixmap.rs",
        "find": "if argb_data.len() != expected_bytes {",
        "replace": "if false && argb_data.len() != expected_bytes {",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "6. size cap",
        "file": "crates/kynex-tray/src/limits.rs",
        "find": "pub const MAX_PIXMAP_DIMENSION: i32 = 64;",
        "replace": "pub const MAX_PIXMAP_DIMENSION: i32 = 512;",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "7. ARGB conversion",
        "file": "crates/kynex-tray/src/pixmap.rs",
        "find": "        rgba_data.push(r);\n        rgba_data.push(g);\n        rgba_data.push(b);\n        rgba_data.push(a);",
        "replace": "        rgba_data.push(a);\n        rgba_data.push(r);\n        rgba_data.push(g);\n        rgba_data.push(b);",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "8. timeout",
        "file": "crates/kynex-tray/src/limits.rs",
        "find": "pub const PROPERTY_TIMEOUT_SECS: u64 = 2;",
        "replace": "pub const PROPERTY_TIMEOUT_SECS: u64 = 60;",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-daemon", "--test", "tray_service_test", "test_hanging_item_timeout"],
    },
    {
        "name": "9. sanitizer",
        "file": "crates/kynex-tray/src/sanitize.rs",
        "find": "pub fn sanitize_text(input: &str, max_chars: usize) -> String {",
        "replace": "pub fn sanitize_text(input: &str, max_chars: usize) -> String { return input.to_string();",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
    {
        "name": "10. PlainText",
        "file": "styles/default/Shell.qml",
        "find": "textFormat: Text.PlainText",
        "replace": "textFormat: Text.RichText",
        "cmd": ["./scripts/test-qml-controls.sh"],
    },
    {
        "name": "11. activate coordinates",
        "file": "styles/default/Shell.qml",
        "find": "trayModel.activate(trayBtn.id, gx, gy);",
        "replace": "trayModel.activate(trayBtn.id, -999, -999);",
        "cmd": ["./scripts/test-qml-controls.sh"],
    },
    {
        "name": "12. attention state",
        "file": "crates/kynex-tray/src/status.rs",
        "find": "Self::NeedsAttention => \"NeedsAttention\",",
        "replace": "Self::NeedsAttention => \"Passive\",",
        "cmd": ["cargo", "nextest", "run", "-p", "kynex-tray"],
    },
]

print("=== Testing 12 Auditor Mutants ===")
results = []
for m in mutants:
    name = m["name"]
    fpath = m["file"]
    find_str = m["find"]
    replace_str = m["replace"]
    cmd = m["cmd"]

    with open(fpath, "r", encoding="utf-8") as f:
        orig = f.read()

    if find_str not in orig:
        print(f"ERROR: Could not find target pattern in {fpath} for mutant {name}")
        results.append((name, "ERROR_NOT_FOUND"))
        continue

    mutated = orig.replace(find_str, replace_str, 1)
    with open(fpath, "w", encoding="utf-8") as f:
        f.write(mutated)

    try:
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if proc.returncode != 0:
            print(f"[{name}] KILLED (Exit code {proc.returncode})")
            results.append((name, "KILLED"))
        else:
            print(f"[{name}] SURVIVED! (Exit code 0)")
            results.append((name, "SURVIVED"))
    finally:
        with open(fpath, "w", encoding="utf-8") as f:
            f.write(orig)

print("\n=== Summary of 12 Mutants ===")
killed = sum(1 for _, res in results if res == "KILLED")
survived = sum(1 for _, res in results if res == "SURVIVED")
print(f"Total: {len(results)} | Killed: {killed} | Survived: {survived}")
for name, res in results:
    print(f"  {name}: {res}")
