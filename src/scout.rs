//! The engine shared by the CLI and the MCP server: fetch, filter, diff
//! against memory, bundle, and manage companies/queries.
//!
//! Nothing in here writes to stdout. Progress goes to stderr (which the MCP
//! stdio transport allows) and results are returned as values.

use crate::bundle;
use crate::config::{Company, Config};
use crate::filter::Filter;
use crate::http;
use crate::model::{self, Job};
use crate::sources;
use crate::store::Seen;
use crate::time;
use std::collections::HashSet;
use std::path::PathBuf;

/// Fetch from every enabled (or explicitly requested) source, de-duplicated.
pub fn collect(cfg: &Config, only: &[String], quiet: bool) -> Result<Vec<Job>, String> {
    if !http::curl_available() {
        return Err("`curl` was not found on PATH; jobscout uses it for HTTPS".into());
    }
    let all = sources::all();
    for name in only {
        if !all.iter().any(|s| s.name() == name) {
            return Err(format!("unknown source '{}' (see `jobscout sources`)", name));
        }
    }

    let mut jobs: Vec<Job> = Vec::new();
    let mut keys: HashSet<String> = HashSet::new();
    let mut urls: HashSet<String> = HashSet::new();
    let mut failures = 0;
    let mut used = 0;

    for src in &all {
        let wanted = if only.is_empty() { cfg.source_enabled(src.name()) } else { only.iter().any(|n| n == src.name()) };
        if !wanted {
            continue;
        }
        if let Some(why) = src.unconfigured(cfg) {
            if !quiet || !only.is_empty() {
                eprintln!("  {:<15} skipped ({})", src.name(), why);
            }
            continue;
        }
        used += 1;
        if !quiet {
            eprint!("  {:<15} ", src.name());
        }
        match src.fetch(cfg) {
            Ok(found) => {
                let mut added = 0;
                for j in found {
                    let uid = model::url_identity(&j.url);
                    if !keys.insert(j.key()) || (!uid.is_empty() && !urls.insert(uid)) {
                        continue;
                    }
                    jobs.push(j);
                    added += 1;
                }
                if !quiet {
                    eprintln!("{} postings", added);
                }
            }
            Err(e) => {
                failures += 1;
                if !quiet {
                    eprintln!("FAILED: {}", e);
                } else {
                    eprintln!("  {}: {}", src.name(), e);
                }
            }
        }
    }
    if used == 0 {
        return Err("no sources enabled; edit config or pass --source".into());
    }
    if failures == used {
        return Err("every source failed; check your network connection".into());
    }
    Ok(jobs)
}

pub fn sort_newest_first(jobs: &mut [Job]) {
    jobs.sort_by(|a, b| b.posted_unix.unwrap_or(i64::MIN).cmp(&a.posted_unix.unwrap_or(i64::MIN)).then_with(|| a.title.cmp(&b.title)));
}

/// One-off search: fetch, filter with the config's (possibly overridden)
/// queries, sort newest first, cap. Does not touch memory.
pub fn search(cfg: &Config, only: &[String], limit: usize, quiet: bool) -> Result<Vec<Job>, String> {
    let filt = Filter::from_config(cfg);
    let now = time::now_unix();
    let mut jobs = collect(cfg, only, quiet)?;
    jobs.retain(|j| filt.keep(j, now));
    sort_newest_first(&mut jobs);
    jobs.truncate(limit);
    Ok(jobs)
}

pub struct RunOutcome {
    pub fetched: usize,
    pub matched: usize,
    pub new: Vec<Job>,
    pub bundle: Option<PathBuf>,
    pub dry_run: bool,
}

/// The scouting run: fetch, filter, keep only postings not seen before,
/// write a bundle and remember them (unless `dry_run`).
pub fn run(cfg: &Config, only: &[String], limit: Option<usize>, dry_run: bool, quiet: bool) -> Result<RunOutcome, String> {
    let filt = Filter::from_config(cfg);
    let now = time::now_unix();
    let mut seen = Seen::load(&cfg.seen_path())?;

    let mut jobs = collect(cfg, only, quiet)?;
    let fetched = jobs.len();
    jobs.retain(|j| filt.keep(j, now));
    let matched = jobs.len();
    jobs.retain(|j| !seen.contains(&j.key()));
    sort_newest_first(&mut jobs);
    if let Some(n) = limit {
        jobs.truncate(n);
    }

    let mut bundle_path = None;
    if !jobs.is_empty() && !dry_run {
        let generated = time::now_iso();
        let (md, _) = bundle::write(&cfg.bundles_dir(), &time::now_stamp(), &generated, &jobs)?;
        for j in &jobs {
            seen.mark(&j.key(), &generated);
        }
        seen.save()?;
        bundle_path = Some(md);
    }
    Ok(RunOutcome { fetched, matched, new: jobs, bundle: bundle_path, dry_run })
}

pub enum AddOutcome {
    Added(Company, usize),
    AlreadyTracked(Company),
}

/// Resolve `input` (name or careers URL) and add it to the config.
pub fn add_company(cfg: &mut Config, input: &str) -> Result<AddOutcome, String> {
    if !http::curl_available() {
        return Err("`curl` was not found on PATH; jobscout uses it for HTTPS".into());
    }
    let (c, count) = sources::companies::detect(input)?;
    if let Some(existing) = cfg.companies.iter().find(|x| x.ats == c.ats && x.slug == c.slug) {
        return Ok(AddOutcome::AlreadyTracked(existing.clone()));
    }
    cfg.companies.push(c.clone());
    if !cfg.source_enabled("companies") {
        cfg.sources.push("companies".into());
    }
    cfg.save()?;
    Ok(AddOutcome::Added(c, count))
}

/// Remove by name or slug (case-insensitive). Returns whether anything matched.
pub fn remove_company(cfg: &mut Config, target: &str) -> Result<bool, String> {
    let t = target.to_lowercase();
    let before = cfg.companies.len();
    cfg.companies.retain(|c| c.name.to_lowercase() != t && c.slug.to_lowercase() != t);
    let removed = cfg.companies.len() != before;
    if removed {
        cfg.save()?;
    }
    Ok(removed)
}

/// Returns false when the query was already present.
pub fn add_query(cfg: &mut Config, phrase: &str) -> Result<bool, String> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return Err("query must not be empty".into());
    }
    if cfg.queries.iter().any(|q| q.eq_ignore_ascii_case(phrase)) {
        return Ok(false);
    }
    cfg.queries.push(phrase.to_string());
    cfg.save()?;
    Ok(true)
}

/// Returns false when no such query existed.
pub fn remove_query(cfg: &mut Config, phrase: &str) -> Result<bool, String> {
    let phrase = phrase.trim();
    let before = cfg.queries.len();
    cfg.queries.retain(|q| !q.eq_ignore_ascii_case(phrase));
    let removed = cfg.queries.len() != before;
    if removed {
        cfg.save()?;
    }
    Ok(removed)
}

/// Human-readable listing used by both the CLI (after its own header) and
/// MCP tool results.
pub fn format_jobs(jobs: &[Job], with_summary: bool) -> String {
    let mut out = String::new();
    for (i, j) in jobs.iter().enumerate() {
        let mut meta: Vec<String> = Vec::new();
        if !j.company.is_empty() {
            meta.push(j.company.clone());
        }
        if !j.location.is_empty() {
            meta.push(j.location.clone());
        }
        if j.remote {
            meta.push("remote".into());
        }
        if !j.salary.is_empty() {
            meta.push(j.salary.clone());
        }
        let date = j.posted_date();
        if !date.is_empty() {
            meta.push(date);
        }
        out.push_str(&format!("{}. {} [{}]\n", i + 1, j.title, j.source));
        if !meta.is_empty() {
            out.push_str(&format!("   {}\n", meta.join(" · ")));
        }
        out.push_str(&format!("   {}\n", j.url));
        if with_summary && !j.summary.is_empty() {
            out.push_str(&format!("   {}\n", j.summary));
        }
    }
    out
}
