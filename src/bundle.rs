//! Writes a "bundle": a digest of new postings as Markdown plus a JSON twin
//! that other tools (or a future web UI) can consume.

use crate::json::Json;
use crate::model::Job;
use std::fs;
use std::path::{Path, PathBuf};

pub fn markdown(title: &str, jobs: &[Job]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", title));
    out.push_str(&format!("{} new posting{}\n\n", jobs.len(), if jobs.len() == 1 { "" } else { "s" }));

    let mut sources: Vec<&str> = jobs.iter().map(|j| j.source.as_str()).collect();
    sources.sort();
    sources.dedup();

    for src in sources {
        let group: Vec<&Job> = jobs.iter().filter(|j| j.source == src).collect();
        out.push_str(&format!("## {} ({})\n\n", src, group.len()));
        for j in group {
            out.push_str(&format!("- **{}** — {}", md_escape(&j.title), md_escape(&j.company)));
            if !j.location.is_empty() {
                out.push_str(&format!(" · {}", md_escape(&j.location)));
            }
            if j.remote {
                out.push_str(" · remote");
            }
            if !j.salary.is_empty() {
                out.push_str(&format!(" · {}", md_escape(&j.salary)));
            }
            let date = j.posted_date();
            if !date.is_empty() {
                out.push_str(&format!(" · {}", date));
            }
            out.push('\n');
            out.push_str(&format!("  <{}>\n", j.url));
            if !j.tags.is_empty() {
                out.push_str(&format!("  tags: {}\n", j.tags.join(", ")));
            }
            if !j.summary.is_empty() {
                out.push_str(&format!("  {}\n", md_escape(&j.summary)));
            }
        }
        out.push('\n');
    }
    out
}

fn md_escape(s: &str) -> String {
    s.replace('*', "\\*").replace('_', "\\_").replace('[', "\\[").replace(']', "\\]")
}

pub fn json(title: &str, generated: &str, jobs: &[Job]) -> Json {
    let mut o = Json::obj();
    o.set("title", Json::Str(title.to_string()));
    o.set("generated", Json::Str(generated.to_string()));
    o.set("count", Json::Num(jobs.len() as f64));
    o.set("jobs", Json::Arr(jobs.iter().map(|j| j.to_json()).collect()));
    o
}

/// Write `<dir>/<stamp>.md` and `<dir>/<stamp>.json`.
pub fn write(dir: &Path, stamp: &str, generated: &str, jobs: &[Job]) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
    let title = format!("Job bundle {}", stamp.replace('_', " "));
    let md_path = dir.join(format!("{}.md", stamp));
    let json_path = dir.join(format!("{}.json", stamp));
    fs::write(&md_path, markdown(&title, jobs)).map_err(|e| format!("write {}: {}", md_path.display(), e))?;
    fs::write(&json_path, json(&title, generated, jobs).to_pretty()).map_err(|e| format!("write {}: {}", json_path.display(), e))?;
    Ok((md_path, json_path))
}

/// Existing bundle Markdown files, newest first.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "md").unwrap_or(false)).collect(),
        Err(_) => Vec::new(),
    };
    found.sort();
    found.reverse();
    found
}
