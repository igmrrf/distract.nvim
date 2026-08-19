"""Imports every scraped codex-pets sheet with its real grid and action names.

Usage:
    python3 tools/codex_pets/scrape_pets.py
    python3 tools/codex_pets/import_pets.py

Writes one directory per pet under `assets/codex_pets/imported/`: a packed
`_sheet.png` for the overlay backend, a `_frames.rgba` sidecar for kitty, and a
manifest scaffold whose physics and transitions still need hand-tuning.
"""

import json
import os
import subprocess
import sys

import paths
import pet_layout
import sidecar


def import_pet(entry, asset_name, version):
    out_dir = os.path.join(paths.IMPORTED_DIR, asset_name)
    command = [
        "cargo", "run", "--quiet", "--manifest-path", paths.CARGO_MANIFEST,
        "--bin", "import_sprite", "--",
        "--spritesheet", entry["path"],
        "--cell", f"{pet_layout.CELL_WIDTH}x{pet_layout.CELL_HEIGHT}",
        "--row-counts", ",".join(map(str, pet_layout.row_counts(version))),
        "--states", pet_layout.states_arg(version),
        "--name", asset_name,
        "--out", out_dir,
        "--manifest-out", os.path.join(out_dir, f"{asset_name}.lua"),
    ]
    result = subprocess.run(command, cwd=paths.REPO_ROOT, capture_output=True, text=True)
    return result, out_dir


def main():
    paths.ensure(paths.IMPORTED_DIR)
    if not os.path.exists(paths.CATALOGUE_PATH):
        print("no catalogue; run scrape_pets.py first", file=sys.stderr)
        return 1

    catalogue = json.load(open(paths.CATALOGUE_PATH))
    problems = []

    for pet_id, entry in sorted(catalogue.items()):
        version = pet_layout.version_for_rows(entry["rows"])
        asset_name = pet_id.replace("-", "_")
        result, out_dir = import_pet(entry, asset_name, version)

        if result.returncode != 0:
            problems.append((pet_id, result.stderr.strip().splitlines()[-1:]))
            print(f"{pet_id:28s} FAILED")
            continue

        parsed = sidecar.read(os.path.join(out_dir, f"{asset_name}_frames.rgba"))
        blank = sum(
            1 for index in range(parsed["count"]) if sidecar.is_blank(parsed, index)
        )
        expected = pet_layout.total_frames(version)
        cutout = result.stderr.count("already alpha-cutout")
        healthy = parsed["count"] == expected and blank == 0
        if not healthy:
            problems.append(
                (pet_id, [f"count={parsed['count']} expected={expected} blank={blank}"])
            )

        print(
            f"{pet_id:28s} v{version} {parsed['count']:>2} frames "
            f"{parsed['width']}x{parsed['height']} "
            f"cutout={cutout}/{parsed['count']} blank={blank} "
            f"{'ok' if healthy else 'PROBLEM'}"
        )

    print(f"\n{len(catalogue)} imported, {len(problems)} problems")
    for pet_id, detail in problems:
        print(f"  {pet_id}: {detail}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
