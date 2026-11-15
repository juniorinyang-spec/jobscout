//! Arbeitnow job board API: https://www.arbeitnow.com/api/job-board-api

use super::{looks_remote, Source};
use crate::config::Config;
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;

pub struct Arbeitnow;

impl Source for Arbeitnow {
    fn name(&self) -> &'static str {
        "arbeitnow"
    }
    fn describe(&self) -> &'static str {
        "arbeitnow.com API (Europe-heavy board, remote + on-site)"
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let mut jobs = Vec::new();
        // The API is paginated; grab a few pages of the newest postings.
        for page in 1..=3 {
            let body = http::get(&format!("https://www.arbeitnow.com/api/job-board-api?page={}", page))?;
            let root = Json::parse(&body)?;
            let items = root.get("data").and_then(|d| d.as_arr()).ok_or("arbeitnow: missing data array")?;
            if items.is_empty() {
                break;
            }
            for it in items {
                let id = it.str_of("slug");
                if id.is_empty() {
                    continue;
                }
                let mut j = Job::new("arbeitnow", id);
                j.title = it.str_of("title");
                j.company = it.str_of("company_name");
                j.location = it.str_of("location");
                j.remote = it.bool_of("remote").unwrap_or(false) || looks_remote(&j.location);
                j.url = it.str_of("url");
                if let Some(ts) = it.num_of("created_at") {
                    j.set_posted_unix(ts as i64);
                }
                j.tags = it.strings_of("tags");
                j.tags.extend(it.strings_of("job_types"));
                j.summary = html::truncate(&html::text(&it.str_of("description")), 240);
                jobs.push(j);
                if jobs.len() >= cfg.max_per_source {
                    return Ok(jobs);
                }
            }
            if root.path(&["links", "next"]).map(|n| n.to_text().is_empty()).unwrap_or(true) {
                break;
            }
        }
        Ok(jobs)
    }
}
