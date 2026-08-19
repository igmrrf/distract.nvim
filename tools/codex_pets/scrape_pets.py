"""Collects codex-pets spritesheets to exercise the import_sprite CLI against.

Two sources, both writing the same catalogue so `import_pets.py` and
`verify_layout.py` do not care which one produced a sheet:

* `codex-pets.net` (the default) publishes each pet's atlas geometry in its API
  response, so the grid recorded from it is read rather than guessed.
* `legeling/awesome-codex-pet` is a 198-pet community gallery reachable without
  a search API, whose pets carry an explicit per-pet licence. Its `pet.json`
  carries no atlas metadata, so the geometry is derived from the sheet's own
  header -- see `awesome_source.py`.

Per-row frame counts are not published per-pet by either source; they are fixed
per sprite version (see `pet_layout.py`) and `verify_layout.py` checks that claim
against the real pixels.

Usage:
    python3 tools/codex_pets/scrape_pets.py [term ...]
    python3 tools/codex_pets/scrape_pets.py --source awesome [--limit 6] [term ...]

Sheets are third-party artwork. Every pet checked in the gallery is fan art of an
existing character under a non-commercial licence, and the site publishes none at
all. They are downloaded into a gitignored working directory as local test
material and are **not for redistribution** -- which is why no codex-pets asset
ships as a built-in.
"""

import json
import sys
import urllib.parse
import urllib.request

import awesome_source
import paths
import pet_layout

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


def collect_from_gallery(terms, limit):
    """Downloads matching pets from the gallery, deriving each one's grid.

    A pet whose sheet is not a whole grid, or whose row count matches no sprite
    version, is skipped with the reason: importing it against a guessed row
    mapping would produce states pointing at the wrong frames, which is the one
    failure this tooling exists to avoid.
    """
    paths.ensure(paths.SHEETS_DIR)
    catalogue = {}

    for slug in awesome_source.list_pets():
        if len(catalogue) >= limit:
            break
        if not awesome_source.matches(slug, terms):
            continue

        try:
            sheet_bytes = awesome_source.fetch(
                f"{awesome_source.RAW_BASE}/{slug}/spritesheet.webp"
            )
        except Exception as error:
            print(f"  {slug}: download failed: {error}", file=sys.stderr)
            continue

        try:
            columns, rows, sprite_version, width, height = awesome_source.grid_of(
                sheet_bytes
            )
        except ValueError as reason:
            print(f"  {slug}: {reason}, skipped")
            continue

        destination = f"{paths.SHEETS_DIR}/{slug}.webp"
        with open(destination, "wb") as handle:
            handle.write(sheet_bytes)

        description = awesome_source.describe(slug)
        catalogue[slug] = {
            "id": slug,
            "display_name": description["name"] or slug,
            "owner": description["author"],
            "sprite_version": sprite_version,
            "kind": description["source_type"],
            "term": "gallery",
            "path": destination,
            "bytes": len(sheet_bytes),
            "cell": [pet_layout.CELL_WIDTH, pet_layout.CELL_HEIGHT],
            "columns": columns,
            "rows": rows,
            "states_detected": None,
            "source_url": f"{awesome_source.RAW_BASE}/{slug}/spritesheet.webp",
            # Recorded so the licence travels with the material rather than
            # living only in a README nobody reads before publishing something.
            "license": description["license"] or "unstated",
        }
        print(
            f"  {slug:34s} v{sprite_version} {width}x{height} "
            f"grid {columns}x{rows} {len(sheet_bytes) // 1024} KiB "
            f"[{catalogue[slug]['license']}]"
        )

    with open(paths.CATALOGUE_PATH, "w") as handle:
        json.dump(catalogue, handle, indent=2)
    print(f"\n{len(catalogue)} sheets -> {paths.SHEETS_DIR}")
    return catalogue


def main(argv):
    source = "codex"
    limit = 6
    terms = []

    index = 0
    while index < len(argv):
        argument = argv[index]
        if argument == "--source":
            index += 1
            source = argv[index] if index < len(argv) else ""
            if source not in ("codex", "awesome"):
                raise SystemExit("--source must be 'codex' or 'awesome'")
        elif argument == "--limit":
            index += 1
            limit = int(argv[index])
        else:
            terms.append(argument)
        index += 1

    if source == "awesome":
        return collect_from_gallery(terms, limit)
    return collect(terms or DEFAULT_TERMS)


if __name__ == "__main__":
    main(sys.argv[1:])
