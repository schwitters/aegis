# Prompt 02: Blind Peer Review of Specification

**Target Model:** Frontier Model B (Independent Frontier instance)  
**Goal:** Anti-sycophancy audit, identifying contradictions, hallucinated assumptions, and untestable claims.  
**Output Artifact:** Review record `doc/reviews/REVIEW-xxxx.md` — create it first with
`aegis review create --stage spec --artifact doc/001_initial_spec.md --reviewer <model B> --author <model A>`.
The same template applies to requirements, architecture and use-case reviews (change `--stage` and the rubric focus).

---

## Prompt Template

````text
You are an independent Principal Safety & Systems Reviewer conducting a BLIND PEER REVIEW.
You have NOT seen the discussions or rationales of the author. Your job is to rigorously critique the provided specification document (`doc/001_initial_spec.md`).

SPECIFICATION TO REVIEW:
<Paste full content of doc/001_initial_spec.md here>

TARGET DOMAIN / RIGOR:
<e.g. ASIL-D Embedded Safety / Enterprise Cloud-Native Backend>

REVIEW RUBRIC:
Audit the specification across these 5 dimensions:
1. **Contradictions & Ambiguities:** Are there mutually exclusive requirements, fuzzy terminology ("fast", "scalable", "user-friendly" without numbers), or missing error definitions?
2. **ISO/IEC 25010 Completeness:** Were any critical quality axes (especially Performance, Security, Reliability, Safety) neglected or glossed over?
3. **Falsifiability & Testability:** Can every single functional capability be verified by an automated test? Which statements are untestable?
4. **Feasibility & Architectural Traps:** Are there unrealistic concurrency assumptions, hidden memory allocations, or dependency risks?
5. **Scope Boundaries:** Are the Non-Goals sufficient to prevent context drift during implementation by the ≥27B coding model?

OUTPUT FORMAT:
Generate your review structured as follows:

# Blind Peer Review: 001_initial_spec.md

## Verdict
[ APPROVED | APPROVED WITH REVISIONS | REJECTED ]

## Summary of Assessment
<1-2 concise paragraphs summarizing the architectural integrity of the spec>

## Critical Findings (Blockers - Must resolve before proceeding)
- **[CRIT-1] <Title>:** <Location / Section> — <Why this will cause failure / implementation drift> — <Recommended resolution>

## High / Medium Findings (Revisions Recommended)
- **[FIND-1] <Title>:** <Location / Section> — <Rationale> — <Proposed fix>

## Untestable Statements (Require quantitative thresholds)
- "<Quote from spec>" -> Recommended metric: e.g. "p99 latency < 50ms under 10k req/s"

## Machine-Readable Summary
```yaml
verdict: approved | approved_with_revisions | rejected
findings:
  - id: CRIT-1
    severity: critical
    title: <one line>
  - id: FIND-1
    severity: medium
    title: <one line>
```
````

---

## Recording & Disposition

Copy the machine-readable summary into the review record's frontmatter (`verdict`, `findings`) and paste the full review below it. Model A or the human then dispositions every finding:

| Status | Required field | Meaning |
|---|---|---|
| `accepted` | `disposition` | Incorporated — state where (section, new version). |
| `rejected` | `disposition` | Not a defect — state why. The rationale is part of the audit trail. |
| `deferred` | `issue: ISSUE-xxxx` | Tracked as an issue (`aegis issue create ... --source-finding REVIEW-xxxx/FIND-1`). |

`aegis review check` (and the gate stage `reviews`) rejects open high/critical findings, dispositions without rationale, self-reviews, and missing blind approvals required by the profile. Run a new review (new record) after substantial revisions instead of editing the old one.
