//! The one data type every source produces.

use crate::json::Json;
use crate::time;

#[derive(Debug, Clone)]
pub struct Job {
    pub source: String,
    pub id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub remote: bool,
    pub url: String,
    /// ISO-8601 UTC, or "" when the source gives no date.
    pub posted: String,
    pub posted_unix: Option<i64>,
    pub tags: Vec<String>,
    pub salary: String,
    pub summary: String,
    /// Came from a company the user tracks. Such postings stay listed for
    /// as long as the role is open, so the age filter does not apply.
    /// Internal only; not written to bundle JSON.
    pub tracked: bool,
}

impl Job {
    pub fn new(source: &str, id: impl Into<String>) -> Job {
        Job {
            source: source.to_string(),
            id: id.into(),
            title: String::new(),
            company: String::new(),
            location: String::new(),
            remote: false,
            url: String::new(),
            posted: String::new(),
            posted_unix: None,
            tags: Vec::new(),
            salary: String::new(),
            summary: String::new(),
            tracked: false,
        }
    }

    /// Stable identity used for de-duplication across runs.
    pub fn key(&self) -> String {
        format!("{}:{}", self.source, self.id)
    }

    pub fn set_posted_unix(&mut self, secs: i64) {
        self.posted_unix = Some(secs);
        self.posted = time::unix_to_iso(secs);
    }

    pub fn set_posted_iso(&mut self, iso: &str) {
        self.posted_unix = time::iso_to_unix(iso);
        self.posted = match self.posted_unix {
            Some(s) => time::unix_to_iso(s),
            None => iso.trim().to_string(),
        };
    }

    pub fn posted_date(&self) -> String {
        match self.posted_unix {
            Some(s) => time::unix_to_local_date(s),
            None => self.posted.chars().take(10).collect(),
        }
    }

    /// Lower-cased text used by keyword filters.
    pub fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.title,
            self.company,
            self.location,
            self.tags.join(" "),
            self.salary,
            self.summary
        )
        .to_lowercase()
    }

    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("source", Json::Str(self.source.clone()));
        o.set("id", Json::Str(self.id.clone()));
        o.set("title", Json::Str(self.title.clone()));
        o.set("company", Json::Str(self.company.clone()));
        o.set("location", Json::Str(self.location.clone()));
        o.set("remote", Json::Bool(self.remote));
        o.set("url", Json::Str(self.url.clone()));
        o.set("posted", Json::Str(self.posted.clone()));
        o.set("tags", Json::str_arr(&self.tags));
        o.set("salary", Json::Str(self.salary.clone()));
        o.set("summary", Json::Str(self.summary.clone()));
        o
    }

    /// Inverse of `to_json`; lets bundle JSON files be read back later.
    #[allow(dead_code)]
    pub fn from_json(v: &Json) -> Option<Job> {
        let source = v.str_of("source");
        let id = v.str_of("id");
        if source.is_empty() || id.is_empty() {
            return None;
        }
        let mut j = Job::new(&source, id);
        j.title = v.str_of("title");
        j.company = v.str_of("company");
        j.location = v.str_of("location");
        j.remote = v.bool_of("remote").unwrap_or(false);
        j.url = v.str_of("url");
        j.set_posted_iso(&v.str_of("posted"));
        j.tags = v.strings_of("tags");
        j.salary = v.str_of("salary");
        j.summary = v.str_of("summary");
        Some(j)
    }
}

/// Normalise a URL for cross-source de-duplication: drop scheme, `www.`,
/// fragment, tracking parameters and trailing slash. The rest of the query
/// string is kept because some boards identify postings by it.
pub fn url_identity(url: &str) -> String {
    let u = url.trim().to_lowercase();
    let u = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(&u);
    let u = u.strip_prefix("www.").unwrap_or(u);
    let u = u.split('#').next().unwrap_or(u);
    let (path, query) = match u.split_once('?') {
        Some((p, q)) => (p, q),
        None => (u, ""),
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| !kv.is_empty() && !kv.starts_with("utm_") && !kv.starts_with("ref=") && !kv.starts_with("source="))
        .collect();
    let path = path.trim_end_matches('/');
    if kept.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, kept.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_identity_keeps_ids() {
        assert_eq!(url_identity("https://news.ycombinator.com/item?id=1"), "news.ycombinator.com/item?id=1");
        assert_ne!(url_identity("https://news.ycombinator.com/item?id=1"), url_identity("https://news.ycombinator.com/item?id=2"));
        assert_eq!(url_identity("HTTP://www.Example.com/a/?utm_source=x#top"), "example.com/a");
    }

    #[test]
    fn json_round_trip() {
        let mut j = Job::new("src", "42");
        j.title = "T".into();
        j.tags = vec!["a".into()];
        j.set_posted_unix(86_400);
        let back = Job::from_json(&j.to_json()).unwrap();
        assert_eq!(back.key(), "src:42");
        assert_eq!(back.posted_unix, Some(86_400));
        assert_eq!(back.tags, vec!["a".to_string()]);
    }
}
