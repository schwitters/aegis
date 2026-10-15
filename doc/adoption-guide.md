# Aegis Adoption Guide: Greenfield & Brownfield

This guide provides actionable, step-by-step instructions for adopting the Aegis systems engineering workflow:
1. **Greenfield:** Bootstrapping a brand new project from an initial specification.
2. **Brownfield:** Incrementally adopting Aegis on an existing codebase using the *Strangler Fig* migration pattern without disrupting production delivery.

---

## 🚀 Scenario A: Greenfield Project (From Scratch)

Greenfield systems adopt the full rigor of the V-Model from day one.

```mermaid
flowchart LR
    A["1. Product Ideation"] --> B["2. aegis init"]
    B --> C["3. Derive REQ / ARCH / UC"]
    C --> D["4. Test Plan, Baseline, Frozen Tests"]
    D --> E["5. Agent Instructions"]
    E --> F["6. Implementation Loop, Gate, Review"]
```

### Step-by-Step Procedure

### 1. Ideation & Initial Specification
Discuss the core architecture and scope with **Frontier Model A** (Claude 3.7 Sonnet / GPT-4.5). Store the result in `doc/001_initial_spec.md`.

### 2. Scaffold Repository
Run `aegis init` with your domain profile and systems language:

```bash
# For high-reliability backend / systems (Rust, C++20, Java):
aegis init --profile enterprise --lang rust

# For safety-critical firmware (C11, Embedded Rust, ASIL-D):
aegis init --profile embedded-safety --lang c

# For web applications with a Rust backend (Axum, SQLx; server-rendered, htmx or SPA/PWA frontend; ASVS L2, GDPR):
# also writes clippy.toml and deny.toml to the repository root and doc/rules/workspace-lints.toml
aegis init --profile web-enterprise --lang rust --name myshop
#   -> crates/myshop-{domain,application,storage,api,server}, tests/, migrations/, openapi/ (layer policy enforced
#      by the gate stage `architecture`); an existing Cargo.toml is kept
#   -> docs/{user,operator,technical,hosting} (MkDocs sites), docs/build.sh (HTML + PDF), docs/collect-legal.sh
```

This creates the required directory tree (`doc/requirements/`, `doc/architecture/`, `doc/usecases/`, `doc/testplan/`, `doc/reviews/`, `doc/issues/`, `doc/rules/`, and `doc/deliverables/manifest.md`).

### 3. Requirements Derivation & Blind Parallel Review
- **Frontier Model A** derives initial requirements (`REQ-001..REQ-010`), architecture elements (`ARCH-001..`, each with `satisfies: [REQ-xxx]`), and use cases (`UC-001..`).
- **Frontier Model B** reviews the artifacts **blindly** (without seeing Model A's rationale) against ISO/IEC 25010 axes. Each review is recorded: `aegis review create --stage requirements ...`; architecture needs two independent reviewers.

### 4. Shift-Left Test Plan, Baseline & Frozen Tests
Before writing implementation code:
- Model A writes test plans in `doc/testplan/TEST-xxx.md` (`verifies: [REQ-xxx]`, integration tests `verifies: [ARCH-xxx]`); the plan is reviewed.
- The human approves the specification: `aegis baseline` (dry run) → `aegis baseline --approve <name>`.
- A test agent implements the tests ([Prompt 04b](prompts/04b_test_implementation.md)); they are reviewed, fail against stubs, and are frozen: `aegis tests freeze`.
- `aegis instructions --out AGENT_INSTRUCTIONS.md` combines profile, rulesets, architecture, requirements, test plan and the loop protocol.

### 5. Implementation Loop, Gate & Review
- Configure the agent harness so that the implementation model cannot write `doc/` or frozen test files (see [Prompt 06](prompts/06_27b_implementation.md)).
- The local 27B model implements one requirement at a time (`@implements REQ-XXX`), running the fast gate after every change, and escalates after `agent_loop.max_iterations` attempts.
- Then:

```bash
aegis gate --profile doc/profile.yaml --skip reviews   # full gate on the green working tree
# Final review of the green diff (Prompt 07), recorded with aegis review create --stage implementation ...
aegis gate --profile doc/profile.yaml                  # acceptance: all stages incl. reviews
```

The acceptance gate verifies all stages (Build, Lint, Static Analysis, Security, Test Freeze, Tests, Coverage, Mutation, Traceability incl. baseline, Issues, Deliverables, Reviews) before sprint completion.

---

## 🏛️ Scenario B: Brownfield Project (Legacy Migration)

Legacy systems typically have code and partial tests, but zero formal requirements and no traceability annotations. A "Big Bang" rewrite always fails.

Instead, Aegis uses **Reverse Spec Extraction** combined with the **Strangler Fig Pattern**:

```mermaid
flowchart TD
    subgraph Phase1 ["Phase 1: Baseline Extraction"]
        Code["Existing Codebase"] --> RevSpec["Frontier Model: Reverse Engineering"]
        RevSpec --> InitReq["doc/requirements/ REQ-001.. (Status Quo)"]
        InitReq --> MapTest["Map Existing Tests"]
    end

    subgraph Phase2 ["Phase 2: Traceability X-Ray"]
        MapTest --> Trace["aegis trace"]
        Trace --> DarkZones["Expose Untested / Dark Logic"]
    end

    subgraph Phase3 ["Phase 3: Incremental Evolution"]
        Change["Every New Bug / Feature"] --> Runbook["Change Runbook (100% Aegis)"]
        Runbook --> Ratchet["Gate Ratcheting (Gradual Strictness)"]
    end
```

### Phase 1: Initialize Brownfield Scaffolding

Run `aegis init --brownfield` inside the existing repository:

```bash
# Initialize brownfield migration scaffolding
aegis init --brownfield --lang rust --profile enterprise
```

This generates `doc/REVERSE_SPEC_EXTRACTION_PROMPT.md` and baseline requirement, use-case and architecture templates.

### Phase 2: Reverse Spec Extraction (Capture Status Quo)

Provide your Frontier model with `doc/REVERSE_SPEC_EXTRACTION_PROMPT.md` and your core modules (e.g. data structures, public API interfaces, controllers):

> **Frontier Prompt:** *"Analyze this existing codebase. Extract the 10-20 primary functional and non-functional requirements that describe what the system currently does. Formulate them as atomic `REQ-001..` documents following ISO/IEC 25010."*

Save the output in `doc/requirements/REQ-001.md`, `REQ-002.md`, etc.

### Phase 3: Traceability X-Ray

1. Tag entry point functions/structs with `@implements REQ-XXX`.
2. Tag existing unit/integration tests with `@verifies TEST-XXX`.
3. Run `aegis trace`:

```bash
aegis trace --out doc/traceability.md
```

`aegis trace` immediately provides an **architectural X-ray**:
- Identifies untracked requirements (`GAP`).
- Exposes orphaned or undocumented code paths (`ORPHAN`).
- Highlights critical business logic lacking automated verification.

### Phase 4: The Strangler Fig Rule (100% Aegis for All New Work)

Freeze legacy debt. Enforce one non-negotiable rule: **every new bug fix, refactor, or feature must go through the Change Runbook**:

1. Log issue: `aegis issue create --title "..." --type ... --severity ... --related REQ-xxx`
2. Update/create `REQ-xxx` in `doc/requirements/` and its `ARCH-xxx` allocation; review; `aegis baseline --approve`.
3. Add `TEST-xxx` in `doc/testplan/`, implement and freeze the test (test strictly precedes code).
4. Implement code with `@implements REQ-xxx` in the bounded implementation loop.
5. Verify with `aegis gate`, record the final review, and close the issue with `aegis issue close`.

*Result:* With every commit and sprint, your legacy codebase incrementally converts to 100% auditable, type-safe, and verified systems architecture.

### Phase 5: Gate Ratcheting

Gradually tighten quality gate constraints:
- **Level 1:** Build + Unit Tests + Traceability Matrix + Test Freeze for new tests.
- **Level 2:** Strict Linters (`clippy`, `clang-tidy`, `checkstyle`) + Security Audit (`cargo-deny` / `cargo-audit`) + approved baseline (`traceability.baseline: required`) + review records.
- **Level 3:** Enforce coverage ratcheting (50% → 70% → 85% branch coverage) and mutation testing on changed code (`cargo mutants --in-diff` via `gate.mutation.command`, then a global `threshold`).
