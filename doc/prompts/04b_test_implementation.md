# Prompt 04b: Test Implementation (Before Production Code)

**Target Model:** Test agent — a Frontier model or a separate agent run. **Never the implementation model**: the model that must pass the tests must not be the one that writes them.  
**Preconditions:** Test plan reviewed, specification baseline approved (`aegis baseline --approve`), public interface contract/stubs defined in production modules.  
**Output Artifacts:** Executable tests in dedicated test files (`@verifies TEST-xxx`), compiling stubs in `src/`, then `doc/testplan/tests.lock`.

---

## Prompt Template

```text
You are a Lead Verification Engineer. Implement the approved test plan as executable, deterministic tests BEFORE
any production code exists. Your tests define the contract the implementation model has to satisfy; they will be
frozen and the implementation model will not be allowed to change them.

TEST PLAN:
<Paste all doc/testplan/TEST-xxx.md files>

REQUIREMENTS & ARCHITECTURE (for interface signatures):
<Paste the referenced REQ-xxx.md and ARCH-xxx.md files, especially public interface contracts>

TARGET LANGUAGE & TEST HARNESS:
<e.g. Rust (cargo test, tests/ directory), C11 (ctest + Unity), C++20 (GTest / Catch2)>

RULES:
1. One test function per test step/variation; every test function carries a comment `@verifies TEST-XXX`.
2. Tests live in DEDICATED test files (Rust: tests/*.rs or `#[cfg(test)] #[path = "x_tests.rs"] mod tests;`),
   never in a file that contains production code — mixed files cannot be frozen.
3. CONTRACT / STUB FREEZE: Prior to freezing tests, create and verify compiling stubs for all public interfaces,
   traits, structs, and headers declared in the architecture (`todo!()` in Rust, empty stub bodies returning error
   in C11/C++20) in the production modules (`src/` or headers), clearly marked `// STUB: replaced by implementation`.
   Stubs are production code: they go into src/, not into test files.
4. COMPILATION CHECK: Tests MUST compile cleanly without errors against these stubs (e.g. `cargo test --no-run` in
   Rust, compilation pass in C11/C++20). Never freeze tests against imaginary or non-compiling signatures.
5. Assert observable behavior and exact error variants, including every edge and failure variation in the plan.
   No assertions that pass for a trivially wrong implementation (e.g. only `is_ok()` where a value is specified).
6. Deterministic: no sleeps, no wall clock, no network, no random seeds without fixing them.
7. Every test must FAIL against the stubs, for the reason the test plan describes (e.g. `not yet implemented`).

OUTPUT:
- The compiling stub declarations in `src/`.
- The test files with full content.
- A table: TEST-ID | test function(s) | file | expected failure against the stubs.
```

---

## Review & Freeze

1. Run the tests against the stubs and confirm every test fails for the documented reason (a test that passes against a stub verifies nothing). `aegis tests freeze` enforces this deterministically for Rust: it refuses unless `cargo test --no-run` compiles and every *newly* frozen test function runs and fails (ignored or passing tests are rejected). Brownfield characterization tests that document existing behaviour use `--characterization` (they must run, may pass); projects without a built-in runner use `--unchecked` after checking by hand — the mode is recorded in `tests.lock`.
2. Blind review of the tests (stage `testplan`): `aegis review create --stage testplan --artifact tests/... --reviewer <model> --author <test agent>`. Focus: does each test falsify its requirement, are edge cases from the plan present, are assertions strong enough?
3. Disposition all findings, then freeze: `aegis tests freeze` (writes `doc/testplan/tests.lock`, commit it).
4. From now on the `test_freeze` gate stage rejects any modification. Changing a frozen test requires a test review and a human re-freeze — never the implementation model.
