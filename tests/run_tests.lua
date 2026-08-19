-- Master Test Runner for distract.nvim
-- Runs all modular spec files and outputs a consolidated report.
--
-- Exit codes: 0 on success, 1 on any failure (via :cquit). Never hangs --
-- every path terminates the editor, so a headless CI job cannot stall.

local harness = require("tests.test_harness")

local SPECS = {
  "tests.init_spec",
  "tests.external_spec",
  "tests.events_spec",
  "tests.manifests_spec",
  "tests.plugin_commands_spec",
  "tests.sprite_gen_spec",
  "tests.sprites_spec",
  "tests.sprite_assets_spec",
  "tests.highlights_spec",
  "tests.quantise_spec",
  "tests.native_sprite_spec",
  "tests.gif_spec",
  "tests.gif_assets_spec",
  "tests.renderer_spec",
  "tests.engine_spec",
  "tests.backends_spec",
  "tests.kitty_spec",
  "tests.position_spec",
  "tests.review_fixes_spec",
  "tests.transparency_spec",
  "tests.physics_parity_spec",
  "tests.sprite_parity_spec",
}

print("==================================================")
print("  Running Modular Test Suites for distract.nvim")
print("==================================================")

local load_errors = {}
for _, spec in ipairs(SPECS) do
  local ok, err = pcall(require, spec)
  if not ok then
    table.insert(load_errors, string.format("%s: %s", spec, tostring(err)))
    print(string.format("\n!! FAILED TO LOAD %s\n   %s", spec, tostring(err)))
  end
end

local reported, report_err = pcall(harness.report)

if reported and #load_errors == 0 then
  print("\nALL MODULAR TEST SUITES PASSED\n")
  vim.cmd("qall!")
else
  if not reported then
    print("\n" .. tostring(report_err))
  end
  if #load_errors > 0 then
    print(string.format("\n%d spec file(s) failed to load.", #load_errors))
  end
  print("\nTEST RUN FAILED\n")
  -- :cquit terminates with a non-zero exit status so CI actually fails.
  vim.cmd("cquit 1")
end
