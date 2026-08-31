# Aegis Prompt Playbook & Agent Instructions Catalog

A standardized collection of copy-pasteable prompt templates for orchestrating the **Spec-Driven Multi-Model Engineering Pipeline**.

## Pipeline Overview

| Stage | Prompt Template | Executing Model | Primary Output |
|---|---|---|---|
| **1. Ideation** | [`01_ideation_to_spec.md`](01_ideation_to_spec.md) | Frontier Model A | `doc/001_initial_spec.md` |
| **2. Spec Review** | [`02_blind_spec_review.md`](02_blind_spec_review.md) | Frontier Model B (Blind) | Review Findings & Risk Analysis |
| **3. Derivation** | [`03_spec_to_requirements.md`](03_spec_to_requirements.md) | Frontier Model A | `doc/requirements/`, `doc/architecture/`, `doc/usecases/` |
| **4. Shift-Left Tests** | [`04_requirements_to_testplan.md`](04_requirements_to_testplan.md) | Frontier Model A | `doc/testplan/TEST-xxx.md` |
| **5. Synthesis** | [`05_synthesize_27b_instructions.md`](05_synthesize_27b_instructions.md) | Frontier Model A or `aegis instructions` | `AGENT_INSTRUCTIONS.md` |
| **6. Implementation** | [`06_27b_implementation.md`](06_27b_implementation.md) | Local 27B Model | Source Code & Tests with `@implements` / `@verifies` |
| **7. Final Review** | [`07_final_review.md`](07_final_review.md) | Frontier Model B | Diff Audit prior to `aegis gate` |

---

## Tooling Integration

You can also generate the synthesized instructions automatically using the Aegis CLI:

```bash
# Automatically synthesize AGENT_INSTRUCTIONS.md from doc/profile.yaml, rules, and requirements:
aegis instructions --out AGENT_INSTRUCTIONS.md
```
