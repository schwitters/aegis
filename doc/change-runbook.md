# Runbook: Change Management & Evolution

When a **defect** is observed or a **feature request** emerges while using a deliverable, it is not patched ad hoc in the code. Instead, it enters the process cleanly through the front door. The agent performs impact analysis, formulates an executable step-by-step plan, and executes it **with human confirmation at transition gates**.

This formal change management protocol draws directly from ISO 26262 Part 8: zero changes without knowing the blast radius and zero merges without re-verifying the affected traceability chain.

## Role of the Agent

The agent acts as the Change Coordinator. It makes no architectural decisions in isolation — at each process boundary, it presents the next step and awaits human confirmation before proceeding.

## Procedure

### Step 0 — Ingestion & Classification

The agent ingests unstructured feedback (*"User session expires too early"*, *"Add OAuth2 support"*) and derives:
- **Kind:** `bug` (malfunction/defect) or `feature` / `wish` (new capability or behavioral change).
- **Severity:** `low`, `medium`, `high`, or `critical`.
- **Anchor:** Which requirement ID, use case ID, or code path is impacted? If ambiguous, the agent queries the user or inspects the traceability matrix.

### Step 1 — Impact Analysis (Blast Radius)

The agent invokes the Aegis impact analyzer:

```bash
# Analyze by Requirement / Use Case ID:
aegis impact --target REQ-002 --kind bug --json

# Or analyze by source code file:
aegis impact --code-file src/auth.rs --kind wish --json
```

Output: The full set of affected requirements, use cases, test cases, and code locations, along with the recommended impact depth.

### Step 2 — Determine Impact Depth

The change is classified into one of three depths:

| Impact Depth | When Applicable | Entry Stage in Pipeline |
|---|---|---|
| **Implementation** | Requirement is correct; only the code or unit test is deficient | Test Plan → Code |
| **Functional** | Requirement or use case itself is incorrect, incomplete, or new | Requirements → Downstream |
| **Conceptual** | Fundamental spec assumptions change; architectural redesign required | Initial Spec → Full Chain |

**Decision Rules:**
- Feature / Wish → At least **Functional** (new or altered requirement).
- Bug where requirement is correct, but code violates it → **Implementation**.
- Bug where requirement itself is flawed or incomplete → **Functional**.
- In doubt, select the deeper level: missing an affected upstream requirement is far more costly than conducting one extra review.

### Step 3 — Present Proposal for Approval

The agent presents the structured change plan (derived from `aegis impact`). Each step details:
1. Which artifact is modified or created.
2. What review or verification gate follows.
3. Why this impact depth was selected.

> [!IMPORTANT]
> The workflow halts here until the human engineer approves the plan and impact depth.

### Step 4 — Step-by-Step Execution

The agent executes the plan, confirming at transition gates:

1. **Create Issue** — `aegis issue create --title "..." --type ... --severity ... --related REQ-xxx`.
2. **(Functional / Conceptual) Update Requirement** — Assign or edit `REQ-xxx` → Blind parallel Frontier review → Human confirmation.
3. **(Functional / Conceptual) Update Use Case** — Link `implements: [REQ-xxx]`.
4. **Update Test Plan** — Add/modify test cases with `verifies: [REQ-xxx]`. Test design strictly precedes implementation (Shift-Left).
5. **Update Agent Instructions** — Update prompt directives if language constraints or patterns are affected.
6. **Implementation** — 27B model writes code and annotations (`@implements REQ-xxx`).
7. **Final Review** — Blind parallel review of diff with Frontier models.
8. **Run Execution Gate** — `aegis gate --profile doc/<profile>.yaml` until green.
9. **Close Issue & Matrix Sync** — `aegis issue close ISSUE-xxxx` and `aegis trace --out doc/traceability.md`.

### Step 5 — Completion Report

The agent reports: Issue closed, Gate passing (green), Traceability chain verified with zero gaps, and the list of committed artifacts.

## Guiding Principles

- **No Bypass of Quality Gates.** Bug fixes and enhancements undergo the exact same verification pipeline as greenfield development.
- **Test Before Fix.** Automated tests reproducing the issue or verifying the feature are committed before implementation code is written.
- **Fail-Closed Gate.** Red gates must be resolved at the root cause, never bypassed by disabling checks.
- **Fail-Safe Depth.** Over-scoping a change to a higher specification level is preferable to under-scoping and accumulating hidden architectural drift.
