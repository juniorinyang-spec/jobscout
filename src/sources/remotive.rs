//! Remotive public API: https://remotive.com/api/remote-jobs

use super::Source;
use crate::config::Config;
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;
use std::collections::HashSet;

pub struct Remotive;

impl Source for Remotive {
    fn name(&self) -> &'static str {
        "remotive"
    }
    fn describe(&self) -> &'static str {
        "remotive.com API (remote jobs; searched per configured query)"
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        // One request per query keeps results relevant; no queries = latest feed.
        let mut urls: Vec<String> = cfg
            .queries
            .iter()
            .filter(|q| !q.trim().is_empty())
            .map(|q| format!("https://remotive.com/api/remote-jobs?limit=100&search={}", http::url_encode(q)))
            .collect();
        if urls.is_empty() {
            urls.push("https://remotive.com/api/remote-jobs?limit=200".to_string());
        }

        let mut seen = HashSet::new();
        let mut jobs = Vec::new();
        let mut last_err = None;
        for url in urls {
            let body = match http::get(&url) {
                Ok(b) => b,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };
            let root = Json::parse(&body)?;
            let items = root.get("jobs").and_then(|j| j.as_arr()).ok_or("remotive: missing jobs array")?;
            for it in items {
                let id = it.str_of("id");
                if id.is_empty() || !seen.insert(id.clone()) {
                    continue;
                }
                let mut j = Job::new("remotive", id);
                j.title = it.str_of("title");
                j.company = it.str_of("company_name");
                j.location = it.str_of("candidate_required_location");
                j.remote = true;
                j.url = it.str_of("url");
                j.set_posted_iso(&it.str_of("publication_date"));
                j.tags = it.strings_of("tags");
                let category = it.str_of("category");
                if !category.is_empty() {
                    j.tags.push(category);
                }
                let kind = it.str_of("job_type");
                if !kind.is_empty() {
                    j.tags.push(kind);
                }
                j.salary = it.str_of("salary");
                j.summary = html::truncate(&html::text(&it.str_of("description")), 240);
                jobs.push(j);
                if jobs.len() >= cfg.max_per_source {
                    return Ok(jobs);
                }
            }
        }
        if jobs.is_empty() {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(jobs)
    }
}
