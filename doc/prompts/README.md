# Aegis Prompt Playbook & Agent Instructions Catalog

A standardized collection of copy-pasteable prompt templates for orchestrating the **Spec-Driven Multi-Model Engineering Pipeline**.

## Pipeline Overview

| Stage | Prompt Template | Executing Model | Primary Output |
|---|---|---|---|
| **1. Ideation** | [`01_ideation_to_spec.md`](01_ideation_to_spec.md) | Frontier Model A | `doc/001_initial_spec.md` |
| **2. Spec Review** | [`02_blind_spec_review.md`](02_blind_spec_review.md) | Frontier Model B (Blind) | Review Findings & Risk Analysis |
| **3. Derivation** | [`03_spec_to_requirements.md`](03_spec_to_requirements.md) | Frontier Model A | `doc/requirements/`, `doc/architecture/`, `doc/usecases/` |
| **4. Shift-Left Tests** | [`04_requirements_to_testplan.md`](04_requirements_to_testplan.md) | Frontier Model A | `doc/testplan/TEST-xxx.md`, then `aegis baseline --approve` |
| **4b. Test Implementation** | [`04b_test_implementation.md`](04b_test_implementation.md) | Test agent (never the implementation model) | Tests with `@verifies`, reviewed, frozen in `doc/testplan/tests.lock` |
| **5. Synthesis** | [`05_synthesize_27b_instructions.md`](05_synthesize_27b_instructions.md) | `aegis instructions` (or Frontier Model A) | `AGENT_INSTRUCTIONS.md` incl. loop protocol |
| **6. Implementation Loop** | [`06_27b_implementation.md`](06_27b_implementation.md) | Local 27B Model in an agent harness | Production code with `@implements`; escalation reports |
| — Full Gate | `aegis gate --skip reviews` | Deterministic | Green working tree |
| **7. Final Review** | [`07_final_review.md`](07_final_review.md) | Frontier Model B | Review record of the green diff, then acceptance gate (`aegis gate`) |
| **8. Handbooks** | [`08_documentation.md`](08_documentation.md) | Documentation agent (per handbook) | `docs/<handbook>/content/*.md` with `documents:`; review stage `documentation` |

Every review (stages 2, 3, 4, 4b, 7, 8) is persisted as `doc/reviews/REVIEW-xxxx.md` with dispositioned findings (`aegis review create`).

---

## Tooling Integration

You can also generate the synthesized instructions automatically using the Aegis CLI:

```bash
# Automatically synthesize AGENT_INSTRUCTIONS.md from doc/profile.yaml, rules, and requirements:
aegis instructions --out AGENT_INSTRUCTIONS.md
```
