-- Master Test Runner for distract.nvim
-- Runs all modular spec files and outputs a consolidated report

local harness = require("tests.test_harness")

print("==================================================")
print("  Running Modular Test Suites for distract.nvim")
print("==================================================")

-- 1. Core Module Specs
require("tests.init_spec")

-- 2. External IPC Specs
require("tests.external_spec")

-- 3. Autocmd & Events Specs
require("tests.events_spec")

-- 4. Manifests & Schema Specs
require("tests.manifests_spec")

-- 5. User Commands & Completions Specs
require("tests.plugin_commands_spec")

-- Final Aggregated Report
harness.report()

print("\n🎉 ALL MODULAR TEST SUITES PASSED SUCCESSFULLY! 🎉\n")
vim.cmd("qall!")
