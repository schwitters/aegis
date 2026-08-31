# Aegis: Traceability- & Process-Toolkit

A modular systems engineering toolkit and process methodology for agent-based software development.
It enforces end-to-end **Requirement → Usecase → Test → Code** traceability, controls rigor via domain profiles, binds 3-tier coding standards, and runs automated, fail-closed quality gates.

## Components

| File / Directory | Purpose |
|---|---|
| [`tools/`](../tools/) (`aegis`) | Precompiled, standalone Rust toolkit. Unifies **Traceability verification**, **Impact analysis**, **Issue lifecycle management**, **Profile inspection**, and the **Quality Gate** without external runtime dependencies. |
| [`tools/gate.sh`](../tools/gate.sh) | Profile-aware, fail-closed verification gate wrapper (delegates to `aegis gate`). Reads `doc/<profile>.yaml`, enforcing stages and coverage thresholds. |
| [Profiles](embedded-safety.yaml) | One process, N profiles: `embedded-safety.yaml` (ISO 26262, ASIL-D) and `enterprise.yaml` configure domain rigor. |
| [`doc/rules/`](rules/README.md) | Reusable, strict **Coding Rulesets** (`c11-strict.yaml`, `c11-embedded-safety.yaml`, `cpp20-core.yaml`, `cpp-strict.yaml`, `rust-safety.yaml`, `java-enterprise.yaml`, `java-google-style.yaml`). |
| [`doc/prompts/`](prompts/README.md) | Standardized **Prompt Playbook** (Stages 01 - 07: Ideation, Blind Review, Derivation, Shift-Left Tests, 27B Synthesis, Implementation, Final Audit). |
| [`doc/adoption-guide.md`](adoption-guide.md) | Practical adoption guide for Greenfield (from scratch) and Brownfield (legacy migration) projects. |
| [`doc/change-runbook.md`](change-runbook.md) | Structured change management — blast radius analysis, impact depth classification, and step-by-step confirmation. |
| [`doc/dev-workflow.md`](dev-workflow.md) | Full specification of the Spec-Driven Multi-Model Development Workflow. |
| [`doc/process-evaluation.md`](process-evaluation.md) | Architectural and process assessment including safety and production considerations. |

---

## 1. Strict 3-Tier Coding Rulesets (`doc/rules/`)

Coding standards (C11, C++, Rust, Java) are decoupled from domain profiles and operate across three tiers:
1. **Agent Instructions:** Strict prompt constraints given to the 27B implementation model.
2. **Review Rubric:** Explicit audit checklist for Frontier models during blind parallel reviews (`RULE-XXX-YYY`).
3. **CI Gate:** Statically mapped to compiler, linter, and SAST checks (`clang-tidy`, `clippy`, `checkstyle`, `scan-build`).

See [`doc/rules/README.md`](rules/README.md) for details.

---

## 2. Bridging Code and Specifications (Traceability)

Source code links directly to requirements via comment annotations:

```python
def verify_password(password, hashed):
    """Verifies password against bcrypt hash.

    @implements REQ-002
    """
    ...
```

Tests annotate `@verifies TEST-XXX`. The annotation lives directly alongside the code, surviving refactorings and serving as an unambiguous instruction for the implementation model.

### Automated Checks

`aegis trace` detects two structural defect classes that single reviews miss:

- **Gap (`GAP`)** — A requirement lacking an associated use case, test, or `@implements` annotation.
- **Orphan (`ORPHAN`)** — Code or test referencing a non-existent requirement ID (`REQ-999`).

Both conditions result in exit code 1, immediately halting the pipeline.

### Document Schema

Requirements, use cases, and tests declare their metadata inside ` ```yaml ` blocks:

```yaml
id: REQ-001
```
```yaml
id: UC-001
implements: [REQ-001, REQ-002]
```
```yaml
id: TEST-001
verifies: [REQ-001]
```

---

## 3. Toolkit Usage (`aegis`)

Build the standalone binary via Cargo:

```bash
cargo build --release --manifest-path tools/Cargo.toml
```

### CLI Commands

```bash
# 1. Verify traceability and write matrix to doc/traceability.md:
./tools/target/release/aegis trace --out doc/traceability.md

# As JSON:
./tools/target/release/aegis trace --json

# 2. Impact analysis for changes (Blast Radius):
./tools/target/release/aegis impact --target REQ-002 --kind bug
./tools/target/release/aegis impact --code-file src/auth.rs --kind wish

# 3. Issue Management:
./tools/target/release/aegis issue create --title "Buffer overflow in parser" --type bug --severity high --related REQ-002
./tools/target/release/aegis issue list
./tools/target/release/aegis issue close ISSUE-0001

# 4. Full profile-aware Execution Gate:
./tools/target/release/aegis gate --profile doc/embedded-safety.yaml
./tools/target/release/aegis gate --profile doc/enterprise.yaml

# Alternatively via shell wrapper:
bash tools/gate.sh --profile doc/embedded-safety.yaml
```

---

## 4. Closed-Loop Evolution: Ingesting Changes

Development is a continuous lifecycle. Defects and feature requests enter through the front door:

1. **Calculate Blast Radius** (`aegis impact --target REQ-002 --kind bug`).
2. **Select Impact Depth** (Implementation / Functional / Conceptual).
3. **Confirm & Execute Plan Step-by-Step** (see [`doc/change-runbook.md`](change-runbook.md)).
4. **Run Execution Gate until Green** (`aegis gate --profile doc/<profile>.yaml`).
