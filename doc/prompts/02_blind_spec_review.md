# Prompt 02: Blind Peer Review of Specification

**Target Model:** Frontier Model B (Independent Frontier instance)  
**Goal:** Anti-sycophancy audit, identifying contradictions, hallucinated assumptions, and untestable claims.

---

## Prompt Template

```text
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
5. **Scope Boundaries:** Are the Non-Goals sufficient to prevent context drift during 27B implementation?

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
```
