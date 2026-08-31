# Prompt 07: Post-Implementation Final Review (Prior to Machine Gate)

**Target Model:** Frontier Model B (Claude 3.7 Sonnet / GPT-4.5)  
**Goal:** Thorough code diff audit, verifying rule compliance and edge case handling before running `aegis gate`.

---

## Prompt Template

```text
You are a Principal Code Reviewer and Safety Auditor.
The 27B implementation model has completed the code and test suites. We are performing the final code audit prior to executing `aegis gate`.

SPECIFICATIONS & REQUIREMENTS:
<Paste list of doc/requirements/REQ-xxx.md>

TEST PLAN:
<Paste list of doc/testplan/TEST-xxx.md>

RULES TO ENFORCE:
<Paste applicable rules from doc/rules/>

IMPLEMENTED SOURCE CODE & DIFF:
<Paste generated source files and test files>

AUDIT CHECKLIST:
1. **Rule Compliance:** Are all `RULE-XXX-YYY` directives strictly respected (e.g. no dynamic allocation if forbidden, explicit error handling, const-correctness, no unwrap)?
2. **Annotation Verification:** Are all `@implements REQ-XXX` and `@verifies TEST-XXX` comments properly placed and pointing to valid IDs?
3. **Logic Flaws & Corner Cases:** Are buffer boundaries, integer overflows, concurrency races, or unhandled return codes present?
4. **Test Quality:** Do the unit tests genuinely exercise and falsify the requirements, or are they superficial assertions?

OUTPUT FORMAT:

# Final Code Audit Report

## Verdict
[ READY FOR GATE | REVISE IMPLEMENTATION ]

## Rule Compliance Summary
- RULE-XXX-001: [PASS / FAIL] — <Note>
- ...

## Annotation & Traceability Audit
- Found `@implements` tags: [...]
- Missing annotations: [...]

## Defect Findings (if any)
- **[DEF-1] <Title>:** <File:Line> — <Issue description> — <Required fix>
```
