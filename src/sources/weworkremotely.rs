//! We Work Remotely RSS feed: https://weworkremotely.com/remote-jobs.rss

use super::Source;
use crate::config::Config;
use crate::html;
use crate::http;
use crate::model::Job;
use crate::time;

pub struct WeWorkRemotely;

impl Source for WeWorkRemotely {
    fn name(&self) -> &'static str {
        "weworkremotely"
    }
    fn describe(&self) -> &'static str {
        "weworkremotely.com RSS feed (all categories)"
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let body = http::get("https://weworkremotely.com/remote-jobs.rss")?;
        let mut jobs = Vec::new();
        for item in html::elements(&body, "item") {
            let link = html::element(item, "link").unwrap_or_default();
            let guid = html::element(item, "guid").unwrap_or_default();
            let id = if !guid.is_empty() { guid } else { link.clone() };
            if id.is_empty() {
                continue;
            }
            let raw_title = html::element(item, "title").unwrap_or_default();
            // Titles look like "Company: Job title".
            let (company, title) = match raw_title.split_once(':') {
                Some((c, t)) => (c.trim().to_string(), t.trim().to_string()),
                None => (String::new(), raw_title.trim().to_string()),
            };
            let mut j = Job::new("weworkremotely", id);
            j.title = title;
            j.company = company;
            j.location = html::element(item, "region").unwrap_or_default();
            j.remote = true;
            j.url = link;
            if let Some(ts) = html::element(item, "pubDate").and_then(|d| time::rfc2822_to_unix(&d)) {
                j.set_posted_unix(ts);
            }
            for tag in ["category", "type"] {
                if let Some(v) = html::element(item, tag) {
                    if !v.trim().is_empty() {
                        j.tags.push(v.trim().to_string());
                    }
                }
            }
            if let Some(desc) = html::elements(item, "description").first() {
                j.summary = html::truncate(&html::text(&html::unwrap_cdata(desc)), 240);
            }
            jobs.push(j);
            if jobs.len() >= cfg.max_per_source {
                break;
            }
        }
        Ok(jobs)
    }
}
