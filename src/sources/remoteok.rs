//! RemoteOK public JSON feed: https://remoteok.com/api

use super::{looks_remote, Source};
use crate::config::Config;
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;

pub struct RemoteOk;

impl Source for RemoteOk {
    fn name(&self) -> &'static str {
        "remoteok"
    }
    fn describe(&self) -> &'static str {
        "remoteok.com public JSON feed (remote jobs, worldwide)"
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let body = http::get("https://remoteok.com/api")?;
        let root = Json::parse(&body)?;
        let items = root.as_arr().ok_or("remoteok: expected a JSON array")?;
        let mut jobs = Vec::new();
        for it in items {
            // The first element is a legal notice object without an id.
            let id = it.str_of("id");
            if id.is_empty() || it.get("position").is_none() {
                continue;
            }
            let mut j = Job::new("remoteok", id);
            j.title = it.str_of("position");
            j.company = it.str_of("company");
            j.location = it.str_of("location");
            j.remote = true;
            j.url = it.str_of("url");
            if j.url.is_empty() {
                j.url = format!("https://remoteok.com/remote-jobs/{}", it.str_of("slug"));
            }
            if let Some(epoch) = it.num_of("epoch") {
                j.set_posted_unix(epoch as i64);
            } else {
                j.set_posted_iso(&it.str_of("date"));
            }
            j.tags = it.strings_of("tags");
            let min = it.num_of("salary_min").unwrap_or(0.0);
            let max = it.num_of("salary_max").unwrap_or(0.0);
            if min > 0.0 || max > 0.0 {
                j.salary = format!("${}k–${}k", (min / 1000.0) as i64, (max / 1000.0) as i64);
            }
            j.summary = html::truncate(&html::text(&it.str_of("description")), 240);
            if !j.remote {
                j.remote = looks_remote(&j.location);
            }
            jobs.push(j);
            if jobs.len() >= cfg.max_per_source {
                break;
            }
        }
        Ok(jobs)
    }
}
