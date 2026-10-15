//! Delivered documentation: CSP support for the help sites served by the application.
//!
//! Static site generators (MkDocs Material) emit a few inline `<script>`/`<style>` blocks. Instead of weakening the
//! help pages' Content-Security-Policy with 'unsafe-inline', the build computes a SHA-256 source for every inline
//! block (`aegis docs csp-hashes --site docs/user/site`) and the server adds exactly these to the CSP of `/help`
//! and `/admin-help` (RULE-RWE-101).

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Default, Serialize)]
pub struct CspHashes {
    /// `'sha256-…'` sources for script-src
    pub script: BTreeSet<String>,
    /// `'sha256-…'` sources for style-src
    pub style: BTreeSet<String>,
    /// Inline `style="…"` attributes (cannot be hashed without 'unsafe-hashes')
    pub style_attributes: usize,
    pub pages: usize,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn csp_source(content: &str) -> String {
    format!("'sha256-{}'", base64(&Sha256::digest(content.as_bytes())))
}

/// Inline blocks of one tag (`script` or `style`) in an HTML document: (attributes, content).
fn inline_blocks<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let lower = html.to_ascii_lowercase();
    let mut blocks = Vec::new();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find(&open).map(|i| i + pos) {
        let after_name = start + open.len();
        // `<scripts>` or `<style-foo>` are other tags
        if !lower[after_name..].starts_with(|c: char| c == '>' || c.is_ascii_whitespace()) {
            pos = after_name;
            continue;
        }
        let Some(gt) = lower[after_name..].find('>').map(|i| i + after_name) else { break };
        let Some(end) = lower[gt + 1..].find(&close).map(|i| i + gt + 1) else { break };
        blocks.push((&html[after_name..gt], &html[gt + 1..end]));
        pos = end + close.len();
    }
    blocks
}

fn executes(attrs: &str) -> bool {
    let a = attrs.to_ascii_lowercase();
    if a.contains("src=") {
        return false;
    }
    match a.split("type=").nth(1) {
        None => true,
        Some(rest) => {
            let t = rest.trim_start_matches(['"', '\'']);
            t.starts_with("text/javascript") || t.starts_with("module") || t.starts_with("application/javascript")
        }
    }
}

pub fn scan_html(html: &str, hashes: &mut CspHashes) {
    for (attrs, content) in inline_blocks(html, "script") {
        if executes(attrs) && !content.is_empty() {
            hashes.script.insert(csp_source(content));
        }
    }
    for (_, content) in inline_blocks(html, "style") {
        if !content.is_empty() {
            hashes.style.insert(csp_source(content));
        }
    }
    hashes.style_attributes += html.to_ascii_lowercase().matches(" style=").count();
}

pub fn csp_hashes(site: &Path) -> Result<CspHashes> {
    if !site.is_dir() {
        anyhow::bail!("{} is not a built site directory (run mkdocs build first)", site.display());
    }
    let mut hashes = CspHashes::default();
    for entry in WalkDir::new(site).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("html") {
            let html = std::fs::read_to_string(p).with_context(|| format!("Failed to read {}", p.display()))?;
            scan_html(&html, &mut hashes);
            hashes.pages += 1;
        }
    }
    if hashes.pages == 0 {
        anyhow::bail!("no HTML pages under {}", site.display());
    }
    Ok(hashes)
}
