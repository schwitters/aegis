use crate::issue;
use crate::profile::Profile;
use crate::traceability::{self, Trace};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, PartialEq)]
pub enum StageResult {
    Inactive,
    Success,
    NotExecutable(String),
    Failed(i32),
}

pub struct GateRunner {
    pub root: PathBuf,
    pub profile: Profile,
    pub results: Vec<(String, StageResult)>,
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
        Self {
            root,
            profile,
            results: Vec::new(),
        }
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

        let stages = [
            ("Build", "build"),
            ("Lint", "lint"),
            ("Static Analysis", "static_analysis"),
            ("Security Scan", "security"),
            ("Tests", "tests"),
            ("Coverage", "coverage"),
            ("Load Test", "load_test"),
            ("Traceability", "traceability"),
            ("Issues", "issues"),
            ("Deliverables", "deliverables"),
        ];

        for (display_name, key) in stages {
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

        let res = match key {
            "build" => self.stage_build(),
            "lint" => self.stage_lint(),
            "static_analysis" => self.stage_static(),
            "security" => self.stage_security(),
            "tests" => self.stage_tests(),
            "coverage" => self.stage_coverage(),
            "load_test" => self.stage_load(),
            "traceability" => self.stage_traceability(),
            "issues" => self.stage_issues(),
            "deliverables" => self.stage_deliverables(),
            _ => StageResult::NotExecutable(format!("Unknown stage: {}", key)),
        };

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
            StageResult::Inactive => {}
        }

        Ok(res)
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
                    let rc = self.run_bash("cargo clippy --all-targets --all-features -- -D warnings");
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
        let mut executed = false;
        if command_exists("pip-audit") {
            let rc = self.run_bash("pip-audit 2>/dev/null");
            if rc != Some(0) { return StageResult::Failed(rc.unwrap_or(1)); }
            executed = true;
        }
        if command_exists("bandit") && self.root.join("src").exists() {
            let rc = self.run_bash("bandit -q -r src/");
            if rc != Some(0) { return StageResult::Failed(rc.unwrap_or(1)); }
            executed = true;
        }
        if command_exists("cargo-audit") && self.root.join("Cargo.lock").exists() {
            let rc = self.run_bash("cargo audit");
            if rc != Some(0) { return StageResult::Failed(rc.unwrap_or(1)); }
            executed = true;
        }

        if executed {
            StageResult::Success
        } else {
            StageResult::NotExecutable("No matching security scanners (pip-audit, bandit, cargo-audit) found".to_string())
        }
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
        let threshold = cov_cfg.and_then(|c| c.threshold).unwrap_or(0);

        println!("    required: {} >= {}%", metric, threshold);

        if metric == "mcdc" {
            println!("    MC/DC requires a qualified, project-specific coverage adapter.");
            return StageResult::NotExecutable("MC/DC adapter not configured".to_string());
        }

        let lang = self.profile.language.as_deref().unwrap_or("");
        if lang == "python" && command_exists("pytest") && command_exists("coverage") {
            let branch_flag = if metric == "branch" { "--branch" } else { "" };
            let cmd_str = format!(
                "coverage run {} -m pytest -q >/dev/null 2>&1 && coverage report --fail-under={}",
                branch_flag, threshold
            );
            let rc = self.run_bash(&cmd_str);
            if rc == Some(0) { StageResult::Success } else { StageResult::Failed(rc.unwrap_or(1)) }
        } else {
            StageResult::NotExecutable(format!("Coverage tool for {} ({}) missing", lang, metric))
        }
    }

    fn stage_load(&self) -> StageResult {
        if command_exists("k6") || command_exists("locust") {
            println!("    (Load test runner available)");
            StageResult::Success
        } else {
            StageResult::NotExecutable("Neither k6 nor locust found".to_string())
        }
    }

    fn stage_traceability(&self) -> StageResult {
        let doc_dir = self.root.join("doc");
        let src_dirs = vec![self.root.join("src"), self.root.join("tests")];

        let mut trace = Trace::default();
        if let Err(e) = traceability::collect_docs(&doc_dir, &mut trace) {
            return StageResult::NotExecutable(format!("Failed to read doc/: {}", e));
        }
        if let Err(e) = traceability::collect_code(&src_dirs, &mut trace) {
            return StageResult::NotExecutable(format!("Failed to scan source code: {}", e));
        }

        let (rows, _, _) = traceability::build_rows(&trace);
        let problems = traceability::find_problems(&trace, &rows);

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

        if all_passed {
            println!("{}", "✔ GATE GREEN — Accepted".green().bold());
        } else {
            println!("{}", "✘ GATE RED — Rejected".red().bold());
        }
    }
}
