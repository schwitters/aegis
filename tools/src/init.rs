use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub profile: String,
    pub lang: String,
    pub brownfield: bool,
}

pub fn run_init(root: &Path, opts: InitOptions) -> Result<()> {
    let doc_dir = root.join("doc");
    let req_dir = doc_dir.join("requirements");
    let uc_dir = doc_dir.join("usecases");
    let test_dir = doc_dir.join("testplan");
    let issues_dir = doc_dir.join("issues");
    let deliv_dir = doc_dir.join("deliverables");
    let rules_dir = doc_dir.join("rules");

    fs::create_dir_all(&req_dir).context("Failed to create doc/requirements")?;
    fs::create_dir_all(&uc_dir).context("Failed to create doc/usecases")?;
    fs::create_dir_all(&test_dir).context("Failed to create doc/testplan")?;
    fs::create_dir_all(&issues_dir).context("Failed to create doc/issues")?;
    fs::create_dir_all(&deliv_dir).context("Failed to create doc/deliverables")?;
    fs::create_dir_all(&rules_dir).context("Failed to create doc/rules")?;

    // 1. Deliverables Manifest
    let manifest_path = deliv_dir.join("manifest.md");
    if !manifest_path.exists() {
        let manifest_content = r#"# Deliverables Manifest

Required deliverables for gate verification:

- path: doc/traceability.md
- path: README.md
"#;
        fs::write(&manifest_path, manifest_content)?;
    }

    // 2. Profile YAML
    let profile_path = doc_dir.join("profile.yaml");
    if !profile_path.exists() {
        let lang = opts.lang.to_lowercase();
        let is_safety = opts.profile == "embedded-safety" || opts.profile == "safety";
        
        let profile_content = if is_safety {
            format!(
                r#"# Domain Profile: Safety-Critical Systems
profile: embedded-safety
rigor: ASIL-D
language: {lang}

rulesets:
  - doc/rules/{lang}-safety.yaml

quality_axes:
  functional_suitability: required
  performance_efficiency: required
  reliability: required
  safety: required
  security: required
  maintainability: recommended
  compatibility: recommended

gate:
  build:
    active: true
  lint:
    active: true
  static_analysis:
    active: true
  tests:
    active: true
  coverage:
    active: true
    metric: mcdc
    threshold: 100
  traceability:
    active: true
    bidirectional: true
  deliverables:
    active: true

coding_constraints:
  - Zero dynamic memory allocation at runtime.
  - Zero recursion (statically bounded stack depth).
  - Every @implements annotation must state the realized REQ-ID.
"#,
                lang = if lang == "c" || lang == "c11" { "c11-embedded" } else { &lang }
            )
        } else {
            format!(
                r#"# Domain Profile: High-Integrity Systems Engineering
profile: enterprise
rigor: none
language: {lang}

rulesets:
  - doc/rules/{lang}-strict.yaml

quality_axes:
  functional_suitability: required
  security: required
  maintainability: required
  compatibility: required
  reliability: recommended
  performance_efficiency: recommended

gate:
  build:
    active: true
  lint:
    active: true
  static_analysis:
    active: true
  security:
    active: true
  tests:
    active: true
  coverage:
    active: true
    metric: branch
    threshold: 80
  traceability:
    active: true
  deliverables:
    active: true

coding_constraints:
  - Explicit error handling with strongly typed domain errors.
  - Zero hardcoded credentials or secrets in source code.
  - Every @implements annotation must state the realized REQ-ID.
"#,
                lang = &lang
            )
        };

        fs::write(&profile_path, profile_content)?;
    }

    // 3. Initial Spec or Brownfield Reverse-Engineering Prompt
    if opts.brownfield {
        let prompt_path = doc_dir.join("REVERSE_SPEC_EXTRACTION_PROMPT.md");
        if !prompt_path.exists() {
            let prompt_content = format!(
                r#"# Frontier Model Prompt: Reverse Spec Extraction (Brownfield Migration)

Copy and paste the following prompt to your Frontier Model (Claude 3.7 Sonnet / GPT-4.5) alongside your codebase files:

---

```text
You are an expert Systems Engineer performing an architectural reverse-engineering extraction for an existing {lang_upper} codebase.

TASK:
Analyze the provided source code, architecture, and tests. Extract the 10-20 core functional and non-functional requirements that the system currently fulfills (Status Quo Baseline).

OUTPUT FORMAT:
For each requirement, output a valid Aegis requirement artifact in Markdown format:

File: doc/requirements/REQ-XXX.md
```yaml
id: REQ-001
title: <Concise Title>
status: active
type: functional
iso25010: functional_suitability
```

## Description
<Precise, unambiguous requirement description>

## Rationale
<Why this requirement exists in the system>

## Verification Criteria
<How this requirement is verified by automated tests>
```

Also extract the corresponding Use Cases in `doc/usecases/UC-XXX.md` with `implements: [REQ-XXX]`.
```
"#,
                lang_upper = opts.lang.to_uppercase()
            );
            fs::write(&prompt_path, prompt_content)?;
        }

        let baseline_req = req_dir.join("REQ-001.md");
        if !baseline_req.exists() {
            let req_content = r#"---
```yaml
id: REQ-001
title: Baseline System Core Initialization
status: active
type: functional
iso25010: functional_suitability
```
---

## Description
The system initializes core components, configurations, and communication interfaces deterministically.

## Rationale
Required for proper baseline execution and startup integrity.

## Verification Criteria
- [ ] System initializes with valid configuration without panics/crashes.
"#;
            fs::write(&baseline_req, req_content)?;
        }

        let baseline_uc = uc_dir.join("UC-001.md");
        if !baseline_uc.exists() {
            let uc_content = r#"---
```yaml
id: UC-001
title: System Startup Sequence
implements: [REQ-001]
```
---

## Actors
- System Operator / Init Process

## Main Flow
1. Load configuration.
2. Initialize memory and peripheral interfaces.
3. Start worker dispatchers.
"#;
            fs::write(&baseline_uc, uc_content)?;
        }
    } else {
        let spec_path = doc_dir.join("001_initial_spec.md");
        if !spec_path.exists() {
            let spec_content = r#"---
title: Initial Product Specification
status: draft
version: 0.1
---

# 001: Initial Product Specification

## 1. Executive Summary & Vision
Concise description of the product idea, target domain, and core problem solved.

## 2. Core Capabilities
- Feature 1: Description
- Feature 2: Description

## 3. Non-Functional & Quality Constraints (ISO/IEC 25010)
- **Performance:** Timing budgets, latency limits.
- **Reliability:** Error recovery, uptime guarantees.
- **Security:** Authentication, tamper resistance, data protection.
- **Safety:** Fail-safe defaults, hazard containment (if applicable).

## 4. Architecture & Interface Hypotheses
Initial thoughts on technology stack, components, and protocol boundaries.

## 5. Scope & Non-Goals
- IN SCOPE: ...
- EXPLICIT NON-GOALS: ...
"#;
            fs::write(&spec_path, spec_content)?;
        }
    }

    println!("{}", "✔ Aegis scaffolding initialized successfully!".green().bold());
    println!("  Directory: {}", doc_dir.display().to_string().cyan());
    println!("  Profile:   {}", doc_dir.join("profile.yaml").display().to_string().yellow());
    if opts.brownfield {
        println!();
        println!("{}", "Brownfield Migration Artifacts Created:".bold());
        println!("  1. Prompt template: {}", doc_dir.join("REVERSE_SPEC_EXTRACTION_PROMPT.md").display().to_string().cyan());
        println!("  2. Baseline REQ/UC: {} & {}", req_dir.join("REQ-001.md").display(), uc_dir.join("UC-001.md").display());
        println!("  3. Next step: Run prompt in Frontier model, tag @implements in code, then run: aegis trace");
    } else {
        println!();
        println!("{}", "Greenfield Next Steps:".bold());
        println!("  1. Refine specification in {}", doc_dir.join("001_initial_spec.md").display().to_string().cyan());
        println!("  2. Run blind review with Frontier Model B");
        println!("  3. Derive requirements in doc/requirements/ and test plan in doc/testplan/");
        println!("  4. Implement with 27B model (@implements REQ-xxx) and run: aegis gate --profile doc/profile.yaml");
    }

    Ok(())
}
