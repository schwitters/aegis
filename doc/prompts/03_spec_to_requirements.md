# Prompt 03: Specification to Requirements & Use Cases

**Target Model:** Frontier Model A (Claude 3.7 Sonnet / GPT-4.5)  
**Output Artifacts:** `doc/requirements/REQ-xxx.md`, `doc/architecture/`, `doc/usecases/UC-xxx.md`

---

## Prompt Template

```text
You are a Lead Requirements Engineer. We have an approved Initial Specification (`doc/001_initial_spec.md`) and must now formally decompose it into atomic Aegis requirements, component architectures, and use cases.

APPROVED SPECIFICATION:
<Paste content of doc/001_initial_spec.md>

INCORPORATED REVIEW FINDINGS:
<Paste approved changes from Prompt 02 review>

YOUR TASK:
Generate the atomic requirement files (`doc/requirements/REQ-xxx.md`) and use cases (`doc/usecases/UC-xxx.md`).

RULES:
1. Every requirement must be atomic, unambiguous, and assigned a stable ID (`REQ-001`, `REQ-002`, ...).
2. Requirements must contain valid YAML frontmatter with `id`, `title`, `status`, `type` (functional|non_functional), and `iso25010`.
3. Every use case (`UC-xxx.md`) must explicitly declare which requirements it realizes via `implements: [REQ-xxx]`.

OUTPUT FORMAT:

### 1. Requirements Artifacts:

For each requirement, output:

File: `doc/requirements/REQ-001.md`
```yaml
id: REQ-001
title: <Short Descriptive Title>
status: active
type: functional
iso25010: functional_suitability
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

File: `doc/architecture/system_overview.md`
- Component diagram (Mermaid)
- Memory and threading model
- Public interface contracts / API signatures

---

### 3. Use Case Artifacts:

For each use case, output:

File: `doc/usecases/UC-001.md`
```yaml
id: UC-001
title: <Use Case Title>
implements: [REQ-001, REQ-002]
```
## Actors & Preconditions
- Primary Actor: ...
- Preconditions: ...

## Main Scenario Flow
1. Step 1...
2. Step 2...

## Exceptional & Edge Flows
- 2a. Edge condition occurs: System handles by...
```
