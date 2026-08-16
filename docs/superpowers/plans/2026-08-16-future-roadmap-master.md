# `future.md` to 100% — Master Program Plan

> **For agentic workers:** This is a *program* plan, not a task plan. It defines
> what "100% of `future.md`" means, records what is already done, and decomposes
> the remainder into 12 workstreams. Each workstream gets its own bite-sized TDD
> plan at `docs/superpowers/plans/YYYY-MM-DD-<workstream-id>.md`, written with
> `superpowers:writing-plans` immediately before that workstream is executed.
> Do not execute from this file — execute from a workstream plan.

**Goal:** Ship every capability described in
[`future.md`](../../../future.md), with both rendering backends in parity, under
the repository's own standards, verified on a real screen.

**Architecture:** The core stays a micro-kernel — ECS simulation, shading,
compositing — and every domain feature (dialogue, memory, LSP, physics, weather,
AI, WPM) arrives through three extension points: the asset registry (done), the
plugin/middleware pipeline (W3), and the spatial obstacle provider (W6). Satellite
plugins are separate repositories that depend on nothing but those three surfaces.

**Tech Stack:** Lua 5.1/LuaJIT (Neovim), Rust 2021 (`engine/`, wgpu), busted-style
custom harness (`tests/run_tests.lua`), `stylua`, `luacheck`, `cargo clippy`.

**Spec:** [`future.md`](../../../future.md) — the roadmap being completed.
**Companion spec:** [`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](../specs/2026-08-16-locomotion-position-kitty-design.md)
— the unit contract, backend/renderer split, and parity harness every workstream
below inherits.
**Live constraints:** [`HANDOFF.md`](../../../HANDOFF.md) — pending work and the
traps that cost time.

---

## Global Constraints

Every task in every workstream inherits these. They are not repeated per task.

- **Size caps:** file ≤ 400 lines, type/struct ≤ 150, function ≤ 60. `engine.lua`
  (>900) and `renderer.lua` (501) are grandfathered debt; **no new file may
  break the caps.**
- **No explanatory comments.** LuaCATS annotations on every public function,
  including the error return.
- **Fail fast:** `nil, error_message` for expected failures, `error()` for broken
  invariants. Never swallow.
- **Unit contract:** positions in terminal cells; velocities, accelerations and
  path amplitudes in sprite pixels per frame at 60 FPS, where one sprite pixel is
  one cell wide and half a cell tall; `z` dimensionless.
- **Cross-engine parity:** any behaviour implemented in both `lua/distract/engine.lua`
  and `engine/src/ecs.rs` must be pinned by a fixture in `tests/fixtures/physics/`
  before the change lands. Regenerate with
  `UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity`.
- **Neither engine measures its own floor.** `events.sync_floor` measures once and
  pushes to both. A new caller of `position.floor_row` inside an engine is a bug.
- **Four gates green before any commit:**
  ```bash
  nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
  cargo test --manifest-path engine/Cargo.toml
  stylua --check lua plugin tests
  cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
  ```
  Baseline at plan time: **325 Lua tests, 145 Rust tests.** Every workstream
  raises both numbers; a workstream that raises neither did not test anything.
- **`luacheck` is broken on the owner's machine** (fails to load under Lua 5.5).
  CI still runs it. A green local run is not evidence luacheck passed.
- **Test hygiene, non-negotiable:** every spec that registers a backend calls
  `backends.reset()` and `kitty.reset()`; every spec that swaps the kitty writer
  uses `captured()`/`with_kitty()`; every spec asserting on `ground_y` calls
  `set_ground_row(nil)` first; every probe manifest gets a unique `probe_N` name.
- **Satellite plugins are separate repositories.** Nothing in W7–W12 may add a
  file under `lua/distract/` in this repo.

---

## What "100%" means

`future.md` is measured section by section. A section is **done** when it is
implemented on both backends where applicable, covered by tests, documented in
`doc/distract.txt` and `README.md`, and listed in `CHANGELOG.md`.

### Already done — do not re-plan

| Capability | Evidence |
|---|---|
| Dynamic Asset Provider API | `lua/distract/init.lua:249` `M.register_asset` |
| Analytical shading, smoothstep AA | `sprite_gen.orb`, `sprite_gen.limb` |
| Multi-point lighting + 4×4 Bayer dithering | `sprite_gen.lua:172-199`, `engine/src/sprite_gen.rs:288-297` |
| `spark`, `arc`, sub-pixel primitives | `sprite_gen.lua:281`, `:296`, ported in Rust |
| Parametric kinematics: locomotion classes, `sine`/`orbital`/`lissajous`/`bezier`, ballistic arcs, capability gating | `lua/distract/locomotion.lua`, engine `path_type` handling, physics fixtures |
| Kitty graphics backend, GIF assets on every backend, placement/anchor/`z`/parallax vocabulary | `CHANGELOG.md [Unreleased]` |

These were removed from `future.md` in the 2026-08-16 rewrite, which now holds
unbuilt work only. Their design is in the companion spec, their history in the
changelog. **Do not re-plan them.**

### Not done — the twelve workstreams

Section numbers below refer to `future.md` **as rewritten on 2026-08-16** to hold
unbuilt work only. The pre-rewrite numbering is gone; do not cite it.

| ID | `future.md` § | Workstream | Depends on |
|---|---|---|---|
| **W1** | §6 Phase 1 | On-screen verification of what shipped | — |
| **W2** | 3 | Art-parity harness + silhouette-first art redo | W1 |
| **W3** | 2.1 | Plugin & middleware hook pipeline | — |
| **W4** | 4.1 | Buffer-constrained & scoped viewport positioning | — |
| **W5** | 4.2 | Application & instance visibility scoping | W4 |
| **W6** | 2.2 | Spatial obstacle & solid platform provider | W4 |
| **W7** | 4.3 | Toroidal edge-splitting & continuous wrap | W4 |
| **W8** | 5.1 | `distract-talk` — speech bubbles | W3, W4 |
| **W9** | 5.2 | `distract-memory` — episodic store | W3 |
| **W10** | 5.3, 5.4 | `distract-lsp`, `distract-physics` | W3, W6 |
| **W11** | 5.5, 5.8 | `distract-weather`, `distract-wpm` | W3 |
| **W12** | 5.6, 5.7 | `distract-ai`, `distract-sprite-craft` MCP | W8, W2 |

### Dependency order

```
W1 ──┬── W2 ─────────────────────────────────┐
     │                                        │
W3 ──┼───────────────┬── W8 ── W12 (ai) ──────┤
     │               │                        │
W4 ──┼── W5          ├── W9                   ├── 100%
     ├── W6 ── W10 ──┘                        │
     └── W7          └── W11 ─────────────────┘
                        W12 (sprite-craft) ◄── W2
```

W1, W3 and W4 have no dependencies and are the three parallel entry points.
W2 gates `distract-sprite-craft` because `validate_sprite_parity` is the harness
W2 builds.

---

## W1 — On-screen verification + roadmap truth-up

**Why first:** every rendering workstream below builds on the kitty backend and
the GIF pipeline, and **nobody has watched either render.** Building W7's
edge-splitting on an unverified compositor means debugging two unknowns at once.

**Files:**
- Modify: `future.md` §6 (strike the verification line once confirmed)
- Modify: `HANDOFF.md` (strike the verification item once a human confirms)
- Create: `tests/screenshots/` entries if the manual session produces reference captures

**Not a code task.** This is a human sitting in Ghostty with the checklist from
`HANDOFF.md` § "The one thing the test suite cannot tell you":

1. `:DistractSpawn cat` in Ghostty. Confirm the kitty backend is chosen without
   `:DistractBackend`, and that a cat appears.
2. If nothing draws: `:set termguicolors?` first — the backend declines without it.
3. Confirm no scrambling from the `U+10EEEE` placeholder (checks Neovim cell width).
4. Confirm no tearing under load — hold a key to force redraws while the sprite moves
   (checks `vim.v.stderr` interleaving).
5. Scroll so the sprite sits below the last buffer line. Confirm the float rows are
   not blanked (checks whether Ghostty paints over a graphics placement).
6. Register a manifest pointing at `assets/cat_walking_1.gif` with
   `frame_width = 32, frame_height = 24`. Confirm it animates, and judge whether it
   reads better than the procedural cat — **this answers W2's art direction.**

**Exit criteria:** all six confirmed, or each failure written up as a defect with
the observed symptom. The GIF-vs-procedural judgement is recorded in the W2 plan.

**Also in this workstream:** answer the two open questions in `HANDOFF.md` —
the acceptable first-draw hitch (~130ms/~375ms today) and whether the half-block
quantiser should run on procedural art. Both are one-line owner decisions that
change W2's scope.

---

## W2 — Art-parity harness + silhouette-first art redo

**Why:** `future.md` §3 promises inner-ear blush, catchlights and anti-aliased
whiskers at 24×16 — which is 24 columns by **8 rows** on a half-block grid. At
that density silhouette is the only thing that reads; the cat currently reads as
a fox. §3 is the one section of `future.md` that is *specified* but not
*achieved*.

**Harness first — this is the gate.** The same art exists twice
(`lua/distract/sprites/*.lua`, `engine/src/sprites/*.rs`) with no automated
parity test. `engine/tests/parity_dump.rs` is `#[ignore]` and dumps geometry, not
physics. Three assets × two implementations = six files that drift the moment one
is touched.

**Files:**
- Create: `tests/fixtures/art/<asset>_<state>_<frame>.json` — per-frame RGB matrices
- Create: `engine/tests/art_parity.rs` — golden generator + Rust-side assertion
- Create: `tests/art_parity_spec.lua` — Lua-side assertion against the same JSON
- Modify: `lua/distract/sprites/{cat,crab,sun}.lua`
- Modify: `engine/src/sprites/{cat,crab,sun}.rs`
- Modify: `lua/distract/sprite_gen.lua`, `engine/src/sprite_gen.rs` if the redo
  needs a flat-fill or contour primitive neither has

**Interfaces:**
- Produces: `validate_sprite_parity` — the same check `future.md` §5.7 names as an
  MCP tool. Build it as a library function so W12 can wrap it rather than reimplement it.

**Tolerance — measured 2026-08-16, do not guess at this.** A dump of all 79
built-in frames from both engines gives 220 mismatched cells out of 27,136
(0.81%): 112 differ in the alpha mask, 108 in colour, and 44 differ by more than
128 on a channel. **An exact-mask assertion is therefore impossible**, and a
per-channel colour tolerance alone will not pass either.

The mechanism: `Canvas::set` floors its coordinates on both sides, and Lua
computes in f64 while Rust computes in f32. A coordinate landing either side of
an integer boundary sends a whole drawing step into the adjacent cell — a
*discontinuous* amplifier, not a rounding error. 204 of the 220 (93%) are
explained by the differing value appearing in an adjacent cell; the other 16 sit
inside a smooth shading gradient where no neighbour holds the identical triple.
**No transcription error was found** — the two ports agree as closely as two float
precisions allow.

So the assertion is: for every cell, the other engine's value must appear either
in that cell or in one of its eight neighbours, **or** be within a per-channel
tolerance of it. Assert on a **budget** — a mismatch count ceiling per asset, set
just above today's measurement — so a transcription error (which produces a large
jump) fails while precision drift does not. Record the measured baseline in the
fixture.

**Art direction (from `HANDOFF.md`):** flat fills, a 1px dark contour, 2–3 tone
bands. Drop `orb`'s five lighting terms on bodies under ~14px wide. Ears larger
than the current 3-pixel stub (`cat.lua`, `EAR_HALF = {0,1,1}`); differentiate the
four leg capsules; delete whiskers and muzzle detail below the resolution floor.

**Scope, owner's call:** **every asset, existing and future** — not the cat alone.

**Secondary win, measured 2026-08-16:** rendering all 79 built-in frames creates
**1,894** live highlight groups against a `max_highlight_groups` cap of 4,096.
Three assets consume 46% of the cap — roughly two assets of headroom before
eviction starts thrashing. A quantised palette is what buys that back.

**Exit criteria:** art-parity harness green on both engines; every asset redrawn;
highlight-group count for the three built-ins measured and recorded before/after;
a human confirms each asset is recognisable at 24×16 in a real terminal.

---

## W3 — Plugin & middleware hook pipeline (§2.1)

**Why:** six of the seven satellite plugins consume this. It is the single highest
-leverage unbuilt surface in `future.md`.

**Files:**
- Create: `lua/distract/plugins.lua` — the registry and dispatch
- Modify: `lua/distract/init.lua` — export `register_plugin`, `unregister_plugin`
- Modify: `lua/distract/engine.lua` — dispatch `on_tick`, `on_state_change`, `on_collision`
- Modify: `lua/distract/events.lua` — dispatch `on_editor_event`
- Create: `tests/plugins_spec.lua`

**Interfaces — the exact contract from `future.md` §2.1:**
```
on_init(world)
on_tick(entity, dt)
on_state_change(entity, from_state, to_state)
on_collision(entity, collision_info)   -- { edge = "top"|"bottom"|"left"|"right"|"obstacle", target = table|nil }
on_editor_event(event_name, context)   -- { cursor_col, cursor_row, buf }
on_teardown()
```

**Design decisions this workstream must settle before code:**
1. **Failure policy.** A hook that errors must not take the engine down, and must
   not be silently swallowed either. Proposal: `xpcall` with a traceback handler,
   report once per plugin per session via `vim.notify`, then **disable that
   plugin** — fail fast at the plugin boundary, not at every tick.
2. **Mutation.** §2.1 says plugins "mutate simulation state". Deciding *what* is
   mutable is the whole contract. Proposal: `on_tick` receives the live entity;
   `on_state_change` may veto by returning `false`; `on_collision` may not mutate.
3. **Ordering.** Registration order, documented and stable. Not a priority number.
4. **Overlay parity.** These hooks run in Lua only. `engine/src/ecs.rs` does not
   gain a plugin system — so a hook that mutates physics **breaks cross-engine
   parity by construction.** Either restrict mutation to non-physics fields, or
   accept that plugins are halfblock/kitty-only and say so in the docs. **This is
   the load-bearing decision of W3.**
5. **Quiescence.** `is_quiescent()` gates redraw. A plugin that animates something
   the engine considers still must be able to mark the world dirty.

**Exit criteria:** all six hooks fire with the documented arguments; a throwing
plugin is reported and disabled without stopping the engine; teardown runs on
`VimLeavePre` and on `engine.stop()`; `plugins.reset()` exists and every spec uses
it; the parity decision from (4) is written into the spec and `doc/distract.txt`.

---

## W4 — Buffer-constrained & scoped viewport positioning (§4.1)

**Why:** the gate for W5, W6 and W8. A speech bubble that overlaps a completion
menu, or a cat that walks over an LSP hover, is worse than no feature.

**Files:**
- Create: `lua/distract/viewport.lua` — scope resolution, exclusion, clipping rect
- Modify: `lua/distract/init.lua` — `positioning` config block with validation
- Modify: `lua/distract/renderer.lua` — clamp against the resolved rect, not the editor grid
- Modify: `lua/distract/external.lua` — send the rect over IPC
- Modify: `engine/src/ipc.rs` — `UpdateViewportScope` message
- Modify: `engine/src/ecs.rs` — clip against the received rect
- Create: `tests/viewport_spec.lua`

**Config, verbatim from §4.1:**
```lua
positioning = {
  scope = "buffer",            -- "buffer" | "window" | "editor" | "absolute"
  exclude_floating = true,
  exclude_filetypes = { "toggleterm", "lazy", "TelescopePrompt", "fzf", "help" },
  z_index_offset = 40,         -- lower than LSP hover / cmp (50+)
}
```

**Traps this workstream will hit:**
- `vim.fn.screenstring` lies inside `nvim -l`. Assert on `nvim_win_get_position` /
  `nvim_win_get_config` for float geometry. `screenstring` is only trustworthy for
  the extmark overlay path.
- The rect changes on `WinScrolled`, `VimResized`, `WinNew`, `WinClosed` and
  `OptionSet`. `events.lua` already registers the first three; adding window
  lifecycle autocmds must go in the same `DistractEvents` group or it leaks a
  duplicate per `setup()`.
- `z_index_offset` interacts with the existing `z` axis. `z` is depth/parallax;
  `z_index_offset` is Neovim float stacking. Two different numbers — name them so
  nobody conflates them.

**Exit criteria:** all four scopes resolve correctly; an excluded filetype or
floating window shrinks the rect; the overlay receives and honours the same rect
(one integration test that round-trips `UpdateViewportScope`); default config
reproduces today's behaviour exactly, pinned by a characterization test written
**before** the change.

---

## W5 — Application & instance visibility scoping (§4.2)

**Why:** `future.md` files this as a bug — sprites render over other applications
and other Neovim instances when focus is lost.

**Files:**
- Modify: `lua/distract/events.lua` — `FocusGained` / `FocusLost` in `DistractEvents`
- Modify: `lua/distract/init.lua` — `restrict_to_instance` config, default `true`
- Modify: `lua/distract/external.lua` — a suspend/resume command
- Modify: `engine/src/ipc.rs`, `engine/src/ecs.rs` — honour it
- Create: `tests/focus_scope_spec.lua`

**Behaviour:**
- `restrict_to_instance = true` (default): on `FocusLost`, hide the overlay and
  stop drawing. The simulation keeps stepping — an entity mid-wrap must not be
  stranded, which is the same reason `is_quiescent` gates redraw and never the step.
- `restrict_to_instance = false`: today's full-screen behaviour, kept because the
  engine is meant to be reusable for standalone desktop animation.
- The in-terminal backends are already instance-scoped; this workstream is mostly
  an overlay concern, plus the split-pane visibility check.

**Exit criteria:** focus loss hides the overlay within one tick and restores on
regain; the simulation state after a hide/show cycle is identical to one without
it (assert on entity positions, not on pixels); `restrict_to_instance = false`
reproduces current behaviour.

---

## W6 — Spatial obstacle & solid platform provider (§2.2)

**Files:**
- Create: `lua/distract/obstacles.lua` — provider registry, per-tick collection, cache
- Modify: `lua/distract/engine.lua` — collision resolution against obstacle rects
- Modify: `engine/src/ecs.rs` — the same resolution, same order
- Create: `tests/fixtures/physics/obstacle_*.json` — parity fixtures
- Create: `tests/obstacles_spec.lua`

**Interface, verbatim from §2.2:**
```lua
distract.register_obstacle_provider(function(win_id, buf_id)
  return {
    { x = 10, y = 15, width = 40, height = 1, type = "solid_platform" },
    { x = 0,  y = 25, width = 80, height = 1, type = "hazard" },
  }
end)
```

**This is the workstream most likely to break parity.** Collision resolution is
physics, it runs in both engines, and the obstacle list originates in Lua. Two
options, decide before writing code:

- **(a) Lua-authoritative:** obstacles are collected in Lua and pushed over IPC
  each time they change, exactly as `sync_floor` pushes the floor to both engines.
  Consistent with the existing rule that neither engine measures its own floor.
  **Recommended.**
- **(b) Duplicated:** each engine collects its own. Reintroduces the divergence
  class the whole parity harness exists to catch. Do not.

Under (a), a provider is called on a documented cadence — not per tick per entity
— because a Tree-sitter query per frame is a performance trap. Proposal: recollect
on `TextChanged`, `WinScrolled` and window lifecycle, debounced through the
existing `events.emit_debounced` path.

**Exit criteria:** an entity lands on a `solid_platform` and stays; `hazard`
triggers a state transition; obstacle collisions fire `on_collision` from W3 with
`edge = "obstacle"` and the rect as `target`; at least three parity fixtures cover
landing, edge-slide and a gap fall; provider errors are reported and the provider
disabled, per W3's failure policy.

---

## W7 — Toroidal edge-splitting & continuous wrap (§4.3)

**Why last of the engine enhancements:** it is the only one that changes the
compositor on both backends, and it wants W1's confirmation that the compositor
works and W4's clipping rect to wrap *against*.

**Files:**
- Modify: `lua/distract/renderer.lua` — emit two surfaces when a sprite straddles an edge
- Modify: `lua/distract/screen_map.lua` — if a split crosses the overlay/float boundary
- Modify: `engine/src/gpu.rs` — emit 2 or 4 `SpriteInstance` quads with scaled UVs in one instanced draw
- Modify: `engine/src/compositor.rs`
- Create: `tests/wrap_split_spec.lua`
- Create: `engine/tests/wrap_instances.rs`

**Today:** `wrap_mode == "wrap"` in `engine.lua:711` teleports. The sprite pops.

**The hard parts, in order:**
1. **Four corners, not two edges.** A sprite in a corner needs four quads. Test the
   corner case first; two-edge cases fall out of it.
2. **The renderer already splits vertically** between an extmark overlay (rows with
   buffer text) and a float (rows below it). A horizontally-wrapped sprite can now
   need overlay *and* float *and* a second pair. `M.place_surface` at
   `renderer.lua:347` is where this lands, and it is already the most intricate
   function in the file. Budget for extracting the split logic into its own module
   rather than growing a 501-line file.
3. **Kitty placements.** A split sprite is two placements of one image, or two
   images. `HANDOFF.md` records that P4 deliberately avoided placement ids — this
   workstream is where that decision gets revisited, because the same image at the
   same scale now genuinely appears twice.
4. **Parallax scaling** multiplies the drawn size, so the split point is computed on
   the *scaled* footprint, not the manifest's.

**Exit criteria:** a sprite crossing any edge or corner is continuous with no gap
and no duplicate row; the GPU path emits the quads in a single instanced draw
(assert on instance count, not on pixels); `clamp`, `bounce`, `despawn` and `none`
are unchanged, pinned by characterization tests written first.

---

## W8 — `distract-talk`: contextual dialogue & speech bubbles (§5.1)

**Repository:** `distract-talk` — separate, depends on `distract` W3 + W4.

**Deliverables:**
- Bubble renderer: rounded box, tail pointing at the owning entity, sized to content
- Placement that respects W4's clipping rect and never covers the cursor line
- Trigger engine subscribing to W3's `on_editor_event` and `on_state_change`
- The four triggers from §5.1: `on_save_untested` (no matching `*_spec.lua`,
  `*_test.go`, `*.test.ts`), `on_git_churn` (high edit velocity with repeated
  undos), `on_long_idle` (15 min), `on_lsp_error` (diagnostic spike)
- A public `say(entity_id, text, opts)` API — W12's AI plugin streams into it

**Exit criteria:** bubble follows its entity; never overlaps the cursor line, a
completion menu (`pumvisible()`), or a floating window; every trigger has a test
driving it from a synthetic event, not from real git or a real LSP; text is
wrapped and bounded (an unbounded model response is a DoS on the renderer).

---

## W9 — `distract-memory`: persistent episodic store (§5.2)

**Repository:** `distract-memory` — depends on `distract` W3.

**Deliverables:**
- Store at `vim.fn.stdpath("data") .. "/distract/memory.json"`, schema `version: 1`
  exactly as §5.2 specifies
- Session tracking, per-language file counts, milestone records
- Contextual greeting selection
- Atomic write (temp file + rename) — a crash mid-write must not lose the store

**Constraints:**
- **Privacy-first, as §5.2 says.** File paths and file contents never enter the
  store. Language names and counts only. This is a hard boundary, and a test asserts
  the serialised store contains no path separator.
- **Time is injected.** `os.time()` is never read inside logic under test.
- **Schema migration, not runtime branching.** A `version` bump gets a migration
  function, not an `if version == 1` in the read path.
- Bounded: cap `milestones` and `languages_spoken` so a decade of use cannot grow
  the file without limit.

**Exit criteria:** round-trip through the file preserves every field; a corrupt or
truncated store is reported and replaced, never silently ignored; greetings are
selected from an injected clock so the test is deterministic.

---

## W10 — `distract-lsp` + `distract-physics` (§5.3, §5.4)

**Repositories:** two, both depending on `distract` W3 + W6.

**`distract-lsp`:**
- `textDocument/documentSymbol` query, debounced, results cached per buffer version
- Symbols become perch points registered through W6's obstacle provider
- 4-quadrant companion planner: Top-Right, Direct-Right, Direct-Left, Bottom-Right,
  scored for non-occlusion against the cursor line, diagnostics and `pumvisible()`
- Diagnostic spikes trigger startled states through W3's dispatch

**`distract-physics`:**
- Tree-sitter queries for function headers, markdown `---` dividers, closed folds
- Each becomes a `solid_platform` rect through W6
- Fold state and buffer edits invalidate the cache

**Shared constraints:** both are pure obstacle/behaviour producers — neither
touches the renderer. LSP requests are async and cancellable; a request in flight
when the buffer changes is cancelled, not awaited. Tree-sitter queries run on the
debounced cadence W6 defines, never per frame.

**Exit criteria:** the cat walks along a function header and falls into an
indented gap; the companion never occludes the cursor line or a visible popup
across a table-driven set of layouts; both degrade to no-op when the LSP client
or parser is absent — that is empty data, not failure, and must not warn.

---

## W11 — `distract-weather` + `distract-wpm` (§5.5, §5.8)

**Repositories:** two, both depending on `distract` W3.

**`distract-weather`** — rain and thunderstorms (density from git diff size,
lightning on syntax errors), sakura petals (drift from scroll momentum), snow
accumulating on the statusline, matrix rain.

The load-bearing question: **particles are many small entities, and the current
ECS was built for three.** Before writing a single effect, measure: spawn 200
entities and profile a tick. If the per-entity cost does not hold at 30 FPS, this
workstream starts with a batched particle path in the core — one entity owning an
array of particles — which is a core change and belongs in `distract`, not the
plugin. **Decide this with a benchmark, not an opinion.**

Also: particles must respect W4's clipping rect and W7's wrap, or rain falls
outside the buffer.

**`distract-wpm`** — WPM measured over a rolling window from `TextChangedI`,
hypersprint above 80 WPM with a particle trail (depends on the same particle
decision), pomodoro focus pose and completion animation.

**Exit criteria:** a 200-particle scene holds the configured frame budget, measured
and recorded; every effect has an off switch and is off by default; WPM is computed
from an injected clock; nothing in either plugin writes to `lua/distract/`.

---

## W12 — `distract-ai` + `distract-sprite-craft` (§5.6, §5.7)

**`distract-ai`** — depends on W8 (`say`). Local models only: SmolLM, Qwen 2.5
0.5B, Ollama. Non-blocking async via `vim.uv`, streaming into the speech bubble.

**Hard requirements, and they are the whole workstream:**
- **Local endpoint only.** No hosted API, no key handling, no telemetry. A
  configured non-localhost endpoint is refused at startup with an explicit error.
- **Off by default, explicit opt-in.** Sending code to any model is a decision the
  user makes, not a default.
- **The prompt is bounded and redacted.** An error snippet, not the buffer. Cap the
  characters sent and say what the cap is. Never send file paths.
- **Failure is silence.** Endpoint down, model missing, timeout — no bubble, one
  `WARN`, no retry storm.
- **Bounded output.** Truncate at a documented character count before it reaches W8.

**`distract-sprite-craft`** — depends on W2. An MCP server exposing three tools:
- `create_sprite_asset` — generate pose curves, shading params, a manifest
- `validate_sprite_parity` — **wraps W2's harness; does not reimplement it**
- `preview_sprite_terminal` — half-block ANSI frames into the agent console

**Exit criteria (ai):** works fully offline against a stub endpoint in tests, with
no network in CI; refuses a remote endpoint; every failure mode covered by a test.
**Exit criteria (sprite-craft):** each tool has a schema and a test; a generated
asset passes `validate_sprite_parity` on both engines; the server runs standalone
without Neovim.

---

## Program-level risks

| Risk | Why it bites | Mitigation |
|---|---|---|
| **Plugin hooks break cross-engine parity** | W3 hooks are Lua-only; the Rust engine has no plugin system. A hook that mutates physics makes one manifest behave differently per backend — the exact defect class this project keeps hitting. | Settle W3 decision (4) *first*. Either restrict mutation to non-physics fields, or declare plugins halfblock/kitty-only in the docs and the config validation. |
| **Obstacles duplicated across engines** | Same defect class, higher surface area. | Lua-authoritative, pushed over IPC like `sync_floor`. Parity fixtures for every collision mode. |
| **Particle counts exceed the ECS design** | ECS built for 3 entities; weather wants hundreds. | Benchmark before W11 starts. A batched particle path is a core change and must be planned as one. |
| **`engine.lua` and `renderer.lua` are past the caps** | W6 and W7 both add to them. Grandfathered debt becomes a wall. | Every workstream touching them extracts a module rather than appending. W7 explicitly budgets the split extraction. |
| **Satellite plugins fork the core** | Seven repos depending on three surfaces; one missing hook and a plugin reaches into internals. | W3, W4 and W6 must be complete and documented before W8 starts. A plugin requiring an unpublished internal is a core bug, filed against the core. |
| **`luacheck` is unverifiable locally** | A whole program's worth of changes lands with one gate only ever checked in CI. | Fix the local Lua 5.5 environment during W1, or pin a working interpreter in the repo tooling. Cheap now, expensive at W12. |
| **The roadmap drifts back into describing shipped work** | A reader who cannot tell built from unbuilt rebuilds locomotion. | `future.md` holds unbuilt work only; every workstream deletes its own section from it as the last commit. |

---

## Definition of done, program level

1. `future.md` is empty of workstream sections, or each remaining one is explicitly cut with a recorded reason.
2. Four gates green; Lua and Rust test counts both materially above the 325/145 baseline.
3. `luacheck` verified green somewhere the owner can run it.
4. Physics **and** art parity harnesses both green, both covering every feature that
   exists in two implementations.
5. `doc/distract.txt` and `README.md` document every public API added: `register_plugin`,
   `register_obstacle_provider`, `positioning`, `restrict_to_instance`.
6. `CHANGELOG.md` `[Unreleased]` lists every workstream.
7. A human has watched: the kitty backend, a GIF asset, every redrawn sprite, an
   edge-split wrap, and one satellite plugin, on a real screen.
8. Each satellite plugin installs standalone against a released `distract` tag and
   needs no change to this repository.

---

## Next step

Write the W1 plan — it is the only workstream with no code dependencies, it
unblocks W2's art direction, and it settles two open owner questions. W3 and W4
can be planned and executed in parallel by a second worker.

Each workstream plan is written with `superpowers:writing-plans` immediately
before execution, not now — writing bite-sized TDD steps for W12 today would
encode assumptions about W3 and W8 that do not exist yet.
