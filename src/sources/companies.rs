//! Company careers pages hosted on a known applicant tracking system (ATS)
//! or careers-site platform. Each exposes a JSON list of open roles, so
//! tracking a company is one entry in `config.companies`.
//!
//! Kinds of platform:
//! - slug-addressed boards (Greenhouse, Lever, Ashby, Workable,
//!   SmartRecruiters): a company name is enough, we probe each one;
//! - site-addressed platforms (Phenom, Workday): the company's own careers
//!   domain hosts a search API, so the user pastes the careers URL and we
//!   fingerprint the page;
//! - company-specific sites with a public JSON search (TikTok);
//! - any other careers site that publishes schema.org JobPosting data
//!   (see `schema.rs`), found through its links and sitemaps.
//!
//! `detect` turns what a user typed into a `Company` either way.

use super::{looks_remote, Source};
use crate::config::{Company, Config};
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;
use crate::time;
use std::collections::HashSet;

/// Boards that can be probed from a bare company name.
pub const SLUG_ATS: [&str; 5] = ["greenhouse", "lever", "ashby", "workable", "smartrecruiters"];
/// Platforms that live on the company's own careers domain.
pub const SITE_ATS: [&str; 2] = ["phenom", "workday"];
/// Companies with their own careers system that jobscout reads directly:
/// (name users type, ats, slug).
pub const OWN_SITES: [(&str, &str, &str); 1] = [("tiktok", "tiktok", "tiktok")];

/// Result of reading a board: the postings fetched (possibly capped) and the
/// board's own total, so `company add` can report the real size.
pub struct Listing {
    pub jobs: Vec<Job>,
    pub total: usize,
}

pub struct Companies;

impl Source for Companies {
    fn name(&self) -> &'static str {
        "companies"
    }
    fn describe(&self) -> &'static str {
        "companies you track (Greenhouse, Lever, Ashby, Workable, SmartRecruiters, Phenom, Workday, TikTok, schema.org careers sites)"
    }
    fn unconfigured(&self, cfg: &Config) -> Option<&'static str> {
        cfg.companies.is_empty().then_some("no companies yet; try `jobscout company add <name or url>`")
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let mut jobs: Vec<Job> = Vec::new();
        let mut errors = Vec::new();
        // The same requisition can surface through two front ends of one
        // company (e.g. a Phenom site and the Workday board behind it), so
        // de-duplicate on company + requisition id.
        let mut seen_req: HashSet<String> = HashSet::new();
        // The cap applies per company; a large employer must not crowd out
        // the others.
        let cache = cfg.home.join("cache");
        for c in &cfg.companies {
            let got = if c.ats == "schema" {
                super::schema::fetch(c, cfg.max_per_source, Some(&cache), cfg.quiet, cfg.crawl_pages).map(|f| {
                    if f.pending > 0 && !cfg.quiet {
                        eprintln!("  {}: {} more job pages will be read on the next run", c.name, f.pending);
                    }
                    Listing { jobs: f.jobs, total: f.total }
                })
            } else {
                fetch_company(c, &cfg.queries, cfg.max_per_source)
            };
            match got {
                Ok(l) => {
                    for mut j in l.jobs {
                        j.tracked = true;
                        let req = j.id.rsplit('/').next().unwrap_or(&j.id).to_lowercase();
                        if seen_req.insert(format!("{}|{}", j.company.to_lowercase(), req)) {
                            jobs.push(j);
                        }
                    }
                }
                Err(e) => errors.push(format!("{}: {}", c.name, e)),
            }
        }
        if jobs.is_empty() && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        if !errors.is_empty() {
            eprintln!("  companies: some failed: {}", errors.join("; "));
        }
        Ok(jobs)
    }
}

/// Fetch one company's open roles. Slug boards return their whole list;
/// site platforms (whose catalogues run to thousands) search server-side
/// for each query, or the newest postings when there are no queries.
pub fn fetch_company(c: &Company, queries: &[String], max: usize) -> Result<Listing, String> {
    match c.ats.as_str() {
        "greenhouse" => greenhouse(c, max),
        "lever" => lever(c, max),
        "ashby" => ashby(c, max),
        "workable" => workable(c, max),
        "smartrecruiters" => smartrecruiters(c, max),
        "phenom" => phenom(c, queries, max),
        "workday" => workday(c, queries, max),
        "tiktok" => tiktok(c, queries, max),
        // Uncached sample; tracked runs go through `schema::fetch` with a cache.
        "schema" => super::schema::fetch(c, max, None, true, None).map(|f| Listing { jobs: f.jobs, total: f.total }),
        other => Err(format!("unknown ats '{}'", other)),
    }
}

/// Resolve user input (company name or careers URL) to a Company, confirming
/// the board exists. Returns the company and its current posting count.
pub fn detect(input: &str) -> Result<(Company, usize), String> {
    let input = input.trim();
    if let Some((ats, slug)) = from_url(input) {
        let c = Company { name: display_name(&slug), ats: ats.to_string(), slug };
        let l = fetch_company(&c, &[], 100).map_err(|e| format!("{} board '{}' did not answer: {}", ats, c.slug, e))?;
        return Ok((c, l.total));
    }
    if input.contains("://") {
        return detect_site(input);
    }
    if input.contains('/') {
        return Err(format!("'{}' looks like a URL; include the https:// prefix", input));
    }
    let name = input.to_string();
    let key = input.to_lowercase().replace([' ', '-'], "");
    if let Some((_, ats, slug)) = OWN_SITES.iter().find(|(n, _, _)| *n == key) {
        let c = Company { name: name.clone(), ats: ats.to_string(), slug: slug.to_string() };
        let l = fetch_company(&c, &[], 100).map_err(|e| format!("{} careers site did not answer: {}", name, e))?;
        return Ok((c, l.total));
    }
    let mut slugs: Vec<String> = Vec::new();
    for s in [input.to_lowercase().replace(' ', ""), input.to_lowercase().replace(' ', "-")] {
        if !slugs.contains(&s) {
            slugs.push(s);
        }
    }
    // Some systems (SmartRecruiters) answer 200 with an empty list for
    // unknown slugs, so a probe only counts when it finds open roles.
    for ats in SLUG_ATS {
        for slug in &slugs {
            let c = Company { name: name.clone(), ats: ats.to_string(), slug: slug.clone() };
            if let Ok(l) = fetch_company(&c, &[], 100) {
                if l.total > 0 {
                    return Ok((c, l.total));
                }
            }
        }
    }
    Err(format!(
        "no board found for '{}' on {}. If the company uses one of these or runs its careers site on Phenom or Workday, paste its careers URL instead.",
        input,
        SLUG_ATS.join(", ")
    ))
}

/// A careers URL on the company's own domain: fetch the page and fingerprint
/// the platform behind it.
fn detect_site(url: &str) -> Result<(Company, usize), String> {
    let host = host_of(url).ok_or_else(|| format!("'{}' is not a valid URL", url))?;
    let page = http::get(url).map_err(|e| {
        if e.contains("403") {
            format!(
                "{} refused automated access (HTTP 403). That is usually bot protection such as Cloudflare, which needs a real browser; jobscout cannot read this site.",
                host
            )
        } else {
            format!("could not fetch {}: {}", url, e)
        }
    })?;
    let lower = page.to_lowercase();

    // A Phenom site whose search API answers; if it does not (some
    // deployments differ), fall through to the other readers below.
    let mut phenom_err = None;
    if lower.contains("ddokey") || lower.contains("phenom") {
        let c = Company { name: display_name(&host), ats: "phenom".into(), slug: host.clone() };
        match fetch_company(&c, &[], 100) {
            Ok(l) => return Ok((c, l.total)),
            Err(e) => phenom_err = Some(e),
        }
    }
    // A page that merely links to a Workday board: follow that instead.
    if let Some(pos) = lower.find(".myworkdayjobs.com/") {
        let start = lower[..pos].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-')).map(|i| i + 1).unwrap_or(0);
        let tail: String = page[start..].chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/')).collect();
        if let Some((ats, slug)) = from_url(&format!("https://{}", tail)) {
            let c = Company { name: display_name(&slug), ats: ats.into(), slug };
            let l = fetch_company(&c, &[], 100).map_err(|e| format!("found a Workday board at {} but it did not answer: {}", c.slug, e))?;
            return Ok((c, l.total));
        }
    }
    // Any other site: does it publish schema.org JobPosting pages?
    let c = Company { name: display_name(&host), ats: "schema".into(), slug: url.trim().to_string() };
    if let Ok(found) = super::schema::fetch(&c, 100, None, true, None) {
        if !found.jobs.is_empty() {
            return Ok((c, found.total));
        }
    }
    let hint = if lower.contains("just a moment") || lower.contains("challenges.cloudflare.com") {
        " The site answered with a bot-protection challenge, so it cannot be read without a browser.".to_string()
    } else if let Some(e) = phenom_err {
        format!(" It looks like a Phenom site, but its search API did not answer ({}).", html::truncate(&e, 80))
    } else {
        String::new()
    };
    Err(format!(
        "{} is not on a platform jobscout understands ({}, {}) and publishes no schema.org job data it could find.{} Paste the URL of the job search page itself if this was a landing page; sites that build their job list in the browser (most big-tech careers sites) cannot be read.",
        host,
        SLUG_ATS.join(", "),
        SITE_ATS.join(", "),
        hint
    ))
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?.to_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// `jobs.cvshealth.com` -> `cvshealth`; `explore.jobs.netflix.net` ->
/// `netflix`; `acme.wd1.myworkdayjobs.com/Careers` -> `acme`.
fn display_name(slug: &str) -> String {
    let s = slug.split_once("://").map(|(_, r)| r).unwrap_or(slug);
    let host = s.split(['/', '?', '#']).next().unwrap_or(s).to_lowercase();
    if !host.contains('.') {
        return slug.to_string();
    }
    let labels: Vec<&str> = host.split('.').collect();
    // Platforms that put the company in the first label.
    if host.ends_with(".myworkdayjobs.com") || host.ends_with(".workable.com") {
        return labels[0].to_string();
    }
    // Otherwise the registrable domain: the label before the TLD, or before
    // a two-part suffix such as .co.uk / .com.au.
    let n = labels.len();
    let second_level = ["co", "com", "org", "net", "ac", "gov", "edu"];
    let i = if n >= 3 && labels[n - 1].len() == 2 && second_level.contains(&labels[n - 2]) { n - 3 } else { n.saturating_sub(2) };
    labels[i].to_string()
}

/// Recognise an ATS-hosted careers URL and pull the board slug out of it.
pub fn from_url(input: &str) -> Option<(&'static str, String)> {
    let rest = input.strip_prefix("https://").or_else(|| input.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let first = |i: usize| segs.get(i).map(|s| s.to_string());

    if host.ends_with("greenhouse.io") {
        // boards.greenhouse.io/<slug>, job-boards.greenhouse.io/<slug>,
        // boards-api.greenhouse.io/v1/boards/<slug>/jobs
        let slug = if segs.first() == Some(&"v1") { first(2) } else { first(0) }?;
        return Some(("greenhouse", slug.to_lowercase()));
    }
    if host.ends_with("lever.co") {
        // jobs.lever.co/<slug>, api.lever.co/v0/postings/<slug>
        let slug = if segs.first() == Some(&"v0") { first(2) } else { first(0) }?;
        return Some(("lever", slug.to_lowercase()));
    }
    if host.ends_with("ashbyhq.com") {
        // jobs.ashbyhq.com/<slug>, api.ashbyhq.com/posting-api/job-board/<slug>
        let slug = if segs.first() == Some(&"posting-api") { first(2) } else { first(0) }?;
        return Some(("ashby", slug.to_lowercase()));
    }
    if host.ends_with("workable.com") {
        // apply.workable.com/<slug>, <slug>.workable.com
        if host.starts_with("apply.") {
            return first(0).map(|s| ("workable", s.to_lowercase()));
        }
        let sub = host.trim_end_matches(".workable.com");
        if !sub.is_empty() && sub != "www" {
            return Some(("workable", sub.to_string()));
        }
        return None;
    }
    if host.ends_with("smartrecruiters.com") {
        // careers.smartrecruiters.com/<Slug>, jobs.smartrecruiters.com/<slug>,
        // api.smartrecruiters.com/v1/companies/<slug>/postings
        let slug = if segs.first() == Some(&"v1") { first(2) } else { first(0) }?;
        return Some(("smartrecruiters", slug));
    }
    if host == "lifeattiktok.com" || host.ends_with(".lifeattiktok.com") || host == "careers.tiktok.com" {
        return Some(("tiktok", "tiktok".into()));
    }
    if host.ends_with(".myworkdayjobs.com") {
        // <tenant>.wd<N>.myworkdayjobs.com/<site>, .../en-US/<site>/...,
        // .../wday/cxs/<tenant>/<site>/jobs
        let site = if segs.first() == Some(&"wday") {
            first(3)
        } else if segs.first().map(|s| is_locale(s)).unwrap_or(false) {
            first(1)
        } else {
            first(0)
        }?;
        return Some(("workday", format!("{}/{}", host, site)));
    }
    None
}

/// `en-US`, `fr-CA`, `de-DE`...
fn is_locale(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5 && b[2] == b'-' && b[..2].iter().all(|c| c.is_ascii_lowercase()) && b[3..].iter().all(|c| c.is_ascii_uppercase())
}

// ------------------------------------------------------------- adapters

fn greenhouse(c: &Company, max: usize) -> Result<Listing, String> {
    let url = format!("https://boards-api.greenhouse.io/v1/boards/{}/jobs", http::url_encode(&c.slug));
    let root = Json::parse(&http::get(&url)?)?;
    let items = root.get("jobs").and_then(|j| j.as_arr()).ok_or("no jobs array")?;
    let mut jobs = Vec::new();
    for it in items {
        let id = it.str_of("id");
        if id.is_empty() {
            continue;
        }
        let mut j = Job::new("greenhouse", format!("{}/{}", c.slug, id));
        j.title = it.str_of("title");
        j.company = c.name.clone();
        j.location = it.path(&["location", "name"]).map(|l| l.to_text()).unwrap_or_default();
        j.remote = looks_remote(&j.location) || looks_remote(&j.title);
        j.url = it.str_of("absolute_url");
        let mut date = it.str_of("first_published");
        if date.is_empty() {
            date = it.str_of("updated_at");
        }
        j.set_posted_iso(&date);
        if let Some(depts) = it.get("departments").and_then(|d| d.as_arr()) {
            j.tags.extend(depts.iter().map(|d| d.str_of("name")).filter(|n| !n.is_empty()));
        }
        jobs.push(j);
        if jobs.len() >= max {
            break;
        }
    }
    Ok(Listing { total: items.len(), jobs })
}

fn lever(c: &Company, max: usize) -> Result<Listing, String> {
    let url = format!("https://api.lever.co/v0/postings/{}?mode=json", http::url_encode(&c.slug));
    let root = Json::parse(&http::get(&url)?)?;
    let items = root.as_arr().ok_or("expected a JSON array")?;
    let mut jobs = Vec::new();
    for it in items {
        let id = it.str_of("id");
        if id.is_empty() {
            continue;
        }
        let mut j = Job::new("lever", format!("{}/{}", c.slug, id));
        j.title = it.str_of("text");
        j.company = c.name.clone();
        let cats = it.get("categories");
        j.location = cats.map(|x| x.str_of("location")).unwrap_or_default();
        let workplace = it.str_of("workplaceType").to_lowercase();
        j.remote = workplace == "remote" || looks_remote(&j.location) || looks_remote(&j.title);
        j.url = it.str_of("hostedUrl");
        if let Some(ms) = it.num_of("createdAt") {
            j.set_posted_unix((ms / 1000.0) as i64);
        }
        if let Some(x) = cats {
            for key in ["team", "commitment", "department"] {
                let v = x.str_of(key);
                if !v.is_empty() {
                    j.tags.push(v);
                }
            }
        }
        if !workplace.is_empty() {
            j.tags.push(workplace);
        }
        j.summary = html::truncate(&html::collapse_ws(&it.str_of("descriptionPlain")), 240);
        jobs.push(j);
        if jobs.len() >= max {
            break;
        }
    }
    Ok(Listing { total: items.len(), jobs })
}

fn ashby(c: &Company, max: usize) -> Result<Listing, String> {
    let url = format!("https://api.ashbyhq.com/posting-api/job-board/{}?includeCompensation=true", http::url_encode(&c.slug));
    let root = Json::parse(&http::get(&url)?)?;
    let items = root.get("jobs").and_then(|j| j.as_arr()).ok_or("no jobs array")?;
    let mut jobs = Vec::new();
    for it in items {
        let id = it.str_of("id");
        if id.is_empty() || it.bool_of("isListed") == Some(false) {
            continue;
        }
        let mut j = Job::new("ashby", format!("{}/{}", c.slug, id));
        j.title = it.str_of("title");
        j.company = c.name.clone();
        j.location = it.str_of("locationName");
        j.remote = it.bool_of("isRemote").unwrap_or(false) || looks_remote(&j.location);
        j.url = it.str_of("jobUrl");
        if j.url.is_empty() {
            j.url = it.str_of("applyUrl");
        }
        j.set_posted_iso(&it.str_of("publishedAt"));
        for key in ["departmentName", "teamName", "employmentType"] {
            let v = it.str_of(key);
            if !v.is_empty() {
                j.tags.push(v);
            }
        }
        j.salary = it.path(&["compensation", "compensationTierSummary"]).map(|s| s.to_text()).unwrap_or_default();
        j.summary = html::truncate(&html::collapse_ws(&it.str_of("descriptionPlain")), 240);
        jobs.push(j);
        if jobs.len() >= max {
            break;
        }
    }
    Ok(Listing { total: items.len(), jobs })
}

fn workable(c: &Company, max: usize) -> Result<Listing, String> {
    let url = format!("https://apply.workable.com/api/v1/widget/accounts/{}", http::url_encode(&c.slug));
    let root = Json::parse(&http::get(&url)?)?;
    let items = root.get("jobs").and_then(|j| j.as_arr()).ok_or("no jobs array")?;
    let company_name = {
        let n = root.str_of("name");
        if n.is_empty() { c.name.clone() } else { n }
    };
    let mut jobs = Vec::new();
    for it in items {
        let id = it.str_of("shortcode");
        if id.is_empty() {
            continue;
        }
        let mut j = Job::new("workable", format!("{}/{}", c.slug, id));
        j.title = it.str_of("title");
        j.company = if c.name == c.slug { company_name.clone() } else { c.name.clone() };
        let place: Vec<String> = ["city", "state", "country"].iter().map(|k| it.str_of(k)).filter(|s| !s.is_empty()).collect();
        j.location = place.join(", ");
        j.remote = it.bool_of("telecommuting").unwrap_or(false) || looks_remote(&j.location);
        j.url = it.str_of("url");
        if j.url.is_empty() {
            j.url = format!("https://apply.workable.com/{}/j/{}/", c.slug, id);
        }
        j.set_posted_iso(&it.str_of("published_on"));
        for key in ["department", "employment_type"] {
            let v = it.str_of(key);
            if !v.is_empty() {
                j.tags.push(v);
            }
        }
        j.summary = html::truncate(&html::text(&it.str_of("description")), 240);
        jobs.push(j);
        if jobs.len() >= max {
            break;
        }
    }
    Ok(Listing { total: items.len(), jobs })
}

fn smartrecruiters(c: &Company, max: usize) -> Result<Listing, String> {
    let mut jobs = Vec::new();
    let mut offset = 0;
    let mut total = 0;
    loop {
        let url = format!("https://api.smartrecruiters.com/v1/companies/{}/postings?limit=100&offset={}", http::url_encode(&c.slug), offset);
        let root = Json::parse(&http::get(&url)?)?;
        let items = root.get("content").and_then(|j| j.as_arr()).ok_or("no content array")?;
        total = total.max(root.num_of("totalFound").unwrap_or(0.0) as usize);
        if items.is_empty() {
            break;
        }
        for it in items {
            let id = it.str_of("id");
            if id.is_empty() {
                continue;
            }
            let mut j = Job::new("smartrecruiters", format!("{}/{}", c.slug, id));
            j.title = it.str_of("name");
            j.company = c.name.clone();
            let loc = it.get("location");
            let place: Vec<String> = ["city", "region", "country"].iter().filter_map(|k| loc.map(|l| l.str_of(k))).filter(|s| !s.is_empty()).collect();
            j.location = place.join(", ");
            j.remote = loc.and_then(|l| l.bool_of("remote")).unwrap_or(false) || looks_remote(&j.title);
            j.url = format!("https://jobs.smartrecruiters.com/{}/{}", c.slug, id);
            j.set_posted_iso(&it.str_of("releasedDate"));
            if let Some(d) = it.path(&["department", "label"]) {
                let v = d.to_text();
                if !v.is_empty() {
                    j.tags.push(v);
                }
            }
            jobs.push(j);
            if jobs.len() >= max {
                return Ok(Listing { jobs, total });
            }
        }
        offset += items.len();
        if offset >= total || items.len() < 100 {
            break;
        }
    }
    Ok(Listing { jobs, total })
}

/// Phenom careers sites (jobs.<company>.com) expose the search their own
/// page uses: POST /widgets with `ddoKey: refineSearch`.
/// A Phenom site's locale path ("us/en", "global/en"), read from where its
/// home page redirects. Job pages live at `https://<site>/<prefix>/job/<id>`.
fn phenom_prefix(slug: &str) -> Option<String> {
    let end = http::final_url(&format!("https://{}/", slug)).ok()?;
    phenom_prefix_from(&end)
}

fn phenom_prefix_from(final_url: &str) -> Option<String> {
    let path = final_url.split("://").nth(1)?.split_once('/')?.1;
    let parts: Vec<&str> = path.split(['/', '?', '#']).filter(|p| !p.is_empty()).take(2).collect();
    let localeish = |s: &str| (s.len() == 2 || s == "global") && s.chars().all(|c| c.is_ascii_lowercase());
    (parts.len() == 2 && localeish(parts[0]) && localeish(parts[1])).then(|| format!("{}/{}", parts[0], parts[1]))
}

fn phenom(c: &Company, queries: &[String], max: usize) -> Result<Listing, String> {
    let url = format!("https://{}/widgets", c.slug);
    // Link to the company's own job page: the Workday/Taleo page behind it
    // (applyUrl) is often shown as "page does not exist" when opened directly.
    let prefix = phenom_prefix(&c.slug);
    let searches: Vec<String> = if queries.is_empty() { vec![String::new()] } else { queries.to_vec() };
    let mut jobs = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0;
    for q in &searches {
        let mut from = 0;
        loop {
            let size = 100.min(max.saturating_sub(jobs.len()).max(1));
            let mut body = Json::obj();
            body.set("lang", Json::Str("en_us".into()));
            body.set("deviceType", Json::Str("desktop".into()));
            body.set("country", Json::Str("us".into()));
            body.set("pageName", Json::Str("search-results".into()));
            body.set("ddoKey", Json::Str("refineSearch".into()));
            body.set("sortBy", Json::Str("Most recent".into()));
            body.set("subsearch", Json::Str(String::new()));
            body.set("from", Json::Num(from as f64));
            body.set("jobs", Json::Bool(true));
            body.set("counts", Json::Bool(false));
            body.set("all_fields", Json::Arr(vec![]));
            body.set("size", Json::Num(size as f64));
            body.set("clearAll", Json::Bool(false));
            body.set("jdsource", Json::Str("facets".into()));
            body.set("isSliderEnable", Json::Bool(false));
            body.set("pageId", Json::Str("page12".into()));
            body.set("siteType", Json::Str("external".into()));
            body.set("keywords", Json::Str(q.clone()));
            body.set("global", Json::Bool(true));
            body.set("selected_fields", Json::obj());
            body.set("locationData", Json::obj());

            let root = Json::parse(&http::post_json(&url, &body.to_compact())?)?;
            let rs = root.get("refineSearch").ok_or("no refineSearch in response")?;
            let hits = rs.num_of("totalHits").unwrap_or(0.0) as usize;
            if queries.is_empty() {
                total = hits;
            } else {
                total += hits;
            }
            let items = rs.path(&["data", "jobs"]).and_then(|j| j.as_arr()).ok_or("no jobs in response")?;
            if items.is_empty() {
                break;
            }
            for it in items {
                let mut id = it.str_of("jobId");
                if id.is_empty() {
                    id = it.str_of("reqId");
                }
                if id.is_empty() || !seen.insert(id.clone()) {
                    continue;
                }
                let mut j = Job::new("phenom", format!("{}/{}", c.slug, id));
                j.title = it.str_of("title");
                j.company = c.name.clone();
                j.location = it.str_of("cityState");
                if j.location.is_empty() {
                    j.location = it.str_of("cityStateCountry");
                }
                if it.bool_of("isMultiLocation") == Some(true) {
                    let n = it.get("multi_location_array").and_then(|a| a.as_arr()).map(|a| a.len()).unwrap_or(0);
                    if n > 1 {
                        j.location = format!("{} (+{} more)", j.location, n - 1);
                    }
                }
                let remote = it.str_of("remote");
                j.remote = remote.to_lowercase() == "remote" || looks_remote(&j.location) || looks_remote(&j.title);
                j.url = match &prefix {
                    Some(p) => format!("https://{}/{}/job/{}", c.slug, p, id),
                    // No locale path found: fall back to the ATS page.
                    None => it.str_of("applyUrl").trim_end_matches('/').trim_end_matches("/apply").to_string(),
                };
                if j.url.is_empty() {
                    j.url = format!("https://{}/job/{}", c.slug, id);
                }
                j.set_posted_iso(&it.str_of("postedDate"));
                for v in [it.str_of("category"), it.str_of("type"), remote] {
                    if !v.is_empty() {
                        j.tags.push(v);
                    }
                }
                j.summary = html::truncate(&html::text(&it.str_of("descriptionTeaser")), 240);
                jobs.push(j);
                if jobs.len() >= max {
                    return Ok(Listing { jobs, total });
                }
            }
            from += items.len();
            if from >= hits || queries.is_empty() {
                break;
            }
        }
    }
    Ok(Listing { jobs, total })
}

/// Workday boards: POST /wday/cxs/<tenant>/<site>/jobs, 20 per page.
fn workday(c: &Company, queries: &[String], max: usize) -> Result<Listing, String> {
    let (host, site) = c.slug.split_once('/').ok_or("workday slug must be host/site")?;
    let tenant = host.split('.').next().unwrap_or(host);
    let url = format!("https://{}/wday/cxs/{}/{}/jobs", host, tenant, site);
    let searches: Vec<String> = if queries.is_empty() { vec![String::new()] } else { queries.to_vec() };
    let now = time::now_unix();
    let mut jobs = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0;
    for q in &searches {
        let mut offset = 0;
        loop {
            let mut body = Json::obj();
            body.set("appliedFacets", Json::obj());
            body.set("limit", Json::Num(20.0));
            body.set("offset", Json::Num(offset as f64));
            body.set("searchText", Json::Str(q.clone()));
            let root = Json::parse(&http::post_json(&url, &body.to_compact())?)?;
            let found = root.num_of("total").unwrap_or(0.0) as usize;
            if queries.is_empty() {
                total = found;
            } else {
                total += found;
            }
            let items = root.get("jobPostings").and_then(|j| j.as_arr()).ok_or("no jobPostings in response")?;
            if items.is_empty() {
                break;
            }
            for it in items {
                let path = it.str_of("externalPath");
                let id = path.rsplit('_').next().filter(|s| !s.is_empty() && !path.is_empty()).map(|s| s.to_string()).unwrap_or_else(|| path.clone());
                if id.is_empty() || !seen.insert(id.clone()) {
                    continue;
                }
                let mut j = Job::new("workday", format!("{}/{}", c.slug, id));
                j.title = it.str_of("title");
                j.company = c.name.clone();
                j.location = it.str_of("locationsText");
                j.remote = looks_remote(&j.location) || looks_remote(&j.title);
                j.url = format!("https://{}/en-US/{}{}", host, site, path);
                if let Some(days) = workday_age_days(&it.str_of("postedOn")) {
                    j.set_posted_unix(now - days * 86_400);
                }
                let kind = it.str_of("timeType");
                if !kind.is_empty() {
                    j.tags.push(kind);
                }
                jobs.push(j);
                if jobs.len() >= max {
                    return Ok(Listing { jobs, total });
                }
            }
            offset += items.len();
            if offset >= found || queries.is_empty() {
                break;
            }
        }
    }
    Ok(Listing { jobs, total })
}

/// "Posted Today" / "Posted Yesterday" / "Posted 3 Days Ago" / "Posted 30+ Days Ago"
/// TikTok's careers site (lifeattiktok.com): a public JSON search API.
/// Postings carry no date, but their ids are ByteDance ids whose top 32
/// bits are the creation time in unix seconds; that is used as the date.
fn tiktok(c: &Company, queries: &[String], max: usize) -> Result<Listing, String> {
    const API: &str = "https://api.lifeattiktok.com/api/v1/public/supplier/search/job/posts";
    let headers = [("accept-language", "en-US"), ("origin", "https://lifeattiktok.com"), ("website-path", "tiktok")];
    let searches: Vec<String> = if queries.is_empty() { vec![String::new()] } else { queries.to_vec() };
    let now = time::now_unix();
    let mut jobs = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0;
    for q in &searches {
        let mut offset = 0;
        loop {
            let limit = 100.min(max.saturating_sub(jobs.len()).max(1));
            let mut body = Json::obj();
            body.set("keyword", Json::Str(q.clone()));
            body.set("limit", Json::Num(limit as f64));
            body.set("offset", Json::Num(offset as f64));
            for k in ["recruitment_id_list", "job_category_id_list", "subject_id_list", "location_code_list"] {
                body.set(k, Json::Arr(vec![]));
            }
            let root = Json::parse(&http::post_json_with(API, &body.to_compact(), &headers)?)?;
            if root.num_of("code").unwrap_or(0.0) != 0.0 {
                return Err(format!("tiktok: {}", root.str_of("message")));
            }
            let data = root.get("data").ok_or("tiktok: no data in response")?;
            let count = data.num_of("count").unwrap_or(0.0) as usize;
            if queries.is_empty() {
                total = count;
            } else {
                total += count;
            }
            let items = data.get("job_post_list").and_then(|a| a.as_arr()).cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            for it in &items {
                let id = it.str_of("id");
                if id.is_empty() || !seen.insert(id.clone()) {
                    continue;
                }
                jobs.push(tiktok_job(c, it, &id, now));
                if jobs.len() >= max {
                    return Ok(Listing { jobs, total });
                }
            }
            offset += items.len();
            if offset >= count {
                break;
            }
        }
    }
    Ok(Listing { jobs, total })
}

fn tiktok_job(c: &Company, it: &Json, id: &str, now: i64) -> Job {
    let mut j = Job::new("tiktok", format!("{}/{}", c.slug, id));
    j.title = it.str_of("title");
    j.company = c.name.clone();
    // city_info nests city -> state/province -> country via `parent`.
    let mut place = Vec::new();
    let mut node = it.get("city_info");
    while let Some(n) = node {
        let name = n.str_of("en_name");
        if !name.is_empty() {
            place.push(name);
        }
        node = n.get("parent").filter(|p| !matches!(p, Json::Null));
    }
    j.location = place.join(", ");
    j.remote = looks_remote(&j.location) || looks_remote(&j.title);
    j.url = format!("https://lifeattiktok.com/search/{}", id);
    if let Some(secs) = bytedance_id_time(id) {
        if secs > 1_420_070_400 && secs <= now + 86_400 {
            j.set_posted_unix(secs);
        }
    }
    for k in ["job_category", "recruit_type"] {
        let v = it.get(k).map(|x| x.str_of("en_name")).unwrap_or_default();
        if !v.is_empty() {
            j.tags.push(v);
        }
    }
    j.summary = html::truncate(&html::collapse_ws(&it.str_of("description")), 240);
    j
}

/// Top 32 bits of a ByteDance id: its creation time in unix seconds.
fn bytedance_id_time(id: &str) -> Option<i64> {
    id.parse::<u64>().ok().map(|n| (n >> 32) as i64)
}

fn workday_age_days(s: &str) -> Option<i64> {
    let l = s.to_lowercase();
    if l.contains("today") {
        return Some(0);
    }
    if l.contains("yesterday") {
        return Some(1);
    }
    let digits: String = l.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names() {
        assert_eq!(display_name("jobs.cvshealth.com"), "cvshealth");
        assert_eq!(display_name("https://explore.jobs.netflix.net/careers"), "netflix");
        assert_eq!(display_name("join.disneycareers.com"), "disneycareers");
        assert_eq!(display_name("nvidia.wd5.myworkdayjobs.com/NVIDIAExternalCareerSite"), "nvidia");
        assert_eq!(display_name("https://careers.acme.co.uk/jobs"), "acme");
        assert_eq!(display_name("https://www.ycombinator.com/jobs"), "ycombinator");
        assert_eq!(display_name("stripe"), "stripe");
    }

    #[test]
    fn tiktok_ids_and_urls() {
        // 7706306078753311029 >> 32 = 1794264204 -> 2026-11-09T21:24:44Z.
        assert_eq!(bytedance_id_time("7706306078753311029").map(time::unix_to_date).as_deref(), Some("2026-11-09"));
        assert_eq!(bytedance_id_time("abc"), None);
        assert_eq!(from_url("https://lifeattiktok.com/search"), Some(("tiktok", "tiktok".to_string())));
        assert_eq!(from_url("https://careers.tiktok.com/position"), Some(("tiktok", "tiktok".to_string())));
        let it = Json::parse(r#"{"id":"7706306078753311029","title":"ML Engineer","city_info":{"en_name":"San Jose","parent":{"en_name":"California","parent":{"en_name":"United States of America","parent":null}}},"job_category":{"en_name":"R&D"},"description":"Build things."}"#).unwrap();
        let c = Company { name: "tiktok".into(), ats: "tiktok".into(), slug: "tiktok".into() };
        let j = tiktok_job(&c, &it, "7706306078753311029", 1_900_000_000);
        assert_eq!(j.location, "San Jose, California, United States of America");
        assert_eq!(j.url, "https://lifeattiktok.com/search/7706306078753311029");
        assert_eq!(j.posted_date().len(), 10);
        assert_eq!(j.tags, vec!["R&D".to_string()]);
    }

    #[test]
    fn phenom_locale_prefix() {
        assert_eq!(phenom_prefix_from("https://jobs.cvshealth.com/us/en/home").as_deref(), Some("us/en"));
        assert_eq!(phenom_prefix_from("https://careers.example.com/global/en").as_deref(), Some("global/en"));
        assert_eq!(phenom_prefix_from("https://careers.example.com/").as_deref(), None);
        assert_eq!(phenom_prefix_from("https://careers.example.com/search-results").as_deref(), None);
    }

    #[test]
    fn url_detection() {
        assert_eq!(from_url("https://boards.greenhouse.io/stripe"), Some(("greenhouse", "stripe".into())));
        assert_eq!(from_url("https://job-boards.greenhouse.io/Stripe/jobs/123"), Some(("greenhouse", "stripe".into())));
        assert_eq!(from_url("https://boards-api.greenhouse.io/v1/boards/acme/jobs"), Some(("greenhouse", "acme".into())));
        assert_eq!(from_url("https://jobs.lever.co/mistral?team=x"), Some(("lever", "mistral".into())));
        assert_eq!(from_url("https://jobs.ashbyhq.com/linear"), Some(("ashby", "linear".into())));
        assert_eq!(from_url("https://apply.workable.com/acme/"), Some(("workable", "acme".into())));
        assert_eq!(from_url("https://acme.workable.com"), Some(("workable", "acme".into())));
        assert_eq!(from_url("https://careers.smartrecruiters.com/Bosch"), Some(("smartrecruiters", "Bosch".into())));
        assert_eq!(
            from_url("https://cvshealth.wd1.myworkdayjobs.com/en-US/CVS_Health_Careers/job/x"),
            Some(("workday", "cvshealth.wd1.myworkdayjobs.com/CVS_Health_Careers".into()))
        );
        assert_eq!(from_url("https://acme.wd5.myworkdayjobs.com/External"), Some(("workday", "acme.wd5.myworkdayjobs.com/External".into())));
        assert_eq!(from_url("https://example.com/careers"), None);
        assert_eq!(from_url("stripe"), None);
    }

    #[test]
    fn names_and_ages() {
        assert_eq!(display_name("jobs.cvshealth.com"), "cvshealth");
        assert_eq!(display_name("cvshealth.wd1.myworkdayjobs.com/CVS_Health_Careers"), "cvshealth");
        assert_eq!(display_name("stripe"), "stripe");
        assert_eq!(workday_age_days("Posted Today"), Some(0));
        assert_eq!(workday_age_days("Posted 30+ Days Ago"), Some(30));
        assert_eq!(workday_age_days("Posted 2 Days Ago"), Some(2));
        assert_eq!(workday_age_days(""), None);
    }
}
