//! Company careers pages hosted on a known applicant tracking system (ATS).
//! Each ATS exposes an unauthenticated JSON list of open roles keyed by a
//! board slug, so tracking a company is one entry in `config.companies`.
//!
//! `detect` turns what a user typed (a name or a careers URL) into a
//! `Company` by reading the hostname or probing each ATS in turn.

use super::{looks_remote, Source};
use crate::config::{Company, Config};
use crate::html;
use crate::http;
use crate::json::Json;
use crate::model::Job;

pub const ATS: [&str; 5] = ["greenhouse", "lever", "ashby", "workable", "smartrecruiters"];

pub struct Companies;

impl Source for Companies {
    fn name(&self) -> &'static str {
        "companies"
    }
    fn describe(&self) -> &'static str {
        "companies you track (Greenhouse, Lever, Ashby, Workable, SmartRecruiters)"
    }
    fn unconfigured(&self, cfg: &Config) -> Option<&'static str> {
        cfg.companies.is_empty().then_some("no companies yet; try `jobscout company add <name or url>`")
    }

    fn fetch(&self, cfg: &Config) -> Result<Vec<Job>, String> {
        let mut jobs = Vec::new();
        let mut errors = Vec::new();
        // The cap applies per company; a large employer must not crowd out
        // the others.
        for c in &cfg.companies {
            match fetch_company(c, cfg.max_per_source) {
                Ok(found) => jobs.extend(found),
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

/// Fetch one company's open roles from its ATS.
pub fn fetch_company(c: &Company, max: usize) -> Result<Vec<Job>, String> {
    match c.ats.as_str() {
        "greenhouse" => greenhouse(c, max),
        "lever" => lever(c, max),
        "ashby" => ashby(c, max),
        "workable" => workable(c, max),
        "smartrecruiters" => smartrecruiters(c, max),
        other => Err(format!("unknown ats '{}'", other)),
    }
}

/// Resolve user input (company name or careers URL) to a Company, confirming
/// the board exists. Returns the company and its current posting count.
pub fn detect(input: &str) -> Result<(Company, usize), String> {
    let input = input.trim();
    if let Some((ats, slug)) = from_url(input) {
        let c = Company { name: slug.clone(), ats: ats.to_string(), slug };
        let jobs = fetch_company(&c, 10_000).map_err(|e| format!("{} board '{}' did not answer: {}", ats, c.slug, e))?;
        return Ok((c, jobs.len()));
    }
    if input.contains("://") || input.contains('/') {
        return Err(format!(
            "'{}' is not a careers URL on a supported ATS ({}). Paste the URL from the company's jobs page, or give the company name to probe.",
            input,
            ATS.join(", ")
        ));
    }
    let name = input.to_string();
    let mut slugs: Vec<String> = Vec::new();
    for s in [input.to_lowercase().replace(' ', ""), input.to_lowercase().replace(' ', "-")] {
        if !slugs.contains(&s) {
            slugs.push(s);
        }
    }
    // Some systems (SmartRecruiters) answer 200 with an empty list for
    // unknown slugs, so a probe only counts when it finds open roles.
    for ats in ATS {
        for slug in &slugs {
            let c = Company { name: name.clone(), ats: ats.to_string(), slug: slug.clone() };
            if let Ok(jobs) = fetch_company(&c, 10_000) {
                if !jobs.is_empty() {
                    return Ok((c, jobs.len()));
                }
            }
        }
    }
    Err(format!(
        "no board found for '{}' on {}. If the company uses one of these, paste its careers URL instead.",
        input,
        ATS.join(", ")
    ))
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
    None
}

// ------------------------------------------------------------- adapters

fn greenhouse(c: &Company, max: usize) -> Result<Vec<Job>, String> {
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
    Ok(jobs)
}

fn lever(c: &Company, max: usize) -> Result<Vec<Job>, String> {
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
    Ok(jobs)
}

fn ashby(c: &Company, max: usize) -> Result<Vec<Job>, String> {
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
    Ok(jobs)
}

fn workable(c: &Company, max: usize) -> Result<Vec<Job>, String> {
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
    Ok(jobs)
}

fn smartrecruiters(c: &Company, max: usize) -> Result<Vec<Job>, String> {
    let mut jobs = Vec::new();
    let mut offset = 0;
    loop {
        let url = format!("https://api.smartrecruiters.com/v1/companies/{}/postings?limit=100&offset={}", http::url_encode(&c.slug), offset);
        let root = Json::parse(&http::get(&url)?)?;
        let items = root.get("content").and_then(|j| j.as_arr()).ok_or("no content array")?;
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
                return Ok(jobs);
            }
        }
        offset += items.len();
        let total = root.num_of("totalFound").unwrap_or(0.0) as usize;
        if offset >= total || items.len() < 100 {
            break;
        }
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(from_url("https://example.com/careers"), None);
        assert_eq!(from_url("stripe"), None);
    }
}
