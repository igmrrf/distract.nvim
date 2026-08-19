# Full-feature completion plan — 2026-08-19

Supersedes the sequencing in
[`2026-08-16-future-roadmap-master.md`](2026-08-16-future-roadmap-master.md) where the
two disagree. Written to close every open item in [`../../../HANDOFF.md`](../../../HANDOFF.md)
and every core section of [`../../../future.md`](../../../future.md).

## Decisions taken, so nothing stalls on a question

1. **2D is the contract. 3D is not built.** The overlay is an instanced-quad wgpu
   renderer over a 2D sprite world; depth exists as `z` + parallax and is a
   compositing order, not a projection. Nothing in the roadmap needs a camera, a
   projection matrix or a mesh pipeline, and adding one would fork the
   half-block and kitty backends off the manifest contract. `z`-ordered parallax
   layers are the depth feature, and they already ship.

2. **§5.1–§5.8 stay out of `lua/distract/`.** `future.md` says each is its own
   repository; that holds. The core deliverable is the two extension points
   (§2.1, §2.2) plus a third — a post-draw layer hook — that make those repos
   buildable, and reference plugins under `examples/plugins/` that exercise all
   three end to end. A bundled speech-bubble or weather module would put editor
   product decisions inside the kernel.

3. **§2.1 hook parity: hooks never mutate physics directly.** `on_tick` receives
   a read-only entity proxy (`__newindex` errors). Every mutation goes through a
   world command — `request_state`, `apply_impulse`, `despawn`, `mark_dirty` —
   which the Lua engine applies locally and the overlay backend forwards over
   IPC as `ApplyCommand`, so one manifest plus one plugin behaves identically on
   both backends. Entity ids already cross the wire (`Spawn.id`), so the two
   worlds can name the same entity.

4. **Hook failure policy:** `xpcall` per callback, report once through
   `vim.notify` at `WARN`, disable that plugin for the session. Dispatch is in
   registration order. A plugin marks the world dirty with `world.mark_dirty()`,
   which clears `is_quiescent()` for one frame.

5. **§2.2 obstacles are collected in Lua only** and pushed to both engines
   exactly as `events.sync_floor` pushes the floor. Providers are called on the
   debounced cadence, never per tick. Landing on a `solid_platform` is physics,
   so it ships with parity fixtures.

6. **Open question 1 (a pet that only walks one way): the manifests already
   answer it, and no engine change is right.** Flipping `heading_x` on a wrap
   would reverse the pet's direction, which is the opposite of what wrapping
   means. The shipped set covers both readings by example: `cat`'s `walk` wraps
   (and §4.3 now draws the seam properly, which is what makes a one-way walk
   look deliberate rather than broken), while `cat`'s `idle` clamps and its
   `pounce` bounces. `cat_walking`, whose art has a real left and right, is the
   asset to give `bounce` to if a user wants a pet that patrols. Wrap stays a
   pure teleport.

7. **Open question 2 (first-draw hitch): not fixed.** 130–375 ms once per asset
   on first draw. The coroutine seam is real work for a hitch nobody has
   reported; recorded in `HANDOFF.md`, not built.

8. **Open question 3 (quantise procedural art): no — and the measurement is the
   reason.** The intent was to cut the 1,894 live highlight groups the three
   built-ins consumed. The §3 redo did that directly: all 79 built-in frames now
   create **123** groups against the 4,096 cap, 3% where it was 46%. Quantising
   art that is already a four-to-eight colour palette can only spend CPU and
   move colours, so the gate in `terminal_sprites.lua` stays and
   `max_sprite_colours` remains what it says it is — a bound on *imported* art.

9. **Exactly 3 codex-pets ship as built-ins; everything else is import-only.**
   236 MB of scraped codex-pets.net artwork has no stated licence and cannot
   go in a published plugin — that stays gitignored local test material, as
   before. But [`legeling/awesome-codex-pet`](https://github.com/legeling/awesome-codex-pet)
   (198-pet community gallery, code MIT, assets blanket-badged CC BY-NC 4.0,
   same `pet.json` + `spriteVersionNumber` schema as codex-pets.net so the
   existing v2 grid in `tools/codex_pets/pet_layout.py` applies unchanged)
   has individual pets whose `submission.json.license` overrides that badge
   with something stronger — original characters, not fan art, under CC BY or
   MIT. Three checked clean:
   - [`gudong--rank`](https://github.com/legeling/awesome-codex-pet/tree/main/pets/gudong--rank) —
     CC BY 4.0, `source_type: "original"`, v2.
   - [`iris--yau-427`](https://github.com/legeling/awesome-codex-pet/tree/main/pets/iris--yau-427) —
     "MIT License; original AI-assisted artwork ... with redistribution
     permission", `source_type: "original-character"`, v2.
   - [`minty--somnusochi`](https://github.com/legeling/awesome-codex-pet/tree/main/pets/minty--somnusochi) —
     MIT License, `source_type: "github"`, v2, full nine-action set.

   These three ship as real built-in pets (Import → `assets/pets/` proper,
   not `assets/codex_pets/`) with an `ATTRIBUTION.md` naming each artist
   (`@rank`, `@yau-427`, `@somnusochi`) and licence, satisfying CC BY's
   attribution term. Everything else in either source — franchise fan art,
   unknown-licence uploads, anything not explicitly original-and-permissive —
   stays local-only test material, never committed. `docs/importing-assets.md`
   gets a "bring your own pet" section: how to run `import_sprite` against a
   codex-pets.net or awesome-codex-pet download, or an original spritesheet,
   so users add whichever pet they want without the plugin shipping it.
   Adding awesome-codex-pet as a second scrape source for local test material
   is tooling work under `tools/codex_pets/`, not a plan phase —
   `scrape_pets.py` gains a GitHub-contents-API path; `pet_layout.py`,
   `import_pets.py`, and `verify_layout.py` need no change.

10. **An invalid engine argv value exits non-zero** after emitting
    `INVALID_ARGUMENT`, and `external.lua` reports a non-zero exit.

## Order of work

Art last, so the goldens are regenerated once rather than per phase.

| # | Work | Touches |
|---|---|---|
| P0 | Engine argv exit code; delete the superseded `parity_dump` dev aid | `main.rs`, `external.lua` |
| P1 | §2.1 hook pipeline + post-draw layer hook | new `lua/distract/plugins.rs`-equivalent Lua module, `engine.lua`, `external.lua`, `ipc.rs`, `ecs.rs` |
| P2 | §4.2 focus / instance visibility scoping | `events.lua`, `renderer.lua`, `ipc.rs`, `main.rs` |
| P3 | §4.1 buffer-scoped viewport + floating-window exclusion | new `viewport.lua`, `renderer.lua`, `ipc.rs`, `ecs.rs` |
| P4 | §2.2 obstacle provider + platform collision, both engines | new `obstacles.lua`, `engine.lua`, `ecs.rs`, physics fixtures |
| P5 | §4.3 toroidal edge-splitting | `renderer.lua` (extract placement), `kitty/`, `gpu.rs` |
| P6 | §3 silhouette-first redo, all assets, both engines; quantiser gate removal; bundle the 3 built-in codex-pets with `ATTRIBUTION.md` | `sprites/*.lua`, `sprites/*.rs`, `terminal_sprites.lua`, goldens, new `assets/pets/`, new `ATTRIBUTION.md` |
| P7 | 200-entity tick benchmark; batched particle path only if the budget misses | `engine.lua`, `ecs.rs` |
| P8 | `examples/plugins/`, `doc/distract.txt`, `docs/` (bring-your-own-pet import guide), `CHANGELOG.md`, rewrite `HANDOFF.md`/`future.md` | docs |

Every phase ends on all four gates green, with a fixture for anything that
changes physics and a regenerated golden for anything that changes art.

## Size-cap discipline

`engine.lua` (951), `renderer.lua` (508), `external.lua` (537) and
`sprite_sources.lua` (394) are already over or at the cap. No phase adds a line
to any of them without extracting first: P1 puts hooks in their own module, P3
puts the viewport rect in its own module, P4 puts obstacles in their own module,
and P5 extracts placement out of `renderer.lua` before it splits a surface.
