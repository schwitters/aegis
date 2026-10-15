use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub profile: String,
    pub lang: String,
    pub brownfield: bool,
    /// Project name for scaffolded workspaces (crate prefix); default: name of the root directory
    pub name: Option<String>,
}

/// Layered workspace of the web-enterprise profile: (layer, description, internal dependencies).
/// The layer policy in doc/web-enterprise.yaml (gate.architecture) must stay in sync with this table.
const WEB_LAYERS: [(&str, &str, &[&str]); 5] = [
    ("domain", "Entities, value objects and domain rules. Pure: no I/O, no async runtime, no framework types.", &[]),
    ("application", "Use cases and the ports (traits) they need for persistence and external systems.", &["domain"]),
    ("storage", "Adapters: SQLx repositories implementing the application ports, outbound clients.", &["domain", "application"]),
    ("api", "HTTP adapter: Axum routes, extractors, DTOs and templates; calls use cases only.", &["domain", "application"]),
    ("server", "Composition root: configuration, wiring of adapters to ports, startup and graceful shutdown.", &["domain", "application", "storage", "api"]),
];

/// Crate-name prefix from an explicit name or the directory name: lowercase, `[a-z0-9-]` only.
pub fn project_slug(explicit: Option<&str>, root: &Path) -> String {
    let raw = explicit
        .map(|s| s.to_string())
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "app".to_string());
    let mut slug = String::new();
    for c in raw.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() || slug.starts_with(|c: char| c.is_ascii_digit()) {
        format!("app-{}", slug).trim_end_matches('-').to_string()
    } else {
        slug
    }
}

fn member_manifest(name: &str, description: &str, deps: &[String], extra: &str) -> String {
    let mut out = format!(
        "[package]\nname = \"{name}\"\ndescription = \"{description}\"\nversion.workspace = true\nedition.workspace = true\n\
         rust-version.workspace = true\npublish.workspace = true\n{extra}\n[dependencies]\n"
    );
    if deps.is_empty() {
        out.push_str("# No workspace crates and no I/O or framework crates (enforced by `aegis gate`, stage architecture).\n");
    }
    for d in deps {
        out.push_str(&format!("{d}.workspace = true\n"));
    }
    out.push_str("\n[lints]\nworkspace = true\n");
    out
}

/// MkDocs site of one handbook. `<slug>` is replaced by the project name.
fn mkdocs_yml(title: &str, extra_plugins: &str, extra_markdown: &str) -> String {
    format!(
        "# Handbook site (RULE-RWE-100). Build: mkdocs build --strict -f <this file>; PDF via docs/build.sh.\n\
         site_name: <slug> {title}\n\
         docs_dir: content\n\
         site_dir: site\n\
         use_directory_urls: true\n\
         strict: true\n\
         theme:\n  name: material\n  font: false              # no Google Fonts (RULE-RWE-103)\n  language: en\n\
         \x20 features: [navigation.sections, navigation.indexes, search.highlight]\n\
         extra:\n  generator: false\n\
         {extra_markdown}\
         plugins:\n  - search\n{extra_plugins}\
         \x20 - print-site:            # must stay last; PDF source: site/print_page/index.html\n\
         \x20     add_cover_page: true\n      add_table_of_contents: true\n      add_print_site_banner: false\n"
    )
}

const DOCS_REQUIREMENTS: &str = "# Toolchain for the four handbooks (RULE-RWE-100). Pin exact versions (pip-compile) before the first release.
# PDF: headless Chromium prints site/print_page (docs/build.sh; set CHROME to the browser binary).
mkdocs>=1.6,<2          # MkDocs 2.0 removes the plugin system (Material team warning, 2026-02): stay on 1.x
mkdocs-material
mkdocs-print-site-plugin
mkdocs-static-i18n      # optional German edition of the user handbook
neoteroi-mkdocs         # API reference generated from openapi/ (technical handbook)
";

const DOCS_BUILD_SH: &str = r#"#!/usr/bin/env bash
# Builds the four handbooks (HTML + PDF) and the CSP hashes for the help sites served by the application.
# Output: docs/<book>/site (HTML; user -> /help, operator -> /admin-help), dist/docs/*.pdf
set -euo pipefail
cd "$(dirname "$0")/.."
CHROME="${CHROME:-chromium}"   # any Chromium/Chrome or Playwright's chrome-headless-shell
command -v "$CHROME" >/dev/null || { echo "build.sh: $CHROME not found (set CHROME=...)" >&2; exit 127; }
bash docs/collect-legal.sh
mkdir -p dist/docs
for book in user operator technical hosting; do
  mkdocs build --strict --quiet -f "docs/$book/mkdocs.yml"
  "$CHROME" --headless --no-sandbox --disable-gpu --no-pdf-header-footer \
    --print-to-pdf="dist/docs/<slug>-$book-manual.pdf" "file://$PWD/docs/$book/site/print_page/index.html"
done
for book in user operator; do
  aegis docs csp-hashes --site "docs/$book/site"      # -> docs/$book/site/csp-hashes.json (RULE-RWE-101)
done
"#;

const DOCS_COLLECT_LEGAL_SH: &str = r#"#!/usr/bin/env bash
# SBOM (CycloneDX) and third-party licenses (cargo-about) for the operator handbook (RULE-RWE-081, RULE-RWE-104).
# Verify the cargo-cyclonedx output file name for your installed version when the copy step fails.
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in cargo-cyclonedx cargo-about; do
  command -v "$tool" >/dev/null || { echo "collect-legal: $tool is not installed" >&2; exit 127; }
done
out=docs/operator/content/legal
mkdir -p "$out"
cargo cyclonedx --format json --all-features
cp crates/<slug>-server/*.cdx.json "$out/sbom.cdx.json"
cargo about generate --fail --manifest-path crates/<slug>-server/Cargo.toml -c about.toml \
  -o "$out/third-party-licenses.md" docs/about.hbs
"#;

const ABOUT_TOML: &str = "# cargo-about (RULE-RWE-104): accepted licenses — keep in sync with deny.toml [licenses].allow\n\
accepted = [\n  \"MIT\", \"Apache-2.0\", \"Apache-2.0 WITH LLVM-exception\", \"BSD-2-Clause\", \"BSD-3-Clause\", \"ISC\", \"Zlib\",\n\
  \"Unicode-3.0\", \"Unicode-DFS-2016\", \"CC0-1.0\", \"BSL-1.0\", \"MPL-2.0\", \"CDLA-Permissive-2.0\",\n]\n";

const ABOUT_HBS: &str = r#"---
title: Third-Party Licenses
---
<!-- Generated by cargo-about from docs/about.hbs by docs/collect-legal.sh. Do not edit (RULE-RWE-102, -104). -->
# Third-Party Licenses

This product includes the following third-party software components.

{{#each overview}}
- **{{name}}** ({{id}}) — {{count}} component(s)
{{/each}}

{{#each licenses}}
## {{name}}

Used by:

{{#each used_by}}
- {{crate.name}} {{crate.version}}
{{/each}}

```text
{{{text}}}
```

{{/each}}
"#;

fn write_book_page(dir: &Path, rel: &str, content: &str, slug: &str) -> Result<()> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_if_missing(&path, &content.replace("<slug>", slug))
}

/// Scaffolds the four handbook sites under docs/ (RULE-RWE-100..104), only files that do not exist yet.
fn scaffold_docs(root: &Path, slug: &str) -> Result<()> {
    let docs = root.join("docs");
    let i18n = "  # Optional German edition (user handbook only): add page.de.md files and enable:\n\
                \x20 # - i18n:\n  #     docs_structure: suffix\n  #     languages:\n\
                \x20 #       - { locale: en, default: true, name: English, build: true }\n\
                \x20 #       - { locale: de, name: Deutsch, build: true }\n";
    let oad = "markdown_extensions:\n  - admonition\n  - tables\n";
    let books: [(&str, &str, &str, &str); 4] = [
        ("user", "User Guide", i18n, ""),
        ("operator", "Administrator Guide", "", ""),
        ("technical", "Technical & Integration Guide", "  - neoteroi.mkdocsoad:\n      use_pymdownx: false\n", oad),
        ("hosting", "Hosting & Setup Guide", "", ""),
    ];
    for (book, title, plugins, md) in books {
        let dir = docs.join(book);
        fs::create_dir_all(dir.join("content"))?;
        write_if_missing(&dir.join("mkdocs.yml"), &mkdocs_yml(title, plugins, md).replace("<slug>", slug))?;
    }
    write_book_page(&docs.join("user"), "content/index.md",
        "# <slug> User Guide\n\nServed by the application under `/help`. Every use case with `audience: user` is documented by a page\nwith `documents: [UC-xxx]` in its frontmatter (RULE-RWE-102).\n", slug)?;
    write_book_page(&docs.join("operator"), "content/index.md",
        "# <slug> Administrator Guide\n\nServed under `/admin-help`, visible to administrators only (RULE-RWE-101). Documents every use case with\n`audience: operator`: user and tenant management, settings, audit log, data subject requests.\n", slug)?;
    write_book_page(&docs.join("operator"), "content/legal/index.md",
        "# Software Bill of Materials & Licenses\n\n- [Third-party licenses](third-party-licenses.md) — generated by cargo-about\n- [SBOM (CycloneDX JSON)](sbom.cdx.json) — generated by cargo-cyclonedx\n\nBoth are regenerated for every release by `docs/collect-legal.sh` (RULE-RWE-104).\n", slug)?;
    write_book_page(&docs.join("technical"), "content/index.md",
        "# <slug> Technical & Integration Guide\n\nArchitecture (one chapter per layer crate / ARCH element), integration, data model, API reference.\n", slug)?;
    write_book_page(&docs.join("technical"), "content/api.md",
        "# API Reference\n\nGenerated from the OpenAPI contract at build time (RULE-RWE-093, RULE-RWE-102). Do not edit by hand.\n\n[OAD(./openapi/<slug>-openapi.yaml)]\n", slug)?;
    write_book_page(&docs.join("hosting"), "content/index.md",
        "# <slug> Hosting & Setup Guide\n\nInstallation, reverse proxy and TLS, configuration, backup and restore, upgrade, monitoring.\n", slug)?;
    write_book_page(&docs.join("hosting"), "content/configuration.md",
        "# Configuration Reference\n\nEvery configuration key of the server crate is listed here; the test `config_keys_documented`\nfails when a key is missing (RULE-RWE-102). Use placeholders for all credentials (RULE-RWE-103).\n\n| Key | Required | Default | Description |\n|---|---|---|---|\n", slug)?;
    write_if_missing(&docs.join("requirements.txt"), DOCS_REQUIREMENTS)?;
    write_if_missing(&docs.join("about.hbs"), ABOUT_HBS)?;
    for (name, content) in [("build.sh", DOCS_BUILD_SH), ("collect-legal.sh", DOCS_COLLECT_LEGAL_SH)] {
        let path = docs.join(name);
        write_if_missing(&path, &content.replace("<slug>", slug))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }
    }
    write_if_missing(&root.join("about.toml"), ABOUT_TOML)?;
    Ok(())
}

/// Personal-data wrapper for the domain crate (RULE-RWE-070).
const PII_RS: &str = r#"//! Personal data wrapper (RULE-RWE-070): `Debug` and `Display` never print the value.

use std::fmt;

/// A personal value (name, e-mail, phone, address, IP address). Read it only via [`Pii::expose`] at the point of
/// use (view model, e-mail delivery) — never in logs, traces or error messages.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pii<T>(T);

impl<T> Pii<T> {
    /// Wraps a personal value.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// The wrapped value. Do not log, trace or format the result.
    pub const fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Pii<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[PII]")
    }
}

impl<T> fmt::Display for Pii<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[PII]")
    }
}

#[cfg(test)]
mod tests {
    use super::Pii;

    #[test]
    fn debug_and_display_are_redacted() {
        let email = Pii::new("jane@example.org".to_string());
        assert_eq!(format!("{email:?}"), "[PII]");
        assert_eq!(format!("{email}"), "[PII]");
        assert_eq!(email.expose(), "jane@example.org");
    }
}
"#;

/// Scaffolds the layered Cargo workspace (only when the root has no Cargo.toml yet).
fn scaffold_rust_web_workspace(root: &Path, slug: &str) -> Result<()> {
    let crate_name = |layer: &str| format!("{slug}-{layer}");
    let mut root_manifest = String::from(
        "# Layered workspace (Aegis web-enterprise). The crate graph is the architecture: a crate can only use what it\n\
         # declares, and `aegis gate` (stage architecture) rejects declarations that break the layer policy.\n\
         [workspace]\nresolver = \"3\"\nmembers = [\"crates/*\", \"tests\"]\n\n\
         [workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\npublish = false\n\n\
         [workspace.dependencies]\n"
    );
    for (layer, _, _) in WEB_LAYERS {
        root_manifest.push_str(&format!("{} = {{ path = \"crates/{}\" }}\n", crate_name(layer), crate_name(layer)));
    }
    root_manifest.push_str("# Third-party versions are pinned here once; members use `<crate>.workspace = true`.\n\n");
    let lints_start = TEMPLATE_RUST_LINTS.find("[workspace.lints.rust]").unwrap_or(0);
    root_manifest.push_str(&TEMPLATE_RUST_LINTS[lints_start..]);
    fs::write(root.join("Cargo.toml"), root_manifest).context("Failed to write Cargo.toml")?;

    for (layer, description, deps) in WEB_LAYERS {
        let dir = root.join("crates").join(crate_name(layer));
        fs::create_dir_all(dir.join("src"))?;
        let deps: Vec<String> = deps.iter().map(|d| crate_name(d)).collect();
        fs::write(dir.join("Cargo.toml"), member_manifest(&crate_name(layer), description, &deps, ""))?;
        let header = format!("//! {description}\n\n#![forbid(unsafe_code)]\n");
        if layer == "server" {
            fs::write(dir.join("src/main.rs"), format!("{header}\nfn main() {{}}\n"))?;
        } else if layer == "domain" {
            fs::write(dir.join("src/lib.rs"), format!("{header}\npub mod pii;\n"))?;
            fs::write(dir.join("src/pii.rs"), PII_RS)?;
        } else {
            fs::write(dir.join("src/lib.rs"), header)?;
        }
    }

    // Integration tests: a virtual workspace does not build a root tests/ directory, so tests/ is a member crate.
    let all: Vec<String> = WEB_LAYERS.iter().map(|(l, _, _)| crate_name(l)).collect();
    let tests_extra = "\n[lib]\npath = \"lib.rs\"\n\n[[test]]\nname = \"it\"\npath = \"it/main.rs\"\n";
    fs::create_dir_all(root.join("tests/it"))?;
    fs::write(
        root.join("tests/Cargo.toml"),
        member_manifest(&crate_name("tests"), "Integration and HTTP tests across all layers.", &all, tests_extra),
    )?;
    fs::write(root.join("tests/lib.rs"), "//! Shared fixtures for the integration tests.\n\n#![forbid(unsafe_code)]\n")?;
    fs::write(
        root.join("tests/it/main.rs"),
        "//! Integration tests: one module per feature, each test annotated with `@verifies` and frozen\n\
         //! in doc/testplan/tests.lock before the implementation starts.\n",
    )?;

    fs::create_dir_all(root.join("migrations"))?;
    write_if_missing(&root.join("migrations/.gitkeep"), "")?;
    fs::create_dir_all(root.join("openapi"))?;
    write_if_missing(
        &root.join(format!("openapi/{slug}-openapi.yaml")),
        &format!(
            "# Contract of the JSON API (source of truth, RULE-RWE-093). Server-rendered-only apps keep it minimal.\n\
             openapi: 3.1.0\ninfo:\n  title: {slug} API\n  version: 0.1.0\npaths: {{}}\n"
        ),
    )?;
    write_if_missing(
        &root.join(".gitignore"),
        "/target\n/dist\ndocs/*/site/\ndocs/operator/content/legal/sbom.cdx.json\ndocs/operator/content/legal/third-party-licenses.md\n",
    )?;
    Ok(())
}

// Embed rulesets so aegis init works standalone anywhere
const RULESET_RUST: &str = include_str!("../../doc/rules/rust-safety.yaml");
const RULESET_C11_STRICT: &str = include_str!("../../doc/rules/c11-strict.yaml");
const RULESET_C11_SAFETY: &str = include_str!("../../doc/rules/c11-embedded-safety.yaml");
const RULESET_CPP_STRICT: &str = include_str!("../../doc/rules/cpp-strict.yaml");
const RULESET_CPP20_CORE: &str = include_str!("../../doc/rules/cpp20-core.yaml");
const RULESET_JAVA_ENTERPRISE: &str = include_str!("../../doc/rules/java-enterprise.yaml");
const RULESET_JAVA_GOOGLE: &str = include_str!("../../doc/rules/java-google-style.yaml");
const RULESET_RUST_STRICT: &str = include_str!("../../doc/rules/rust-strict.yaml");
const RULESET_RUST_SERVICE: &str = include_str!("../../doc/rules/rust-service.yaml");
const RULESET_RUST_WEB: &str = include_str!("../../doc/rules/rust-web-enterprise.yaml");
const RULESET_RUST_WEB_HTMX: &str = include_str!("../../doc/rules/rust-web-htmx.yaml");
const RULESET_RUST_WEB_PWA: &str = include_str!("../../doc/rules/rust-web-pwa.yaml");
const PROFILE_WEB: &str = include_str!("../../doc/web-enterprise.yaml");
const TEMPLATE_WEB_CLIPPY: &str = include_str!("../../templates/rust-web/clippy.toml");
const TEMPLATE_WEB_DENY: &str = include_str!("../../templates/rust-web/deny.toml");
const TEMPLATE_RUST_LINTS: &str = include_str!("../../templates/rust/workspace-lints.toml");

fn write_if_missing(path: &Path, content: &str) -> Result<()> {
    if !path.exists() {
        fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

pub fn run_init(root: &Path, opts: InitOptions) -> Result<()> {
    if !matches!(opts.profile.as_str(), "enterprise" | "embedded-safety" | "safety" | "web-enterprise" | "web") {
        anyhow::bail!(
            "aegis init scaffolds the profiles web-enterprise, enterprise and embedded-safety. For `{}`, run init with \
             --profile enterprise and copy doc/{}.yaml plus the rulesets it lists from the Aegis repository.",
            opts.profile,
            opts.profile
        );
    }
    let doc_dir = root.join("doc");
    let req_dir = doc_dir.join("requirements");
    let arch_dir = doc_dir.join("architecture");
    let reviews_dir = doc_dir.join("reviews");
    let uc_dir = doc_dir.join("usecases");
    let test_dir = doc_dir.join("testplan");
    let issues_dir = doc_dir.join("issues");
    let deliv_dir = doc_dir.join("deliverables");
    let rules_dir = doc_dir.join("rules");

    fs::create_dir_all(&req_dir).context("Failed to create doc/requirements")?;
    fs::create_dir_all(&arch_dir).context("Failed to create doc/architecture")?;
    fs::create_dir_all(&reviews_dir).context("Failed to create doc/reviews")?;
    fs::create_dir_all(&uc_dir).context("Failed to create doc/usecases")?;
    fs::create_dir_all(&test_dir).context("Failed to create doc/testplan")?;
    fs::create_dir_all(&issues_dir).context("Failed to create doc/issues")?;
    fs::create_dir_all(&deliv_dir).context("Failed to create doc/deliverables")?;
    fs::create_dir_all(&rules_dir).context("Failed to create doc/rules")?;

    let lang = opts.lang.to_lowercase();
    let is_safety = opts.profile == "embedded-safety" || opts.profile == "safety";
    let is_web = opts.profile == "web-enterprise" || opts.profile == "web";
    if is_web && lang != "rust" {
        anyhow::bail!("Profile web-enterprise is available for --lang rust only");
    }

    // 0. Web profile: complete Rust web setup (rulesets, profile, clippy/deny templates)
    if is_web {
        write_if_missing(&rules_dir.join("rust-strict.yaml"), RULESET_RUST_STRICT)?;
        write_if_missing(&rules_dir.join("rust-service.yaml"), RULESET_RUST_SERVICE)?;
        write_if_missing(&rules_dir.join("rust-web-enterprise.yaml"), RULESET_RUST_WEB)?;
        // optional overlay, enabled in doc/profile.yaml when the frontend uses htmx
        write_if_missing(&rules_dir.join("rust-web-htmx.yaml"), RULESET_RUST_WEB_HTMX)?;
        // optional overlay, enabled in doc/profile.yaml for installable / offline-capable frontends
        write_if_missing(&rules_dir.join("rust-web-pwa.yaml"), RULESET_RUST_WEB_PWA)?;
        write_if_missing(&doc_dir.join("profile.yaml"), PROFILE_WEB)?;
        write_if_missing(&root.join("clippy.toml"), TEMPLATE_WEB_CLIPPY)?;
        write_if_missing(&root.join("deny.toml"), TEMPLATE_WEB_DENY)?;
        write_if_missing(&rules_dir.join("workspace-lints.toml"), TEMPLATE_RUST_LINTS)?;
        fs::create_dir_all(doc_dir.join("privacy")).context("Failed to create doc/privacy")?;
        let slug = project_slug(opts.name.as_deref(), root);
        if !root.join("Cargo.toml").exists() {
            scaffold_rust_web_workspace(root, &slug)?;
        }
        scaffold_docs(root, &slug)?;
    }

    // 1. Copy Embedded Rulesets into target doc/rules/
    match lang.as_str() {
        _ if is_web => {}
        "rust" => {
            let p = rules_dir.join("rust-safety.yaml");
            if !p.exists() {
                fs::write(&p, RULESET_RUST)?;
            }
        }
        "c" | "c11" => {
            let p1 = rules_dir.join("c11-strict.yaml");
            let p2 = rules_dir.join("c11-embedded-safety.yaml");
            if !p1.exists() {
                fs::write(&p1, RULESET_C11_STRICT)?;
            }
            if !p2.exists() {
                fs::write(&p2, RULESET_C11_SAFETY)?;
            }
        }
        "cpp" | "cpp20" | "c++" => {
            let p1 = rules_dir.join("cpp-strict.yaml");
            let p2 = rules_dir.join("cpp20-core.yaml");
            if !p1.exists() {
                fs::write(&p1, RULESET_CPP_STRICT)?;
            }
            if !p2.exists() {
                fs::write(&p2, RULESET_CPP20_CORE)?;
            }
        }
        "java" => {
            let p1 = rules_dir.join("java-enterprise.yaml");
            let p2 = rules_dir.join("java-google-style.yaml");
            if !p1.exists() {
                fs::write(&p1, RULESET_JAVA_ENTERPRISE)?;
            }
            if !p2.exists() {
                fs::write(&p2, RULESET_JAVA_GOOGLE)?;
            }
        }
        _ => {
            let p = rules_dir.join(format!("{}-rules.yaml", lang));
            if !p.exists() {
                fs::write(&p, RULESET_RUST)?;
            }
        }
    }

    // 2. Deliverables Manifest
    let manifest_path = deliv_dir.join("manifest.md");
    if !manifest_path.exists() {
        let manifest_content = r#"# Deliverables Manifest

Required deliverables for gate verification:

- path: doc/traceability.md
- path: README.md
"#;
        fs::write(&manifest_path, manifest_content)?;
    }

    // 3. Profile YAML
    let profile_path = doc_dir.join("profile.yaml");
    if !profile_path.exists() {
        let profile_content = if is_safety {
            let ruleset_name = if lang == "c" || lang == "c11" { "c11-embedded-safety.yaml" } else { "rust-safety.yaml" };
            format!(
                r#"# Domain Profile: Safety-Critical Systems
profile: embedded-safety
rigor: ASIL-D
language: {lang}

rulesets:
  - doc/rules/{ruleset_name}

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
  security:
    active: true
    checks: [dependency_audit]
  test_freeze:
    active: true
  coverage:
    active: true
    metric: mcdc
    threshold: 100
  mutation:
    active: true
    threshold: 90
  traceability:
    active: true
    bidirectional: true
    baseline: required
  deliverables:
    active: true
  reviews:
    active: true
    required: [spec, requirements, "architecture:2", testplan, implementation]
agent_loop:
  max_iterations: 3
  fast_stages: [build, lint, test_freeze, tests]

coding_constraints:
  - Zero dynamic memory allocation at runtime.
  - Zero recursion (statically bounded stack depth).
  - Every @implements annotation must state the realized REQ-ID.
"#,
                lang = &lang,
                ruleset_name = ruleset_name
            )
        } else {
            let ruleset_name = match lang.as_str() {
                "c" | "c11" => "c11-strict.yaml",
                "cpp" | "cpp20" | "c++" => "cpp-strict.yaml",
                "java" => "java-enterprise.yaml",
                _ => "rust-safety.yaml",
            };
            format!(
                r#"# Domain Profile: High-Integrity Systems Engineering
profile: enterprise
rigor: none
language: {lang}

rulesets:
  - doc/rules/{ruleset_name}

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
    checks: [dependency_audit]
  test_freeze:
    active: true
  tests:
    active: true
  coverage:
    active: true
    metric: line
    threshold: 80
  mutation:
    active: true
    threshold: 70
  traceability:
    active: true
    baseline: required
  deliverables:
    active: true
  reviews:
    active: true
    required: [spec, requirements, "architecture:2", implementation]
agent_loop:
  max_iterations: 3
  fast_stages: [build, lint, test_freeze, tests]

coding_constraints:
  - Explicit error handling with strongly typed domain errors.
  - Zero hardcoded credentials or secrets in source code.
  - Every @implements annotation must state the realized REQ-ID.
"#,
                lang = &lang,
                ruleset_name = ruleset_name
            )
        };

        fs::write(&profile_path, profile_content)?;
    }

    // 4. Create root gate.sh wrapper if not present
    let gate_sh_path = root.join("gate.sh");
    if !gate_sh_path.exists() {
        let gate_sh_content = r#"#!/usr/bin/env bash
set -euo pipefail

# 🛡️ Aegis Gate Wrapper
AEGIS_BIN=""
if command -v aegis >/dev/null 2>&1; then
    AEGIS_BIN="aegis"
elif [ -f "tools/aegis/target/release/aegis" ]; then
    AEGIS_BIN="tools/aegis/target/release/aegis"
elif [ -f "aegis/target/release/aegis" ]; then
    AEGIS_BIN="aegis/target/release/aegis"
elif [ -f "tools/target/release/aegis" ]; then
    AEGIS_BIN="tools/target/release/aegis"
elif [ -f "target/release/aegis" ]; then
    AEGIS_BIN="target/release/aegis"
else
    if [ -f "tools/aegis/Cargo.toml" ]; then
        echo "Building aegis subproject in tools/aegis..."
        cargo build --release --manifest-path tools/aegis/Cargo.toml
        AEGIS_BIN="tools/aegis/target/release/aegis"
    elif [ -f "aegis/Cargo.toml" ]; then
        echo "Building aegis subproject in aegis/..."
        cargo build --release --manifest-path aegis/Cargo.toml
        AEGIS_BIN="aegis/target/release/aegis"
    fi
fi

if [ -z "$AEGIS_BIN" ]; then
    echo "Error: aegis binary not found. Please bind aegis as a subproject or install it." >&2
    exit 1
fi

exec "$AEGIS_BIN" gate --profile doc/profile.yaml "$@"
"#;
        fs::write(&gate_sh_path, gate_sh_content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&gate_sh_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&gate_sh_path, perms)?;
        }
    }

    // 5. Initial Spec or Brownfield Reverse-Engineering Prompt
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

Also extract the corresponding Use Cases in `doc/usecases/UC-XXX.md` with `implements: [REQ-XXX]`, and the
architecture elements (components) in `doc/architecture/ARCH-XXX.md` with `satisfies: [REQ-XXX]`.
```
"#,
                lang_upper = opts.lang.to_uppercase()
            );
            fs::write(&prompt_path, prompt_content)?;
        }

        let baseline_req = req_dir.join("REQ-001.md");
        if !baseline_req.exists() {
            let req_content = r#"---
id: REQ-001
title: Baseline System Core Initialization
status: active
type: functional
iso25010: functional_suitability
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
id: UC-001
title: System Startup Sequence
implements: [REQ-001]
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

        let baseline_arch = arch_dir.join("ARCH-001.md");
        if !baseline_arch.exists() {
            let arch_content = r#"---
id: ARCH-001
title: Core Runtime & Startup
satisfies: [REQ-001]
---

## Responsibility
Owns configuration loading, component wiring and the startup sequence.

## Interfaces
- Public entry point and configuration schema (to be extracted from the existing code).

## Integration Verification
An integration test (`TEST-xxx` with `verifies: [ARCH-001]`) starts the system with a valid configuration.
"#;
            fs::write(&baseline_arch, arch_content)?;
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
    println!("  Ruleset:   {}", rules_dir.display().to_string().yellow());
    println!("  Gate script: {}", root.join("gate.sh").display().to_string().green());
    if is_web {
        println!();
        println!("{}", "Web Profile Setup:".bold());
        println!("  clippy.toml and deny.toml written to the repository root (if missing).");
        let slug = project_slug(opts.name.as_deref(), root);
        if root.join("crates").join(format!("{slug}-domain")).exists() {
            println!("  Layered workspace: crates/{slug}-{{domain,application,storage,api,server}}, tests/, migrations/, openapi/");
        } else {
            println!(
                "  Existing Cargo.toml kept: merge {} ([workspace.lints]) and align crate names with gate.architecture.layers.",
                rules_dir.join("workspace-lints.toml").display().to_string().cyan()
            );
        }
        println!("  Create doc/privacy/data-inventory.yaml with the first migration (RULE-RWE-071).");
        println!("  Handbooks: docs/{{user,operator,technical,hosting}} (MkDocs; pip install -r docs/requirements.txt; docs/build.sh).");
        println!("  htmx frontend? Enable doc/rules/rust-web-htmx.yaml under `rulesets:` in doc/profile.yaml.");
        println!("  SPA/PWA frontend? Add its build/lint/test commands to the gate stages (see comments in the profile);");
        println!("  for a PWA also enable doc/rules/rust-web-pwa.yaml.");
    }
    if opts.brownfield {
        println!();
        println!("{}", "Brownfield Migration Artifacts Created:".bold());
        println!("  1. Prompt template: {}", doc_dir.join("REVERSE_SPEC_EXTRACTION_PROMPT.md").display().to_string().cyan());
        println!(
            "  2. Baseline REQ/UC/ARCH: {}, {} & {}",
            req_dir.join("REQ-001.md").display(),
            uc_dir.join("UC-001.md").display(),
            arch_dir.join("ARCH-001.md").display()
        );
        println!("  3. Next step: Run prompt in Frontier model, tag @implements in code, then run: ./gate.sh");
    } else {
        println!();
        println!("{}", "Greenfield Next Steps:".bold());
        println!("  1. Refine specification in {}", doc_dir.join("001_initial_spec.md").display().to_string().cyan());
        println!("  2. Run blind review with Frontier Model B; record it: aegis review create --stage spec ...");
        println!("  3. Derive requirements, architecture (ARCH-xxx), use cases and test plan; review; aegis baseline --approve <you>");
        println!("  4. Implement and review the tests, then freeze them: aegis tests freeze");
        println!("  5. Synthesize instructions: aegis instructions --out AGENT_INSTRUCTIONS.md");
        println!("  6. Implementation loop (fast gate), full gate, final review, acceptance gate: ./gate.sh");
    }

    Ok(())
}
