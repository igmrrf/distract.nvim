"""Where this tooling reads and writes.

Sheets and imported frames are tracked under `assets/codex_pets/` so contributors
can test against the same material without re-scraping. They are large: a v2
`.rgba` sidecar is ~11 MB, and `imported/` is regenerable from `sheets/` with
`import_pets.py`. Override the root with `CODEX_PETS_WORK`.
"""

import os

TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(TOOLS_DIR))

WORK_ROOT = os.environ.get("CODEX_PETS_WORK") or os.path.join(
    REPO_ROOT, "assets", "codex_pets"
)
SHEETS_DIR = os.path.join(WORK_ROOT, "sheets")
IMPORTED_DIR = os.path.join(WORK_ROOT, "imported")
VERIFY_DIR = os.path.join(WORK_ROOT, "verify")
CATALOGUE_PATH = os.path.join(SHEETS_DIR, "catalogue.json")

CARGO_MANIFEST = os.path.join("engine", "Cargo.toml")


def ensure(*directories):
    for directory in directories:
        os.makedirs(directory, exist_ok=True)
