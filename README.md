<div align="center">

# 🛡️ Aegis

**Spec-Driven Multi-Model Systems Engineering & Quality Gate Toolkit**

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Standards: ISO 25010 / ISO 26262](https://img.shields.io/badge/Standards-ISO%2025010%20%7C%20ISO%2026262-green.svg)](doc/dev-workflow.md)
[![Traceability](https://img.shields.io/badge/Traceability-Bidirectional-brightgreen.svg)](doc/traceability.md)

*A rigorous, fail-closed engineering framework and standalone CLI toolkit designed for multi-agent LLM software development.*

</div>

---

## 🌟 Overview

**Aegis** bridges formal systems engineering (V-Model, Clean-Room Software Engineering, ISO 26262, DO-178C, ISO/IEC 25010) with modern agentic AI coding.

Instead of relying on fragile single-agent prompts or passive chat reviews, Aegis orchestrates a **heterogeneous multi-model pipeline**:
- **Frontier Models** (e.g. Claude 3.7 Sonnet / GPT-4.5) drive conceptual specifications, test case design, architecture, and blind parallel reviews.
- **Local / Smaller Models** (e.g. Qwen 2.5 27B / Claude Haiku) implement deterministic code constrained by strict prompts and 3-tier rulesets.
- **Test Agents** implement the test plan *before* the production code; the tests are reviewed and frozen, so the implementation model cannot weaken them.
- **Aegis CLI** acts as the deterministic supervisor: verifying traceability (`REQ → ARCH → UC → TEST → Code`), detecting suspect links against an approved baseline, calculating blast radius for changes, recording reviews, managing issue lifecycles, and enforcing fail-closed CI execution gates.

---

## 🏗️ Architecture & Process

```mermaid
flowchart TD
    subgraph P1 ["1. Specification & Shift-Left (Frontier Models)"]
        Idea["Product Idea"] --> Spec["Initial Spec"]
        Spec --> Rev1["Blind Parallel Review (recorded)"]
        Rev1 --> Req["Requirements & ISO 25010"]
        Req --> Arch["Architecture (ARCH satisfies REQ) & Use Cases"]
        Arch --> TP["Test Plan BEFORE Implementation"]
        TP --> Base["aegis baseline --approve"]
        Base --> TI["Test Agent: implement + review tests"]
        TI --> Freeze["aegis tests freeze"]
    end

    subgraph P2 ["2. Synthesis & Implementation Loop (27B Model)"]
        Prof["Domain Profile: doc/*.yaml"] --> Comp["aegis instructions"]
        Rules["Coding Ruleset: doc/rules/*.yaml"] --> Comp
        Freeze --> Comp
        Comp --> Task["Next task: one REQ"]
        Task --> Code["Implement (@implements REQ-xxx)"]
        Code --> Fast{"Fast gate: build, lint, test_freeze, tests"}
        Fast -->|red, attempts left| Code
        Fast -->|green| Task
        Fast -->|limit reached| Esc["Escalation report"]
        Esc --> Spec
    end

    subgraph P3 ["3. Verification & Acceptance"]
        Task -->|queue done| Gate["Full gate: security, coverage, mutation, traceability"]
        Gate --> Rev2["Final Review of green diff (recorded)"]
        Rev2 -->|accepted findings| Task
        Rev2 --> Acc["Acceptance gate incl. reviews"]
        Acc --> Deliv["Deliverables Manifest"]
    end

    subgraph P4 ["4. Evolution & Change Management"]
        Feedback["Bug / Wish / Feedback"] --> Impact["aegis impact (Blast Radius, --changed)"]
        Impact --> Depth{"Impact Depth"}
        Depth -->|Implementation| TP
        Depth -->|Functional| Req
        Depth -->|Conceptual| Spec
    end

    Deliv --> Feedback
```

---

## ⚡ Key Features

1. **Traceability (`aegis trace`)**
   - Tracks `Requirement -> Architecture -> Use Case -> Test Plan -> Source Code`; integration tests verify architecture elements.
   - Source code annotates `@implements REQ-XXX`; tests annotate `@verifies TEST-XXX`; every planned test must be implemented.
   - Detects **Gaps**, **Orphans**, malformed or duplicate artifacts, and an empty specification with fail-closed exit codes.

2. **Baselines & Suspect Links (`aegis baseline`)**
   - `aegis baseline --approve <name>` records the reviewed specification in `doc/baseline.lock`.
   - A changed requirement marks every downstream ARCH/UC/TEST and code location as `SUSPECT` until re-reviewed and re-baselined.

3. **Frozen Tests & Mutation Testing (`aegis tests freeze`)**
   - Tests are written by a test agent before the code, reviewed, and frozen; the gate rejects any modification.
   - The `mutation` stage (cargo-mutants, PIT) measures whether the tests actually detect wrong code.

4. **Review Records (`aegis review`)**
   - Every blind review is a work product in `doc/reviews/` with dispositioned findings (accepted / rejected / deferred to an issue).
   - The gate enforces required independent approvals per stage, e.g. two blind architecture reviews.

5. **Blast-Radius Impact Analyzer (`aegis impact`)**
   - Ingests bug reports or feature requests and computes affected artifacts across docs, tests, and code.
   - Recommends impact depth (*Implementation*, *Functional*, *Conceptual*) according to ISO 26262 Part 8.

6. **Issue Lifecycle Management (`aegis issue`)**
   - Version-controlled issues in `doc/issues/ISSUE-XXXX.md` with structured YAML frontmatter.
   - Quality gates halt if unclosed `critical` or `high` severity blockers exist.

7. **Domain Profiles & 3-Tier Rulesets**
   - Declarative domain tailoring via `doc/embedded-safety.yaml` (ASIL-D / MC/DC), `doc/enterprise.yaml`, `doc/web-enterprise.yaml` (Rust web, ASVS L2, GDPR) and `doc/local-first.yaml`.
   - The gate executes the rulesets' toolchain commands and a deviation scan: suppressions only with permitted `AEGIS-DEVIATION` markers.
   - Decoupled, reusable rulesets for C11, C++, Rust, and Java operating across prompt instructions, review rubrics, and SAST linters.

8. **Interactive Dashboard**
   - Open [`doc/process-dashboard.html`](doc/process-dashboard.html) in any browser to inspect the real-time radar, ISO 25010 metrics, and interactive CLI simulator.

---

## ⚡ One-Line Bootstrap (Add Aegis to any Project)

Run this single command inside any new or existing repository to build and scaffold Aegis immediately:

```bash
# Greenfield (New Repository):
git clone --depth 1 https://github.com/schwitters/aegis.git /tmp/aegis-bin && cargo build --release --manifest-path /tmp/aegis-bin/Cargo.toml && /tmp/aegis-bin/target/release/aegis init --profile enterprise --lang rust && rm -rf /tmp/aegis-bin

# Brownfield (Existing Codebase Migration):
git clone --depth 1 https://github.com/schwitters/aegis.git /tmp/aegis-bin && cargo build --release --manifest-path /tmp/aegis-bin/Cargo.toml && /tmp/aegis-bin/target/release/aegis init --brownfield --profile enterprise --lang rust && rm -rf /tmp/aegis-bin

# Alternatively, bind as a Git Submodule:
git submodule add https://github.com/schwitters/aegis.git tools/aegis && cargo build --release --manifest-path tools/aegis/Cargo.toml && ./tools/aegis/target/release/aegis init --profile enterprise --lang rust
```

---

## 🚀 Quick Start

### 1. Build Aegis CLI Locally

```bash
# Build release binary (zero external dependencies)
cargo build --release

# Add to PATH or symlink
export PATH="$PATH:$(pwd)/target/release"
```

### 2. Verify Traceability

```bash
# Check traceability matrix and output markdown report
aegis trace --out doc/traceability.md
```

### 3. Run Blast Radius Impact Analysis

```bash
# Analyze impact of a bug or feature request
aegis impact --target REQ-001 --kind bug
```

### 4. Create and Track Issues

```bash
# Create a new issue (automatically assigns ISSUE-0001)
aegis issue create \
  --title "Buffer overflow in packet parser" \
  --type bug \
  --severity high \
  --related REQ-001

# List all open issues
aegis issue list

# Close an issue once resolved
aegis issue close ISSUE-0001
```

### 5. Baseline, Freeze, Review

```bash
# Show unapproved specification changes and suspect links, then approve
aegis baseline
aegis baseline --approve alice

# Freeze the reviewed tests before implementation starts
aegis tests freeze --profile doc/profile.yaml

# Record a blind review and validate all review records
aegis review create --stage architecture --artifact doc/architecture/ARCH-001.md --reviewer model-b --author model-a
aegis review check --required spec architecture:2
```

### 6. Implementation Loop & Quality Gate

```bash
# Synthesize AGENT_INSTRUCTIONS.md (incl. loop protocol) for the implementation model
aegis instructions --profile doc/profile.yaml --out AGENT_INSTRUCTIONS.md

# Inner loop of the implementation model (partial run, never an acceptance)
aegis gate --profile doc/profile.yaml --only build,lint,test_freeze,tests

# Full gate before the final review, then the acceptance gate with all stages
aegis gate --profile doc/profile.yaml --skip reviews
aegis gate --profile doc/profile.yaml
```

---

## 📁 Repository Structure

```
├── doc/
│   ├── dev-workflow.md         # Full V-Model specification & DoD
│   ├── change-runbook.md       # Change management & impact protocol
│   ├── adoption-guide.md       # Greenfield & brownfield practical guide
│   ├── prompts/                # Complete Prompt Playbook (Stages 01 - 07, incl. 04b test implementation)
│   ├── process-evaluation.md   # Architectural evaluation
│   ├── process-dashboard.html  # Interactive UI & simulator
│   ├── embedded-safety.yaml    # ISO 26262 / ASIL-D profile
│   ├── enterprise.yaml         # Cloud-native backend profile
│   ├── rules/                  # Strict language rulesets (C11, C++, Rust, Java)
│   ├── requirements/           # REQ-001.md, ...
│   ├── architecture/           # ARCH-001.md, ... (satisfies: [REQ-xxx])
│   ├── usecases/               # UC-001.md, ...
│   ├── testplan/               # TEST-001.md, ... + tests.lock
│   ├── reviews/                # REVIEW-0001.md, ... (review records)
│   ├── baseline.lock           # Approved specification baseline
│   ├── deliverables/           # Deliverables manifest
│   └── issues/                 # ISSUE-0001.md, ...
├── tools/                      # Aegis Rust Standalone CLI
│   ├── Cargo.toml
│   ├── src/
│   └── gate.sh                 # Thin auto-building wrapper
└── README.md
```

---

## 📄 Documentation

- 📋 [Prompt Playbook & Instructions Catalog](doc/prompts/README.md)
- 🚀 [Adoption Guide (Greenfield & Brownfield)](doc/adoption-guide.md)
- 📖 [Development Workflow Specification](doc/dev-workflow.md)
- 🔄 [Change Management Runbook](doc/change-runbook.md)
- ⚖️ [Architectural Evaluation](doc/process-evaluation.md)
- 📐 [Coding Rulesets Catalog](doc/rules/README.md)
- 🛡️ [Domain Profiles Reference](doc/embedded-safety.yaml)

---

## 📜 License

Distributed under the MIT License.
