"""Checks the published layout against what the real sheets actually contain.

Imports every scraped sheet claiming every cell (8 per row), then counts the
non-empty cells per row from the resulting `.rgba` sidecar. If `pet_layout.py`'s
row -> frame-count table is right, the counts match it exactly.

Usage:
    python3 tools/codex_pets/scrape_pets.py
    python3 tools/codex_pets/verify_layout.py
"""

import json
import os
import subprocess
import sys

import paths
import pet_layout
import sidecar


def import_every_cell(sheet_path, asset_name, rows):
    out_dir = os.path.join(paths.VERIFY_DIR, asset_name)
    command = [
        "cargo", "run", "--quiet", "--manifest-path", paths.CARGO_MANIFEST,
        "--bin", "import_sprite", "--",
        "--spritesheet", sheet_path,
        "--cell", f"{pet_layout.CELL_WIDTH}x{pet_layout.CELL_HEIGHT}",
        "--row-counts", ",".join([str(pet_layout.COLUMNS)] * rows),
        "--name", asset_name,
        "--out", out_dir,
        "--manifest-out", os.path.join(out_dir, f"{asset_name}.lua"),
    ]
    result = subprocess.run(command, cwd=paths.REPO_ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        return None, result.stderr.strip().splitlines()[-1:]
    return os.path.join(out_dir, f"{asset_name}_frames.rgba"), None


def occupied_per_row(sidecar_path, rows):
    parsed = sidecar.read(sidecar_path)
    counts = []
    for row in range(rows):
        used = 0
        for column in range(pet_layout.COLUMNS):
            index = row * pet_layout.COLUMNS + column
            if index >= parsed["count"]:
                break
            if not sidecar.is_blank(parsed, index):
                used += 1
        counts.append(used)
    return counts


def main():
    paths.ensure(paths.VERIFY_DIR)
    if not os.path.exists(paths.CATALOGUE_PATH):
        print("no catalogue; run scrape_pets.py first", file=sys.stderr)
        return 1

    catalogue = json.load(open(paths.CATALOGUE_PATH))
    disagreeing = 0

    for pet_id, entry in sorted(catalogue.items(), key=lambda item: item[1]["rows"]):
        rows = entry["rows"]
        version = pet_layout.version_for_rows(rows)
        expected = pet_layout.row_counts(version)
        asset_name = pet_id.replace("-", "_")

        sidecar_path, failure = import_every_cell(entry["path"], asset_name, rows)
        if not sidecar_path:
            print(f"{pet_id:28s} IMPORT FAILED: {failure}")
            disagreeing += 1
            continue

        actual = occupied_per_row(sidecar_path, rows)
        agrees = actual == expected
        disagreeing += 0 if agrees else 1
        print(f"{pet_id:28s} v{version} {'match ' if agrees else 'DIFFER'} "
              f"actual={','.join(map(str, actual))}")
        if not agrees:
            print(f"{'':28s}      expected={','.join(map(str, expected))}")

    print(f"\n{len(catalogue)} sheets, {disagreeing} disagreeing with the published layout")
    return 1 if disagreeing else 0


if __name__ == "__main__":
    sys.exit(main())
