# Prompt 03: Specification to Requirements & Use Cases

**Target Model:** Frontier Model A (Claude 3.7 Sonnet / GPT-4.5)  
**Output Artifacts:** `doc/requirements/REQ-xxx.md`, `doc/architecture/ARCH-xxx.md`, `doc/usecases/UC-xxx.md`

---

## Prompt Template

````text
You are a Lead Requirements Engineer. We have an approved Initial Specification (`doc/001_initial_spec.md`) and must now formally decompose it into atomic Aegis requirements, component architectures, and use cases.

APPROVED SPECIFICATION:
<Paste content of doc/001_initial_spec.md>

INCORPORATED REVIEW FINDINGS:
<Paste approved changes from Prompt 02 review>

YOUR TASK:
Generate the atomic requirement files (`doc/requirements/REQ-xxx.md`) and use cases (`doc/usecases/UC-xxx.md`).

RULES:
1. Every requirement must be atomic, unambiguous, and assigned a stable ID (`REQ-001`, `REQ-002`, ...).
2. Every artifact starts with YAML frontmatter (`---` ... `---`) containing at least `id`. Requirements also carry `title`, `status`, `type` (functional|non_functional), and `iso25010`.
3. Every requirement is allocated to at least one architecture element: each `ARCH-xxx.md` declares `satisfies: [REQ-xxx]`.
4. Every use case (`UC-xxx.md`) must explicitly declare which requirements it realizes via `implements: [REQ-xxx]`.
5. List fields contain only artifact IDs — no prose, no ranges like `REQ-001..005` (the trace check rejects them).

OUTPUT FORMAT:

### 1. Requirements Artifacts:

For each requirement, output:

File: `doc/requirements/REQ-001.md`
```markdown
---
id: REQ-001
title: <Short Descriptive Title>
status: active
type: functional
iso25010: functional_suitability
---
```
## Description
<Detailed, unambiguous statement of requirement>

## Rationale
<Why this requirement is necessary>

## Verification Criteria
- [ ] <Concrete observable condition 1>
- [ ] <Concrete observable condition 2>

---

### 2. Architecture Specification:

File: `doc/architecture/system_overview.md` (free-form, no ID)
- Component diagram (Mermaid) naming every ARCH element
- Memory and threading model

For each component / architecture element, output (Rust workspaces: one ARCH element per layer crate, e.g.
`<project>-domain`, `<project>-application`, `<project>-storage`, `<project>-api`, `<project>-server`; the
allowed dependencies between them are fixed by the profile's `gate.architecture` policy):

File: `doc/architecture/ARCH-001.md`
```markdown
---
id: ARCH-001
title: <Component Name>
satisfies: [REQ-001, REQ-004]
---
```
## Responsibility
<What this element owns — and explicitly what it does not>

## Public Interface Contract
<Exact signatures, error variants, invariants. The test agent codes against these.>

## Threading, Memory & Resource Model
<Ownership, bounds, blocking behavior>

## Integration Verification
<Which integration behavior proves this element works with its neighbors (becomes a TEST with `verifies: [ARCH-001]`)>

---

### 3. Use Case Artifacts:

For each use case, output:

File: `doc/usecases/UC-001.md`
```markdown
---
id: UC-001
title: <Use Case Title>
implements: [REQ-001, REQ-002]
audience: user        # handbook that must document it: user | operator | none (internal/system use case)
---
```
## Actors & Preconditions
- Primary Actor: ...
- Preconditions: ...

## Main Scenario Flow
1. Step 1...
2. Step 2...

## Exceptional & Edge Flows
- 2a. Edge condition occurs: System handles by...
````
