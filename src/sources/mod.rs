//! Job board adapters. Each one turns a public feed/API into `Vec<Job>`.
//! Adding a board = one file implementing `Source`, plus a line in `all()`.

use crate::config::Config;
use crate::model::Job;

pub mod arbeitnow;
pub mod companies;
pub mod hackernews;
pub mod remoteok;
pub mod remotive;
pub mod weworkremotely;

pub trait Source {
    /// Short identifier used in config, CLI flags and job keys.
    fn name(&self) -> &'static str;
    /// One-line description for `jobscout sources`.
    fn describe(&self) -> &'static str;
    /// Why this source cannot run with the given config, if anything.
    fn unconfigured(&self, _cfg: &Config) -> Option<&'static str> {
        None
    }
    /// Fetch the current postings. Keyword filtering happens later, but a
    /// source may use `cfg.queries` to narrow server-side searches.
    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String>;
}

pub fn all() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(remoteok::RemoteOk),
        Box::new(remotive::Remotive),
        Box::new(arbeitnow::Arbeitnow),
        Box::new(weworkremotely::WeWorkRemotely),
        Box::new(hackernews::HackerNews),
        Box::new(companies::Companies),
    ]
}

/// Shared heuristic: does this location/text describe a remote role?
pub fn looks_remote(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("remote") || t.contains("anywhere") || t.contains("worldwide") || t.contains("work from home")
}
