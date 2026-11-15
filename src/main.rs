//! jobscout: a zero-dependency job posting scout.
//!
//! Polls public job boards, filters postings against your keywords, remembers
//! what it has already shown you, and bundles the new ones into Markdown +
//! JSON digests.

mod bundle;
mod config;
mod filter;
mod html;
mod http;
mod json;
mod model;
mod sources;
mod store;
mod time;

use config::Config;
use filter::Filter;
use json::Json;
use model::Job;
use std::collections::HashSet;
use std::process::exit;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
jobscout - find new job postings across boards and bundle them into digests

USAGE
  jobscout <command> [options]

COMMANDS
  init                 Write a starter config to ~/.jobscout/config.json
  search [words...]    One-off search; words override the configured queries
  run                  Fetch, filter, bundle NEW postings, remember them
  watch                Run repeatedly (default every 30m) until stopped
  bundles              List bundles written so far
  sources              List available job boards
  company add X...     Track a company by name or careers URL (ATS auto-detected:
                       Greenhouse, Lever, Ashby, Workable, SmartRecruiters)
  company list         Show tracked companies
  company remove X     Stop tracking a company
  query add WORDS      Add a search query (all words must appear)
  query remove WORDS   Remove a query
  query list           Show queries
  config               Show the active config and paths
  forget               Clear the memory of seen postings
  help, version

OPTIONS
  -s, --source NAME    Only use this board (repeatable)
  -n, --limit N        Max postings to print (search: 50). With `run`, only
                       the N shown are remembered; the rest stay new.
  -r, --remote         Remote postings only
      --every DUR      watch interval: 90s, 30m, 2h, 1d
      --dry-run        run: print new postings but write nothing
      --json           Print postings as JSON instead of text
  -q, --quiet          Less progress output

ENVIRONMENT
  JOBSCOUT_HOME        Data directory (default ~/.jobscout)
";

#[derive(Default, Debug)]
struct Args {
    cmd: String,
    words: Vec<String>,
    sources: Vec<String>,
    limit: Option<usize>,
    every: Option<u64>,
    json: bool,
    dry_run: bool,
    remote: bool,
    quiet: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            i += 1;
            argv.get(i).cloned().ok_or_else(|| format!("{} needs a value", name))
        };
        match arg {
            "-s" | "--source" => a.sources.push(value(arg)?),
            "-n" | "--limit" => a.limit = Some(value(arg)?.parse().map_err(|_| "--limit must be a number".to_string())?),
            "--every" => {
                let v = value(arg)?;
                a.every = Some(time::parse_duration(&v).ok_or_else(|| format!("bad duration '{}'", v))?);
            }
            "--json" => a.json = true,
            "--dry-run" | "--dry" => a.dry_run = true,
            "-r" | "--remote" => a.remote = true,
            "-q" | "--quiet" => a.quiet = true,
            "-h" | "--help" => a.cmd = "help".into(),
            "-V" | "--version" => a.cmd = "version".into(),
            _ if arg.starts_with('-') => return Err(format!("unknown option '{}'", arg)),
            _ if a.cmd.is_empty() => a.cmd = arg.to_string(),
            _ => a.words.push(arg.to_string()),
        }
        i += 1;
    }
    if a.cmd.is_empty() {
        a.cmd = "help".into();
    }
    Ok(a)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        exit(1);
    }
}

fn run() -> Result<(), String> {
    let a = parse_args()?;
    match a.cmd.as_str() {
        "help" => {
            print!("{}", HELP);
            Ok(())
        }
        "version" => {
            println!("jobscout {}", VERSION);
            Ok(())
        }
        "init" => cmd_init(),
        "config" => cmd_config(),
        "sources" => cmd_sources(),
        "search" => cmd_search(&a),
        "run" => cmd_run(&a).map(|_| ()),
        "watch" => cmd_watch(&a),
        "bundles" => cmd_bundles(),
        "company" | "companies" => cmd_company(&a),
        "query" | "queries" => cmd_query(&a),
        "forget" => cmd_forget(),
        other => Err(format!("unknown command '{}' (see `jobscout help`)", other)),
    }
}

// ------------------------------------------------------------------ commands

fn cmd_init() -> Result<(), String> {
    let cfg = Config::defaults(config::home_dir());
    if cfg.config_path().exists() {
        println!("config already exists: {}", cfg.config_path().display());
        return Ok(());
    }
    let path = cfg.save()?;
    println!("wrote {}", path.display());
    println!("edit it to set your queries, then run `jobscout run`.");
    Ok(())
}

fn cmd_config() -> Result<(), String> {
    let cfg = Config::load()?;
    println!("config:  {}{}", cfg.config_path().display(), if cfg.config_path().exists() { "" } else { "  (not written yet; showing defaults)" });
    println!("seen:    {}", cfg.seen_path().display());
    println!("bundles: {}", cfg.bundles_dir().display());
    println!();
    print!("{}", cfg.to_json().to_pretty());
    Ok(())
}

fn cmd_sources() -> Result<(), String> {
    let cfg = Config::load()?;
    for s in sources::all() {
        let state = if cfg.source_enabled(s.name()) { "on " } else { "off" };
        println!("{}  {:<15} {}", state, s.name(), s.describe());
    }
    if !cfg.companies.is_empty() {
        println!();
        println!("tracked companies:");
        for c in &cfg.companies {
            println!("     {:<20} {} ({})", c.name, c.ats, c.slug);
        }
    }
    Ok(())
}

fn cmd_company(a: &Args) -> Result<(), String> {
    let mut cfg = Config::load()?;
    let sub = a.words.first().map(|s| s.as_str()).unwrap_or("list");
    let rest = &a.words[a.words.len().min(1)..];
    match sub {
        "list" | "ls" => {
            if cfg.companies.is_empty() {
                println!("no companies tracked yet. Add one with `jobscout company add <name or careers url>`.");
            }
            for c in &cfg.companies {
                println!("{:<20} {:<16} {}", c.name, c.ats, c.slug);
            }
            Ok(())
        }
        "add" => {
            if rest.is_empty() {
                return Err("usage: jobscout company add <name or careers url> [...]".into());
            }
            if !http::curl_available() {
                return Err("`curl` was not found on PATH; jobscout uses it for HTTPS".into());
            }
            let mut failures = 0;
            for input in rest {
                match sources::companies::detect(input) {
                    Ok((c, count)) => {
                        if let Some(existing) = cfg.companies.iter().find(|x| x.ats == c.ats && x.slug == c.slug) {
                            println!("{:<20} already tracked as {} ({})", existing.name, existing.ats, existing.slug);
                            continue;
                        }
                        println!("{:<20} -> {} '{}', {} open role{}", c.name, c.ats, c.slug, count, if count == 1 { "" } else { "s" });
                        cfg.companies.push(c);
                    }
                    Err(e) => {
                        failures += 1;
                        eprintln!("{}: {}", input, e);
                    }
                }
            }
            if !cfg.source_enabled("companies") {
                cfg.sources.push("companies".into());
            }
            cfg.save()?;
            if failures > 0 {
                Err(format!("{} compan{} could not be added", failures, if failures == 1 { "y" } else { "ies" }))
            } else {
                Ok(())
            }
        }
        "remove" | "rm" => {
            if rest.is_empty() {
                return Err("usage: jobscout company remove <name or slug>".into());
            }
            for target in rest {
                let t = target.to_lowercase();
                let before = cfg.companies.len();
                cfg.companies.retain(|c| c.name.to_lowercase() != t && c.slug.to_lowercase() != t);
                if cfg.companies.len() == before {
                    eprintln!("{}: not tracked", target);
                } else {
                    println!("removed {}", target);
                }
            }
            cfg.save()?;
            Ok(())
        }
        other => Err(format!("unknown subcommand 'company {}' (add, list, remove)", other)),
    }
}

fn cmd_query(a: &Args) -> Result<(), String> {
    let mut cfg = Config::load()?;
    let sub = a.words.first().map(|s| s.as_str()).unwrap_or("list");
    let phrase = a.words[a.words.len().min(1)..].join(" ").trim().to_string();
    match sub {
        "list" | "ls" => {
            for q in &cfg.queries {
                println!("{}", q);
            }
            Ok(())
        }
        "add" => {
            if phrase.is_empty() {
                return Err("usage: jobscout query add <words>".into());
            }
            if cfg.queries.iter().any(|q| q.eq_ignore_ascii_case(&phrase)) {
                println!("already present: {}", phrase);
            } else {
                cfg.queries.push(phrase.clone());
                cfg.save()?;
                println!("added: {}", phrase);
            }
            Ok(())
        }
        "remove" | "rm" => {
            let before = cfg.queries.len();
            cfg.queries.retain(|q| !q.eq_ignore_ascii_case(&phrase));
            if cfg.queries.len() == before {
                return Err(format!("no such query: {}", phrase));
            }
            cfg.save()?;
            println!("removed: {}", phrase);
            Ok(())
        }
        other => Err(format!("unknown subcommand 'query {}' (add, list, remove)", other)),
    }
}

fn cmd_search(a: &Args) -> Result<(), String> {
    let mut cfg = Config::load()?;
    if a.remote {
        cfg.remote_only = true;
    }
    if !a.words.is_empty() {
        // Words given on the command line form one query; Remotive uses them server-side too.
        cfg.queries = vec![a.words.join(" ")];
    }
    let filt = Filter::from_config(&cfg);
    let now = time::now_unix();
    let mut jobs = collect(&cfg, &a.sources, a.quiet)?;
    jobs.retain(|j| filt.keep(j, now));
    sort_newest_first(&mut jobs);
    let limit = a.limit.unwrap_or(50);
    let shown: Vec<Job> = jobs.into_iter().take(limit).collect();
    output(&shown, a.json);
    if !a.json {
        eprintln!("{} matching posting{} shown", shown.len(), if shown.len() == 1 { "" } else { "s" });
    }
    Ok(())
}

fn cmd_run(a: &Args) -> Result<usize, String> {
    let mut cfg = Config::load()?;
    if a.remote {
        cfg.remote_only = true;
    }
    let filt = Filter::from_config(&cfg);
    let now = time::now_unix();
    let mut seen = store::Seen::load(&cfg.seen_path())?;

    let mut jobs = collect(&cfg, &a.sources, a.quiet)?;
    let fetched = jobs.len();
    jobs.retain(|j| filt.keep(j, now));
    let matching = jobs.len();
    jobs.retain(|j| !seen.contains(&j.key()));
    sort_newest_first(&mut jobs);
    if let Some(n) = a.limit {
        jobs.truncate(n);
    }

    output(&jobs, a.json);

    let stamp = time::now_stamp();
    let generated = time::now_iso();
    if !jobs.is_empty() && !a.dry_run {
        let (md, _) = bundle::write(&cfg.bundles_dir(), &stamp, &generated, &jobs)?;
        for j in &jobs {
            seen.mark(&j.key(), &generated);
        }
        seen.save()?;
        eprintln!("bundle written: {}", md.display());
    }
    eprintln!(
        "{} fetched, {} matched, {} new{}",
        fetched,
        matching,
        jobs.len(),
        if a.dry_run { " (dry run, nothing saved)" } else { "" }
    );
    Ok(jobs.len())
}

fn cmd_watch(a: &Args) -> Result<(), String> {
    let every = a.every.unwrap_or(30 * 60);
    eprintln!("watching every {}; press Ctrl-C to stop", time::human_duration(every));
    loop {
        eprintln!("--- {} ---", time::now_iso());
        if let Err(e) = cmd_run(a) {
            eprintln!("error: {}", e);
        }
        eprintln!("next run at {}", time::unix_to_iso(time::now_unix() + every as i64));
        std::thread::sleep(std::time::Duration::from_secs(every));
    }
}

fn cmd_bundles() -> Result<(), String> {
    let cfg = Config::load()?;
    let files = bundle::list(&cfg.bundles_dir());
    if files.is_empty() {
        println!("no bundles yet in {}", cfg.bundles_dir().display());
        return Ok(());
    }
    for f in files {
        println!("{}", f.display());
    }
    Ok(())
}

fn cmd_forget() -> Result<(), String> {
    let cfg = Config::load()?;
    let mut seen = store::Seen::load(&cfg.seen_path())?;
    let n = seen.len();
    seen.clear();
    seen.save()?;
    println!("forgot {} posting{}", n, if n == 1 { "" } else { "s" });
    Ok(())
}

// ------------------------------------------------------------------ helpers

/// Fetch from every enabled (or explicitly requested) source, de-duplicated.
fn collect(cfg: &Config, only: &[String], quiet: bool) -> Result<Vec<Job>, String> {
    if !http::curl_available() {
        return Err("`curl` was not found on PATH; jobscout uses it for HTTPS".into());
    }
    let all = sources::all();
    for name in only {
        if !all.iter().any(|s| s.name() == name) {
            return Err(format!("unknown source '{}' (see `jobscout sources`)", name));
        }
    }

    let mut jobs: Vec<Job> = Vec::new();
    let mut keys: HashSet<String> = HashSet::new();
    let mut urls: HashSet<String> = HashSet::new();
    let mut failures = 0;
    let mut used = 0;

    for src in &all {
        let wanted = if only.is_empty() { cfg.source_enabled(src.name()) } else { only.iter().any(|n| n == src.name()) };
        if !wanted {
            continue;
        }
        if let Some(why) = src.unconfigured(cfg) {
            if !quiet || !only.is_empty() {
                eprintln!("  {:<15} skipped ({})", src.name(), why);
            }
            continue;
        }
        used += 1;
        if !quiet {
            eprint!("  {:<15} ", src.name());
        }
        match src.fetch(cfg) {
            Ok(found) => {
                let mut added = 0;
                for j in found {
                    let uid = model::url_identity(&j.url);
                    if !keys.insert(j.key()) || (!uid.is_empty() && !urls.insert(uid)) {
                        continue;
                    }
                    jobs.push(j);
                    added += 1;
                }
                if !quiet {
                    eprintln!("{} postings", added);
                }
            }
            Err(e) => {
                failures += 1;
                if !quiet {
                    eprintln!("FAILED: {}", e);
                } else {
                    eprintln!("  {}: {}", src.name(), e);
                }
            }
        }
    }
    if used == 0 {
        return Err("no sources enabled; edit config or pass --source".into());
    }
    if failures == used {
        return Err("every source failed; check your network connection".into());
    }
    Ok(jobs)
}

fn sort_newest_first(jobs: &mut [Job]) {
    jobs.sort_by(|a, b| b.posted_unix.unwrap_or(i64::MIN).cmp(&a.posted_unix.unwrap_or(i64::MIN)).then_with(|| a.title.cmp(&b.title)));
}

fn output(jobs: &[Job], as_json: bool) {
    if as_json {
        println!("{}", Json::Arr(jobs.iter().map(|j| j.to_json()).collect()).to_pretty().trim_end());
        return;
    }
    for j in jobs {
        print_job(j);
    }
}

fn print_job(j: &Job) {
    let mut meta: Vec<String> = Vec::new();
    if !j.company.is_empty() {
        meta.push(j.company.clone());
    }
    if !j.location.is_empty() {
        meta.push(j.location.clone());
    }
    if j.remote {
        meta.push("remote".into());
    }
    if !j.salary.is_empty() {
        meta.push(j.salary.clone());
    }
    let date = j.posted_date();
    if !date.is_empty() {
        meta.push(date);
    }
    println!("[{}] {}", j.source, j.title);
    if !meta.is_empty() {
        println!("    {}", meta.join(" · "));
    }
    println!("    {}", j.url);
}
