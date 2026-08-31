# Prompt 05: Synthesizing Agent Instructions for 27B Implementation Model

**Target Model:** Frontier Model A (Claude 3.7 Sonnet / GPT-4.5) or synthesized via `aegis instructions`  
**Output Artifact:** `AGENT_INSTRUCTIONS.md` (or prompt payload given to the 27B model)  
**Core Purpose:** Externalizing all implicit architectural knowledge, rules, and requirements into an unambiguous, bullet-proof prompt.

---

## Prompt Template

```text
You are a Principal AI Agent Prompt Synthesizer.
We are preparing the implementation phase. A local 27B coding model will implement the codebase.
The 27B model possesses strong syntax knowledge but zero implicit context: what is not explicitly written in its instructions will be guessed or hallucinated.

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
5. **Step-by-Step Implementation Sequence:**
   - Step 1: Data types and error enums
   - Step 2: Core modules
   - Step 3: Test cases covering all TEST-xxx files
6. **Definition of Done for the 27B Model:**
   - Builds without warnings.
   - All tests pass.
   - Zero missing `@implements` or `@verifies` annotations.
```
