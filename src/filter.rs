//! Keyword / recency / remote filtering.

use crate::config::Config;
use crate::model::Job;

#[derive(Debug, Clone)]
pub struct Filter {
    pub queries: Vec<String>,
    pub exclude: Vec<String>,
    pub remote_only: bool,
    pub max_age_days: u64,
}

impl Filter {
    pub fn from_config(cfg: &Config) -> Filter {
        Filter {
            queries: cfg.queries.iter().map(|q| q.to_lowercase()).collect(),
            exclude: cfg.exclude.iter().map(|q| q.to_lowercase()).collect(),
            remote_only: cfg.remote_only,
            max_age_days: cfg.max_age_days,
        }
    }

    pub fn keep(&self, job: &Job, now: i64) -> bool {
        if self.remote_only && !job.remote {
            return false;
        }
        // A tracked company's board lists only open roles, and a role opened
        // months ago is still worth seeing; the age limit is for job boards.
        if self.max_age_days > 0 && !job.tracked {
            if let Some(posted) = job.posted_unix {
                if now - posted > (self.max_age_days as i64) * 86_400 {
                    return false;
                }
            }
        }
        let hay = job.haystack();
        if !self.exclude.is_empty() {
            // "go to market" must also drop "Go-to-Market" (and vice versa):
            // compare exclude phrases with hyphens read as spaces.
            let flat = dehyphen(&hay);
            if self.exclude.iter().map(|x| dehyphen(x)).any(|x| !x.is_empty() && contains_word(&flat, &x)) {
                return false;
            }
        }
        let queries: Vec<&String> = self.queries.iter().filter(|q| !q.trim().is_empty()).collect();
        if queries.is_empty() {
            return true;
        }
        queries.iter().any(|q| q.split_whitespace().all(|word| contains_word(&hay, word)))
    }
}

/// Hyphens as spaces, whitespace collapsed: `go-to-market` -> `go to market`.
fn dehyphen(s: &str) -> String {
    s.replace('-', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whole-word match: `rust` must not match `trust`. A word boundary is any
/// non-alphanumeric character, so `c++`, `node.js` and `go` work as expected.
pub fn contains_word(hay: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = hay[from..].find(word) {
        let start = from + pos;
        let end = start + word.len();
        let before_ok = hay[..start].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        let after_ok = hay[end..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        from = start + hay[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(title: &str, remote: bool, age_days: i64) -> Job {
        let mut j = Job::new("test", "1");
        j.title = title.into();
        j.remote = remote;
        j.set_posted_unix(1_000_000 - age_days * 86_400);
        j
    }

    fn filter(q: &[&str], ex: &[&str]) -> Filter {
        Filter {
            queries: q.iter().map(|s| s.to_string()).collect(),
            exclude: ex.iter().map(|s| s.to_string()).collect(),
            remote_only: false,
            max_age_days: 7,
        }
    }

    #[test]
    fn keyword_logic() {
        let f = filter(&["rust backend", "go"], &["senior staff"]);
        assert!(f.keep(&job("Backend Engineer (Rust)", true, 1), 1_000_000));
        assert!(!f.keep(&job("Backend Engineer (Python)", true, 1), 1_000_000));
        assert!(f.keep(&job("Go developer", true, 1), 1_000_000));
        assert!(!f.keep(&job("Senior Staff Rust backend", true, 1), 1_000_000));
    }

    #[test]
    fn whole_words() {
        assert!(contains_word("senior rust engineer", "rust"));
        assert!(!contains_word("trust & safety", "rust"));
        assert!(contains_word("c++ developer", "c++"));
        assert!(contains_word("node.js / go", "go"));
        assert!(!contains_word("django", "go"));
        assert!(contains_word("ingénieur rust", "rust"));
        let f = filter(&["rust"], &[]);
        assert!(!f.keep(&job("Trust Engineer", true, 1), 1_000_000));
    }

    #[test]
    fn age_and_remote() {
        let mut f = filter(&[], &[]);
        assert!(f.keep(&job("x", false, 3), 1_000_000));
        assert!(!f.keep(&job("x", false, 30), 1_000_000));
        f.remote_only = true;
        assert!(!f.keep(&job("x", false, 1), 1_000_000));
        assert!(f.keep(&job("x", true, 1), 1_000_000));
    }

    #[test]
    fn tracked_companies_ignore_age() {
        let f = filter(&[], &[]);
        let mut j = job("Product Engineer", false, 200);
        assert!(!f.keep(&j, 1_000_000));
        j.tracked = true;
        assert!(f.keep(&j, 1_000_000));
    }

    #[test]
    fn exclude_is_whole_word_phrase() {
        let f = filter(&["go"], &["go to market", "intern"]);
        assert!(f.keep(&job("Go developer", true, 1), 1_000_000));
        assert!(!f.keep(&job("Senior Associate Go To Market", true, 1), 1_000_000));
        assert!(!f.keep(&job("Senior Associate - Go-to-Market Dämmung", true, 1), 1_000_000));
        assert!(!f.keep(&job("Go-To-Market  & Growth Intern", true, 1), 1_000_000));
        let f2 = filter(&["go"], &["go-to-market"]);
        assert!(!f2.keep(&job("Go To Market lead", true, 1), 1_000_000));
        assert!(f2.keep(&job("Go backend engineer", true, 1), 1_000_000));
        assert!(!f.keep(&job("Go intern", true, 1), 1_000_000));
        assert!(f.keep(&job("Go engineer, internal tools", true, 1), 1_000_000));
    }
}
