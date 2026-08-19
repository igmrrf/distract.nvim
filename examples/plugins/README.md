# Reference plugins

Working plugins that exercise every extension point the core exposes. They are
examples, not features: nothing here is loaded by `distract.nvim` itself, and
each is small enough to read in one sitting and copy into your own config.

The ecosystem plugins sketched in [`future.md`](../../future.md) §5 —
`distract-talk`, `distract-memory`, `distract-lsp`, `distract-physics`,
`distract-weather`, `distract-ai`, `distract-wpm` — are separate repositories
built on exactly these three surfaces. If one of them cannot be written against
what is here, that is a gap in the core.

| File | Surface it exercises |
|---|---|
| [`reactions.lua`](reactions.lua) | `register_plugin` — every lifecycle hook, and the world command queue |
| [`headers_as_platforms.lua`](headers_as_platforms.lua) | `register_obstacle_provider` — solid ground from the buffer's own text |

## Trying them

```lua
require("distract").setup({})
-- Sprites need somewhere to stand for the platform example to be visible.
require("distract").setup({ position = { ground = "text" } })

dofile(vim.fn.expand("~/path/to/distract.nvim/examples/plugins/reactions.lua"))
dofile(vim.fn.expand("~/path/to/distract.nvim/examples/plugins/headers_as_platforms.lua"))

vim.cmd("DistractSpawn cat")
vim.cmd("DistractStart")
```

## What the surfaces guarantee

- **Hooks run in Lua on every backend.** The in-terminal engines simulate in
  Lua and dispatch directly; the overlay simulates in its own process and
  reports back over IPC, and nothing is put on the wire unless a registered
  plugin actually subscribes to it.
- **The entity a hook receives is read-only.** Assigning to it raises. Every
  change goes through the `world` handle, which queues a command that the
  running backend applies — locally, or over IPC — so one plugin behaves the
  same way on all three backends.
- **A hook that raises is reported once and its plugin is disabled** for the
  session. One broken plugin cannot take the others down, and cannot produce a
  notification per frame.
- **Obstacle providers are called on a debounced cadence**, never per tick per
  entity. A Tree-sitter query per frame is a performance trap; the provider
  contract exists to keep you out of it.
