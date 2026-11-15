//! Any careers site that publishes schema.org `JobPosting` data.
//!
//! Many company careers sites put a machine-readable `JobPosting` (JSON-LD)
//! on every job page, because that is what Google for Jobs reads. That makes
//! one reader work for many companies without an adapter each:
//!
//! 1. find job page URLs: links on the careers page the user gave, and the
//!    site's sitemaps (from robots.txt, else /sitemap.xml), newest first;
//! 2. read each job page's JSON-LD once and cache it, so later runs fetch
//!    only new postings (a few pages at a time, a capped number per run);
//! 3. drop postings whose URL left the sitemap or whose `validThrough` passed.
//!
//! robots.txt `Disallow` rules for all user agents are respected.

use crate::config::Company;
use crate::html;
use crate::http;
use crate::json::Json;
use crate::location;
use crate::model::Job;
use crate::time;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Job pages fetched in parallel. Kept low: careers sites rate-limit, and a
/// tool many people run must not look like an attack.
const WORKERS: usize = 2;
/// Pause between one worker's requests, unless robots.txt asks for more.
const DEFAULT_DELAY_MS: u64 = 400;
/// New job pages read per company per run; the rest wait for the next run.
const NEW_PAGES_PER_RUN: usize = 120;
/// Pages sampled by `company add` to confirm the site has JobPosting data.
const DETECT_SAMPLE: usize = 4;

pub struct Found {
    pub jobs: Vec<Job>,
    /// Job URLs discovered (the board's size, for `company add`).
    pub total: usize,
    /// URLs not read yet because of the per-run cap.
    pub pending: usize,
}

// ------------------------------------------------------------ discovery

fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| format!("{}://{}", scheme.to_lowercase(), host.to_lowercase()))
}

fn host_of(url: &str) -> String {
    url.split_once("://").map(|(_, r)| r.split(['/', '?', '#']).next().unwrap_or("").to_lowercase()).unwrap_or_default()
}

/// Resolve `href` against the page it appeared on.
fn join_url(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || href.starts_with("mailto:") || href.starts_with("javascript:") || href.starts_with("tel:") {
        return None;
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    let origin = origin_of(base)?;
    if let Some(rest) = href.strip_prefix("//") {
        return Some(format!("{}://{}", base.split("://").next()?, rest));
    }
    if href.starts_with('/') {
        return Some(format!("{}{}", origin, href));
    }
    // Relative path: replace everything after the last '/' of the base path.
    let path = base[origin.len()..].split(['?', '#']).next().unwrap_or("");
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in href.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    Some(format!("{}/{}", origin, parts.join("/")))
}

/// Does this URL look like a single job's page (not a search or team page)?
fn looks_like_job_url(url: &str) -> bool {
    let path = url.split_once("://").and_then(|(_, r)| r.split_once('/')).map(|(_, p)| p).unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("").to_lowercase();
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let jobby = ["job", "jobs", "career", "careers", "position", "positions", "opening", "openings", "vacancy", "vacancies", "role", "roles", "requisition", "posting", "postings", "details", "detail", "opportunity"];
    let has_marker = segs.iter().any(|s| jobby.contains(s));
    let last = segs.last().copied().unwrap_or("");
    let idish = segs.iter().any(|s| s.chars().filter(|c| c.is_ascii_digit()).count() >= 4) || last.matches('-').count() >= 2;
    has_marker && idish && segs.len() >= 2
}

/// robots.txt rules for every user agent (`User-agent: *`): (allow, pattern).
fn robots_rules(robots: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut applies = false;
    let mut last_was_agent = false;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim().to_lowercase(), v.trim());
        match k.as_str() {
            "user-agent" => {
                if !last_was_agent {
                    applies = false;
                }
                applies |= v == "*";
                last_was_agent = true;
            }
            "disallow" | "allow" => {
                last_was_agent = false;
                if applies && !v.is_empty() {
                    out.push((k == "allow", v.to_string()));
                }
            }
            _ => last_was_agent = false,
        }
    }
    out
}

/// `Crawl-delay:` for every user agent, in milliseconds (capped at 10 s).
fn crawl_delay_ms(robots: &str) -> Option<u64> {
    let mut applies = false;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once(':') else { continue };
        match k.trim().to_lowercase().as_str() {
            "user-agent" => applies = v.trim() == "*",
            "crawl-delay" if applies => {
                return v.trim().parse::<f64>().ok().filter(|d| *d > 0.0).map(|d| ((d * 1000.0) as u64).min(10_000));
            }
            _ => {}
        }
    }
    None
}

/// Does a robots pattern (`*` wildcard, trailing `$` anchor) match `path`?
fn robots_match(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if let Some(f) = path[pos..].find(part) {
            pos += f + part.len();
        } else {
            return false;
        }
    }
    !anchored || pos == path.len() || parts.last().map(|p| p.is_empty()).unwrap_or(false)
}

/// Standard precedence: the longest matching rule wins; Allow wins a tie.
fn allowed(url: &str, rules: &[(bool, String)]) -> bool {
    let path = url.split_once("://").and_then(|(_, r)| r.find('/').map(|i| &r[i..])).unwrap_or("/");
    let mut best: Option<(usize, bool)> = None;
    for (allow, pat) in rules {
        if robots_match(pat, path) {
            let len = pat.len();
            best = match best {
                Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                _ => Some((len, *allow)),
            };
        }
    }
    best.map(|(_, a)| a).unwrap_or(true)
}

/// `<loc>` entries, each with its `<lastmod>` (unix seconds) when present.
fn sitemap_entries(xml: &str) -> Vec<(String, Option<i64>)> {
    let mut out = Vec::new();
    for chunk in xml.split("<loc>").skip(1) {
        let Some(end) = chunk.find("</loc>") else { continue };
        let loc = html::decode_entities(chunk[..end].trim()).replace("<![CDATA[", "").replace("]]>", "").trim().to_string();
        let lastmod = chunk.find("<lastmod>").and_then(|s| {
            let rest = &chunk[s + 9..];
            rest.find("</lastmod>").and_then(|e| time::iso_to_unix(rest[..e].trim()))
        });
        out.push((loc, lastmod));
    }
    out
}

/// Job page URLs from the site's sitemaps, newest `lastmod` first.
fn sitemap_job_urls(origin: &str, robots: &str) -> Vec<(String, Option<i64>)> {
    let mut queue: Vec<String> = robots
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim().eq_ignore_ascii_case("sitemap")).then(|| v.trim().to_string())
        })
        .collect();
    if queue.is_empty() {
        queue.push(format!("{}/sitemap.xml", origin));
    }
    let mut seen_maps = HashSet::new();
    let mut jobs: Vec<(String, Option<i64>)> = Vec::new();
    let mut fetched = 0;
    while let Some(sm) = queue.pop() {
        if !seen_maps.insert(sm.clone()) || fetched >= 12 || sm.ends_with(".gz") {
            continue;
        }
        fetched += 1;
        let Ok(xml) = http::get(&sm) else { continue };
        let entries = sitemap_entries(&xml);
        if xml.get(..3000).unwrap_or(&xml).contains("<sitemapindex") {
            // Prefer the job sitemaps of a big site's index.
            let mut subs: Vec<&(String, Option<i64>)> =
                entries.iter().filter(|(u, _)| ["job", "career", "position", "posting", "requisition"].iter().any(|k| u.to_lowercase().contains(k))).collect();
            if subs.is_empty() {
                subs = entries.iter().take(4).collect();
            }
            for (u, _) in subs.into_iter().take(8) {
                queue.push(u.clone());
            }
        } else {
            jobs.extend(entries.into_iter().filter(|(u, _)| looks_like_job_url(u) && origin_of(u).as_deref() == Some(origin)));
        }
    }
    jobs.sort_by(|a, b| b.1.unwrap_or(0).cmp(&a.1.unwrap_or(0)));
    let mut seen = HashSet::new();
    jobs.retain(|(u, _)| seen.insert(u.clone()));
    jobs
}

/// Job page links on the careers page itself (same site only).
fn page_job_links(page_url: &str, page: &str) -> Vec<String> {
    let host = host_of(page_url);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for chunk in page.split("href=").skip(1) {
        let q = chunk.chars().next().unwrap_or(' ');
        if q != '"' && q != '\'' {
            continue;
        }
        let Some(end) = chunk[1..].find(q) else { continue };
        let href = html::decode_entities(&chunk[1..1 + end]);
        if let Some(u) = join_url(page_url, &href) {
            let u = u.split('#').next().unwrap_or(&u).to_string();
            if host_of(&u) == host && looks_like_job_url(&u) && seen.insert(u.clone()) {
                out.push(u);
            }
        }
    }
    out
}

// ------------------------------------------------------------ JSON-LD

/// Every `JobPosting` object in the page's JSON-LD blocks.
pub fn job_postings(page: &str) -> Vec<Json> {
    let lower = page.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = lower[from..].find("application/ld+json") {
        let at = from + p;
        let Some(open_end) = lower[at..].find('>').map(|i| at + i + 1) else { break };
        let Some(close) = lower[open_end..].find("</script>").map(|i| open_end + i) else { break };
        let raw = page[open_end..close].trim();
        let raw = raw.trim_start_matches("<!--").trim_end_matches("-->").trim();
        let parsed = Json::parse(raw).or_else(|_| Json::parse(&html::decode_entities(raw)));
        if let Ok(v) = parsed {
            collect_postings(&v, &mut out, 0);
        }
        from = close;
    }
    out
}

fn is_posting(v: &Json) -> bool {
    match v.get("@type") {
        Some(Json::Str(t)) => t == "JobPosting",
        Some(Json::Arr(a)) => a.iter().any(|t| t.as_str() == Some("JobPosting")),
        _ => false,
    }
}

fn collect_postings(v: &Json, out: &mut Vec<Json>, depth: usize) {
    if depth > 6 {
        return;
    }
    match v {
        Json::Arr(a) => a.iter().for_each(|x| collect_postings(x, out, depth + 1)),
        Json::Obj(_) => {
            if is_posting(v) {
                out.push(v.clone());
            }
            for k in ["@graph", "itemListElement", "item", "mainEntity"] {
                if let Some(x) = v.get(k) {
                    collect_postings(x, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// A schema.org value that may be a string, an object with `name`, or a list.
fn name_of(v: &Json) -> String {
    match v {
        Json::Str(s) => s.trim().to_string(),
        Json::Obj(_) => v.str_of("name").trim().to_string(),
        Json::Arr(a) => a.iter().map(name_of).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

fn place_text(place: &Json) -> String {
    let addr = place.get("address").unwrap_or(place);
    if let Json::Str(s) = addr {
        return s.trim().to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for k in ["addressLocality", "addressRegion"] {
        let v = addr.get(k).map(name_of).unwrap_or_default();
        if !v.is_empty() && !parts.contains(&v) {
            parts.push(v);
        }
    }
    let mut country = addr.get("addressCountry").map(name_of).unwrap_or_default();
    if country.len() == 2 {
        let full = location::country_name(&country.to_uppercase());
        if !full.is_empty() {
            country = full.to_string();
        }
    }
    if !country.is_empty() && !parts.contains(&country) {
        parts.push(country);
    }
    if parts.is_empty() {
        name_of(place)
    } else {
        parts.join(", ")
    }
}

/// One JobPosting -> Job. `None` when it has no title or has expired.
pub fn posting_to_job(c: &Company, p: &Json, page_url: &str, now: i64) -> Option<Job> {
    let title = html::text(&p.str_of("title"));
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let valid = p.str_of("validThrough");
    if let Some(until) = time::iso_to_unix(&valid) {
        // A date-only validThrough means the end of that day.
        let until = if valid.trim().len() == 10 { until + 86_400 } else { until };
        if until < now {
            return None;
        }
    }
    let ident = p.get("identifier").map(|i| match i {
        Json::Obj(_) => i.str_of("value"),
        other => other.to_text(),
    });
    let key = ident.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        let path = page_url.split_once("://").and_then(|(_, r)| r.split_once('/')).map(|(_, p)| p).unwrap_or(page_url);
        path.split(['?', '#']).next().unwrap_or(path).trim_matches('/').replace('/', "_")
    });
    let site = host_of(&c.slug);
    let site = if site.is_empty() { c.slug.clone() } else { site };
    let mut j = Job::new("schema", format!("{}/{}", site, key.trim().replace('/', "_")));
    j.title = title.to_string();
    // The name the user tracks the company under, not the legal entity name
    // in hiringOrganization ("Netflix" rather than "Netflix, Inc.").
    j.company = c.name.clone();
    let places: Vec<String> = match p.get("jobLocation") {
        Some(Json::Arr(a)) => a.iter().map(place_text).filter(|s| !s.is_empty()).collect(),
        Some(v) => vec![place_text(v)].into_iter().filter(|s| !s.is_empty()).collect(),
        None => vec![],
    };
    let telecommute = p.str_of("jobLocationType").to_uppercase().contains("TELECOMMUTE");
    let applicant = p.get("applicantLocationRequirements").map(name_of).unwrap_or_default();
    let mut loc = match places.len() {
        0 => String::new(),
        1 => places[0].clone(),
        n => format!("{} (+{} more)", places[0], n - 1),
    };
    if telecommute {
        loc = match (applicant.is_empty(), loc.is_empty()) {
            (false, _) => format!("Remote ({})", applicant),
            (true, true) => "Remote".into(),
            (true, false) => format!("Remote, {}", loc),
        };
    }
    j.location = loc;
    j.remote = telecommute || super::looks_remote(&j.location) || super::looks_remote(&j.title);
    j.url = {
        let u = p.str_of("url");
        if u.starts_with("http") && host_of(&u) == host_of(page_url) {
            u
        } else {
            page_url.to_string()
        }
    };
    j.set_posted_iso(&p.str_of("datePosted"));
    for v in [p.get("employmentType").map(name_of).unwrap_or_default(), p.get("occupationalCategory").map(name_of).unwrap_or_default()] {
        for t in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            j.tags.push(t.replace('_', " ").to_lowercase());
        }
    }
    j.salary = salary_text(p.get("baseSalary"));
    j.summary = html::truncate(&html::collapse_ws(&html::text(&p.str_of("description"))), 240);
    Some(j)
}

fn salary_text(v: Option<&Json>) -> String {
    let Some(v) = v else { return String::new() };
    let cur = v.str_of("currency");
    let val = v.get("value").unwrap_or(v);
    let (lo, hi) = (val.num_of("minValue"), val.num_of("maxValue"));
    let single = val.num_of("value");
    let unit = val.str_of("unitText").to_lowercase();
    let fmt = |n: f64| if n >= 1000.0 { format!("{}k", (n / 1000.0).round()) } else { format!("{}", n) };
    let body = match (lo, hi, single) {
        (Some(a), Some(b), _) if a > 0.0 && b > 0.0 => format!("{}–{}", fmt(a), fmt(b)),
        (_, _, Some(x)) if x > 0.0 => fmt(x),
        _ => return String::new(),
    };
    let unit = if unit.is_empty() { String::new() } else { format!("/{}", unit) };
    format!("{} {}{}", cur, body, unit).trim().to_string()
}

// ------------------------------------------------------------ cache

/// url -> parsed job (Null when the page had no usable JobPosting).
struct Cache {
    path: Option<PathBuf>,
    entries: HashMap<String, Json>,
}

impl Cache {
    fn load(dir: Option<&Path>, slug: &str) -> Cache {
        let path = dir.map(|d| d.join(format!("schema-{}.json", slug.replace(['/', ':', '?', '&', '=', '\\'], "_"))));
        let mut entries = HashMap::new();
        if let Some(p) = &path {
            if let Some(Json::Obj(o)) = fs::read_to_string(p).ok().and_then(|t| Json::parse(&t).ok()).and_then(|v| v.get("pages").cloned()) {
                entries.extend(o);
            }
        }
        Cache { path, entries }
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut keys: Vec<&String> = self.entries.keys().collect();
        keys.sort();
        let pages = Json::Obj(keys.into_iter().map(|k| (k.clone(), self.entries[k].clone())).collect());
        let mut root = Json::obj();
        root.set("pages", pages);
        let _ = crate::config::write_atomic(p, &root.to_compact());
    }
}

/// Fetch pages a few at a time with a pause between requests; returns
/// url -> body for the pages read. Stops early, leaving the rest for the
/// next run, as soon as the site starts refusing (403/429/503).
fn fetch_all(urls: &[String], delay_ms: u64) -> (Vec<(String, String)>, bool) {
    let queue = Mutex::new(urls.iter().rev().cloned().collect::<Vec<_>>());
    let results = Mutex::new(Vec::new());
    let refused = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        for _ in 0..WORKERS.min(urls.len().max(1)) {
            s.spawn(|| loop {
                if refused.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let next = queue.lock().unwrap().pop();
                let Some(u) = next else { break };
                match http::get(&u) {
                    Ok(body) => results.lock().unwrap().push((u, body)),
                    Err(e) if ["403", "429", "503"].iter().any(|c| e.contains(c)) => {
                        refused.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {}
                }
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            });
        }
    });
    (results.into_inner().unwrap(), refused.into_inner())
}

// ------------------------------------------------------------ entry points

/// Read a company's schema.org careers site. `cache_dir` None = no cache
/// (used by `company add`, which only samples a few pages).
pub fn fetch(c: &Company, max: usize, cache_dir: Option<&Path>, quiet: bool, budget: Option<usize>) -> Result<Found, String> {
    let start = if c.slug.starts_with("http") { c.slug.clone() } else { format!("https://{}", c.slug) };
    let origin = origin_of(&start).ok_or_else(|| format!("'{}' is not a URL", start))?;
    let robots = http::get(&format!("{}/robots.txt", origin)).unwrap_or_default();
    let rules = robots_rules(&robots);
    let now = time::now_unix();

    // The careers page: JobPostings embedded directly, plus links to job pages.
    let page = http::get(&start).map_err(|e| format!("could not fetch {}: {}", start, e))?;
    let mut jobs: Vec<Job> = job_postings(&page).iter().filter_map(|p| posting_to_job(c, p, &start, now)).collect();
    let mut urls: Vec<String> = page_job_links(&start, &page);
    for (u, _) in sitemap_job_urls(&origin, &robots) {
        if !urls.contains(&u) {
            urls.push(u);
        }
    }
    urls.retain(|u| allowed(u, &rules));
    let total = urls.len().max(jobs.len());

    let mut cache = Cache::load(cache_dir, &c.slug);
    let live: HashSet<&String> = urls.iter().collect();
    // Postings whose page left the site are closed.
    cache.entries.retain(|u, _| live.contains(u));
    let budget = if cache_dir.is_some() { budget.unwrap_or(NEW_PAGES_PER_RUN) } else { DETECT_SAMPLE };
    let todo: Vec<String> = urls.iter().filter(|u| !cache.entries.contains_key(*u)).take(budget).cloned().collect();
    let pending = urls.iter().filter(|u| !cache.entries.contains_key(*u)).count().saturating_sub(todo.len());
    if !quiet && todo.len() > 20 {
        eprint!("[{}: reading {} new job pages] ", c.name, todo.len());
    }
    let delay = crawl_delay_ms(&robots).unwrap_or(DEFAULT_DELAY_MS).max(DEFAULT_DELAY_MS);
    let (pages, refused) = fetch_all(&todo, delay);
    let read = pages.len();
    for (u, body) in pages {
        let job = job_postings(&body).iter().find_map(|p| posting_to_job(c, p, &u, now));
        cache.entries.insert(u, job.map(|j| j.to_json()).unwrap_or(Json::Null));
    }
    cache.save();
    if refused && !quiet {
        eprint!("[{}: the site asked jobscout to slow down; read {} pages, the rest next run] ", c.name, read);
    }
    let pending = pending + todo.len().saturating_sub(read);

    let mut seen: HashSet<String> = jobs.iter().map(|j| j.id.clone()).collect();
    for u in &urls {
        if let Some(v) = cache.entries.get(u) {
            if let Some(mut j) = Job::from_json(v) {
                if seen.insert(j.id.clone()) {
                    j.company = c.name.clone();
                    jobs.push(j);
                }
            }
        }
    }
    jobs.sort_by(|a, b| b.posted_unix.unwrap_or(0).cmp(&a.posted_unix.unwrap_or(0)));
    jobs.truncate(max);
    Ok(Found { jobs, total, pending })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn co() -> Company {
        Company { name: "acme".into(), ats: "schema".into(), slug: "https://careers.acme.com/jobs".into() }
    }

    #[test]
    fn urls() {
        assert_eq!(join_url("https://a.com/careers/search?x=1", "/job/123").as_deref(), Some("https://a.com/job/123"));
        assert_eq!(join_url("https://a.com/careers/search", "job/123").as_deref(), Some("https://a.com/careers/job/123"));
        assert_eq!(join_url("https://a.com/careers/search", "../jobs/9-x").as_deref(), Some("https://a.com/jobs/9-x"));
        assert_eq!(join_url("https://a.com/x", "//b.com/y").as_deref(), Some("https://b.com/y"));
        assert_eq!(join_url("https://a.com/x", "mailto:hi@a.com"), None);
        assert!(looks_like_job_url("https://explore.jobs.netflix.net/careers/job/790319500144-senior-manager"));
        assert!(looks_like_job_url("https://join.disneycareers.com/global/en/job/1374922BR/Hotel-Culinary-Dinner-Cook"));
        assert!(looks_like_job_url("https://www.ycombinator.com/companies/coris/jobs/h0nDly8-backend-engineer-ai-native"));
        assert!(!looks_like_job_url("https://www.salesforce.com/company/careers/teams/"));
        assert!(!looks_like_job_url("https://a.com/careers/search"));
        assert!(!looks_like_job_url("https://a.com/blog/2024/how-we-hire-engineers"));
    }

    #[test]
    fn robots() {
        let r = "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /private/\nDisallow: /referral\nAllow: /jobs\nSitemap: https://a.com/sm.xml\n";
        let d = robots_rules(r);
        assert_eq!(d.len(), 3);
        assert!(allowed("https://a.com/jobs/1-a-b", &d));
        assert!(!allowed("https://a.com/private/jobs/1", &d));
        // Netflix: everything disallowed except what is allowed explicitly.
        assert_eq!(crawl_delay_ms("User-agent: *\nCrawl-delay: 2\n"), Some(2000));
        assert_eq!(crawl_delay_ms("User-agent: bingbot\nCrawl-delay: 5\n"), None);
        assert_eq!(crawl_delay_ms("User-agent: *\nCrawl-delay: 600\n"), Some(10_000));
        let nf = robots_rules("User-agent: *\nDisallow: /\nAllow: /$\nAllow: /careers\n");
        assert!(allowed("https://x.net/careers/job/7903-a-b", &nf));
        assert!(allowed("https://x.net/", &nf));
        assert!(!allowed("https://x.net/admin", &nf));
        let wild = robots_rules("User-agent: *\nDisallow: /*?sort=\nDisallow: /*.pdf$\n");
        assert!(!allowed("https://a.com/jobs?sort=new", &wild));
        assert!(!allowed("https://a.com/files/x.pdf", &wild));
        assert!(allowed("https://a.com/files/x.pdf.html", &wild));
        assert!(allowed("https://a.com/jobs/1234-a", &wild));
    }

    #[test]
    fn sitemaps() {
        let xml = "<urlset><url><loc>https://a.com/job/1234-a</loc><lastmod>2026-11-13</lastmod></url><url><loc><![CDATA[https://a.com/job/5678-b]]></loc></url></urlset>";
        let e = sitemap_entries(xml);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].0, "https://a.com/job/1234-a");
        assert!(e[0].1.is_some());
        assert_eq!(e[1].0, "https://a.com/job/5678-b");
    }

    #[test]
    fn postings() {
        let page = r#"<html><script type="application/ld+json">{"@context":"https://schema.org","@graph":[{"@type":"Organization","name":"Acme"},{"@type":"JobPosting","title":"Senior &amp; Rust Engineer","datePosted":"2026-11-13","validThrough":"2099-01-01","identifier":{"@type":"PropertyValue","value":"R-1/2"},"hiringOrganization":{"@type":"Organization","name":"Acme Inc"},"jobLocation":[{"@type":"Place","address":{"@type":"PostalAddress","addressLocality":"Austin","addressRegion":"TX","addressCountry":"US"}},{"@type":"Place","address":{"addressLocality":"Berlin","addressCountry":{"@type":"Country","name":"Germany"}}}],"employmentType":["FULL_TIME"],"baseSalary":{"@type":"MonetaryAmount","currency":"USD","value":{"@type":"QuantitativeValue","minValue":150000,"maxValue":190000,"unitText":"YEAR"}},"description":"<p>Build <b>things</b>.</p>"}]}</script>
        <SCRIPT TYPE='application/ld+json'>[{"@type":["JobPosting"],"title":"Old","validThrough":"2001-01-01"}]</SCRIPT></html>"#;
        let ps = job_postings(page);
        assert_eq!(ps.len(), 2);
        let now = time::iso_to_unix("2026-11-15T00:00:00Z").unwrap();
        let j = posting_to_job(&co(), &ps[0], "https://careers.acme.com/job/r-1", now).unwrap();
        assert_eq!(j.title, "Senior & Rust Engineer");
        assert_eq!(j.location, "Austin, TX, United States (+1 more)");
        assert_eq!(j.id, "careers.acme.com/R-1_2");
        assert_eq!(j.posted.get(..10), Some("2026-11-13"));
        assert_eq!(j.salary, "USD 150k–190k/year");
        assert_eq!(j.tags, vec!["full time".to_string()]);
        assert_eq!(j.summary, "Build things.");
        assert!(posting_to_job(&co(), &ps[1], "https://careers.acme.com/job/old", now).is_none(), "expired");
    }

    #[test]
    fn remote_postings() {
        let p = Json::parse(r#"{"@type":"JobPosting","title":"SRE","jobLocationType":"TELECOMMUTE","applicantLocationRequirements":{"@type":"Country","name":"USA"}}"#).unwrap();
        let j = posting_to_job(&co(), &p, "https://careers.acme.com/job/sre-1234", 0).unwrap();
        assert!(j.remote);
        assert_eq!(j.location, "Remote (USA)");
        assert_eq!(j.id, "careers.acme.com/job_sre-1234");
    }
}
