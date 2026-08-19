# The Plugin API — stability contract

What a downstream plugin may depend on, what it may not, and what a version
number promises. `doc/distract.txt` describes what each surface *does*; this
file says what will still be true in six months.

Applies from **v0.1.0**. This project follows
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

---

## 1. The public surface

**`require("distract")` is the entire public API.** Nothing else is.

```lua
local distract = require("distract")
```

### Lifecycle

| Item | Contract |
|---|---|
| `setup(opts)` | Optional; the first command calls it with defaults. Merges over previous calls. |
| `start()` / `stop()` / `is_running()` | Idempotent. `stop()` on a stopped engine is a no-op. |
| `spawn(asset_name, opts)` | `opts`: `x`, `y`, `z`, `anchor`, `flip_x`. Returns nil on a refused manifest, after reporting why. |
| `action(action_name, target)` | Triggers a declared capability. |
| `clear()` / `status()` | |

### Registration

| Item | Contract |
|---|---|
| `register_asset(name, spec)` | `spec.manifest`, `spec.sprites`. Either may be omitted. |
| `register_plugin(name, spec)` | Hooks below. An unknown hook key is refused, not ignored. |
| `unregister_plugin(name)` | Returns `boolean`. |
| `register_obstacle_provider(fn)` | Returns an opaque id. |
| `unregister_obstacle_provider(id)` | Returns `boolean`. Ids are stable and never reused within a session. |

### Query

| Item | Contract |
|---|---|
| `get_backend()` | `"halfblock"` \| `"kitty"` \| `"overlay"` |
| `get_backend_capabilities()` | `{ scale, alpha, native_resolution }` |
| `get_available_backends()` | |
| `get_asset_names()` / `get_all_actions()` / `get_plugin_names()` | Sorted, and safe to call before `setup()`. |
| `get_render()` / `set_render(opts)` | |
| `set_backend(name)` / `is_overlay()` | |

### Installation

| Item | Contract |
|---|---|
| `build(on_success)` / `download(on_success)` | Asynchronous. `on_success` runs only on a verified install. |

### Hooks

Every hook is optional. The set is closed — a key outside it raises at
registration rather than being silently dropped.

```
on_init(world)                              on_collision(entity, collision)
on_tick(entity, dt)                         on_editor_event(name, context)
on_state_change(entity, from, to)           on_draw(layers)
on_teardown()
```

### The world handle

```
world.backend                    world.request_state(id, state)
world.entities()                 world.apply_impulse(id, vx, vy)
world.mark_dirty()               world.despawn(id)
```

### `distract.config`

Readable. **Writing to it is not supported** and is not covered by this
contract: `setup()` is the supported way to change configuration, and several
fields are pushed to a running engine on assignment through `setup` only.

---

## 2. What is *not* public

**Every `distract.*` submodule is internal.** All 67 of them.

```lua
require("distract.engine")          -- NOT public
require("distract.renderer")        -- NOT public
require("distract.external")        -- NOT public
require("distract.sprite_gen")      -- NOT public
```

Lua cannot enforce this, so it is stated instead: these move, split, merge and
disappear between any two versions, including patch releases. `renderer.lua`
and `engine.lua` have each already been decomposed into several modules once.
A plugin reaching into one is depending on an implementation detail, and no
version number protects it.

If you need something only reachable through an internal module, **open an
issue**. That is a gap in this contract, and the fix is to widen the public
surface deliberately rather than to have you route around it.

Also not public: the IPC wire format between the plugin and the overlay engine,
the `.rgba` sidecar format, the golden JSON in `tests/fixtures/`, the generated
highlight-group names, and anything under `engine/src/`.

---

## 3. What a version bump means

| Change | Bump |
|---|---|
| Removing a public function, or narrowing what it accepts | **major** |
| Adding a required parameter | **major** |
| Renaming a hook, or removing one | **major** |
| Changing a documented return type | **major** |
| Changing manifest schema in a way that invalidates existing manifests | **major** |
| Adding a function, a hook, an optional parameter, a config key | minor |
| Adding a manifest field with a default that preserves current behaviour | minor |
| A new bundled asset | minor |
| Bug fix, performance, internal decomposition | patch |

**A manifest is part of the contract.** Manifests are written by users, not just
by this repository, so the schema is a public interface: a field that changes
meaning breaks every manifest already written against it. This is why weighted
`on_event` targets are held for a deliberate major rather than folded into a fix
pass — see the pending-work checklist in `README.md`.

Pre-1.0 note: semver permits breaking changes in minor releases below 1.0. This
project does not use that latitude. A break gets a major bump at any version.

---

## 4. Deprecation

Nothing public is removed without warning first.

1. The item keeps working and gains a deprecation notice naming its replacement.
2. It stays for **at least one minor release**.
3. It is removed in the next major.

A deprecation notice fires once per session at `WARN`, through `vim.notify`,
and names the replacement — never a bare "deprecated".

---

## 5. What is guaranteed across backends

This is the load-bearing promise, and the one the parity harnesses exist to
keep: **one manifest describes one behaviour on every backend.**

A plugin written against `halfblock` behaves the same on `kitty` and `overlay`.
Where a backend genuinely cannot do something — the half-block renderer cannot
scale a sprite, so it cannot show parallax — `get_backend_capabilities()` says
so before you rely on it, and the degradation is reported once rather than
happening silently.

Three harnesses pin this in CI, not by review: `physics_parity` pins one
manifest to one trajectory on both engines, `sprite_parity` pins the generated
art pixel for pixel, `voxel_parity` pins the 3D models vertex for vertex.

What is **not** guaranteed to match across backends: exact pixel output at a
given resolution, sub-cell placement, and colour after quantisation. A backend
with a 4,096 highlight-group ceiling and one with a real alpha channel do not
produce identical images, and no version promises they will.

---

## 6. Verifying against this contract

`tests/public_api_spec.lua` pins the exact exported surface. Adding or removing
anything on `require("distract")` fails it, so a break has to be deliberate and
arrives with the version bump this file requires.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
```

If you maintain a downstream plugin, the useful thing to copy is the shape of
that spec: assert the surface you depend on exists, and you will find out from
your own suite rather than from a user's bug report.
