"""Collects codex-pets spritesheets to exercise the import_sprite CLI against.

The site publishes each pet's atlas geometry in its API response, so the grid
this records is read rather than guessed. Per-row frame counts are not published
anywhere per-pet; they are fixed per sprite version (see `pet_layout.py`) and
`verify_layout.py` checks that claim against the real pixels.

Usage:
    python3 tools/codex_pets/scrape_pets.py [term ...]

Sheets are third-party artwork with no stated licence. They are downloaded into a
gitignored working directory and are not for redistribution.
"""

import json
import sys
import urllib.parse
import urllib.request

import paths

API = "https://codex-pets.net/api/pets"
DEFAULT_TERMS = ["goku", "sengoku", "naruto", "cat", "dog", "dragon", "slime", "fox"]
PER_TERM = 2
USER_AGENT = "distract.nvim-import-sprite-test/1.0"
TIMEOUT_SECONDS = 60


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
        return response.read()


def dimensions_of(text):
    width, height = text.split("x")
    return int(width), int(height)


def collect(terms):
    paths.ensure(paths.SHEETS_DIR)
    catalogue = {}

    for term in terms:
        try:
            payload = json.loads(fetch(f"{API}?q={urllib.parse.quote(term)}"))
        except Exception as error:
            print(f"  {term}: query failed: {error}", file=sys.stderr)
            continue

        taken = 0
        for pet in payload.get("pets", []):
            if taken >= PER_TERM or pet["id"] in catalogue:
                continue
            report = pet.get("validationReport")
            sheet_url = pet.get("spritesheetUrl")
            if not report or not sheet_url:
                print(f"  {pet['id']}: no atlas metadata, skipped")
                continue

            cell_width, cell_height = dimensions_of(report["cellSize"])
            atlas_width, atlas_height = dimensions_of(report["atlasSize"])
            if atlas_width % cell_width or atlas_height % cell_height:
                print(f"  {pet['id']}: atlas is not a whole grid, skipped")
                continue

            destination = f"{paths.SHEETS_DIR}/{pet['id']}.webp"
            try:
                payload_bytes = fetch(sheet_url)
            except Exception as error:
                print(f"  {pet['id']}: download failed: {error}", file=sys.stderr)
                continue
            with open(destination, "wb") as handle:
                handle.write(payload_bytes)

            catalogue[pet["id"]] = {
                "id": pet["id"],
                "display_name": pet["displayName"],
                "owner": pet.get("ownerHandle"),
                "sprite_version": pet.get("spriteVersionNumber"),
                "kind": pet.get("kind"),
                "term": term,
                "path": destination,
                "bytes": len(payload_bytes),
                "cell": [cell_width, cell_height],
                "columns": atlas_width // cell_width,
                "rows": atlas_height // cell_height,
                "states_detected": report.get("statesDetected"),
                "source_url": sheet_url,
            }
            taken += 1
            print(
                f"  {pet['id']:28s} v{pet.get('spriteVersionNumber')} "
                f"{atlas_width}x{atlas_height} cell {cell_width}x{cell_height} "
                f"grid {atlas_width // cell_width}x{atlas_height // cell_height} "
                f"{len(payload_bytes) // 1024} KiB"
            )

    with open(paths.CATALOGUE_PATH, "w") as handle:
        json.dump(catalogue, handle, indent=2)
    print(f"\n{len(catalogue)} sheets -> {paths.SHEETS_DIR}")
    return catalogue


if __name__ == "__main__":
    collect(sys.argv[1:] or DEFAULT_TERMS)
