.PHONY: test test-rust test-lua

test: test-rust test-lua

test-rust:
	cargo test --manifest-path engine/Cargo.toml

test-lua:
	nvim --headless --noplugin -u tests/minimal_init.lua \
		-c "PlenaryBustedDirectory tests/ { minimal_init = 'tests/minimal_init.lua' }"
