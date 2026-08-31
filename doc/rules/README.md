# Coding Rulesets

Strict, technology- and language-specific coding standards (e.g., C11, C++, Java, Rust).
They decouple **language-level engineering constraints** from **domain-level process profiles** (`doc/*.yaml`), enabling flexible multi-ruleset composition (*composition over inheritance*).

## The 3 Enforcement Tiers

Every rule operates simultaneously across three sequential enforcement layers:

| Tier | Mechanism | Executing Entity |
|---|---|---|
| **1. Agent Instructions** | Rules are formulated as **strict prompt directives** (with positive and negative examples) given to the 27B implementation model. | 27B Model |
| **2. Review Rubric** | Rules serve as an **explicit audit checklist** (`RULE-XXX-YYY`) during blind parallel reviews. | Frontier Models |
| **3. Machine Gate** | Where statically verifiable, rules map directly to **linter, SAST, or compiler checks** in the automated gate. | `clang-tidy`, `clippy`, `checkstyle`, `ruff` |

---

## Schema of a Ruleset File (`.yaml`)

Every file under `doc/rules/<language>-<variant>.yaml` follows this structure:

```yaml
ruleset: c11-strict
description: Strict C11 rules for robust, deterministic production software
standard: ISO/IEC 9899:2011 (C11)
language: c

linter_config:
  tool: clang-tidy
  config_file: .clang-tidy
  flags: ["-warnings-as-errors=*"]

rules:
  - id: RULE-C11-001
    title: Zero Dynamic Memory Allocation
    category: memory
    severity: error
    instruction: >
      Do not use malloc, calloc, realloc, or free. Buffers must be static
      or stack-allocated with fixed maximum capacities at initialization.
    rationale: Prevents heap fragmentation and non-deterministic OOM faults.
    linter_check: "clang-tidy:bugprone-suspicious-memory-comparison,cert-mem50-cpp"

  - id: RULE-C11-002
    title: Fixed-Width Integers
    category: types
    severity: error
    instruction: >
      Exclusively use <stdint.h> types (uint8_t, int16_t, uint32_t, etc.).
      Do not use raw 'int', 'short', 'long', or 'char' for numeric quantities.
    linter_check: "clang-tidy:google-runtime-int"
```

### Rule Fields

- **`id`** — Unique identifier in the format `RULE-<LANG>-<NUMBER>` for precise citation in review findings and issues.
- **`title`** — Concise rule description.
- **`category`** — Classification: `memory`, `memory_safety`, `types`, `control_flow`, `error_handling`, `defensive_programming`, `concurrency`, `design`, `security`, `observability`, `resource_management`, `build`, `naming`, `style`, `documentation`, `testing`.
- **`severity`** — `error` (strict gate failure) or `warning` (justification required for deviations).
- **`instruction`** — Unambiguous directive for the 27B implementation model.
- **`rationale`** — Technical rationale (guides Frontier models when evaluating trade-offs).
- **`linter_check`** — (Optional) Exact compiler/linter check. If no 100% matching static check exists, the rule remains an explicit instruction and review check.

---

## Profile Integration

In domain profiles (`doc/embedded-safety.yaml`, `doc/enterprise.yaml`), rulesets are referenced via the `rulesets` list:

```yaml
profile: embedded-safety
rigor: ASIL-D

# --- Reusable Rulesets ---
rulesets:
  - doc/rules/c11-embedded-safety.yaml

# --- Project-Specific Overrides ---
coding_constraints:
  - Every @implements annotation must state the REQ-ID of the realized requirement.
  - Maximum call graph stack depth is 2048 bytes.
```

---

## Available Rulesets

- **[`c11-strict.yaml`](c11-strict.yaml)**: Strict, defensive C11 for general production backend services/CLI tools.
- **[`c11-embedded-safety.yaml`](c11-embedded-safety.yaml)**: Deterministic C11, zero heap, zero recursion, fixed-width types, MISRA/CERT-C alignment for safety-critical firmware (ASIL/SIL).
- **[`cpp20-core.yaml`](cpp20-core.yaml)**: Modern C++20, RAII, smart pointers, concepts, zero raw owning pointers. Allows exceptions for exceptional states.
- **[`cpp-strict.yaml`](cpp-strict.yaml)**: Strict C++ with Google C++ Style, RAII, CMake+vcpkg, **no exceptions in application core** (Result/Status error handling).
- **[`rust-safety.yaml`](rust-safety.yaml)**: Strict memory safety, mandatory `// SAFETY:` proofs for `unsafe` blocks, zero `unwrap()` in production paths.
- **[`java-enterprise.yaml`](java-enterprise.yaml)**: Java 17/21 Backend, immutability by default, records, null-safety via `Optional`.
- **[`java-google-style.yaml`](java-google-style.yaml)**: Formatting, naming, and Javadoc conforming to the [Google Java Style Guide](https://google.github.io/styleguide/javaguide.html).
