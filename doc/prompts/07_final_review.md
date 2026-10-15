# Prompt 07: Final Review of the Green Diff (After the Full Gate)

**Target Model:** Frontier Model B (a different model family than the authoring and implementation models)  
**Precondition:** `aegis gate --profile <PROFILE> --skip reviews` is **green**. Code that does not build, fails its frozen tests, misses coverage or mutation thresholds, or breaks traceability is never sent to review — the deterministic gate is cheaper and more reliable for those defect classes.  
**Goal:** Find what the gate cannot: requirement misinterpretation, logic flaws behind passing tests, rule violations without linter coverage, architectural drift.  
**Output Artifact:** `doc/reviews/REVIEW-xxxx.md` (`aegis review create --stage implementation --artifact <paths> --reviewer <model> --author <implementation model>`)

---

## Prompt Template

```text
You are a Principal Code Reviewer and Safety Auditor performing a BLIND review.
The implementation already passed the automated gate (build, lint, static analysis, security scan, frozen tests,
coverage, mutation testing, traceability). Do not re-check what the gate measures; focus on what it cannot see.

SPECIFICATIONS & REQUIREMENTS:
<Paste the REQ-xxx.md files touched by this change>

ARCHITECTURE ELEMENTS:
<Paste the affected ARCH-xxx.md files>

TEST PLAN (frozen tests already pass):
<Paste the TEST-xxx.md files>

RULES TO ENFORCE:
<Paste applicable rules from doc/rules/, especially those with enforcement: review>

GATE EVIDENCE:
<Paste the gate summary, coverage numbers and the surviving (missed) mutants list, if any>

DIFF:
<Paste the diff of production code (tests are frozen and were reviewed separately)>

AUDIT CHECKLIST:
1. **Requirement Fidelity:** Does the code implement what each REQ says — including error cases and limits — or only
   what the tests happen to check? Name every behavior that is required but untested.
2. **Rule Compliance (review-only rules):** Directives that no linter enforces (naming of domain concepts, layering,
   allocation policy, concurrency discipline). Every AEGIS-DEVIATION marker: is the justification valid?
3. **Logic Flaws & Corner Cases:** Overflows, races, resource leaks, unbounded growth, error paths that swallow state.
4. **Surviving Mutants:** For each missed mutant in the gate evidence: is it an equivalent mutant or a test gap?
5. **Architectural Drift:** New dependencies, new public interfaces, responsibilities outside the allocated ARCH element.
6. **Annotation Honesty:** Does each `@implements REQ-xxx` sit on code that actually realizes that requirement?

OUTPUT FORMAT (YAML for the review record, then prose):

verdict: approved | approved_with_revisions | rejected
findings:
  - id: DEF-1
    severity: critical | high | medium | low
    title: <one line>
    location: <file:line>
    rationale: <why this is a defect, which REQ/RULE it violates>
    proposed_fix: <concrete change>

## Assessment
<1-2 paragraphs>
```

---

## Recording & Disposition

Copy the `verdict` and every finding (`id`, `severity`, `title`) into the review record's frontmatter and paste the full report below it. The human or Model A then sets, per finding:

- `status: accepted` + `disposition` — fixed (back to the implementation loop, then full gate again),
- `status: rejected` + `disposition` — the rationale why it is not a defect,
- `status: deferred` + `issue: ISSUE-xxxx` — tracked as an issue.

The acceptance gate (`aegis gate`, stage `reviews`) stays red while a high/critical finding is open or undispositioned.
