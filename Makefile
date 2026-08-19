.PHONY: test test-rust test-lua lint fmt

test: test-rust test-lua

test-rust:
	cargo test --manifest-path engine/Cargo.toml

# tests/run_tests.lua, not PlenaryBustedDirectory: this repository has its own
# harness in tests/test_harness.lua and does not depend on Plenary. The Plenary
# runner found six specs it could drive and reported success, while the other
# five hundred never ran.
test-lua:
	nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua

fmt:
	cargo fmt --manifest-path engine/Cargo.toml
	stylua lua plugin tests

lint:
	cargo fmt --manifest-path engine/Cargo.toml -- --check
	cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
	stylua --check lua plugin tests
	luacheck lua plugin tests
