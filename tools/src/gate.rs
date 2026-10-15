use crate::architecture;
use crate::baseline;
use crate::deviation;
use crate::issue;
use crate::profile::Profile;
use crate::review;
use crate::rulesets::{self, RulesetFile};
use crate::testfreeze;
use crate::traceability::{self, Trace};
use anyhow::{bail, Result};
use colored::Colorize;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, PartialEq)]
pub enum StageResult {
    Inactive,
    /// Deselected via `--only` / `--skip`: the run is partial and not an acceptance
    Skipped,
    Success,
    NotExecutable(String),
    Failed(i32),
}

pub struct GateRunner {
    pub root: PathBuf,
    pub profile: Profile,
    pub results: Vec<(String, StageResult)>,
    pub only: Vec<String>,
    pub skip: Vec<String>,
    /// Rulesets referenced by the profile (toolchain.gate_commands, clippy flags, deviation policy)
    pub rulesets: Vec<RulesetFile>,
    /// Set when a referenced ruleset is missing, unparsable or declares unknown gate stages: the gate is red.
    pub ruleset_error: Option<String>,
}

/// Execution order. Cheap deterministic checks first; reviews last (the final review happens on a green gate).
pub const STAGES: [(&str, &str); 14] = [
    ("Build", "build"),
    ("Architecture", "architecture"),
    ("Lint", "lint"),
    ("Static Analysis", "static_analysis"),
    ("Security Scan", "security"),
    ("Test Freeze", "test_freeze"),
    ("Tests", "tests"),
    ("Coverage", "coverage"),
    ("Mutation Testing", "mutation"),
    ("Load Test", "load_test"),
    ("Traceability", "traceability"),
    ("Issues", "issues"),
    ("Deliverables", "deliverables"),
    ("Reviews", "reviews"),
];

/// Stages implemented by Aegis itself; a `command` override would bypass their checks.
const INTERNAL_STAGES: [&str; 6] = ["architecture", "test_freeze", "traceability", "issues", "deliverables", "reviews"];

/// Splits a command line into words, honouring single and double quotes (enough to find the program name).
fn shell_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// The executable a shell command line needs, skipping `!` and leading VAR=value assignments; `cargo <sub>` maps to
/// the `cargo-<sub>` plugin for subcommands that are not built into cargo. `None` for shell builtins.
pub fn command_tool(cmd: &str) -> Option<String> {
    let words = shell_words(cmd);
    let mut it = words
        .iter()
        .skip_while(|w| *w == "!" || (w.contains('=') && !w.starts_with('-')));
    let first = it.next()?;
    match first.as_str() {
        "test" | "[" | "[[" | "true" | "false" | "echo" | "printf" | "cd" | "exit" | "export" | "set" | "command"
        | "source" | "." | ":" | "(" | "{" | "if" | "for" | "while" | "case" => None,
        "cargo" => match it.next().map(|s| s.as_str()) {
            None | Some("build" | "check" | "test" | "doc" | "run" | "bench" | "metadata" | "tree" | "fetch") => {
                Some("cargo".to_string())
            }
            Some(sub) if sub.starts_with('-') || sub.starts_with('+') => Some("cargo".to_string()),
            Some(sub) => Some(format!("cargo-{}", sub)),
        },
        w => Some(w.to_string()),
    }
}

fn pass_if_zero(rc: Option<i32>) -> StageResult {
    if rc == Some(0) {
        StageResult::Success
    } else {
        StageResult::Failed(rc.unwrap_or(1))
    }
}

fn command_exists(cmd: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let full = dir.join(cmd);
            if full.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = full.metadata() {
                        if meta.permissions().mode() & 0o111 != 0 {
                            return true;
                        }
                    }
                }
                #[cfg(not(unix))]
                return true;
            }
        }
    }
    false
}

impl GateRunner {
    pub fn new(root: PathBuf, profile: Profile) -> Self {
        let (rulesets, ruleset_error) = match rulesets::load_referenced(&root, &profile) {
            Ok(rs) => {
                let unknown: Vec<String> = rs
                    .iter()
                    .flat_map(|rf| {
                        rf.toolchain
                            .iter()
                            .flat_map(|t| t.gate_commands.keys())
                            .filter(|k| !STAGES.iter().any(|(_, s)| s == k))
                            .map(move |k| format!("{}: toolchain.gate_commands.{}", rf.ruleset, k))
                    })
                    .collect();
                if unknown.is_empty() {
                    (rs, None)
                } else {
                    (rs, Some(format!("unknown gate stage(s): {}", unknown.join(", "))))
                }
            }
            Err(e) => (Vec::new(), Some(format!("{:#}", e))),
        };
        Self {
            root,
            profile,
            results: Vec::new(),
            only: Vec::new(),
            skip: Vec::new(),
            rulesets,
            ruleset_error,
        }
    }

    /// Restricts the run to a subset of stages. Unknown stage names are rejected.
    pub fn with_selection(mut self, only: Vec<String>, skip: Vec<String>) -> Result<Self> {
        for name in only.iter().chain(skip.iter()) {
            if !STAGES.iter().any(|(_, k)| k == name) {
                let known: Vec<&str> = STAGES.iter().map(|(_, k)| *k).collect();
                bail!("Unknown gate stage `{}` (known: {})", name, known.join(", "));
            }
        }
        self.only = only;
        self.skip = skip;
        Ok(self)
    }

    fn is_selected(&self, key: &str) -> bool {
        (self.only.is_empty() || self.only.iter().any(|s| s == key)) && !self.skip.iter().any(|s| s == key)
    }

    fn run_cmd(&self, program: &str, args: &[&str]) -> Option<i32> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(&self.root);
        cmd.status().ok().and_then(|s| s.code())
    }

    fn run_bash(&self, script: &str) -> Option<i32> {
        let mut cmd = Command::new("bash");
        cmd.args(["-c", script]);
        cmd.current_dir(&self.root);
        cmd.status().ok().and_then(|s| s.code())
    }

    fn lang(&self) -> &str {
        self.profile.language.as_deref().unwrap_or("")
    }

    pub fn run_all(&mut self) -> Result<bool> {
        let root_str = self.root.display().to_string();
        let prof_name = &self.profile.profile;
        let rigor = self.profile.rigor.as_deref().unwrap_or("—");
        let lang = self.profile.language.as_deref().unwrap_or("not set");

        println!("{}", format!("Execution Gate — {}", root_str).bold());
        println!(
            "{}",
            format!("Profile: {} (Rigor: {}, Language: {})", prof_name, rigor, lang).bold()
        );
        println!();

        if let Some(err) = self.ruleset_error.clone() {
            println!("{}", "▶ Rulesets".bold());
            println!("  {}", format!("✘ Rulesets ({})", err).red());
            println!();
            self.results.push(("Rulesets".to_string(), StageResult::NotExecutable(err)));
            self.print_summary(false);
            return Ok(false);
        }

        for (display_name, key) in STAGES {
            let res = self.execute_stage(display_name, key)?;
            let failed = matches!(res, StageResult::NotExecutable(_) | StageResult::Failed(_));
            self.results.push((display_name.to_string(), res));

            if failed {
                self.print_summary(false);
                return Ok(false);
            }
        }

        self.print_summary(true);
        Ok(true)
    }

    fn execute_stage(&self, display_name: &str, key: &str) -> Result<StageResult> {
        if !self.is_selected(key) {
            return Ok(StageResult::Skipped);
        }

        // "issues" is active if doc/issues/ exists or is explicitly set in profile
        if key == "issues" {
            let issues_dir = self.root.join("doc/issues");
            let explicitly_active = self.profile.is_stage_active("issues");
            if !explicitly_active && !issues_dir.exists() {
                println!("{}", format!("▶ {}", display_name).bold());
                println!("  {}", format!("∅ {} (inactive in profile)", display_name).yellow());
                println!();
                return Ok(StageResult::Inactive);
            }
        } else if !self.profile.is_stage_active(key) {
            println!("{}", format!("▶ {}", display_name).bold());
            println!("  {}", format!("∅ {} (inactive in profile)", display_name).yellow());
            println!();
            return Ok(StageResult::Inactive);
        }

        println!("{}", format!("▶ {}", display_name).bold());

        let override_cmd = self.profile.get_stage_config(key).and_then(|c| c.command.clone());
        let res = match override_cmd {
            Some(_) if INTERNAL_STAGES.contains(&key) => {
                StageResult::NotExecutable(format!("`command` override is not allowed for internal stage {}", key))
            }
            Some(cmd) => {
                println!("    $ {}", cmd);
                pass_if_zero(self.run_bash(&cmd))
            }
            None => self.run_builtin(key),
        };
        // Ruleset additions (deviation scan, toolchain.gate_commands) run on top of the stage's own check.
        let res = if res == StageResult::Success { self.ruleset_checks(key) } else { res };

        match &res {
            StageResult::Success => {
                println!("  {}", format!("✔ {}", display_name).green());
                println!();
            }
            StageResult::NotExecutable(reason) => {
                let note = if reason.is_empty() {
                    "active stage not executable".to_string()
                } else {
                    format!("active stage not executable: {}", reason)
                };
                println!("  {}", format!("✘ {} ({})", display_name, note).red());
                println!();
            }
            StageResult::Failed(rc) => {
                println!("  {}", format!("✘ {} (rc={})", display_name, rc).red());
                println!();
            }
            StageResult::Inactive | StageResult::Skipped => {}
        }

        Ok(res)
    }

    /// Checks contributed by the profile's rulesets for one stage, after the stage itself passed.
    fn ruleset_checks(&self, key: &str) -> StageResult {
        if key == "lint" && self.rulesets.iter().any(|rf| rf.deviation_policy.is_some()) {
            let table = deviation::rule_table(&self.rulesets);
            let findings = deviation::scan_dirs(&self.profile.source_paths(&self.root), &self.root, self.lang(), &table);
            if !findings.is_empty() {
                for f in &findings {
                    println!("    {}", format!("DEVIATION {}: {}", f.location, f.message).red());
                }
                return StageResult::Failed(1);
            }
            println!("    deviation scan: all suppressions covered by permitted AEGIS-DEVIATION markers");
        }
        for rf in &self.rulesets {
            for cmd in rf.gate_commands(key) {
                let tool = command_tool(&cmd);
                if let Some(t) = tool.as_deref() {
                    if !command_exists(t) {
                        return StageResult::NotExecutable(format!("{} (from {}) not installed: {}", t, rf.ruleset, cmd));
                    }
                }
                println!("    $ {}   [{}]", cmd, rf.ruleset);
                let res = pass_if_zero(self.run_bash(&cmd));
                if res != StageResult::Success {
                    return res;
                }
            }
        }
        StageResult::Success
    }

    /// Clippy invocation: `-D warnings` plus the extra flags the rulesets declare (deduplicated).
    fn clippy_command(&self) -> String {
        let mut extra: Vec<String> = Vec::new();
        for rf in &self.rulesets {
            let flags = rf.clippy_flags();
            // v1 rulesets spell cargo args and lint args in one list: keep only what follows `--`.
            let lint_flags: Vec<String> = match flags.iter().position(|f| f == "--") {
                Some(i) => flags[i + 1..].to_vec(),
                None => flags,
            };
            let joined = lint_flags.join(" ");
            if !joined.is_empty() && joined != "-D warnings" && !extra.contains(&joined) {
                extra.push(joined);
            }
        }
        let mut cmd = "cargo clippy --all-targets --all-features -- -D warnings".to_string();
        for e in extra {
            cmd.push(' ');
            cmd.push_str(&e);
        }
        cmd
    }

    fn run_builtin(&self, key: &str) -> StageResult {
        match key {
            "build" => self.stage_build(),
            "architecture" => self.stage_architecture(),
            "lint" => self.stage_lint(),
            "static_analysis" => self.stage_static(),
            "security" => self.stage_security(),
            "test_freeze" => self.stage_test_freeze(),
            "tests" => self.stage_tests(),
            "coverage" => self.stage_coverage(),
            "mutation" => self.stage_mutation(),
            "load_test" => self.stage_load(),
            "traceability" => self.stage_traceability(),
            "issues" => self.stage_issues(),
            "deliverables" => self.stage_deliverables(),
            "reviews" => self.stage_reviews(),
            _ => StageResult::NotExecutable(format!("Unknown stage: {}", key)),
        }
    }

    fn stage_build(&self) -> StageResult {
        let lang = self.profile.language.as_deref().unwrap_or("");
        match lang {
            "python" => {
                if command_exists("python3") {
                    let py_files_exist = self.root.join("src").exists();
                    if py_files_exist {
                        let rc = self.run_bash("python3 -m py_compile src/*.py 2>/dev/null");
                        if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                    } else {
                        StageResult::NotExecutable("No src/*.py files found".to_string())
                    }
                } else {
                    StageResult::NotExecutable("python3 not found".to_string())
                }
            }
            "c" | "cpp" => {
                if command_exists("cmake") && self.root.join("CMakeLists.txt").exists() {
                    let rc = self.run_bash("cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON && cmake --build build");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("cmake or CMakeLists.txt missing".to_string())
                }
            }
            "rust" => {
                if command_exists("cargo") && self.root.join("Cargo.toml").exists() {
                    let rc = self.run_cmd("cargo", &["build", "--locked"]);
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("cargo or Cargo.toml missing".to_string())
                }
            }
            "java" => {
                if self.root.join("mvnw").exists() {
                    let rc = self.run_bash("./mvnw -q -DskipTests package");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if command_exists("mvn") && self.root.join("pom.xml").exists() {
                    let rc = self.run_bash("mvn -q -DskipTests package");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if self.root.join("gradlew").exists() {
                    let rc = self.run_bash("./gradlew assemble");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("Maven/Gradle build tool missing".to_string())
                }
            }
            _ => StageResult::NotExecutable(format!("No build support configured for language: {}", lang)),
        }
    }

    fn stage_lint(&self) -> StageResult {
        let lang = self.profile.language.as_deref().unwrap_or("");
        match lang {
            "python" => {
                if command_exists("ruff") {
                    let rc = self.run_cmd("ruff", &["check", "src/", "tests/"]);
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if command_exists("flake8") {
                    let rc = self.run_cmd("flake8", &["src/", "tests/"]);
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("Linter (ruff/flake8) missing".to_string())
                }
            }
            "c" | "cpp" => {
                if command_exists("run-clang-tidy") && self.root.join("build/compile_commands.json").exists() {
                    let rc = self.run_bash("run-clang-tidy -p build");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("run-clang-tidy or build/compile_commands.json missing".to_string())
                }
            }
            "rust" => {
                if command_exists("cargo") && self.root.join("Cargo.toml").exists() {
                    let rc = self.run_bash(&self.clippy_command());
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("cargo missing".to_string())
                }
            }
            "java" => {
                if self.root.join("mvnw").exists() && self.root.join("checkstyle.xml").exists() {
                    let rc = self.run_bash("./mvnw -q checkstyle:check");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if command_exists("mvn") && self.root.join("pom.xml").exists() && self.root.join("checkstyle.xml").exists() {
                    let rc = self.run_bash("mvn -q checkstyle:check");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("Checkstyle configuration or tool missing".to_string())
                }
            }
            _ => StageResult::NotExecutable(format!("No linting support configured for language: {}", lang)),
        }
    }

    fn stage_static(&self) -> StageResult {
        let lang = self.profile.language.as_deref().unwrap_or("");
        match lang {
            "python" => {
                if command_exists("mypy") {
                    let rc = self.run_cmd("mypy", &["src/"]);
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("mypy missing".to_string())
                }
            }
            "c" | "cpp" => {
                if command_exists("scan-build") && self.root.join("CMakeLists.txt").exists() {
                    let rc = self.run_bash("scan-build --status-bugs cmake --build build");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("scan-build missing".to_string())
                }
            }
            "rust" => {
                if command_exists("cargo") && self.root.join("Cargo.toml").exists() {
                    let rc = self.run_bash("cargo check --all-targets --all-features");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("cargo missing".to_string())
                }
            }
            "java" => {
                if self.root.join("mvnw").exists() {
                    let rc = self.run_bash("./mvnw -q spotbugs:check");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if command_exists("mvn") && self.root.join("pom.xml").exists() {
                    let rc = self.run_bash("mvn -q spotbugs:check");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("SpotBugs tool missing".to_string())
                }
            }
            _ => StageResult::NotExecutable(format!("No static analysis configured for language: {}", lang)),
        }
    }

    fn stage_security(&self) -> StageResult {
        let checks = self
            .profile
            .get_stage_config("security")
            .map(|c| c.checks.clone())
            .unwrap_or_default();
        let mut covered: Vec<&str> = Vec::new();

        match self.lang() {
            "rust" => {
                if !self.root.join("Cargo.lock").exists() {
                    return StageResult::NotExecutable("Cargo.lock missing (dependency audit needs a lock file)".to_string());
                }
                if self.root.join("deny.toml").exists() {
                    if !command_exists("cargo-deny") {
                        return StageResult::NotExecutable("deny.toml present but cargo-deny is not installed".to_string());
                    }
                    let rc = self.run_bash("cargo deny check");
                    if rc != Some(0) {
                        return StageResult::Failed(rc.unwrap_or(1));
                    }
                    covered.extend(["dependency_audit", "license_scan", "sources", "bans"]);
                }
                if command_exists("cargo-audit") {
                    let rc = self.run_bash("cargo audit");
                    if rc != Some(0) {
                        return StageResult::Failed(rc.unwrap_or(1));
                    }
                    covered.push("dependency_audit");
                }
            }
            "python" => {
                if command_exists("pip-audit") {
                    let rc = self.run_bash("pip-audit");
                    if rc != Some(0) {
                        return StageResult::Failed(rc.unwrap_or(1));
                    }
                    covered.push("dependency_audit");
                }
                if command_exists("bandit") && self.root.join("src").exists() {
                    let rc = self.run_bash("bandit -q -r src/");
                    if rc != Some(0) {
                        return StageResult::Failed(rc.unwrap_or(1));
                    }
                    covered.push("sast");
                }
            }
            _ => {}
        }

        if covered.is_empty() {
            return StageResult::NotExecutable(format!(
                "no built-in security scanner for language `{}` available; configure gate.security.command",
                self.lang()
            ));
        }
        // Fail closed: every check the profile promises must actually have run.
        let uncovered: Vec<&str> = checks
            .iter()
            .map(|c| c.as_str())
            .filter(|c| !covered.contains(c))
            .collect();
        if !uncovered.is_empty() {
            return StageResult::NotExecutable(format!(
                "checks without a built-in scanner: {}; configure gate.security.command to cover all checks",
                uncovered.join(", ")
            ));
        }
        StageResult::Success
    }

    fn stage_tests(&self) -> StageResult {
        let lang = self.profile.language.as_deref().unwrap_or("");
        match lang {
            "python" => {
                if command_exists("pytest") {
                    let rc = self.run_cmd("pytest", &["-q"]);
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("pytest missing".to_string())
                }
            }
            "c" | "cpp" => {
                if command_exists("ctest") && self.root.join("build").exists() {
                    let rc = self.run_bash("ctest --test-dir build --output-on-failure");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("ctest or build/ directory missing".to_string())
                }
            }
            "rust" => {
                if command_exists("cargo") && self.root.join("Cargo.toml").exists() {
                    let rc = self.run_bash("cargo test --all-targets --all-features");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("cargo missing".to_string())
                }
            }
            "java" => {
                if self.root.join("mvnw").exists() {
                    let rc = self.run_bash("./mvnw -q test");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if command_exists("mvn") && self.root.join("pom.xml").exists() {
                    let rc = self.run_bash("mvn -q test");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else if self.root.join("gradlew").exists() {
                    let rc = self.run_bash("./gradlew test");
                    if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
                } else {
                    StageResult::NotExecutable("Java test runner missing".to_string())
                }
            }
            _ => StageResult::NotExecutable(format!("No test runner configured for language: {}", lang)),
        }
    }

    fn stage_coverage(&self) -> StageResult {
        let cov_cfg = self.profile.get_stage_config("coverage");
        let metric = cov_cfg.and_then(|c| c.metric.as_deref()).unwrap_or("statement");
        let Some(threshold) = cov_cfg.and_then(|c| c.threshold) else {
            return StageResult::NotExecutable("gate.coverage.threshold not set".to_string());
        };

        println!("    required: {} >= {}%", metric, threshold);

        if metric == "mcdc" {
            println!("    MC/DC requires a qualified, project-specific coverage adapter.");
            return StageResult::NotExecutable("MC/DC adapter not configured (gate.coverage.command)".to_string());
        }

        match (self.lang(), metric) {
            ("python", "statement" | "line" | "branch") if command_exists("pytest") && command_exists("coverage") => {
                let branch_flag = if metric == "branch" { "--branch" } else { "" };
                let cmd_str = format!(
                    "coverage run {} -m pytest -q >/dev/null 2>&1 && coverage report --fail-under={}",
                    branch_flag, threshold
                );
                pass_if_zero(self.run_bash(&cmd_str))
            }
            ("rust", "statement" | "line") if command_exists("cargo-llvm-cov") => pass_if_zero(self.run_bash(&format!(
                "cargo llvm-cov --all-targets --all-features --fail-under-lines {}",
                threshold
            ))),
            (lang, _) => StageResult::NotExecutable(format!(
                "no built-in {} coverage for `{}` available; configure gate.coverage.command",
                metric, lang
            )),
        }
    }

    fn stage_load(&self) -> StageResult {
        // Having a load-test tool installed proves nothing; the scenario and its thresholds are project specific.
        StageResult::NotExecutable(
            "no built-in load test; configure gate.load_test.command (e.g. `k6 run --quiet load/slo.js`)".to_string(),
        )
    }

    fn stage_mutation(&self) -> StageResult {
        let Some(threshold) = self.profile.get_stage_config("mutation").and_then(|c| c.threshold) else {
            return StageResult::NotExecutable("gate.mutation.threshold (minimum mutation score in %) not set".to_string());
        };
        println!("    required: mutation score >= {}%", threshold);

        match self.lang() {
            "rust" => {
                if !command_exists("cargo-mutants") {
                    return StageResult::NotExecutable("cargo-mutants missing (cargo install cargo-mutants)".to_string());
                }
                // Exit codes 2 (missed mutants) and 3 (timeouts) still produce a scoreable report.
                let rc = self.run_bash("cargo mutants --no-shuffle --output target/aegis-mutants");
                if !matches!(rc, Some(0) | Some(2) | Some(3)) {
                    return StageResult::Failed(rc.unwrap_or(1));
                }
                let report = self.root.join("target/aegis-mutants/mutants.out/outcomes.json");
                match mutation_score(&report) {
                    Ok((_, 0)) => StageResult::NotExecutable("cargo-mutants produced no viable mutants".to_string()),
                    Ok((caught, total)) => {
                        let score = caught * 100 / total;
                        println!("    mutation score: {}% ({} of {} viable mutants caught)", score, caught, total);
                        if score >= u64::from(threshold) {
                            StageResult::Success
                        } else {
                            println!("    {}", "see target/aegis-mutants/mutants.out/missed.txt".red());
                            StageResult::Failed(1)
                        }
                    }
                    Err(e) => StageResult::NotExecutable(e),
                }
            }
            "java" => {
                let mvn = if self.root.join("mvnw").exists() {
                    "./mvnw"
                } else if command_exists("mvn") && self.root.join("pom.xml").exists() {
                    "mvn"
                } else {
                    return StageResult::NotExecutable("Maven missing (PIT mutation testing)".to_string());
                };
                pass_if_zero(self.run_bash(&format!(
                    "{} -q org.pitest:pitest-maven:mutationCoverage -DmutationThreshold={}",
                    mvn, threshold
                )))
            }
            lang => StageResult::NotExecutable(format!(
                "no built-in mutation testing for `{}`; configure gate.mutation.command",
                lang
            )),
        }
    }

    fn stage_architecture(&self) -> StageResult {
        if self.lang() != "rust" {
            return StageResult::NotExecutable(format!("no architecture check for language `{}`", self.lang()));
        }
        if !command_exists("cargo") || !self.root.join("Cargo.toml").exists() {
            return StageResult::NotExecutable("cargo or Cargo.toml missing".to_string());
        }
        let cfg = self.profile.get_stage_config("architecture").cloned().unwrap_or_default();
        let output = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
            .current_dir(&self.root)
            .output();
        let metadata: serde_json::Value = match output {
            Ok(o) if o.status.success() => match serde_json::from_slice(&o.stdout) {
                Ok(v) => v,
                Err(e) => return StageResult::NotExecutable(format!("cannot parse cargo metadata: {}", e)),
            },
            Ok(o) => {
                return StageResult::NotExecutable(format!(
                    "cargo metadata failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ))
            }
            Err(e) => return StageResult::NotExecutable(format!("cannot run cargo metadata: {}", e)),
        };
        let problems = architecture::check(&metadata, &cfg.layers, &cfg.restricted);
        if problems.is_empty() {
            let n = metadata.get("packages").and_then(|p| p.as_array()).map(|p| p.len()).unwrap_or(0);
            println!("    {} crate(s) conform to the layer policy", n);
            StageResult::Success
        } else {
            for p in &problems {
                println!("    {}", p.red());
            }
            StageResult::Failed(1)
        }
    }

    fn stage_test_freeze(&self) -> StageResult {
        let doc_dir = self.root.join("doc");
        let src_dirs = self.profile.source_paths(&self.root);
        match testfreeze::verify(&self.root, &doc_dir, &src_dirs) {
            Ok(problems) if problems.is_empty() => StageResult::Success,
            Ok(problems) => {
                for p in &problems {
                    println!("    {}", p.red());
                }
                StageResult::Failed(1)
            }
            Err(e) => StageResult::NotExecutable(format!("{:#}", e)),
        }
    }

    fn stage_traceability(&self) -> StageResult {
        let doc_dir = self.root.join("doc");
        let src_dirs = self.profile.source_paths(&self.root);

        let mut trace = Trace::default();
        if let Err(e) = traceability::collect_docs(&doc_dir, &mut trace) {
            return StageResult::NotExecutable(format!("Failed to read doc/: {}", e));
        }
        if let Err(e) = traceability::collect_code(&src_dirs, &mut trace) {
            return StageResult::NotExecutable(format!("Failed to scan source code: {}", e));
        }

        let (rows, _, _) = traceability::build_rows(&trace);
        let mut problems = traceability::find_problems(&trace, &rows);

        let baseline_required = self
            .profile
            .get_stage_config("traceability")
            .and_then(|c| c.baseline.as_deref())
            == Some("required");
        match baseline::load(&doc_dir) {
            Ok(Some(b)) => match baseline::current(&self.root, &trace) {
                Ok(now) => problems.extend(baseline::check(&trace, &baseline::diff(&b, &now))),
                Err(e) => problems.push(format!("INVALID  baseline: {:#}", e)),
            },
            Ok(None) if baseline_required => problems.push(
                "UNREVIEWED no approved baseline (doc/baseline.lock) — required by profile; \
                 review the specification, then run `aegis baseline --approve <name>`"
                    .to_string(),
            ),
            Ok(None) => {}
            Err(e) => problems.push(format!("INVALID  baseline: {:#}", e)),
        }

        // Write matrix to doc/traceability.md
        let out_file = doc_dir.join("traceability.md");
        let report_md = traceability::render_report_markdown(&rows, &problems);
        let _ = std::fs::write(&out_file, report_md);

        if problems.is_empty() {
            StageResult::Success
        } else {
            for p in &problems {
                println!("    {}", p.red());
            }
            StageResult::Failed(1)
        }
    }

    fn stage_reviews(&self) -> StageResult {
        let doc_dir = self.root.join("doc");
        let reviews = match review::collect_reviews(&doc_dir) {
            Ok(r) => r,
            Err(e) => return StageResult::NotExecutable(format!("{:#}", e)),
        };
        let issue_ids: BTreeSet<String> = match issue::collect_issues(&doc_dir) {
            Ok(is) => is.into_iter().map(|i| i.meta.id).collect(),
            Err(e) => return StageResult::NotExecutable(format!("{:#}", e)),
        };
        let required = self
            .profile
            .get_stage_config("reviews")
            .map(|c| c.required.clone())
            .unwrap_or_default();

        let problems = review::check(&reviews, &issue_ids, &required);
        println!("    ({} review record(s))", reviews.len());
        if problems.is_empty() {
            StageResult::Success
        } else {
            for p in &problems {
                println!("    {}", p.red());
            }
            StageResult::Failed(1)
        }
    }

    fn stage_issues(&self) -> StageResult {
        let doc_dir = self.root.join("doc");
        let issues = match issue::collect_issues(&doc_dir) {
            Ok(is) => is,
            Err(e) => return StageResult::NotExecutable(format!("Failed to read doc/issues: {}", e)),
        };

        let mut blockers = Vec::new();
        for i in &issues {
            if i.meta.status != "done" && (i.meta.severity == "critical" || i.meta.severity == "high") {
                blockers.push(i);
            }
        }

        if blockers.is_empty() {
            let open_count = issues.iter().filter(|i| i.meta.status != "done").count();
            if open_count > 0 {
                println!("    ({} non-blocking open issues)", open_count);
            }
            StageResult::Success
        } else {
            for b in &blockers {
                println!(
                    "    {}",
                    format!(
                        "Open Blocker: {} [{}] ({}) - {}",
                        b.meta.id, b.meta.severity, b.meta.r#type, b.meta.title
                    )
                    .red()
                );
            }
            StageResult::Failed(1)
        }
    }

    fn stage_deliverables(&self) -> StageResult {
        let manifest_path = self.root.join("doc/deliverables/manifest.md");
        if !manifest_path.exists() {
            return StageResult::NotExecutable("doc/deliverables/manifest.md does not exist".to_string());
        }

        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(_) => return StageResult::NotExecutable("Could not read manifest.md".to_string()),
        };

        let path_re = Regex::new(r"^\s*-\s*path:\s*(.+)").unwrap();
        let mut missing = Vec::new();

        for line in content.lines() {
            if let Some(cap) = path_re.captures(line) {
                if let Some(rel) = cap.get(1) {
                    let rel_str = rel.as_str().trim();
                    if !rel_str.is_empty() {
                        let full = self.root.join(rel_str);
                        if !full.exists() {
                            missing.push(rel_str.to_string());
                        }
                    }
                }
            }
        }

        if missing.is_empty() {
            StageResult::Success
        } else {
            for m in &missing {
                println!("    missing: {}", m.red());
            }
            StageResult::Failed(1)
        }
    }

    fn print_summary(&self, all_passed: bool) {
        let prof_name = &self.profile.profile;
        let rigor = self.profile.rigor.as_deref().unwrap_or("—");

        println!(
            "{}",
            format!(
                "─── Gate Results [Profile: {}, Rigor: {}] ───",
                prof_name, rigor
            )
            .bold()
        );

        for (name, res) in &self.results {
            match res {
                StageResult::Inactive => {
                    println!("  ∅  {} (profile-inactive)", name);
                }
                StageResult::Skipped => {
                    println!("  –  {} (skipped: not selected)", name);
                }
                StageResult::Success => {
                    println!("  ✔  {}", name);
                }
                StageResult::NotExecutable(_) => {
                    println!("  ✘  {} (active stage not executable)", name);
                }
                StageResult::Failed(_) => {
                    println!("  ✘  {}", name);
                }
            }
        }

        let skipped = self.results.iter().filter(|(_, r)| *r == StageResult::Skipped).count();
        if all_passed && skipped > 0 {
            println!(
                "{}",
                format!("✔ PARTIAL GATE GREEN — {} stage(s) skipped; this is not an acceptance", skipped)
                    .yellow()
                    .bold()
            );
        } else if all_passed {
            println!("{}", "✔ GATE GREEN — Accepted".green().bold());
        } else {
            println!("{}", "✘ GATE RED — Rejected".red().bold());
        }
    }
}

/// Mutation score input from a cargo-mutants `outcomes.json`: (caught incl. timeouts, viable mutants).
pub fn mutation_score(path: &std::path::Path) -> std::result::Result<(u64, u64), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("cannot parse {}: {}", path.display(), e))?;
    let num = |k: &str| v.get(k).and_then(|x| x.as_u64());
    if let (Some(caught), Some(missed)) = (num("caught"), num("missed")) {
        let timeout = num("timeout").unwrap_or(0);
        return Ok((caught + timeout, caught + missed + timeout));
    }
    let outcomes = v
        .get("outcomes")
        .and_then(|o| o.as_array())
        .ok_or_else(|| format!("{}: neither summary counts nor `outcomes` found", path.display()))?;
    let (mut caught, mut total) = (0, 0);
    for o in outcomes {
        match o.get("summary").and_then(|s| s.as_str()) {
            Some("CaughtMutant") | Some("Timeout") => {
                caught += 1;
                total += 1;
            }
            Some("MissedMutant") => total += 1,
            _ => {}
        }
    }
    Ok((caught, total))
}
