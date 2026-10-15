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

# Or analyze everything changed since the approved baseline:
aegis impact --changed --kind feature
```

Output: The full set of affected requirements, use cases, test cases, and code locations, along with the recommended impact depth.

### Step 2 — Determine Impact Depth

The change is classified into one of three depths:

| Impact Depth | When Applicable | Entry Stage in Pipeline |
|---|---|---|
| **Implementation** | Requirement is correct; only the code or unit test is deficient | Test Plan → Code |
| **Functional** | Requirement or use case itself is incorrect, incomplete, or new | Requirements → Downstream |
| **Conceptual** | Fundamental spec assumptions change; architectural redesign required (`--kind concept`) | Initial Spec → Full Chain |

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
2. **(Conceptual) Revise Spec & Architecture** — Update `doc/001_initial_spec.md` and the affected `ARCH-xxx` → two independent blind reviews, recorded with `aegis review create --stage architecture ...`.
3. **(Functional / Conceptual) Update Requirement** — Assign or edit `REQ-xxx` → blind parallel Frontier review (recorded) → human confirmation.
4. **(Functional / Conceptual) Update Allocation & Use Case** — `ARCH-xxx` `satisfies: [REQ-xxx]`, use case `implements: [REQ-xxx]`.
5. **Update Test Plan** — Add/modify test cases with `verifies: [REQ-xxx]` (and `[ARCH-xxx]` for integration tests). Test design strictly precedes implementation (Shift-Left).
6. **Resolve Suspect Links & Re-Baseline** — `aegis trace` lists every `SUSPECT` artifact downstream of the changed ones. Each is reviewed and updated or confirmed; then the human approves: `aegis baseline --approve <name>`.
7. **Implement & Freeze Tests** — The test agent writes the reproducing / verifying tests; they are reviewed and must fail against the current code (bug) or the missing feature; then `aegis tests freeze`.
8. **Update Agent Instructions** — `aegis instructions` (if language constraints, patterns or loop limits are affected).
9. **Implementation Loop** — The implementation model works task by task with the fast gate (`aegis gate --only build,lint,test_freeze,tests`), bounded by `agent_loop.max_iterations`; unresolved tasks come back as escalation reports.
10. **Full Gate** — `aegis gate --profile doc/<profile>.yaml --skip reviews` until green.
11. **Final Review** — Blind parallel review of the *green* diff with Frontier models; record findings and dispositions (`aegis review create --stage implementation ...`). Accepted findings → back to 9.
12. **Acceptance Gate, Close Issue & Matrix Sync** — `aegis gate --profile doc/<profile>.yaml` (all stages), `aegis issue close ISSUE-xxxx`, `aegis trace --out doc/traceability.md`.

### Step 5 — Completion Report

The agent reports: Issue closed, acceptance gate passing (green, all stages), traceability chain verified with zero gaps and zero suspect links, the review records created, and the list of committed artifacts.

## Guiding Principles

- **No Bypass of Quality Gates.** Bug fixes and enhancements undergo the exact same verification pipeline as greenfield development.
- **Test Before Fix.** Automated tests reproducing the issue or verifying the feature are written by the test agent, reviewed and frozen before implementation code is written.
- **No Silent Spec Drift.** A changed requirement keeps the gate red (`UNREVIEWED` / `SUSPECT`) until its downstream chain was re-reviewed and re-baselined.
- **Fail-Closed Gate.** Red gates must be resolved at the root cause, never bypassed by disabling checks.
- **Fail-Safe Depth.** Over-scoping a change to a higher specification level is preferable to under-scoping and accumulating hidden architectural drift.
