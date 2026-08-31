use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use aegis::gate::GateRunner;
use aegis::impact;
use aegis::issue::{self, CreateIssueOptions};
use aegis::profile::Profile;
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
            let problems = traceability::find_problems(&trace, &rows);

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
            kind,
            json,
        } => {
            if target.is_empty() && code_file.is_none() {
                eprintln!("Please specify at least --target <ID...> or --code-file <path>.");
                exit(2);
            }

            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let doc_path = root_path.join(doc);
            let src_paths: Vec<PathBuf> = src.iter().map(|s| root_path.join(s)).collect();

            let mut trace = Trace::default();
            traceability::collect_docs(&doc_path, &mut trace)?;
            traceability::collect_code(&src_paths, &mut trace)?;

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
        Commands::Gate { profile, root } => {
            let root_path = Path::new(&root).canonicalize().unwrap_or_else(|_| PathBuf::from(&root));
            let profile_path = if Path::new(&profile).is_absolute() {
                PathBuf::from(&profile)
            } else {
                root_path.join(&profile)
            };

            let prof = Profile::load_from_file(&profile_path)
                .with_context(|| format!("Could not load profile: {}", profile_path.display()))?;

            let mut runner = GateRunner::new(root_path, prof);
            let passed = runner.run_all()?;

            if !passed {
                exit(1);
            }
        }
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
