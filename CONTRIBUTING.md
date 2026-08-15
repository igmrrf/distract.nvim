# Contributing to distract.nvim

Thank you for your interest in contributing to **distract.nvim**! We welcome contributions, bug reports, feature requests, and documentation improvements.

---

## 🛠️ Development Setup

### Prerequisites

- **Neovim** (>= 0.9.0)
- **Rust** (stable toolchain with `cargo` and `rustfmt`)
- **Git**

### Clone Repository

```bash
git clone https://github.com/igmrrf/distract.nvim.git
cd distract.nvim
```

### Enable Git Pre-Commit Hooks

We provide automated test hooks before each commit. You can enable them using native Git:

```bash
git config core.hooksPath .githooks
```

Or using `pre-commit`:

```bash
pip install pre-commit
pre-commit install
```

---

## 🧪 Running Tests

### 1. Run All Tests
```bash
make test
```

### 2. Run Rust Unit & Integration Tests
```bash
cargo test --manifest-path engine/Cargo.toml
```

### 3. Run Neovim Lua Test Suite
```bash
nvim --headless -u NONE \
  -c "set rtp+=." \
  -c "runtime plugin/distract.lua" \
  -c "luafile tests/run_tests.lua" \
  -c "q"
```

---

## 📋 Pull Request Workflow

1. **Fork the Repository** and create a feature branch (`git checkout -b feature/amazing-feature`).
2. **Make your changes**:
   - For engine modifications, update Rust files in `engine/src/`.
   - For plugin modifications, update Lua modules in `lua/distract/` and manifests in `lua/distract/manifests/`.
3. **Add or update tests**: Ensure all tests pass (`make test`).
4. **Follow Conventional Commits**:
   - `feat: add dog pet manifest`
   - `fix: resolve window resizing race condition`
   - `docs: update setup configuration in README`
5. **Update [CHANGELOG.md](file:///Users/igmrrf/Desktop/packages/distract.nvim/CHANGELOG.md)** under the `[Unreleased]` section.
6. **Submit a Pull Request** describing your changes and link any relevant issues.

---

## 💬 Code of Conduct

Please review our [Code of Conduct](file:///Users/igmrrf/Desktop/packages/distract.nvim/CODE_OF_CONDUCT.md) before participating in discussions or submitting contributions.
