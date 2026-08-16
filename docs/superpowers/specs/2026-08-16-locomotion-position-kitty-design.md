# Locomotion, position, and the kitty backend

Design for handoff steps 3 and 5, taken together. Written 2026-08-16 against
`main` at `58394c4` plus the uncommitted steps 1–2 work.

---

## Why these two steps together

Step 3 widens the manifest schema: three locomotion classes, five path types,
capability gating, an anchor/floor system, and a `z` axis. Step 5 adds a third
rendering backend.

They are specified together because `z` only earns its second meaning —
parallax scale — if a backend exists that can scale a sprite. The halfblock
backend cannot: a half-block cell is a fixed 1×0.5 cells and has no
sub-cell scaling. The overlay can. Kitty can, via the graphics protocol's
`c`/`r` placement keys. Designing the schema against one consumer and adding
the second later is how the divergences that step 1 spent its whole budget
repairing got introduced.

## Goals

- One manifest describes one behaviour on every backend, **enforced by tests**
  rather than by reviewer attention.
- Configurable placement (`top`, `bottom`, explicit `(x, y)` or `(x, y, z)`)
  constrained by what an entity can physically do.
- Parametric 2D motion: the sun drifts freely, the cat and crab are bound by
  gravity.
- In-terminal fidelity approaching `assets/cat_walking_1.gif`.

## Non-goals

- Step 4's art redo and palette quantisation. One piece is pulled forward
  (§ 8.3) because GIF-on-halfblock makes it a correctness issue.
- Toroidal edge-splitting (`future.md` § 4.3).
- Buffer-scoped positioning (`future.md` § 4.1).

## Unit contract

Unchanged, and load-bearing:

- **Position, `ground_y`, anchors** — terminal cells.
- **Velocity, acceleration, path amplitude** — sprite pixels per 60 FPS frame.
- **`z`** — dimensionless.

No third unit is introduced. `external.lua` continues to convert cells to
overlay pixels at the IPC boundary.

---

## 1. The parity harness and the `dt` seam

The recurring defect class in this codebase is Lua/Rust physics divergence.
Step 1 found three instances (`wrap`, `bounce`, `animation.flip_x`) plus two
schema fields read by nothing. Step 3 adds five path types and three
locomotion classes — the largest divergence surface yet. The harness lands
**before** the schema widens.

### 1.1 The seam

`engine.tick()` reads `uv.hrtime()` and `vim.o.columns/lines` inline, which is
why handoff trap #3 exists. Split it:

```lua
M.step(dt, bounds)  -- pure: no clock, no vim.o. bounds = { columns, lines }
M.tick()            -- wall-clock dt, 0.1s clamp, vim.o bounds -> M.step
```

Existing engine tests that assert direction-not-magnitude can then assert
magnitude. That is a strict improvement to the current 145 tests, independent
of parity.

### 1.2 Normalisation

The two loops are already algebraically identical. Lua integrates in cells
(`CELLS_PER_SPRITE_PX_X = 1.0`, `_Y = 0.5`); Rust integrates in pixels
(`scale_x = cell_w`, `scale_y = cell_h / 2`):

| | Lua | Rust | Normalised |
|---|---|---|---|
| x | `x += vx*step*1.0` | `x_px += vx*step*cell_w` | `x_px / cell_w` |
| y | `y += vy*step*0.5` | `y_px += vy*step*cell_h/2` | `y_px / cell_h` |
| sine amp | `amp*0.5` cells | `amp*cell_h/2` px | identical |

Comparison happens in cells, with no fudge factor.

### 1.3 Topology

Rust and Lua never share a process, so neither test drives the other:

```
tests/fixtures/physics/<case>.json         input
tests/fixtures/physics/<case>.golden.json  trajectory, in cells
```

- `engine/tests/physics_parity.rs` runs `World::update`, normalises, asserts
  against the golden. `UPDATE_GOLDEN=1` regenerates. Catches stale goldens.
- `tests/physics_parity_spec.lua` runs `M.step`, asserts against the same
  golden. Catches Lua drift.

Neither suite can pass while the engines disagree.

`engine/tests/parity_dump.rs` stays `#[ignore]`. It dumps **sprite geometry**,
not physics, so it belongs to step 4's art-parity work
(`validate_sprite_parity`, `future.md` § 5.8) rather than here. Folding it in
would have miscategorised it.

Input fixture shape:

```json
{
  "physics":    { "target_vx": 1.5, "gravity": 0.0, "wrap_mode": "wrap" },
  "spawn":      { "x": 40, "y": 12, "heading_x": 1 },
  "sprite":     { "w": 24, "h": 16 },
  "cell":       { "w": 10, "h": 20 },
  "bounds":     { "columns": 120, "lines": 40 },
  "dt": 0.0166666, "steps": 240
}
```

Trajectory entry: `{ x, y, vx, vy, flip_x, state }` per step, in cells.

### 1.4 Tolerance

`1e-3` cells. Rust is `f32`, Lua is `f64`; exact equality is not available.
Real divergence bugs are order-of-cells.

### 1.5 The harness's own blind spot

Goldens are generated from Rust, making Rust the reference implementation by
construction. If Rust is wrong, the harness cements it.

Mitigation: analytically checkable cases carry hand-computed assertions rather
than only self-consistency — constant-velocity displacement, ballistic apex
height and flight time, sine extrema and period, wrap-around positions.

### 1.6 Fixture matrix

Four `wrap_mode`s × three locomotion classes; gravity with floor; `accel_x`
and `accel_y` floorless; each of the five path types; edge transitions firing
(`on_edge_left`/`on_edge_right`/`on_land`); `flip_x` XOR heading; quiescence
onset. Goldens for **current** behaviour are generated first, and the schema
work must preserve them bit-for-bit.

---

## 2. Locomotion

| `locomotion` | gravity | floor | `path_type` | new transition |
|---|---|---|---|---|
| `grounded` | > 0 | clamp at `ground_y` | `linear` | — |
| `ballistic` | > 0 | clamp, fires `on_land` | `linear` | `on_land` |
| `omnidirectional` | must be 0 | none | all five | — |

`grounded` and `ballistic` share the integrator. The difference is `on_land`,
which does not exist today: the cat's jump returns via the animation's
`on_finish`, so it lands when the art happens to end rather than when it
touches the ground.

`on_land` fires once, on the tick where a `ballistic` entity's `y` clamps to
`ground_y` with `vy > 0`.

### 2.1 Backward compatibility

No existing manifest sets `locomotion`. Omitted, it is derived:

- `gravity > 0` → `grounded`
- otherwise → `omnidirectional`

Every current manifest keeps its exact behaviour, which the § 1.6 pre-generated
goldens enforce.

---

## 3. Path primitives

Phase-driven, evaluated identically on both sides:

```
linear     no positional override; pure velocity integration
sine       y = base_y + sin(freq_y*phase)*amp_y
orbital    x = base_x + cos(freq_x*phase)*amp_x
           y = base_y + sin(freq_y*phase)*amp_y
lissajous  x = base_x + sin(freq_x*phase + phase_delta)*amp_x
           y = base_y + sin(freq_y*phase)*amp_y
bezier     cubic over path_params.points, phase wrapped to [0,1]
```

Phase advances at a base rate, and per-axis frequency multiplies **inside** the
trigonometric term:

```
phase += dt * (path_params.freq or 1.0)
```

This matters for exactness. The existing implementation advances
`path_phase += dt * path_frequency` and then takes `sin(path_phase)`. With
`freq` defaulting to `1.0` and the `path_frequency → freq_y` alias, the new
form evaluates `sin(freq_y * t)` — identical. Folding frequency into the phase
advance instead would double-apply it on `lissajous`, where `freq_x` and
`freq_y` must differ against one shared phase.

Amplitudes are in the manifest unit — sprite pixels — so `amp_x * 1.0` cells
and `amp_y * 0.5` cells, byte-identical to how `path_amplitude` is handled
today.

`bezier` control points: `path_params.points = { {x,y}, {x,y}, {x,y}, {x,y} }`,
in sprite pixels relative to the spawn position, cubic, phase looping over
`[0,1]`.

### 3.1 Legacy aliases

`path_amplitude` → `amp_y`, `path_frequency` → `freq_y`. The sun's manifest
uses both; they keep working, on both engines.

### 3.2 Gating

Anything past `linear` and `sine` writes `x` directly, which fights a floor.
Non-`linear`/`sine` paths therefore require `omnidirectional`, enforced by the
same load-time gate as § 4.

---

## 4. Capability gating

Top-level in the manifest, permissive by default:

```lua
capabilities = { locomotion = { "grounded", "ballistic" } }
```

Omitted means all three are allowed, so no existing user manifest can newly
fail. Only a manifest that declares capabilities can violate them.

Built-ins declare:

| asset | locomotion |
|---|---|
| cat | `grounded`, `ballistic` |
| crab | `grounded` |
| sun | `omnidirectional` |

Validated **at manifest load, across all states at once** — never per frame.

- Rust: parser returns
  `Err(ManifestError::CapabilityViolation { state, locomotion, allowed })`.
- Lua: `vim.notify(..., ERROR)` with the same wording; `spawn` returns `nil`
  and creates no entity.

---

## 5. Position, anchors, and the floor

```lua
require("distract").setup({
  position = {
    anchor = "bottom",   -- "bottom" | "top" | { x = , y = , z = }
    ground = "screen",   -- "screen" | "text"
    parallax = { per_unit = 0.0, min = 0.4, max = 1.6 },
  },
})
```

Per-spawn override of any field: `spawn("cat", { x =, y =, z =, anchor =, ground = })`.

### 5.1 The two floors

Both are computed **in Lua, for both backends**. `external.lua` already owns
the cells→pixels conversion at the IPC boundary, so it owns this too, and the
overlay never needs a buffer concept.

- `"screen"` — `lines - cmdheight - laststatus_rows - sprite_h`. Recomputed on
  `VimResized` and on `OptionSet` for `cmdheight` and `laststatus`.
- `"text"` — the screen row of the last buffer line, via the screen map step 2
  already built. Recomputed only when the `getwininfo()` fingerprint changes,
  which is the gate the overlay path already uses, so a stationary screen
  still costs zero API calls.

Pushed to the overlay as an extra field on the existing message, on change,
never per frame:

```rust
UpdateGrid { width, height, cell_width, cell_height, scale_factor,
             ground_y: Option<f32> }   // #[serde(default)]
```

`#[serde(default)]` keeps older clients wire-compatible.

### 5.2 `z`

Two meanings, one field:

- **Order** — `z` overrides the manifest's `z_index`. `compositor.rs:138` and
  `gpu.rs:61` already sort by it; `renderer.lua:322` already maps it to
  `zindex`. No new machinery.
- **Parallax** — `scale = clamp(1 + z * per_unit, min, max)`. Both `vx` and
  `vy` are damped by the same factor, so distant things drift slower in both
  axes.

`per_unit` defaults to `0.0`. **Parallax is off unless asked for**, and no
existing configuration changes behaviour.

### 5.3 Backend capabilities

Replaces the ad-hoc `SUBSTITUTED_ALIASES` warning in `init.lua` with a table
that backends register into:

| backend | scale | alpha | `z` |
|---|---|---|---|
| `halfblock` | ✗ | per-cell | order |
| `overlay` | ✓ | per-pixel | order + parallax |
| `kitty` | ✓ | per-pixel | order + parallax |

`halfblock` with `per_unit ≠ 0` warns **once** and honours order only. A
declared degradation, not a silent divergence. Because it is table-driven, the
kitty backend registers rather than special-cases.

### 5.4 `:DistractSpawn`

`plugin/distract.lua:53` currently calls `distract().spawn(pet_type)` and drops
opts entirely. It parses and forwards `x=`, `y=`, `z=`, `anchor=`, and
completion offers them.

---

## 6. Quiescence

`engine.lua` gains `is_quiescent()` mirroring `ecs.rs:650` semantics exactly;
`tick` returns early when true. Today `tick` returns early only at zero
entities, so a screen of sleeping cats wakes the editor loop 30×/sec forever.

Because "exactly" is a parity claim, quiescence onset is a § 1.6 fixture case.

---

## 7. The kitty backend

### 7.1 Placement

Unicode placeholders (`U+10EEEE` with row/column diacritics, `U=1`), not direct
cursor placement. Placeholder cells are ordinary text, so kitty rides the
`virt_text_win_col` overlay path step 2 built, inheriting the screen map, run
merging, float tail, and the zero-API-calls-while-stationary signature. Direct
placement would fight Neovim's redraw and need a parallel positioning system.

### 7.2 Transmission and caching

Raw RGBA (`f=32`), base64, chunked at 4096 with `m=1`/`m=0`. No PNG encoder,
no zlib, no external process. Per-pixel alpha gives real transparency with no
half-block subsampling.

Images are cached on `(asset, frame, flip_x)` — the identical key
`get_rendered_frame` and `get_frame_buffer` already use. Transmitted once,
re-placed cheaply, deleted with `a=d,d=i` from `reset_cache`. Same lifetime
rules and same invalidation as step 2's frame buffers.

**Footprint is unchanged.** A W×H sprite occupies W cols × H/2 rows on kitty
exactly as on halfblock. Fidelity comes from pixel density inside that rect,
not a larger rect, so positions, `ground_y`, and the unit contract are
untouched. Parallax scales the rect via `c`/`r` and kitty resamples.

### 7.3 Writing to the terminal — settled by the P0 spike

Two mechanisms reach the terminal from inside Neovim 0.12, verified by driving
a real `nvim` TUI under a pty and capturing the byte stream:

| Mechanism | Result |
|---|---|
| `io.stdout:write(seq)` | reaches the tty |
| `vim.api.nvim_chan_send(vim.v.stderr, seq)` | reaches the tty |
| `vim.api.nvim_chan_send(vim.api.nvim_list_uis()[1].chan, seq)` | **fails** — `Can't send raw data to rpc channel` |
| `io.open("/dev/tty", "w")` | fails without a controlling terminal |

The UI-channel route is the one to avoid, and it is the obvious one to reach
for. On 0.12 the entry `nvim_list_uis()` returns is an **RPC** channel, so raw
bytes are rejected outright.

**`vim.v.stderr` is the primary**, with `io.stdout` as fallback. Neovim's own
TUI renders through stdout, so writing escapes there risks interleaving into
the middle of one of its sequences; stderr reaches the same terminal without
sharing a stream with the renderer.

### 7.4 Detection

Confirmed against ghostty: `TERM=xterm-ghostty`, `TERM_PROGRAM=ghostty`, and
the protocol query is answered:

```
-> \x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\
<- \x1b_Gi=31;OK\x1b\\
```

So detection cannot key on kitty-specific variables alone — `$KITTY_WINDOW_ID`
is absent under ghostty. The env check (`$TERM`, `$TERM_PROGRAM`,
`$KITTY_WINDOW_ID`) is a fast path only; the `a=q` query with a timeout is the
authority, and anything that does not answer `OK` falls back to halfblock. The
three `SUBSTITUTED_ALIASES` in `init.lua` stop warning and resolve for real.

### 7.5 Testability

The backend emits through an **injectable writer**. Escape generation is pure
string building, so headless tests capture the byte stream and assert chunk
boundaries, base64 payloads, placeholder diacritic encoding, `z` values, and
delete-on-despawn — no tty required.

End-to-end confirmation does need a pty, since `nvim --headless` attaches no
UI. `pty.openpty()` from Python drives a real `nvim` TUI and captures its
output; `script(1)` does **not** work in a sandboxed session
(`tcgetattr/ioctl: Operation not supported on socket`) because it needs a
controlling terminal to inherit.

---

## 8. GIF support

A pure-Lua GIF decoder, `lua/distract/gif.lua`: GIF87a/89a, LZW, interlacing,
global and local palettes, the transparency index, and disposal methods 0–3.
No dependencies, no external process.

### 8.1 One manifest, every backend

GIF assets are declared through the **existing** `spritesheet.path` field that
Rust already reads, not a new Lua-only field. The Lua side decodes lazily into
frame matrices and feeds the same `terminal_sprites` cache every other asset
uses.

The consequence is worth stating plainly: a GIF asset then works on halfblock,
kitty, and overlay with no per-backend branching — at half-block fidelity,
full pixel fidelity, and full pixel fidelity respectively.

### 8.2 Frame timing

GIFs carry per-frame delays; manifests carry `animation.fps`. If the asset is a
GIF and the state's animation omits `fps`, the GIF's own per-frame delays are
used. An explicit `fps` overrides them.

### 8.3 Palette quantisation on halfblock

Handoff trap #4: 1,909 global highlight groups already exist for three built-in
assets, created by `nvim_set_hl` and never released. A GIF carries up to 256
colours per frame and would make this materially worse.

So the halfblock path quantises GIF-sourced frames to a configurable cap
(default 128 colours) and puts an LRU over the generated highlight groups.
Kitty and overlay take full colour and are unaffected.

This pulls one piece of step 4 forward. It is in scope here because GIF support
turns highlight-group growth from untidiness into a correctness problem.

---

## 9. Testing strategy

| Layer | Mechanism | Gates |
|---|---|---|
| Physics parity | § 1 golden files | both suites |
| Locomotion, gating, paths | Lua and Rust unit tests | both suites |
| Anchors and floor | Lua tests against synthetic `getwininfo` | Lua suite |
| Kitty protocol | injectable writer, captured byte stream | Lua suite |
| GIF decoder | fixture GIFs with known pixel values | Lua suite |
| Visual | real kitty terminal | manual |

Existing gates continue to apply and must stay green:

```bash
nvim --headless -u tests/minimal_init.lua -c "luafile tests/run_tests.lua"
```

```bash
cargo test --manifest-path engine/Cargo.toml
```

```bash
stylua --check lua plugin tests
```

```bash
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Baseline at the time of writing: 145 Lua tests, 102 Rust tests, all passing.
`luacheck` is broken in this environment (handoff § "Verify the current state")
and may still run in CI.

---

## 10. Implementation phases

| Phase | Content | Precondition |
|---|---|---|
| P0 | Kitty protocol spike, throwaway | **done** — see § 7.3, § 7.4 |
| P1 | `dt` seam, parity harness, goldens for **current** behaviour | — |
| P2 | Locomotion, capabilities, path primitives, `on_land`, quiescence, `:DistractSpawn` opts | P1 green |
| P3 | Position, anchors, floor, `z`, backend capability table, `UpdateGrid.ground_y` | P2 green |
| P4 | Kitty backend, procedural sprites | P0 answered, P3 green |
| P5 | GIF decoder, `terminal_sprites` wiring, halfblock quantiser | P4 green |

P1 before P2 is the load-bearing ordering: goldens must capture current
behaviour before the schema widens, or the harness cannot tell a refactor from
a regression.

---

## 11. Risks

1. **Rust as reference by construction** (§ 1.5). Mitigated by hand-computed
   assertions on analytically checkable cases.
2. **Kitty tty writes from Neovim** (§ 7.5). Mitigated by the P0 spike ahead of
   any dependent work.
3. **Highlight-group growth** (§ 8.3). Mitigated by quantisation plus an LRU,
   but the underlying unbounded-growth problem is step 4's to solve properly.
4. **`ground = "text"` and wrapped or folded lines.** Step 2's screen map only
   maps the row a line *starts* on. The text floor inherits that limitation and
   falls back to the screen floor where a row is unmappable.
5. **`setup` cannot remove a field** (handoff trap #2). `capabilities` is a
   list, so a user narrowing it via `setup` merges rather than replaces.
   Documented; the per-spawn override is the escape hatch.
