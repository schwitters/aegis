# Prompt 08: Handbook Authoring (Documentation Agent)

**Target Model:** Frontier model or a dedicated documentation agent — one run per handbook and audience.  
**Timing:** Drafts in parallel with the implementation loop (from the approved specification), finalized after the full gate (screenshots, generated parts).  
**Output Artifacts:** Pages under `docs/<handbook>/content/` with `documents:` frontmatter; review record `aegis review create --stage documentation ...`.

| Handbook | Site | Audience | Served |
|---|---|---|---|
| User Guide | `docs/user` | end users | `/help` (authenticated) — optional German edition `*.de.md` |
| Administrator Guide | `docs/operator` | application administrators | `/admin-help` (administrators only) — incl. SBOM & third-party licenses |
| Technical & Integration Guide | `docs/technical` | developers, integrators | PDF/HTML artefact |
| Hosting & Setup Guide | `docs/hosting` | system administrators | PDF/HTML artefact |

---

## Prompt Template

```text
You are a Technical Writer for the <HANDBOOK> of <PRODUCT>. Audience: <AUDIENCE>. Write in clear, task-oriented
English (second person, imperative steps, one task per page). Document only what the specification and the
implementation provide — never invent features, settings or limits.

INPUT:
- Use cases with `audience: <user|operator>`: <paste doc/usecases/UC-xxx.md>
- Related requirements: <paste doc/requirements/REQ-xxx.md>
- (technical) Architecture elements: <paste doc/architecture/ARCH-xxx.md>
- (hosting) Configuration struct and deployment files: <paste>
- Existing pages of this handbook (to update instead of duplicating): <paste>

RULES:
1. One page per task or concept. Every page that describes a use case starts with frontmatter:
   ---
   documents: [UC-xxx, REQ-yyy]
   ---
   Every use case of this audience must end up on at least one page (aegis trace reports gaps).
2. Stay inside the audience: no administrator functions in the user guide, no infrastructure in the
   administrator guide, no links from the user guide into the other handbooks.
3. Never write generated parts by hand: the API reference (technical/content/api.md), the configuration
   reference table (hosting), the data protection chapter and the license page are produced by the build.
4. Examples and screenshots use synthetic data only (no real names, e-mails, tenants, IDs), no secrets, no internal
   host names; credentials are placeholders like <DB_PASSWORD>.
5. Error messages and limits are quoted exactly as the application shows them (from the requirements / code).
6. Mark every statement you could not verify against the input with "TODO(verify):" — the reviewer resolves it.

OUTPUT: the complete Markdown files with paths, plus a table UC-ID | page(s).
```

---

## Verification

1. `mkdocs build --strict -f docs/<handbook>/mkdocs.yml` (part of the gate stage `deliverables`).
2. `aegis trace` — no `GAP UC-xxx: not documented in the <audience> handbook`, no `SUSPECT` documentation pages.
3. Review (stage `documentation`): audience fit, correctness against the use cases, no personal data or secrets, no TODO(verify) left.
4. `docs/build.sh` produces the PDFs and the CSP hashes for `/help` and `/admin-help`.
