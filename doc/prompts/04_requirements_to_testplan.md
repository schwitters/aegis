# Prompt 04: Shift-Left Test Plan Derivation

**Target Model:** Frontier Model A (current frontier model)  
**Output Artifacts:** `doc/testplan/TEST-xxx.md`  
**Core Principle:** Test design strictly precedes implementation instructions.

---

## Prompt Template

````text
You are a Lead Verification & Test Architect. We have defined the requirements and use cases.
Before creating implementation instructions for the ≥27B coding model, we must design the complete Shift-Left Test Plan (`doc/testplan/TEST-xxx.md`).

REQUIREMENTS LIST:
<Paste list or content of all doc/requirements/REQ-xxx.md files>

ARCHITECTURE ELEMENTS:
<Paste all doc/architecture/ARCH-xxx.md files (interfaces and integration verification sections)>

USE CASES LIST:
<Paste list or content of all doc/usecases/UC-xxx.md files>

TARGET LANGUAGE & TEST HARNESS:
<e.g. Rust (cargo test), C11 (ctest / custom runner), C++20 (GTest / Catch2), Java (JUnit 5)>

YOUR TASK:
For every requirement (`REQ-xxx`), create at least one executable, falsifiable test plan specification (`TEST-xxx.md`).
For every architecture element (`ARCH-xxx`), create at least one integration test specification (`verifies: [ARCH-xxx]`).

RULES:
1. Every test file must specify what it validates via `verifies: [REQ-xxx]` (requirement tests) or `verifies: [ARCH-xxx]` (integration tests of an architecture element against its neighbors).
2. Every test must specify: Setup, Action, Expected Result, and Edge/Failure Cases (Boundary conditions, overflow, error return paths).
3. Tests must be deterministic (no random flakiness, no hardcoded time sleeps).

OUTPUT FORMAT:

For each test case, output:

File: `doc/testplan/TEST-001.md`
```markdown
---
id: TEST-001
title: <Descriptive Test Title>
verifies: [REQ-001]
type: unit   # unit | integration | performance | safety
---
```

## Objective
<What exact behavior or boundary condition is validated>

## Preconditions & Setup
<Input state, mocked interfaces, or allocated buffers>

## Test Steps
1. Execute call `foo(arg1, arg2)` with valid input.
2. Assert return status is `STATUS_OK`.
3. Assert output state matches expected value.

## Edge & Failure Variations
- **Variation A (Invalid Parameter):** Execute with NULL / out-of-bounds argument; assert `STATUS_INVALID_ARG` is returned.
- **Variation B (Boundary Overflow):** Execute with max capacity; assert buffer overflow is detected without corruption.
````

---

## Next Steps

1. Review the test plan (record it: `aegis review create --stage testplan ...`).
2. Approve the specification baseline: `aegis baseline` (dry run), then `aegis baseline --approve <name>`.
3. Implement the tests with a test agent and freeze them: [`04b_test_implementation.md`](04b_test_implementation.md).
