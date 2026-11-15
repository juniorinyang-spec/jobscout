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
                // Title = first segment after the company that is not a bare URL.
                let is_url = |s: &str| s.starts_with("http://") || s.starts_with("https://") || (s.contains('.') && !s.contains(' '));
                let title_idx = (1..parts.len()).find(|&i| !is_url(parts[i]));
                j.title = match title_idx {
                    Some(i) => parts[i].to_string(),
                    None => html::truncate(&header, 120),
                };
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
                let rest: Vec<&str> = lines.collect();
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
