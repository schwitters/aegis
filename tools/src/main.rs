use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use aegis::baseline;
use aegis::gate::GateRunner;
use aegis::impact;
use aegis::issue::{self, CreateIssueOptions};
use aegis::profile::Profile;
use aegis::review::{self, CreateReviewOptions};
use aegis::testfreeze;
use aegis::traceability::{self, Trace};
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser)]
#[command(
    name = "aegis",
    about = "Aegis: Spec-Driven Systems Engineering Toolkit (Traceability, Impact Analysis, Issues, Profiles, and Quality Gate)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Traceability verification: Requirement -> Usecase -> Test -> Code
    Trace {
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
        #[arg(long, num_args = 1.., default_values_t = vec!["src".to_string(), "tests".to_string()])]
        src: Vec<String>,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Impact analysis for change requests (Blast Radius)
    Impact {
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
        #[arg(long, num_args = 1.., default_values_t = vec!["src".to_string(), "tests".to_string()])]
        src: Vec<String>,
        #[arg(long, num_args = 1..)]
        target: Vec<String>,
        #[arg(long)]
        code_file: Option<String>,
        /// Seed the analysis with every artifact changed since the approved baseline (doc/baseline.lock)
        #[arg(long)]
        changed: bool,
        /// bug | feature | wish | concept (spec or architecture assumption changes)
        #[arg(long, default_value = "unknown")]
        kind: String,
        #[arg(long)]
        json: bool,
    },
    /// Profile-aware Execution Gate (CI Pipeline)
    Gate {
        #[arg(long)]
        profile: String,
        #[arg(long, default_value = ".")]
        root: String,
        /// Run only these stages (comma separated), e.g. the agent's inner loop: build,lint,test_freeze,tests
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Skip these stages (comma separated). A partial run is never an acceptance.
        #[arg(long, value_delimiter = ',')]
        skip: Vec<String>,
    },
    /// Approved baseline of all specification artifacts (suspect-link detection)
    Baseline {
        /// Record the current state as approved by <name>. Without it, only the pending changes are shown.
        #[arg(long)]
        approve: Option<String>,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// Frozen test files (tests are written and reviewed before the implementation)
    Tests {
        #[command(subcommand)]
        cmd: TestsCommands,
    },
    /// Review records with findings and dispositions (doc/reviews/)
    Review {
        #[command(subcommand)]
        cmd: ReviewCommands,
    },
    /// Inspect domain profiles
    Profile {
        #[command(subcommand)]
        cmd: ProfileCommands,
    },
    /// Issue management (doc/issues/)
    Issue {
        #[command(subcommand)]
        cmd: IssueCommands,
    },
    /// Initialize a new (greenfield) or existing (brownfield) repository with Aegis
    Init {
        #[arg(long, default_value = "enterprise")]
        profile: String,
        #[arg(long, default_value = "rust")]
        lang: String,
        #[arg(long)]
        brownfield: bool,
        #[arg(long, default_value = ".")]
        root: String,
    },
    /// Synthesize AGENT_INSTRUCTIONS.md for the 27B implementation model
    Instructions {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        out: Option<String>,
        #[arg(long, default_value = ".")]
        root: String,
    },
}

#[derive(Subcommand)]
enum TestsCommands {
    /// Hash every file with @verifies annotations into doc/testplan/tests.lock
    Freeze {
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
        #[arg(long, num_args = 1.., default_values_t = vec!["src".to_string(), "tests".to_string()])]
        src: Vec<String>,
        /// Take the source roots from the profile (gate.traceability.paths) instead of --src
        #[arg(long)]
        profile: Option<String>,
    },
    /// Check that frozen test files are unchanged and that no test file is unfrozen
    Verify {
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
        #[arg(long, num_args = 1.., default_values_t = vec!["src".to_string(), "tests".to_string()])]
        src: Vec<String>,
        /// Take the source roots from the profile (gate.traceability.paths) instead of --src
        #[arg(long)]
        profile: Option<String>,
    },
}

#[derive(Subcommand)]
enum ReviewCommands {
    /// Create a review record skeleton with the next free ID (REVIEW-0001.md)
    Create {
        /// spec | requirements | architecture | usecases | testplan | implementation
        #[arg(long)]
        stage: String,
        #[arg(long, num_args = 1..)]
        artifact: Vec<String>,
        /// Reviewing model / person (must differ from the author)
        #[arg(long)]
        reviewer: String,
        /// Authoring model / person
        #[arg(long)]
        author: Option<String>,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// List review records with open findings
    List {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// Validate all review records (same check as the gate stage `reviews`)
    Check {
        /// Required blind approvals, e.g. --required spec architecture:2
        #[arg(long, num_args = 0..)]
        required: Vec<String>,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Output profile details as YAML or JSON
    Show {
        #[arg(long)]
        profile: String,
        #[arg(long)]
        json: bool,
    },
    /// Get single field (e.g. language, rigor, profile)
    Get {
        #[arg(long)]
        profile: String,
        field: String,
    },
}

#[derive(Subcommand)]
enum IssueCommands {
    /// Create new issue with next available ID (e.g. ISSUE-0001.md)
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "bug")]
        r#type: String,
        #[arg(long, default_value = "medium")]
        severity: String,
        #[arg(long)]
        stage: Option<String>,
        #[arg(long)]
        source_doc: Option<String>,
        #[arg(long)]
        source_finding: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long, num_args = 0..)]
        related: Vec<String>,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// List all issues in table format or JSON
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// Close an issue (status: done, closed: date)
    Close {
        id: String,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
    /// Show issue details
    Show {
        id: String,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = ".")]
        root: String,
        #[arg(long, default_value = "doc")]
        doc: String,
    },
}

/// Source roots from the profile (`gate.traceability.paths`) when given, else from `--src`.
fn source_roots(root: &Path, src: &[String], profile: Option<&str>) -> Result<Vec<PathBuf>> {
    match profile {
        Some(p) => {
            let path = if Path::new(p).is_absolute() { PathBuf::from(p) } else { root.join(p) };
            Ok(Profile::load_from_file(&path)?.source_paths(root))
        }
        None => Ok(src.iter().map(|s| root.join(s)).collect()),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Trace {
            root,
            doc,
            src,
            out,
            json,
        } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let doc_path = root_path.join(doc);
            let src_paths: Vec<PathBuf> = src.iter().map(|s| root_path.join(s)).collect();

            let mut trace = Trace::default();
            traceability::collect_docs(&doc_path, &mut trace)?;
            traceability::collect_code(&src_paths, &mut trace)?;

            let (rows, _, _) = traceability::build_rows(&trace);
            let mut problems = traceability::find_problems(&trace, &rows);
            if let Some(b) = baseline::load(&doc_path)? {
                let now = baseline::current(&root_path, &trace)?;
                problems.extend(baseline::check(&trace, &baseline::diff(&b, &now)));
            }

            if json {
                let report = traceability::TraceReport {
                    rows: rows.clone(),
                    problems: problems.clone(),
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let report_md = traceability::render_report_markdown(&rows, &problems);
                print!("{}", report_md);
            }

            if let Some(out_rel) = out {
                let out_path = root_path.join(out_rel);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let report_md = traceability::render_report_markdown(&rows, &problems);
                std::fs::write(&out_path, report_md)?;
            }

            if !problems.is_empty() {
                exit(1);
            }
        }
        Commands::Impact {
            root,
            doc,
            src,
            target,
            code_file,
            changed,
            kind,
            json,
        } => {
            if target.is_empty() && code_file.is_none() && !changed {
                eprintln!("Please specify at least --target <ID...>, --code-file <path> or --changed.");
                exit(2);
            }

            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let doc_path = root_path.join(doc);
            let src_paths: Vec<PathBuf> = src.iter().map(|s| root_path.join(s)).collect();

            let mut trace = Trace::default();
            traceability::collect_docs(&doc_path, &mut trace)?;
            traceability::collect_code(&src_paths, &mut trace)?;

            let mut target = target;
            if changed {
                let b = baseline::load(&doc_path)?
                    .context("--changed needs an approved baseline (doc/baseline.lock); run `aegis baseline --approve`")?;
                let d = baseline::diff(&b, &baseline::current(&root_path, &trace)?);
                target.extend(d.unapproved());
                target.extend(d.removed);
            }

            let root_str = format!("{}/", root_path.display());
            let blast = impact::compute_impact(&trace, &target, code_file.as_deref(), &root_str);
            let depth = impact::classify_depth(&kind, &blast);
            let plan = impact::make_plan(&kind, &depth, &blast);

            if json {
                let report = impact::ImpactReport {
                    depth,
                    blast,
                    plan,
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let text = impact::render_report_text(&kind, &depth, &blast, &plan);
                print!("{}", text);
            }
        }
        Commands::Gate {
            profile,
            root,
            only,
            skip,
        } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let profile_path = if Path::new(&profile).is_absolute() {
                PathBuf::from(&profile)
            } else {
                root_path.join(&profile)
            };

            let prof = Profile::load_from_file(&profile_path)
                .with_context(|| format!("Could not load profile: {}", profile_path.display()))?;

            let mut runner = GateRunner::new(root_path, prof).with_selection(only, skip)?;
            let passed = runner.run_all()?;

            if !passed {
                exit(1);
            }
        }
        Commands::Baseline { approve, root, doc } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let doc_path = root_path.join(doc);

            let mut trace = Trace::default();
            traceability::collect_docs(&doc_path, &mut trace)?;
            let (rows, _, _) = traceability::build_rows(&trace);
            let blocking = baseline::blocking_for_approval(&traceability::find_problems(&trace, &rows));

            let now = baseline::current(&root_path, &trace)?;
            let previous = baseline::load(&doc_path)?;
            let d = match &previous {
                Some(b) => baseline::diff(b, &now),
                None => baseline::BaselineDiff {
                    added: now.keys().cloned().collect(),
                    ..Default::default()
                },
            };

            if d.is_empty() {
                println!("✔ Baseline is current ({} artifacts).", now.len());
                return Ok(());
            }
            for (label, ids) in [("new", &d.added), ("changed", &d.changed), ("removed", &d.removed)] {
                for id in ids {
                    println!("  {:<8} {}", label, id);
                }
            }
            if previous.is_some() {
                for p in baseline::check(&trace, &d).iter().filter(|p| p.starts_with("SUSPECT")) {
                    println!("  {}", p);
                }
            }

            match approve {
                None => {
                    println!();
                    println!("Review the items above (and their SUSPECT links), then run: aegis baseline --approve <name>");
                    exit(1);
                }
                Some(name) => {
                    if !blocking.is_empty() {
                        for p in &blocking {
                            eprintln!("  {}", p);
                        }
                        eprintln!("✘ Refusing to baseline a structurally broken specification.");
                        exit(1);
                    }
                    let b = baseline::Baseline {
                        approved_by: Some(name),
                        approved_at: Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
                        artifacts: now,
                    };
                    let path = baseline::write(&doc_path, &b)?;
                    println!("✔ Baseline approved: {}", path.strip_prefix(&root_path).unwrap_or(&path).display());
                }
            }
        }
        Commands::Tests { cmd } => match cmd {
            TestsCommands::Freeze { root, doc, src, profile } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let src_paths = source_roots(&root_path, &src, profile.as_deref())?;
                let (path, n) = testfreeze::freeze(&root_path, &root_path.join(doc), &src_paths)?;
                println!(
                    "✔ Froze {} test file(s): {}",
                    n,
                    path.strip_prefix(&root_path).unwrap_or(&path).display()
                );
            }
            TestsCommands::Verify { root, doc, src, profile } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let src_paths = source_roots(&root_path, &src, profile.as_deref())?;
                let problems = testfreeze::verify(&root_path, &root_path.join(doc), &src_paths)?;
                if problems.is_empty() {
                    println!("✔ All test files frozen and unchanged.");
                } else {
                    for p in &problems {
                        println!("{}", p);
                    }
                    exit(1);
                }
            }
        },
        Commands::Review { cmd } => match cmd {
            ReviewCommands::Create {
                stage,
                artifact,
                reviewer,
                author,
                root,
                doc,
            } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let (id, path) = review::create_review(
                    &root_path.join(doc),
                    CreateReviewOptions {
                        stage,
                        artifacts: artifact,
                        reviewer,
                        author,
                    },
                )?;
                println!(
                    "✔ Review record created: {} ({})",
                    path.strip_prefix(&root_path).unwrap_or(&path).display(),
                    id
                );
            }
            ReviewCommands::List { json, root, doc } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let reviews = review::collect_reviews(&root_path.join(doc))?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&reviews)?);
                } else {
                    print!("{}", review::render_table(&reviews));
                }
            }
            ReviewCommands::Check { required, root, doc } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let doc_path = root_path.join(doc);
                let reviews = review::collect_reviews(&doc_path)?;
                let issue_ids = issue::collect_issues(&doc_path)?.into_iter().map(|i| i.meta.id).collect();
                let problems = review::check(&reviews, &issue_ids, &required);
                if problems.is_empty() {
                    println!("✔ {} review record(s) valid.", reviews.len());
                } else {
                    for p in &problems {
                        println!("{}", p);
                    }
                    exit(1);
                }
            }
        },
        Commands::Profile { cmd } => match cmd {
            ProfileCommands::Show { profile, json } => {
                let prof = Profile::load_from_file(&profile)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&prof)?);
                } else {
                    println!("{}", serde_yaml::to_string(&prof)?);
                }
            }
            ProfileCommands::Get { profile, field } => {
                let prof = Profile::load_from_file(&profile)?;
                match field.as_str() {
                    "profile" | "name" => println!("{}", prof.profile),
                    "rigor" => println!("{}", prof.rigor.as_deref().unwrap_or("")),
                    "language" => println!("{}", prof.language.as_deref().unwrap_or("")),
                    "description" => println!("{}", prof.description.as_deref().unwrap_or("")),
                    _ => {
                        eprintln!("Unknown profile field: {}", field);
                        exit(2);
                    }
                }
            }
        },
        Commands::Issue { cmd } => match cmd {
            IssueCommands::Create {
                title,
                r#type,
                severity,
                stage,
                source_doc,
                source_finding,
                assignee,
                related,
                context,
                description,
                root,
                doc,
            } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let doc_path = root_path.join(doc);

                let (id, file_path) = issue::create_issue(
                    &doc_path,
                    CreateIssueOptions {
                        title,
                        r#type,
                        severity,
                        stage,
                        source_doc,
                        source_finding,
                        assignee,
                        related,
                        context,
                        description,
                    },
                )?;

                let rel_path = file_path.strip_prefix(&root_path).unwrap_or(&file_path);
                println!("✔ Issue created: {} ({})", rel_path.display(), id);
            }
            IssueCommands::List {
                status,
                severity,
                r#type,
                json,
                root,
                doc,
            } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let doc_path = root_path.join(doc);

                let mut issues = issue::collect_issues(&doc_path)?;

                if let Some(ref st) = status {
                    if st != "all" {
                        issues.retain(|i| i.meta.status.eq_ignore_ascii_case(st));
                    }
                }
                if let Some(ref sev) = severity {
                    issues.retain(|i| i.meta.severity.eq_ignore_ascii_case(sev));
                }
                if let Some(ref tp) = r#type {
                    issues.retain(|i| i.meta.r#type.eq_ignore_ascii_case(tp));
                }

                if json {
                    println!("{}", serde_json::to_string_pretty(&issues)?);
                } else {
                    print!("{}", issue::render_issues_table(&issues));
                }
            }
            IssueCommands::Close { id, root, doc } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let doc_path = root_path.join(doc);

                let file_path = issue::close_issue(&doc_path, &id)?;
                let rel_path = file_path.strip_prefix(&root_path).unwrap_or(&file_path);
                println!("✔ Issue closed: {} ({})", rel_path.display(), id);
            }
            IssueCommands::Show {
                id,
                json,
                root,
                doc,
            } => {
                let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
                let doc_path = root_path.join(doc);

                let issues = issue::collect_issues(&doc_path)?;
                let target_id = if id.starts_with("ISSUE-") {
                    id.clone()
                } else if let Ok(num) = id.parse::<u32>() {
                    format!("ISSUE-{:04}", num)
                } else {
                    id.clone()
                };

                let issue = issues
                    .iter()
                    .find(|i| i.meta.id == target_id)
                    .with_context(|| format!("Issue {} not found", target_id))?;

                if json {
                    println!("{}", serde_json::to_string_pretty(&issue)?);
                } else {
                    let content = std::fs::read_to_string(&issue.file_path)?;
                    print!("{}", content);
                }
            }
        },
        Commands::Init {
            profile,
            lang,
            brownfield,
            root,
        } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            aegis::init::run_init(
                &root_path,
                aegis::init::InitOptions {
                    profile,
                    lang,
                    brownfield,
                },
            )?;
        }
        Commands::Instructions {
            profile,
            out,
            root,
        } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let prof_path = profile.map(|p| {
                if Path::new(&p).is_absolute() {
                    PathBuf::from(p)
                } else {
                    root_path.join(p)
                }
            });
            let out_path = out.map(PathBuf::from);

            let rendered = aegis::instructions::synthesize_instructions(
                aegis::instructions::SynthesizeOptions {
                    root: root_path,
                    profile_path: prof_path,
                    out: out_path.clone(),
                },
            )?;

            if out_path.is_none() {
                print!("{}", rendered);
            }
        }
    }

    Ok(())
}
