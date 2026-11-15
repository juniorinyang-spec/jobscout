//! User configuration, stored as JSON under `$JOBSCOUT_HOME` (default `~/.jobscout`).

use crate::json::Json;
use std::fs;
use std::path::{Path, PathBuf};

pub const ALL_SOURCES: [&str; 5] = ["remoteok", "remotive", "arbeitnow", "weworkremotely", "companies"];

/// A company whose careers page is hosted on a known applicant tracking
/// system (ATS). `ats` is one of `sources::companies::ATS`.
#[derive(Debug, Clone, PartialEq)]
pub struct Company {
    pub name: String,
    pub ats: String,
    pub slug: String,
}

impl Company {
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("name", Json::Str(self.name.clone()));
        o.set("ats", Json::Str(self.ats.clone()));
        o.set("slug", Json::Str(self.slug.clone()));
        o
    }

    pub fn from_json(v: &Json) -> Option<Company> {
        let ats = v.str_of("ats");
        let slug = v.str_of("slug");
        if ats.is_empty() || slug.is_empty() {
            return None;
        }
        let name = v.str_of("name");
        Some(Company { name: if name.is_empty() { slug.clone() } else { name }, ats, slug })
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub home: PathBuf,
    /// Any query matches -> job is kept. Each query's words must all appear.
    pub queries: Vec<String>,
    /// Any exclude phrase present -> job is dropped.
    pub exclude: Vec<String>,
    pub remote_only: bool,
    /// Drop postings older than this (0 = keep all).
    pub max_age_days: u64,
    pub sources: Vec<String>,
    pub companies: Vec<Company>,
    pub max_per_source: usize,
    /// Country code the user searches from ("US"), or "" for anywhere.
    /// Postings clearly located elsewhere are hidden; unclear ones are kept.
    pub location: String,
    /// Per-invocation window (`--since 24h`, MCP `posted_within_hours`):
    /// keep only postings dated within this many seconds, tracked companies
    /// included. Never saved to the config file.
    pub since_secs: Option<u64>,
    /// Per-invocation calendar day (`--on 2026-11-13`, MCP `posted_on`):
    /// [start, end) unix seconds. Never saved.
    pub on_day: Option<(i64, i64)>,
}

pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("JOBSCOUT_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(".jobscout")
}

impl Config {
    pub fn config_path(&self) -> PathBuf {
        self.home.join("config.json")
    }
    pub fn seen_path(&self) -> PathBuf {
        self.home.join("seen.json")
    }
    pub fn bundles_dir(&self) -> PathBuf {
        self.home.join("bundles")
    }
    pub fn source_enabled(&self, name: &str) -> bool {
        self.sources.iter().any(|s| s == name)
    }

    pub fn defaults(home: PathBuf) -> Config {
        Config {
            home,
            queries: vec!["software engineer".into()],
            exclude: vec![],
            remote_only: false,
            max_age_days: 14,
            // Tracked companies are the main place to search; the public
            // boards are opt-in (`jobscout source on remoteok`).
            sources: vec!["companies".into()],
            companies: vec![],
            max_per_source: 500,
            location: String::new(),
            since_secs: None,
            on_day: None,
        }
    }

    /// Load from disk; falls back to defaults when no config file exists yet.
    pub fn load() -> Result<Config, String> {
        let home = home_dir();
        let path = home.join("config.json");
        if !path.exists() {
            return Ok(Config::defaults(home));
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        let v = Json::parse(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
        Ok(Config::from_json(home, &v))
    }

    pub fn from_json(home: PathBuf, v: &Json) -> Config {
        let d = Config::defaults(home);
        let sources = match v.get("sources") {
            Some(Json::Obj(o)) => o.iter().filter(|(_, on)| on.as_bool().unwrap_or(false)).map(|(k, _)| k.clone()).collect(),
            Some(Json::Arr(_)) => v.strings_of("sources"),
            _ => d.sources.clone(),
        };
        let mut companies: Vec<Company> = v
            .get("companies")
            .and_then(|c| c.as_arr())
            .map(|a| a.iter().filter_map(Company::from_json).collect())
            .unwrap_or_default();
        // Older configs listed boards per ATS; fold them into `companies`.
        for (key, ats) in [("greenhouse_boards", "greenhouse"), ("lever_boards", "lever")] {
            for slug in v.strings_of(key) {
                if !companies.iter().any(|c| c.ats == ats && c.slug == slug) {
                    companies.push(Company { name: slug.clone(), ats: ats.into(), slug });
                }
            }
        }
        Config {
            home: d.home,
            queries: if v.get("queries").is_some() { v.strings_of("queries") } else { d.queries },
            exclude: v.strings_of("exclude"),
            remote_only: v.bool_of("remote_only").unwrap_or(d.remote_only),
            max_age_days: v.num_of("max_age_days").map(|n| n.max(0.0) as u64).unwrap_or(d.max_age_days),
            sources,
            companies,
            max_per_source: v.num_of("max_per_source").map(|n| n.max(1.0) as usize).unwrap_or(d.max_per_source),
            location: {
                let l = v.str_of("location");
                crate::location::resolve_country(&l).map(|c| c.to_string()).unwrap_or_default()
            },
            since_secs: None,
            on_day: None,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut sources = Json::obj();
        for s in ALL_SOURCES {
            sources.set(s, Json::Bool(self.source_enabled(s)));
        }
        let mut o = Json::obj();
        o.set("queries", Json::str_arr(&self.queries));
        o.set("exclude", Json::str_arr(&self.exclude));
        o.set("location", Json::Str(self.location.clone()));
        o.set("remote_only", Json::Bool(self.remote_only));
        o.set("max_age_days", Json::Num(self.max_age_days as f64));
        o.set("max_per_source", Json::Num(self.max_per_source as f64));
        o.set("sources", sources);
        o.set("companies", Json::Arr(self.companies.iter().map(|c| c.to_json()).collect()));
        o
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = self.config_path();
        fs::create_dir_all(&self.home).map_err(|e| format!("create {}: {}", self.home.display(), e))?;
        fs::write(&path, self.to_json().to_pretty()).map_err(|e| format!("write {}: {}", path.display(), e))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_old_board_lists() {
        let v = Json::parse(r#"{"greenhouse_boards":["stripe"],"lever_boards":["mistral"],"companies":[{"name":"Linear","ats":"ashby","slug":"linear"}]}"#).unwrap();
        let cfg = Config::from_json(PathBuf::from("/tmp/x"), &v);
        assert_eq!(cfg.companies.len(), 3);
        assert_eq!(cfg.companies[0].ats, "ashby");
        assert!(cfg.companies.iter().any(|c| c.ats == "greenhouse" && c.slug == "stripe"));
        let again = Config::from_json(PathBuf::from("/tmp/x"), &cfg.to_json());
        assert_eq!(again.companies, cfg.companies);
    }

    #[test]
    fn location_and_default_sources() {
        let d = Config::defaults(PathBuf::from("/tmp/x"));
        assert_eq!(d.sources, vec!["companies".to_string()]);
        assert_eq!(d.location, "");
        let v = Json::parse(r#"{"location":"United States"}"#).unwrap();
        let cfg = Config::from_json(PathBuf::from("/tmp/x"), &v);
        assert_eq!(cfg.location, "US");
        let again = Config::from_json(PathBuf::from("/tmp/x"), &cfg.to_json());
        assert_eq!(again.location, "US");
    }
}
