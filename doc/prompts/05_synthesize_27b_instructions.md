# Prompt 05: Synthesizing Agent Instructions for ≥27B Coding Model

**Target Model:** Frontier Model A (current frontier model) or synthesized via `aegis instructions` (preferred: it renders the loop protocol from the profile's `agent_loop`)  
**Output Artifact:** `AGENT_INSTRUCTIONS.md` (or prompt payload given to the ≥27B coding model)  
**Core Purpose:** Externalizing all implicit architectural knowledge, rules, and requirements into an unambiguous, bullet-proof prompt.

---

## Prompt Template

```text
You are a Principal AI Agent Prompt Synthesizer.
We are preparing the implementation phase. A ≥27B coding model will implement the codebase.
The ≥27B coding model possesses strong syntax knowledge but zero implicit context: what is not explicitly written in its instructions will be guessed or hallucinated.

INPUT CONTEXT:
1. Domain Profile (`doc/profile.yaml` or `doc/embedded-safety.yaml` / `doc/enterprise.yaml`):
<Paste content of profile.yaml>

2. Language Coding Ruleset (`doc/rules/*.yaml`):
<Paste content of referenced ruleset files>

3. Full Requirements (`doc/requirements/*.md`):
<Paste content of all REQ-xxx.md files>

4. Full Test Plan (`doc/testplan/*.md`):
<Paste content of all TEST-xxx.md files>

5. Deliverables Manifest (`doc/deliverables/manifest.md`):
<Paste content of manifest.md>

YOUR TASK:
Synthesize all input files into a single, cohesive, self-contained `AGENT_INSTRUCTIONS.md`.

MANDATORY SECTIONS IN SYNTHESIZED INSTRUCTIONS:
1. **Mission & System Overview:** Crisp 2-sentence objective and target language/version.
2. **Hard Architecture & Language Constraints:**
   - Explicit listing of all applicable Rule IDs (`RULE-XXX-YYY`) and exact coding directives.
   - Exact memory management, concurrency, and error handling rules.
3. **Exact Requirements Matrix (`REQ-xxx`):**
   - Precise listing of every REQ-ID and its functional contract.
4. **Mandatory Code Annotations (Non-Negotiable):**
   - Direct the model that EVERY function/method/struct implementing a requirement MUST contain:
     `@implements REQ-XXX`
   - Direct the model that EVERY unit test MUST contain:
     `@verifies TEST-XXX`
5. **Frozen Tests:**
   - The tests listed in `doc/testplan/tests.lock` already exist and are read-only for the model.
   - The model makes them pass by changing production code only; it never edits `doc/` or test files.
6. **Task Queue & Loop Protocol:**
   - Ordered list of tasks, one requirement each, with the frozen tests that verify it
     (order: data types and error enums first, then core modules along the dependency order of the ARCH elements).
   - After every change: `aegis gate --profile <PROFILE> --only build,lint,test_freeze,tests`.
   - At most `agent_loop.max_iterations` gate runs per task; then stop and write `ESCALATION-<REQ-ID>.md`
     (format in Prompt 06). Escalate immediately on ambiguity instead of guessing.
7. **Definition of Done for the ≥27B Coding Model:**
   - Fast gate green for every task (builds without warnings, all frozen tests pass, tests unmodified).
   - Zero missing `@implements` annotations.
   - Every unfinished task has an escalation report.
```
