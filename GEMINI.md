# Repository Coding Standards

- **Feature Locked:** No adding features outside the scope of documented features. Only improvements (performance, memory, bug fixes, reliability, testing) are permitted.
- **Zero Explanatory Comments:** Write self-documenting code.
- **Fail Fast & Explicitly:** Never swallow errors or use empty catches.
- **Strict Size Caps:** File <= 400 lines, Component/Struct <= 150 lines, Function <= 60 lines.
- **Refactoring:** Zero behavior/contract changes without characterization tests.

# Universal Coding Rules & Engineering Standards

- **Feature lock is absolute.** Core features are sealed to the documented contracts. New behaviors belong in external plugins.
- **No explanatory comments.** Write code that reads on its own.
- **No backwards-compatibility shims in application logic.** Migrations, versioned schemas, and routing layers handle compatibility—not runtime `if`-branches.
- **Fail fast and explicitly.** Never silently swallow errors or fall back to ambiguous default values.

---

## 0. How to Read These Standards

- **This document is language-agnostic and applies to every project.** Language guides (`rust/`, `go/`, `typescript/`, `python/`, `lua/`) add idioms, toolchains, and carve-outs. They never relax a universal rule; where a language cannot honour one, the language guide states the exception explicitly.
- **Rule scope tags.** Rules apply everywhere unless tagged:
  - `[service]` — applies only to network-facing services (HTTP, gRPC, queue consumers, RPC).
  - `[app]` — applies only to deployed applications, not to reusable libraries.
  - `[lib]` — applies only to libraries and SDKs consumed by third parties.
- **Non-negotiable vs. tunable.** Rules about correctness, safety, and failure handling are non-negotiable. Numeric thresholds (size caps, nesting depth, parameter counts) and named tools are project-configurable defaults: a project may raise or lower them once, in writing, repository-wide — never per file, never ad hoc.
- **Tool names are defaults, not requirements.** Where a specific formatter, linter, or library is named, the requirement is *"exactly one, configured repository-wide, enforced in CI"*. The named tool is the default choice when the project has no existing one.
- **Adopting into an existing repository.** Apply to changed files first (see the lint ratchet in §1). A standard that would require a repo-wide rewrite to land a one-line fix is being applied wrongly.

---

## 1. Automated Formatting & Linting

- **The automated formatter is the sole authority.** Never format code by hand or manually organize imports. Run the repository's configured formatter before committing.
- **Configuration is repository-wide.** Formatter and linter configurations are uniform across the project and are never overridden on a per-file basis.
- **Lint is a ratchet, not an ad-hoc cleanup task.** Whole-repo errors are a hard CI gate (0 allowed). Warnings on untouched legacy files may exist temporarily, but any modified file must be 100% clean of both errors and warnings.
- **Never enforce an unratcheted zero-warning gate to force unrelated cleanup.** Blanket cleanups block focused PRs. The changed-files ratchet prevents new technical debt.
- **Autofix only what is mechanically safe.** Scrutinize automated autofixes; never apply unsafe autofixes that alter types, contracts, or runtime behavior.
- **Delete dead code manually.** Silencing a warning by renaming an unused variable to an ignored prefix retains dead code. Delete unused bindings completely.

---

## 2. Naming & Ubiquitous Language

Names are the primary documentation. Precise naming eliminates the need for inline comments.

- **Say what the thing is.** Avoid cryptic abbreviations that a newcomer would not immediately understand (`countryCode` not `cc`, `beneficiary` not `bnf`).
- **No single-letter names.** Variables, parameters, callback arguments, loop variables, catch bindings, and generic type parameters must have descriptive names (`record` not `r`, `error` not `e`, `index` not `i`).
  - This is a deliberate deviation from the terse-identifier habits of several languages. Each language guide lists the **only** permitted exceptions — conventional idioms whose meaning is universal and which linters expect (`ctx`, `t *testing.T`, Go method receivers, mathematical coordinates in a formula). Anything not on that list is a violation.
- **Standard casing follows the language's own convention, not a cross-language one.** Use the casing the language's formatter and standard library use; do not import another language's style:
  - Values, variables, functions: `camelCase` or `snake_case` per language standard.
  - Types, structs, interfaces, classes, traits, enums: `PascalCase` (or the language's equivalent).
  - Module-level constants and static globals: `SCREAMING_SNAKE_CASE` where the language has the concept.
- **Booleans read as predicates:** `isActive`, `hasPermission`, `canProceed`, `shouldRetry`. Never a bare noun (`active`) or an inverted negative (`isNotReady`).
- **Functions are verbs:** `buildTransactionRequest`, `resolveVariant`, `fetchBeneficiaryDetails`. Functions named for a noun must be values or getters.
- **No type noise in names:** `userList` not `userArray`, `accountMap` not `accountHashMap`, `Account` not `IAccount` or `AccountInterface`.
- **One concept, one word repository-wide.** Pick `beneficiary` or `recipient`, `sender` or `originator`, and never alternate. Synonyms for the same domain entity create ambiguity.
- **Names reflect current responsibility.** When a refactor alters a module's behavior, update its name immediately.

---

## 3. Comments & Documentation

- **No explanatory comments in implementation bodies.** Code structure, variable naming, and function decomposition must convey intent.
- **Document the "why", never the "what".** A comment is permitted only when explaining an external constraint, hardware limitation, or unintuitive business quirk that cannot be expressed in code.
- **No commented-out code.** Version control preserves history. Delete unused code completely.
- **No changelog or attribution comments.** Version control metadata tracks authorship and dates.
- **No section-divider banners.** Banners (`// === SECTION ===`) indicate that a file is too large and should be decomposed.
- **TODO/FIXME requirements:** Must include an owner and a concrete condition for resolution. Anonymous TODOs are technical debt and must not ship.
- **Public API contracts:** Public libraries, shared modules, and exported interfaces must document their contracts, error conditions, and invariants.

---

## 4. Functions & Control Flow

- **Single responsibility:** One task per function. A function that fetches, validates, transforms, and persists represents four distinct functions.
- **Guard clauses and early exits:** Handle validation, errors, and base cases at the beginning of the function, allowing the happy path to remain unindented.
- **Nesting depth ceiling:** Maximum 3 levels of indentation. Deeper nesting requires decomposing logic into helper functions.
- **Parameter limit:** Maximum 3 positional parameters. If more parameters are required, pass a typed configuration or request object.
- **No boolean behavior flags:** Avoid functions with flag parameters that alter control flow (`processOrder(order, true)`). Provide separate named functions or an explicit enum/options object.
- **No mutating caller-owned arguments:** Functions must not mutate inputs passed by the caller unless explicitly designed as an in-place buffer with a clear contract. Return new transformed values.
- **Single level of abstraction:** A function either coordinates high-level workflow steps or performs low-level operations—never both.

---

## 5. Size Caps & Modular Decomposition

Soft caps enforced to ensure readability and maintainability:

| Unit | Cap |
|---|---|
| File / Module | 400 lines |
| Type / Struct / Component Body | 150 lines |
| Function / Method | 60 lines |

Crossing a cap is a signal to decompose, not an automatic failure — see §0 on tunable thresholds.

**Exempt** (never counted, never hand-edited): generated code, vendored code, lockfiles, database migrations, and pure static data tables (ISO codes, mapping tables) placed in dedicated data files.

**Never satisfy a cap by making the code worse.** Splitting a cohesive unit into fragments that must be read together to be understood trades one readability problem for a harder one. If decomposition has no natural seam, record the justification and keep the unit whole.

---

## 6. Types & Data Invariants

- **Derive types from a single source of truth.** Schemas, database definitions, or protobuf specifications define the contract; do not maintain parallel manually written types that can drift.
- **Make invalid states unrepresentable.** Use discriminated unions, tagged enums, and the typestate pattern to model valid state transitions instead of structs with optional fields.
- **Zero untyped escapes in production code.** Never use untyped dynamic escapes (e.g. `any`, raw unchecked pointers) in production paths. Narrow untyped inputs at the system boundary.
- **Exhaustive matching:** Match all variants of domain enums explicitly without fallthrough catch-alls so that new variants trigger compile-time validation.
- **One declaration per concept:** Define and export domain entities from their owning module. Do not duplicate identical type shapes across subsystems.

---

## 7. State, Purity & Side Effects

- **Pure core, impure edges:** Business logic should be pure functions (deterministic output given the same input, zero I/O, zero clock dependency). Confine side effects (network, storage, timers, logging) to the application boundary.
- **Immutable updates:** Prefer immutable data structures and transformations over in-place state mutation.
- **Deterministic lifecycle management:** Any spawned background worker, timer, subscription, file handle, or connection pool must have an explicit teardown mechanism to prevent resource leaks. Acquire and release in the same scope using the language's scoped-release construct (`defer`, `with`, RAII/`Drop`, `try`-with-resources).
- **Derive state; do not duplicate:** If two values must remain synchronized, store one canonical value and compute the second on demand.
- **Inject time, randomness, and identity.** Clock reads, random number generation, and ID/UUID creation are I/O. Pass them in at the boundary so business logic stays deterministic and testable.
- **Bounded queues and explicit backpressure:** Every buffer, work queue, and channel between producers and consumers has a documented capacity limit. Unbounded buffering converts a slow consumer into an out-of-memory crash.
- **Cancellation propagates:** Long-running and I/O-bound operations accept and honour a cancellation or deadline signal from their caller, and pass it down to everything they call.
- **No shared mutable state without a documented discipline:** Concurrent access is guarded by ownership, immutability, or an explicit lock whose scope is as small as possible. Locks are never held across an I/O or `await` boundary unless the invariant demands it and the code says why.

---

## 8. Module Boundaries, Layering & Folder Architecture

- **One-directional dependency flow:** Dependencies point inward. Delivery and infrastructure code may depend on the domain; the domain must never import from delivery, infrastructure, or outer feature directories.

  ```
  Delivery (HTTP / CLI / UI / consumer)  ──┐
                                           ├──▶  Application (use cases)  ──▶  Domain (entities, invariants)
  Infrastructure (DB / network adapters) ──┘                                            ▲
                                                                                        │
                              infrastructure implements interfaces owned by ────────────┘
  ```

  The layer *names* are a template, not a mandate — a library or CLI may collapse them. The **direction** is the rule: the pure core never imports the impure edge.
- **Feature-driven vertical slices ("Screaming Architecture"):** Organize folders by business domain features (e.g. `billing/`, `transfers/`) rather than pure technical groupings (`controllers/`, `views/`, `models/`). Projects with a single cohesive domain (most libraries and CLIs) skip this layer rather than inventing artificial features.
- **"Delete with one keystroke" cohesion:** A feature folder must be self-contained so that deleting it cleanly removes all its UI, state, queries, types, and tests without leaving orphaned files.
- **No generic junk drawers:** Banish catch-all `utils/` or `common/` directories. Name utility modules by concrete responsibility (`date/`, `crypto/`, `formatting/`).
- **Shallow hierarchy ceiling:** Keep directory nesting shallow (maximum 3 to 4 levels). Over-nested hierarchies impede discovery and refactoring.
- **Colocation beats premature abstraction:** Keep a helper, hook, or sub-component colocated within the single feature that uses it. Promote to global shared modules only when a second genuine consumer exists.
- **No circular dependencies:** Circular dependencies indicate improper separation of concerns. Break cycles by introducing a shared interface boundary or consolidating colocated logic.
- **Explicit exports:** Expose only the minimal necessary public API from a module. Keep internal implementation helpers private.

---

## 9. Constants & Magic Values

- **No magic literals in business logic.** Extract numeric thresholds, timeout durations, and string constants into named identifiers (`MAX_RETRY_ATTEMPTS`, `SESSION_TIMEOUT_SECONDS`).
- **Shared constants:** Strings or codes used across multiple modules (status strings, header keys, event names) must be exported from a single definition.
- **Units in identifier names:** Always include measurement units in variable names (`timeoutMs`, `intervalSeconds`, `fileSizeBytes`, `amountMinor`).

---

## 10. Errors & Failure Handling

- **Never swallow an error.** A catch or error-handling block must handle the error meaningfully, enrich and rethrow it, or log and convert it into a typed failure object.
- **Fail fast and loud at boundaries:** Validate inputs and preconditions immediately upon entry.
- **Typed errors over prose:** Error handling must be based on typed error structures, error codes, or domain enums—never by parsing error message strings.
- **Distinguish empty data from failure:** A missing resource (`None`, `NotFound`) is distinct from a network/database execution failure. Do not conflate the two.

---

## 11. Logging & Observability

- **Use structured logging:** Log with structured key-value pairs rather than unstructured string concatenation.
- **Semantic log levels:**
  - `ERROR`: System failure requiring operational intervention.
  - `WARN`: Recovered or degraded state that warrants investigation.
  - `INFO`: Significant lifecycle milestone (service started, batch completed).
  - `DEBUG`: Verbose diagnostic data for local troubleshooting (disabled in production).
- **Never log sensitive data:** Passwords, API tokens, cryptographic keys, full payment identifiers, and PII must never appear in logs.
- **Log at the decision point once:** Do not log an error at every layer of the call stack. Return the error upwards and log it once at the entry boundary.

---

## 12. Duplication, Abstraction & Reuse

- **Reuse before inventing:** Check existing shared libraries and domain modules before creating new utilities.
- **No single-use abstractions:** Do not build complex generic factories or speculative frameworks for a single call site.
- **Deletion beats abstraction:** Before unifying duplicate modules, check if the duplicate copies are still actively reachable. Delete obsolete code before refactoring.
- **Three occurrences rule:** Two similar implementations are a coincidence; three establish a pattern. Wait for the third concrete use case before extracting a shared abstraction.

---

## 13. Dead Code & Reachability

- **Verify reachability before modifying code:** Confirm that an endpoint, function, or component has active callers before writing tests or refactoring.
- **Resolve references via AST/imports:** Verify symbol usage through compiler bindings and import resolution, not simple substring text search.
- **Clean up associated artifacts:** When deleting an obsolete module, delete its tests, route registrations, configuration keys, and dedicated dependencies in the same change.
- **Verify external integrations:** External webhooks, public APIs, and background queue workers may have zero in-repo callers. Verify external consumers before removing integration points.

---

## 14. Boundaries & Service Contracts `[service]`

Every request handler follows the same execution sequence. Steps that do not apply to a given transport (a queue consumer has no rate limiter) are omitted deliberately, not forgotten.

1. **Authentication & Authorization:** Verify identity and permissions.
2. **Rate Limiting & Throttling:** Protect write and sensitive endpoints.
3. **Input Validation:** Parse and validate the incoming request schema.
4. **Domain Execution:** Invoke business use cases and infrastructure ports.
5. **Standardized Response:** Return a typed success or error result.

- **Idempotency for non-idempotent operations:** Any handler that creates or moves state and can be retried by a client, proxy, or queue must be idempotent — via an idempotency key, a natural unique constraint, or a conditional write.

---

## 15. Validation at System Boundaries

- **Strict schema validation:** All untrusted external input (request bodies, query strings, headers, message queue payloads, file uploads, environment variables, CLI arguments, deserialized cache entries) must be parsed and validated into a typed value at the boundary. Interior code receives validated types only, never raw input.
- **Parse, don't validate:** Validation returns a narrowed type. A function that returns `bool` and leaves the caller holding the raw value invites re-validation drift.
- **Centralized validation error formatting:** Format validation failures through a consistent error structure.
- **Reject unexpected fields:** Enforce strict payload parsing to prevent parameter injection and unintended field assignment.
- **Bound every input:** Enforce maximum body size, collection length, string length, and numeric range. Unbounded input is a denial-of-service vector.

---

## 16. Response Envelopes & Error Contracts `[service]`

- **One response contract per project, applied everywhere.** Whatever shape is chosen, every endpoint uses it and every client can rely on it. The default for a JSON/HTTP API with no existing convention:
  - Success: `{ success: true, data: ... }`
  - Failure: `{ success: false, error: { code: ..., message: ... } }`

  Projects bound to an existing contract — RFC 9457 Problem Details, gRPC status codes, GraphQL `errors`, JSON:API — use that contract instead. Do not wrap a standard envelope inside a second custom one.
- **Stable machine-readable error codes:** Clients branch on `code`, never on the human-readable message. Message text may change without a breaking-change bump; codes may not.
- **Never leak internals:** Stack traces, internal hostnames and IPs, SQL, and schema details must be omitted from responses in production. Correlate instead: return an opaque request ID that appears in the server-side log.

---

## 17. Configuration & Secrets Management

- **Validated configuration at startup:** Parse and validate all required environment variables and configuration files at application initialization. Fail startup immediately if configuration is invalid.
- **Zero raw inline environment reads:** Centralize environment access in dedicated, validated configuration modules.
- **Strict secrets isolation:** Secrets must never be hardcoded in source files, committed in fixtures, or stored in version control.

---

## 18. Security Baseline

- **Server-side authorization:** Always enforce access control on the server against verified session identity, never trusting client claims.
- **Parameterized queries:** Prevent injection attacks by parameterizing all database queries, command executions, and template interpolations.
- **Fail closed:** Security gates and signature verifications must default to denying access on unexpected errors.
- **Synthetic test data:** Use synthetic, anonymized data for test suites and snapshots. Never commit real customer data.

---

## 19. Refactoring Protocol: Parity is Absolute

Refactoring is strictly structural. A refactor must introduce **zero behavior, layout, or contract changes**.

1. **Characterization Tests First:** Write characterization tests against the existing implementation to pin current behavior before modifying code.
2. **Incremental Extraction:** Refactor in small, verified steps.
3. **Preserve Interface Contracts:** Keep public interfaces and serialization formats identical.
4. **Preserve Branch Order:** Maintain condition evaluation order when branches are not mutually exclusive.
5. **Separate Bug Fixes:** If a bug is uncovered during refactoring, document and pin it with a test first; fix the bug in a separate, dedicated change.

---

## 20. Testing Standards

- **Colocated or standard test suites:** Place unit tests where the language's ecosystem expects them — colocated with the source or in the conventional test directory. Follow the language guide; do not invent a third location.
- **Behavior-driven assertions:** Assert against observable behavior and public contracts rather than internal private state. A test that breaks on a pure refactor was testing the implementation.
- **Deterministic and isolated:** Tests must be deterministic, order-independent, parallel-safe, and free of external network, real wall-clock, and unseeded randomness. Each test creates the state it needs and cleans up after itself.
- **Test at the boundary that can break:** Cover business rules and edge cases at the unit level, and cover each system boundary (serialization, persistence, transport) with at least one integration test that exercises the real thing.
- **Every bug fix ships with a regression test** that fails before the fix and passes after.
- **Coverage is a diagnostic, not a target.** Use it to find untested branches; never write assertions solely to move the number.
- **Continuous Integration gate:** The test suite must pass 100% green before any change can be merged. Skipped, quarantined, or flaky-retried tests are tracked with an owner and a removal date, never left silently disabled.

---

## 21. Dependency Hygiene

- **Minimal external dependencies:** Favor standard library capabilities and existing dependencies before adding new third-party packages.
- **Audit and security checks:** Every dependency must pass license compliance and vulnerability auditing in CI.
- **Committed lockfile, reproducible builds:** Applications commit an exact lockfile and CI installs from it without resolving. Libraries declare permissive version ranges and test against the lowest supported version.
- **Remove orphaned dependencies:** When removing a feature, remove its unique dependencies immediately.

---

## 21b. Public API Stability `[lib]`

- **The public surface is the smallest thing that works.** Anything exported is a contract; anything not exported can change freely. Default new items to private.
- **Semantic versioning is binding:** Removing or narrowing a public item, adding a required parameter, or changing a serialization format is a major version. Additive changes are minor.
- **Deprecate before deleting:** Mark the old item deprecated with the replacement named in the message, keep it working for at least one minor release, and remove it in the next major.
- **Document contracts, error conditions, and invariants** for every public item (see §3).

---

## 22. Verification Checklist Before Completion

Before declaring any task complete or submitting a pull request, verify:

1. **Formatting:** Automated formatter ran cleanly with zero modifications.
2. **Linting:** Zero linter errors and zero linter warnings on modified files.
3. **Type Checking:** Strict type checker runs with zero errors.
4. **Test Suite:** 100% of unit and integration tests pass.
5. **Size Validation:** No file or function exceeds repository size caps without documented justification.

---

## 23. Working Protocol

- **One logical change per commit:** Commits must be focused and reversible.
- **Accurate commit descriptions:** Commit messages must describe the motivation and scope of the change.
- **Language-specific standards:** For language-specific idioms, toolchains, and configurations, consult:
  - [**Rust Standards**](rust/CODING.md)
  - [**Go Standards**](go/CODING.md)
  - [**TypeScript Standards**](typescript/CODING.md)
  - [**Python Standards**](python/CODING.md)
  - [**Lua Standards**](lua/CODING.md)

# Lua Coding Rules & Standards

Applies to **every** Lua project — standalone application, embedded scripting layer, Neovim or game plugin, OpenResty service, or library. Lua is almost always embedded in a host, so **the host's conventions and API win where they conflict with this document**; deviations are stated, not silently taken.

Read [`../CODING.md`](../CODING.md) first — it is the universal baseline. This document adds Lua specifics and never relaxes it.

- **No explanatory comments.** Write code that reads on its own.
- **Fail fast and explicitly.** Return `nil, err` for expected failures; `error()` for broken invariants.
- **`local` by default, always.** Every variable and function is `local` unless the host genuinely requires a global. An accidental global is a process-wide, cross-module data race.

---

## 0. Target Runtime

**State the target runtime and version in the project README, and hold to one.** Lua dialects are not interchangeable and a rule that is correct on one is a syntax error on another:

| Runtime | Notes that change the rules below |
|---|---|
| **Lua 5.1 / LuaJIT** | No integer subtype, no `goto` in 5.1, no `<close>`/`<const>`, `unpack` not `table.unpack`, `setfenv`-era environments. LuaJIT adds FFI and its own performance rules. |
| **Lua 5.2–5.3** | `goto`, `_ENV`, integer subtype in 5.3, `table.unpack`, bitwise operators in 5.3. |
| **Lua 5.4** | `<const>` and `<close>` attributes, integer division `//`, generational GC. Prefer these where available. |

- **Do not use a feature the target runtime lacks**, and do not write compatibility shims for runtimes the project does not support. If the project must span versions, isolate the differences in one compatibility module.

---

## 1. Formatting & Toolchain

- **`stylua` is the single formatting authority.** Configuration lives in `stylua.toml` at the repository root and is never overridden per file. Never hand-format.
- **`luacheck` is a mandatory lint gate**, configured in `.luacheckrc` with the target `std` (`lua51`, `luajit`, `lua54`, `ngx_lua`, or a host-specific list such as Neovim's `vim` global) and an explicit `globals` / `read_globals` allowlist. `selene` is an acceptable alternative; pick one.
- **Undefined and unused globals are errors, not warnings.** This is the single highest-value check in a language with implicit global assignment.
- **Type annotations are required on public functions.** Lua has no static type system, so annotations plus a checker (`lua-language-server` in strict mode, or `Teal` for a genuinely typed dialect) are the only compile-time safety net available. Run the checker in CI.
- **Lint is a ratchet:** 0 errors repo-wide; any modified file must be clean of warnings too.

---

## 2. Naming & Conventions

- **Casing:**
  - `snake_case`: local variables, functions, methods, and module fields. Default choice; a project embedded in a host that uses `camelCase` follows the host instead — consistently, repo-wide.
  - `PascalCase`: class-like tables and constructors.
  - `SCREAMING_SNAKE_CASE`: module-level constants.
  - `snake_case.lua`: file names, matching the module name used in `require`.
- **A single leading underscore marks module-private.** Lua cannot enforce privacy; the convention plus not exporting the field is the enforcement.
- **No single-letter names.** Use `index` not `i`, `key`/`value` not `k`/`v`, `error_message` not `e`.
  - **Only permitted exception:** `self`.
- **Predicates read as questions:** `is_valid`, `has_permission`, `can_retry`, `should_refresh`.
- **Functions are verbs:** `build_request`, `resolve_path`, `fetch_config`.
- **One concept, one word repo-wide.**

---

## 3. Comments & Documentation

- **No explanatory comments in function bodies.**
- **Annotate every public function** with EmmyLua/LuaCATS annotations — they are documentation *and* the input to the language server's type checking:

  ```lua
  ---@class Account
  ---@field id string
  ---@field balance_minor integer

  ---Fetches an account by identifier.
  ---@param account_id string
  ---@return Account|nil account
  ---@return string|nil error_message
  local function fetch_account(account_id) end
  ```

- **Annotate the error return**, not just the success one. An unannotated `nil, err` contract is invisible to callers and to the checker.
- **No commented-out code, no changelog comments.**

---

## 4. Functions & Control Flow

- **Guard clauses and early returns.** Validate at the top; keep the happy path unindented.
- **Maximum nesting depth: 3 levels.**
- **Parameter limit: 3 positional parameters.** Beyond that, take a single options table — which also removes the positional-boolean-flag problem:

  ```lua
  local function create_transfer(opts)
    -- opts.source_account_id, opts.amount_minor, opts.idempotency_key
  end
  ```

- **Validate options-table fields explicitly.** A typo in a caller's key is silently `nil`; check required fields and fail loudly.
- **`ipairs` for sequences, `pairs` for maps.** `pairs` iteration order is undefined and varies between runs — never rely on it for output ordering or serialization.
- **Tables are 1-indexed**, and a `nil` in the middle of a sequence makes `#` undefined. Never store `nil` as a meaningful array element; use a sentinel or a separate presence field.
- **Declare `local` before use, and cache hot lookups.** Locals are register slots; globals and nested table fields are hash lookups per access. In a loop, hoist `local insert = table.insert`.
- **Build strings with `table.concat`, never repeated `..` in a loop** — repeated concatenation is quadratic and produces garbage on every iteration.
- **Multiple returns are a contract, not a convenience.** `select("#", ...)` to count varargs correctly; storing `...` in a table loses embedded `nil`s unless you use `table.pack`.

---

## 5. Size Caps

Soft caps (see §0 of the universal standards on tunable thresholds):

| Unit | Cap |
|---|---|
| Module | 400 lines |
| Class-like table (method set) | 150 lines |
| Function | 60 lines |

Lua 5.1/LuaJIT also caps 200 locals and 60 upvalues per function — hitting either is a hard signal the function should have been several.

---

## 6. Modules & State

- **A module builds one `local` table and returns it.** No globals, no `module()` (removed in 5.2), no side effects at require time — `require` caches, so an import that opens a socket or reads a file runs once, unpredictably, in whatever order the host loads modules.

  ```lua
  local M = {}

  function M.parse(input) end

  return M
  ```

- **Pass state explicitly.** Module-level mutable state is process-global and survives across every consumer; a factory function returning an instance is almost always the right shape.
- **Declare the public surface deliberately.** Everything not on the returned table is private — keep helpers as file-locals rather than exporting and hoping.
- **Metatables are for behavior, not cleverness.** `__index` for method lookup and defaults is idiomatic; `__index`/`__newindex` chains that make a table pretend to be something else make debugging impossible.
- **Set `__name` and a `__tostring` metamethod** on class-like tables so errors and logs identify the object.
- **Never modify tables you do not own** — not the standard library, not the host's globals, not a table received as an argument. Monkey-patching a shared runtime is invisible to every other consumer in the process.

---

## 7. Error Handling

- **Expected failures return `nil, error_message` (or `false, err`).** Callers check the first return; never make them parse a message string to decide what happened. Return a table with a `code` field when callers need to branch.
- **`error()` for broken invariants and programmer errors** — a contract violation, not a runtime condition the caller could reasonably handle. Pass a table for structured errors; pass a level argument so the reported position points at the caller.
- **`assert()` is for invariants, and it is not free.** `assert(fetch())` collapses a `nil, err` contract into a thrown string and loses the error's structure. Handle the `nil` explicitly.
- **`pcall` / `xpcall` only at boundaries** — the host callback, the request handler, the plugin entry point. Wrapping every call in `pcall` reproduces `except: pass`.
- **`xpcall` with a traceback handler** when the traceback matters; plain `pcall` discards it.
- **Never swallow.** A `pcall` whose error result is discarded is a bug; log it or return it.
- **Clean up deterministically.** Lua's GC is not a resource manager: close files, sockets, and handles explicitly on every path, including the error path. On 5.4, `local handle <close> = ...` does this correctly.

---

## 8. Logging & Observability

- **Log through the host's logging facility** (`vim.notify`, `ngx.log`, the game engine's logger, or one project-local logger module) — never `print` outside a CLI's actual stdout output.
- **Structured fields over concatenated strings**, where the host supports it.
- **Never log secrets or PII.**
- **Log once, at the boundary** — return errors upward and log them where they are handled.

---

## 9. Project Layout

```
my-project/
├── .luacheckrc              # std, globals allowlist, lint config
├── stylua.toml
├── my-project-dev-1.rockspec  # [lib] LuaRocks packaging
├── lua/                     # or src/ — one root, matching the host's require path
│   └── my_project/
│       ├── init.lua         # Public API surface only
│       ├── config.lua       # Validated settings, parsed once
│       ├── errors.lua       # Shared error constructors / codes
│       └── <domain>/        # Cohesive units
└── spec/                    # or tests/ — mirrors the source tree
```

- **`init.lua` re-exports; it does not implement.** `require("my_project")` should be cheap and side-effect free.
- **The directory layout *is* the `require` path.** Renaming a file renames a public identifier — treat it as an API change.
- **No `utils.lua` junk drawer.** Name modules by responsibility: `string_util` becomes `formatting`, `helpers` becomes `retry`.
- **Isolate host API calls behind one adapter module** so the domain logic stays testable without the host running.

---

## 10. Testing

- **`busted` is the default choice** (`luassert`, `luacov` for coverage); `luaunit` or the host's own harness are acceptable where the embedding requires it. One runner per project.
- **Specs mirror the source tree** under `spec/`, named `*_spec.lua`.
- **Deterministic:** inject the clock and the random source, seed anything stochastic, and stub host and network calls. No `os.time()` or `math.random()` read directly in logic under test.
- **Test the public module table**, not file-locals. A test that needs a private function is telling you the module has two responsibilities.
- **Assert on the full `nil, err` contract**, both returns, not just truthiness.
- **Every bug fix ships with a regression test.**
- **CI gate:** green on every supported runtime version the project claims (§0).

---

## 11. Verification Commands

```bash
# 1. Format
stylua .

# 2. Lint (undefined/unused globals are errors)
luacheck .

# 3. Type/annotation check
lua-language-server --check . --checklevel=Error

# 4. Test suite
busted --coverage
```


