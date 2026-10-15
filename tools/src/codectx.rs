//! Deterministic code context for work packets: Rust items extracted by brace matching (no LLM, no language server).
//!
//! Used by `aegis tasks` to embed the frozen test functions, the stubs a task has to fill in (`todo!()` or a
//! `// STUB` marker) and the definitions of the types they mention, so a small model does not have to search the
//! repository.

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// End line (inclusive, 0-based) of the item whose header starts at or after `from`: the matching `}` of its first
/// `{`, or the first `;` at depth 0 before any `{`. Strings, chars and comments are skipped.
pub fn item_end(lines: &[&str], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut opened = false;
    let mut in_block_comment = false;
    for (i, line) in lines.iter().enumerate().skip(from) {
        let chars: Vec<char> = line.chars().collect();
        let mut j = 0;
        let mut in_str = false;
        while j < chars.len() {
            let c = chars[j];
            let next = chars.get(j + 1).copied();
            if in_block_comment {
                if c == '*' && next == Some('/') {
                    in_block_comment = false;
                    j += 1;
                }
            } else if in_str {
                if c == '\\' {
                    j += 1;
                } else if c == '"' {
                    in_str = false;
                }
            } else if c == '/' && next == Some('/') {
                break;
            } else if c == '/' && next == Some('*') {
                in_block_comment = true;
                j += 1;
            } else if c == '"' {
                in_str = true;
            } else if c == '\'' && chars.get(j + 2) == Some(&'\'') {
                j += 2; // char literal 'x'
            } else if c == '{' {
                depth += 1;
                opened = true;
            } else if c == '}' {
                depth = depth.saturating_sub(1);
                if opened && depth == 0 {
                    return Some(i);
                }
            } else if c == ';' && !opened && depth == 0 {
                return Some(i);
            }
            j += 1;
        }
    }
    None
}

/// Start of the item including its doc comments and attributes directly above `line`.
fn item_start(lines: &[&str], line: usize) -> usize {
    let mut start = line;
    while start > 0 {
        let above = lines[start - 1].trim_start();
        if above.starts_with("///") || above.starts_with("#[") || above.starts_with("//") {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

fn rust_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    crate::traceability::source_files(dirs)
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rs"))
        .collect()
}

/// Source of the test function annotated at `file:line` (1-based line of the `@verifies` comment).
pub fn test_source(loc: &str) -> Option<String> {
    let (file, line) = loc.rsplit_once(':')?;
    let line: usize = line.parse().ok()?;
    let text = std::fs::read_to_string(file).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = item_start(&lines, line.saturating_sub(1));
    let fn_re = Regex::new(r"\bfn\s+\w+").unwrap();
    let header = (line.saturating_sub(1)..lines.len())
        .take(10)
        .find(|&i| fn_re.is_match(lines[i]));
    if let Some(h) = header {
        if let Some(end) = item_end(&lines, h) {
            return Some(lines[start..=end].join("\n"));
        }
    }
    let take_lines = lines.len().saturating_sub(start).min(30);
    if take_lines > 0 {
        Some(lines[start..start + take_lines].join("\n"))
    } else {
        None
    }
}

/// Functions in `dirs` that are still stubs: body contains `todo!(` / `unimplemented!(`, or a `// STUB` marker.
pub fn stubs(dirs: &[PathBuf], root: &Path) -> Vec<(String, String)> {
    let fn_re =
        Regex::new(r"^\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+\w+").unwrap();
    let mut out = Vec::new();
    for file in rust_files(dirs) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if fn_re.is_match(lines[i]) {
                if let Some(end) = item_end(&lines, i) {
                    let start = item_start(&lines, i);
                    let item = lines[start..=end].join("\n");
                    if item.contains("todo!(")
                        || item.contains("unimplemented!(")
                        || item.contains("// STUB")
                    {
                        let rel = file
                            .strip_prefix(root)
                            .unwrap_or(&file)
                            .display()
                            .to_string();
                        out.push((format!("{}:{}", rel, i + 1), item));
                    }
                    i = end + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

/// Definitions (`struct`, `enum`, `trait`, `type`) in `dirs` whose names occur in `texts`, keyed by name.
pub fn referenced_types(
    texts: &[String],
    dirs: &[PathBuf],
    root: &Path,
) -> BTreeMap<String, (String, String)> {
    let ident_re = Regex::new(r"\b[A-Z][A-Za-z0-9]+\b").unwrap();
    let wanted: BTreeSet<String> = texts
        .iter()
        .flat_map(|t| ident_re.find_iter(t).map(|m| m.as_str().to_string()))
        .collect();
    let def_re =
        Regex::new(r"^\s*(pub(\([^)]*\))?\s+)?(struct|enum|trait|type)\s+([A-Z][A-Za-z0-9]*)")
            .unwrap();
    let mut out = BTreeMap::new();
    for file in rust_files(dirs) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(cap) = def_re.captures(line) else {
                continue;
            };
            let name = cap[4].to_string();
            if !wanted.contains(&name) || out.contains_key(&name) {
                continue;
            }
            if let Some(end) = item_end(&lines, i) {
                let rel = file
                    .strip_prefix(root)
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                out.insert(
                    name,
                    (
                        format!("{}:{}", rel, i + 1),
                        lines[item_start(&lines, i)..=end].join("\n"),
                    ),
                );
            }
        }
    }
    out
}
