//! Hacker News "Ask HN: Who is hiring?" monthly thread, via the Algolia HN API.
//! Each top-level comment is one posting, conventionally formatted as
//! `Company | Role | Location | REMOTE | Salary`.

use super::{looks_remote, Source};
use crate::config::Config;
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;

pub struct HackerNews;

const API: &str = "https://hn.algolia.com/api/v1";

fn latest_thread_id() -> Result<String, String> {
    let url = format!("{}/search_by_date?tags=story,author_whoishiring&hitsPerPage=12", API);
    let body = http::get(&url)?;
    let root = Json::parse(&body)?;
    let hits = root.get("hits").and_then(|h| h.as_arr()).ok_or("hackernews: missing hits")?;
    hits.iter()
        .find(|h| h.str_of("title").to_lowercase().contains("who is hiring"))
        .map(|h| h.str_of("objectID"))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "hackernews: could not find the current 'Who is hiring' thread".to_string())
}

impl Source for HackerNews {
    fn name(&self) -> &'static str {
        "hackernews"
    }
    fn describe(&self) -> &'static str {
        "Hacker News monthly 'Who is hiring?' thread (Algolia API)"
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let story = latest_thread_id()?;
        let mut jobs = Vec::new();
        for page in 0..4 {
            let url = format!("{}/search_by_date?tags=comment,story_{}&hitsPerPage=500&page={}", API, story, page);
            let body = http::get(&url)?;
            let root = Json::parse(&body)?;
            let hits = root.get("hits").and_then(|h| h.as_arr()).ok_or("hackernews: missing hits")?;
            if hits.is_empty() {
                break;
            }
            for h in hits {
                // Only top-level comments are postings; replies are discussion.
                if h.str_of("parent_id") != story {
                    continue;
                }
                let id = h.str_of("objectID");
                let text = html::text_lines(&h.str_of("comment_text"));
                let mut lines = text.lines();
                let header = lines.next().unwrap_or("").trim().to_string();
                if header.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = header.split('|').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
                // Job seekers sometimes post their "Who wants to be hired" entry in
                // the wrong thread; those have no `|` header and start with
                // "Location:" or "SEEKING ...". Skip them.
                let lower = header.to_lowercase();
                if parts.len() < 2 && (lower.starts_with("location:") || lower.starts_with("seeking")) {
                    continue;
                }

                let mut j = Job::new("hackernews", id.clone());
                j.company = parts.first().map(|s| s.to_string()).unwrap_or_default();
                let rest: Vec<&str> = lines.collect();
                let mut title_idx = pick_title(&parts);
                // When the header names no role ("Fly.io | REMOTE | Full time"),
                // the roles are listed in the body; take the first short line
                // that names one, and keep every header segment as location.
                if let Some(i) = title_idx.filter(|&i| !names_role(parts[i])) {
                    if let Some(line) = role_line(&rest) {
                        j.title = line;
                        title_idx = None;
                    } else {
                        j.title = parts[i].to_string();
                    }
                } else {
                    j.title = match title_idx {
                        Some(i) => parts[i].to_string(),
                        None => html::truncate(&header, 120),
                    };
                }
                j.location = parts
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i > 0 && Some(*i) != title_idx)
                    .map(|(_, s)| *s)
                    .collect::<Vec<_>>()
                    .join(" · ");
                j.remote = looks_remote(&header);
                j.url = format!("https://news.ycombinator.com/item?id={}", id);
                j.set_posted_iso(&h.str_of("created_at"));
                j.tags.push(format!("by {}", h.str_of("author")));
                j.summary = html::truncate(&html::collapse_ws(&rest.join(" ")), 280);
                jobs.push(j);
                if jobs.len() >= cfg.max_per_source {
                    return Ok(jobs);
                }
            }
            let pages = root.num_of("nbPages").unwrap_or(1.0) as i64;
            if page as i64 + 1 >= pages {
                break;
            }
        }
        Ok(jobs)
    }
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || (s.contains('.') && !s.contains(' '))
}

const ROLE_WORDS: &[&str] = &[
    "engineer", "developer", "designer", "scientist", "manager", "architect", "analyst", "researcher",
    "programmer", "administrator", "specialist", "consultant", "recruiter", "director", "lead", "head of",
    "sre", "devops", "swe", "cto", "founding", "intern", "technician", "writer", "marketer", "accountant",
];

/// A segment that only describes the arrangement: remote/onsite, contract
/// type, salary, visa. Never a title.
fn is_meta(s: &str) -> bool {
    let l = s.to_lowercase();
    const META: &[&str] = &[
        "remote", "onsite", "on-site", "on site", "hybrid", "full-time", "full time", "fulltime", "part-time",
        "part time", "contract", "freelance", "visa", "relocation", "equity", "salary", "anywhere", "worldwide",
    ];
    let money = l.starts_with('$') || l.starts_with('€') || l.starts_with('£') || l.contains("k -") || l.contains("k–") || l.contains("k-");
    money || (META.iter().any(|m| l.contains(m)) && !ROLE_WORDS.iter().any(|r| l.contains(r)))
}

/// Which `|`-separated header segment is the job title. Headers follow
/// `Company | Role | Location | REMOTE | Salary` loosely, so prefer a
/// segment that names a role, then any segment that is not a URL or meta.
fn pick_title(parts: &[&str]) -> Option<usize> {
    let candidates = || (1..parts.len()).filter(|&i| !is_url(parts[i]));
    candidates()
        .find(|&i| names_role(parts[i]))
        .or_else(|| candidates().find(|&i| !is_meta(parts[i])))
        .or_else(|| candidates().next())
}

fn names_role(s: &str) -> bool {
    let l = s.to_lowercase();
    ROLE_WORDS.iter().any(|r| l.contains(r))
}

/// First short line near the top of the body that names a role, with list
/// bullets and a trailing colon or link stripped.
fn role_line(body: &[&str]) -> Option<String> {
    body.iter()
        .take(8)
        .map(|l| l.trim().trim_start_matches(['-', '*', '•', '·']).trim())
        .map(|l| l.split(" http").next().unwrap_or(l).trim().trim_end_matches([':', '-', '–']).trim())
        .find(|l| !l.is_empty() && l.chars().count() <= 90 && names_role(l))
        .map(|l| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title<'a>(header: &'a str) -> &'a str {
        let parts: Vec<&str> = header.split('|').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
        pick_title(&parts).map(|i| parts[i]).unwrap_or("")
    }

    #[test]
    fn skips_arrangement_segments() {
        assert_eq!(title("Bubblehouse | Fully REMOTE | Full-Time | $200–250k | Principal Engineer"), "Principal Engineer");
        assert_eq!(title("Acme | https://acme.com | Senior Rust Engineer | Remote (US)"), "Senior Rust Engineer");
        assert_eq!(title("Radar Labs | Software Engineers (SRE, ML) | Remote (US), NYC | Full Time"), "Software Engineers (SRE, ML)");
        // No role word anywhere: first non-meta segment.
        assert_eq!(title("Foo | REMOTE | Berlin | Growth"), "Berlin");
        // Only meta: still return something rather than nothing.
        assert_eq!(title("Foo | REMOTE"), "REMOTE");
    }

    #[test]
    fn role_from_body_when_header_has_none() {
        let body = ["We build a global cloud.", "", "- Senior Backend Engineer: https://fly.io/jobs/x", "- Designer"];
        assert_eq!(role_line(&body).as_deref(), Some("Senior Backend Engineer"));
        let long = ["We are looking for an engineer who loves distributed systems and has shipped at scale for many years, ideally in Rust or Go."];
        assert_eq!(role_line(&long), None);
    }
}
