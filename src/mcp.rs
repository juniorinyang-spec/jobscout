//! Model Context Protocol server over stdio, so Claude Desktop, Claude Code
//! and other MCP clients can use jobscout as a tool.
//!
//! Wire format: newline-delimited JSON-RPC 2.0 on stdin/stdout. Only
//! protocol messages may be written to stdout; diagnostics go to stderr.
//! Implements `initialize`, `ping`, `tools/list`, `tools/call`,
//! `resources/list`, `resources/read` and `prompts/list` (empty).

use crate::bundle;
use crate::config::Config;
use crate::json::Json;
use crate::time;
use crate::scout::{self, AddOutcome, Phrases};
use std::fs;
use std::io::{self, BufRead, Write};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "jobscout finds job postings at the companies the user tracks (the main source) and, if the user turned them on, public boards (RemoteOK, Remotive, Arbeitnow, We Work Remotely). \
The user's location (get_config, set_location) hides postings located elsewhere; remote postings open to that country and postings with an unclear location are kept. \
Use `new_jobs` for the user's digest of postings not shown before (it remembers what it returned). \
Use `search_jobs` for ad-hoc questions; it never affects memory. \
`add_company` accepts a company name or careers URL. Queries are whole-word matches; all words of a query must appear.";

pub fn serve() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut line = String::new();
    eprintln!("jobscout mcp: serving on stdio");
    loop {
        line.clear();
        let n = stdin.lock().read_line(&mut line).map_err(|e| format!("stdin: {}", e))?;
        if n == 0 {
            return Ok(()); // client closed the pipe
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let msg = match Json::parse(text) {
            Ok(m) => m,
            Err(e) => {
                emit(&mut stdout, &error_response(Json::Null, -32700, &format!("parse error: {}", e)))?;
                continue;
            }
        };
        let msgs = match msg {
            Json::Arr(items) => items,
            other => vec![other],
        };
        for m in msgs {
            if let Some(resp) = handle(&m) {
                emit(&mut stdout, &resp)?;
            }
        }
    }
}

fn emit(out: &mut io::Stdout, msg: &Json) -> Result<(), String> {
    let mut s = msg.to_compact();
    s.push('\n');
    out.write_all(s.as_bytes()).and_then(|_| out.flush()).map_err(|e| format!("stdout: {}", e))
}

fn result_response(id: Json, result: Json) -> Json {
    let mut o = Json::obj();
    o.set("jsonrpc", Json::Str("2.0".into()));
    o.set("id", id);
    o.set("result", result);
    o
}

fn error_response(id: Json, code: i64, message: &str) -> Json {
    let mut err = Json::obj();
    err.set("code", Json::Num(code as f64));
    err.set("message", Json::Str(message.to_string()));
    let mut o = Json::obj();
    o.set("jsonrpc", Json::Str("2.0".into()));
    o.set("id", id);
    o.set("error", err);
    o
}

/// Returns the response to send, or None for notifications.
fn handle(msg: &Json) -> Option<Json> {
    let id = msg.get("id").cloned();
    let method = msg.str_of("method");
    let params = msg.get("params").cloned().unwrap_or_else(Json::obj);

    if method.is_empty() {
        return id.map(|id| error_response(id, -32600, "missing method"));
    }
    if method.starts_with("notifications/") {
        return None;
    }
    let id = id?; // a request without an id cannot be answered

    let outcome: Result<Json, (i64, String)> = match method.as_str() {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(Json::obj()),
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(&params),
        "resources/list" => resources_list(),
        "resources/templates/list" => Ok(obj(vec![("resourceTemplates", Json::Arr(vec![]))])),
        "resources/read" => resources_read(&params),
        "prompts/list" => Ok(obj(vec![("prompts", Json::Arr(vec![]))])),
        _ => Err((-32601, format!("method not found: {}", method))),
    };
    Some(match outcome {
        Ok(result) => result_response(id, result),
        Err((code, message)) => error_response(id, code, &message),
    })
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    let mut o = Json::obj();
    for (k, v) in pairs {
        o.set(k, v);
    }
    o
}

fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}

fn initialize(params: &Json) -> Json {
    let requested = params.str_of("protocolVersion");
    let version = if PROTOCOL_VERSIONS.contains(&requested.as_str()) { requested } else { PROTOCOL_VERSIONS[0].to_string() };
    obj(vec![
        ("protocolVersion", s(&version)),
        (
            "capabilities",
            obj(vec![
                ("tools", obj(vec![("listChanged", Json::Bool(false))])),
                ("resources", obj(vec![("subscribe", Json::Bool(false)), ("listChanged", Json::Bool(false))])),
                ("prompts", obj(vec![("listChanged", Json::Bool(false))])),
            ]),
        ),
        ("serverInfo", obj(vec![("name", s("jobscout")), ("version", s(env!("CARGO_PKG_VERSION")))])),
        ("instructions", s(INSTRUCTIONS)),
    ])
}

// ------------------------------------------------------------------- tools

struct Prop {
    name: &'static str,
    kind: &'static str,
    desc: &'static str,
    required: bool,
}

const fn p(name: &'static str, kind: &'static str, desc: &'static str) -> Prop {
    Prop { name, kind, desc, required: false }
}
const fn req(name: &'static str, kind: &'static str, desc: &'static str) -> Prop {
    Prop { name, kind, desc, required: true }
}

fn schema(props: &[Prop]) -> Json {
    let mut properties = Json::obj();
    let mut required = Vec::new();
    for pr in props {
        let mut def = Json::obj();
        if pr.kind == "string[]" {
            def.set("type", s("array"));
            def.set("items", obj(vec![("type", s("string"))]));
        } else {
            def.set("type", s(pr.kind));
        }
        def.set("description", s(pr.desc));
        properties.set(pr.name, def);
        if pr.required {
            required.push(Json::Str(pr.name.to_string()));
        }
    }
    let mut sch = obj(vec![("type", s("object")), ("properties", properties)]);
    if !required.is_empty() {
        sch.set("required", Json::Arr(required));
    }
    sch.set("additionalProperties", Json::Bool(false));
    sch
}

fn tool(name: &str, desc: &str, props: &[Prop]) -> Json {
    obj(vec![("name", s(name)), ("description", s(desc)), ("inputSchema", schema(props))])
}

const SOURCES_DESC: &str = "Restrict to these sources: remoteok, remotive, arbeitnow, weworkremotely, companies. Default: all enabled.";

const SINCE_DESC: &str = "Only postings published within this many hours (e.g. 24 for 'today', 168 for 'this week'). Unlike max_age_days this also applies to tracked companies; postings without a date are left out.";

/// `posted_within_hours` -> seconds, if given and positive.
fn since_arg(args: &Json) -> Option<u64> {
    args.num_of("posted_within_hours").filter(|h| *h > 0.0).map(|h| (h * 3600.0).round() as u64)
}

const ON_DESC: &str = "Only postings published on this calendar day in the user's local time: 'YYYY-MM-DD', 'today', 'yesterday', or a weekday name ('friday' = the most recent Friday, today included). Applies to tracked companies too; undated postings are left out.";

/// `posted_on` -> local day window; Err for an unreadable value.
fn on_arg(args: &Json) -> Result<Option<(i64, i64)>, String> {
    let v = args.str_of("posted_on");
    if v.trim().is_empty() {
        return Ok(None);
    }
    time::parse_day(&v, time::now_unix(), time::local_offset_secs())
        .map(Some)
        .ok_or_else(|| format!("posted_on: cannot read '{}' (use YYYY-MM-DD, today, yesterday or a weekday)", v))
}

const LOCATION_DESC: &str = "Country to search from for this call only, e.g. 'United States' or 'Germany'; 'anywhere' turns the location filter off. Default: the user's saved location.";

/// `location` argument -> Some(country code or "" for anywhere), None when absent.
fn location_arg(args: &Json) -> Result<Option<String>, String> {
    let v = args.str_of("location");
    let v = v.trim();
    if v.is_empty() {
        return Ok(None);
    }
    if v.eq_ignore_ascii_case("anywhere") {
        return Ok(Some(String::new()));
    }
    crate::location::resolve_country(v)
        .map(|c| Some(c.to_string()))
        .ok_or_else(|| format!("location: cannot tell which country '{}' is", v))
}

fn tools_list() -> Json {
    let tools = vec![
        tool(
            "search_jobs",
            "Search current job postings across boards and tracked companies. Does not affect the user's seen-postings memory. Results are newest first.",
            &[
                p("query", "string", "Words that must all appear (whole-word, case-insensitive) in the title, company, location, tags or summary. Omit to use the user's configured queries."),
                p("remote_only", "boolean", "Only remote postings."),
                p("max_age_days", "integer", "Ignore job-board postings older than this many days (default from config, usually 14; 0 = no limit). Tracked companies' open roles are never age-filtered."),
                p("posted_within_hours", "number", SINCE_DESC),
                p("posted_on", "string", ON_DESC),
                p("location", "string", LOCATION_DESC),
                p("sources", "string[]", SOURCES_DESC),
                p("limit", "integer", "Maximum postings to return (default 25)."),
                p("format", "string", "'text' (default) or 'json' for structured records."),
            ],
        ),
        tool(
            "new_jobs",
            "Run the scout: fetch everything, apply the configured queries, and return only postings never shown before. Writes a bundle and remembers them unless dry_run is true.",
            &[
                p("dry_run", "boolean", "Preview without writing a bundle or updating memory (default false)."),
                p("posted_within_hours", "number", SINCE_DESC),
                p("posted_on", "string", ON_DESC),
                p("location", "string", LOCATION_DESC),
                p("sources", "string[]", SOURCES_DESC),
                p("limit", "integer", "Maximum postings to return; with dry_run false only these are remembered."),
            ],
        ),
        tool("list_companies", "List the companies jobscout tracks, with the ATS and board slug for each.", &[]),
        tool(
            "add_company",
            "Track a company. Accepts a company name (probes Greenhouse, Lever, Ashby, Workable, SmartRecruiters) or a careers URL (also Phenom sites and Workday boards). Reports the board's open-role count.",
            &[req("input", "string", "Company name or careers page URL.")],
        ),
        tool("remove_company", "Stop tracking a company.", &[req("name", "string", "Company name or slug as shown by list_companies.")]),
        tool("list_queries", "List the configured search queries.", &[]),
        tool("add_query", "Add a search query. All its words must appear in a posting for it to match.", &[req("query", "string", "Query words, e.g. 'rust backend'.")]),
        tool("remove_query", "Remove a search query.", &[req("query", "string", "Exact query to remove.")]),
        tool("list_excludes", "List the exclude phrases; a posting containing any of them is dropped.", &[]),
        tool(
            "add_exclude",
            "Add an exclude phrase (whole words, case-insensitive). Useful against noise, e.g. 'go to market' when a query is 'go'.",
            &[req("phrase", "string", "Phrase to exclude, e.g. 'staff' or 'go to market'.")],
        ),
        tool("remove_exclude", "Remove an exclude phrase.", &[req("phrase", "string", "Exact phrase to remove.")]),
        tool(
            "set_location",
            "Save the country the user searches from (e.g. 'United States'). Postings located elsewhere are then hidden; remote postings open to that country stay. 'anywhere' clears it.",
            &[req("place", "string", "Country name, or a city/state inside it, or 'anywhere'.")],
        ),
        tool(
            "set_source",
            "Turn a job board on or off. Boards are off by default; tracked companies ('companies') are the main source.",
            &[
                req("name", "string", "One of: remoteok, remotive, arbeitnow, weworkremotely, companies."),
                req("enabled", "boolean", "true to turn it on, false to turn it off."),
            ],
        ),
        tool("list_bundles", "List digests written by previous runs, newest first.", &[p("limit", "integer", "Maximum entries (default 10).")]),
        tool("get_bundle", "Read one digest (Markdown).", &[req("name", "string", "Bundle name from list_bundles, e.g. 2026-11-15_09-00-00.")]),
        tool("get_config", "Show the active configuration: queries, excludes, filters, sources, companies and file paths.", &[]),
    ];
    obj(vec![("tools", Json::Arr(tools))])
}

fn text_result(text: String, is_error: bool) -> Json {
    let block = obj(vec![("type", s("text")), ("text", Json::Str(text))]);
    let mut r = obj(vec![("content", Json::Arr(vec![block]))]);
    if is_error {
        r.set("isError", Json::Bool(true));
    }
    r
}

fn tools_call(params: &Json) -> Result<Json, (i64, String)> {
    let name = params.str_of("name");
    let args = params.get("arguments").cloned().unwrap_or_else(Json::obj);
    let known = [
        "search_jobs",
        "new_jobs",
        "list_companies",
        "add_company",
        "remove_company",
        "list_queries",
        "add_query",
        "remove_query",
        "list_excludes",
        "add_exclude",
        "remove_exclude",
        "set_location",
        "set_source",
        "list_bundles",
        "get_bundle",
        "get_config",
    ];
    if !known.contains(&name.as_str()) {
        return Err((-32602, format!("unknown tool: {}", name)));
    }
    Ok(match run_tool(&name, &args) {
        Ok(text) => text_result(text, false),
        Err(e) => text_result(e, true),
    })
}

fn int_arg(args: &Json, key: &str) -> Option<usize> {
    args.num_of(key).map(|n| n.max(0.0) as usize)
}

fn run_tool(name: &str, args: &Json) -> Result<String, String> {
    let mut cfg = Config::load()?;
    match name {
        "search_jobs" => {
            let q = args.str_of("query");
            if !q.trim().is_empty() {
                cfg.queries = vec![q.trim().to_string()];
            }
            if args.bool_of("remote_only") == Some(true) {
                cfg.remote_only = true;
            }
            if let Some(d) = int_arg(args, "max_age_days") {
                cfg.max_age_days = d as u64;
            }
            cfg.since_secs = since_arg(args);
            cfg.on_day = on_arg(args)?;
            if let Some(l) = location_arg(args)? {
                cfg.location = l;
            }
            let only = args.strings_of("sources");
            let limit = int_arg(args, "limit").unwrap_or(25).max(1);
            let jobs = scout::search(&cfg, &only, limit, true)?;
            if args.str_of("format") == "json" {
                return Ok(Json::Arr(jobs.iter().map(|j| j.to_json()).collect()).to_pretty());
            }
            if jobs.is_empty() {
                return Ok(format!("No postings matched (queries: {}).", cfg.queries.join(" | ")));
            }
            Ok(format!("{} posting{} (newest first):\n\n{}", jobs.len(), if jobs.len() == 1 { "" } else { "s" }, scout::format_jobs(&jobs, true)))
        }
        "new_jobs" => {
            let only = args.strings_of("sources");
            let dry = args.bool_of("dry_run").unwrap_or(false);
            cfg.since_secs = since_arg(args);
            cfg.on_day = on_arg(args)?;
            if let Some(l) = location_arg(args)? {
                cfg.location = l;
            }
            let out = scout::run(&cfg, &only, int_arg(args, "limit"), dry, true)?;
            let mut text = format!(
                "{} fetched, {} matched the queries ({}), {}{}.\n",
                out.fetched,
                out.matched,
                cfg.queries.join(" | "),
                out.new_summary(),
                if out.dry_run { " (dry run: nothing remembered)" } else { "" }
            );
            if let Some(b) = &out.bundle {
                text.push_str(&format!("Bundle written: {}\n", b.display()));
            }
            if out.new.is_empty() {
                text.push_str("No new postings since the last run.\n");
            } else {
                text.push('\n');
                text.push_str(&scout::format_jobs(&out.new, true));
            }
            Ok(text)
        }
        "list_companies" => {
            if cfg.companies.is_empty() {
                return Ok("No companies tracked yet. Use add_company with a name or careers URL.".into());
            }
            Ok(cfg.companies.iter().map(|c| format!("{} — {} ({})", c.name, c.ats, c.slug)).collect::<Vec<_>>().join("\n"))
        }
        "add_company" => {
            let input = args.str_of("input");
            if input.trim().is_empty() {
                return Err("input is required".into());
            }
            match scout::add_company(&mut cfg, &input)? {
                AddOutcome::Added(c, n) => Ok(format!("Now tracking {} on {} (slug '{}'), {} open role{}.", c.name, c.ats, c.slug, n, if n == 1 { "" } else { "s" })),
                AddOutcome::AlreadyTracked(c) => Ok(format!("{} is already tracked ({} '{}').", c.name, c.ats, c.slug)),
            }
        }
        "remove_company" => {
            let target = args.str_of("name");
            if scout::remove_company(&mut cfg, &target)? {
                Ok(format!("Removed {}.", target))
            } else {
                Err(format!("{} is not tracked.", target))
            }
        }
        "list_queries" => Ok(if cfg.queries.is_empty() { "No queries configured (everything matches).".to_string() } else { cfg.queries.join("\n") }),
        "add_query" | "add_exclude" => {
            let (which, key, noun) = if name == "add_query" { (Phrases::Queries, "query", "query") } else { (Phrases::Exclude, "phrase", "exclude phrase") };
            let q = args.str_of(key);
            if scout::add_phrase(&mut cfg, which, &q)? {
                Ok(format!("Added {}: {}", noun, q.trim()))
            } else {
                Ok(format!("Already present: {}", q.trim()))
            }
        }
        "remove_query" | "remove_exclude" => {
            let (which, key, noun) = if name == "remove_query" { (Phrases::Queries, "query", "query") } else { (Phrases::Exclude, "phrase", "exclude phrase") };
            let q = args.str_of(key);
            if scout::remove_phrase(&mut cfg, which, &q)? {
                Ok(format!("Removed {}: {}", noun, q.trim()))
            } else {
                Err(format!("No such {}: {}", noun, q.trim()))
            }
        }
        "set_location" => {
            let place = args.str_of("place");
            let code = match location_arg(&obj(vec![("location", Json::Str(place.clone()))]))? {
                Some(c) => c,
                None => return Err("place is required (a country, or 'anywhere')".into()),
            };
            cfg.location = code;
            cfg.save()?;
            Ok(if cfg.location.is_empty() {
                "Location cleared: searching anywhere.".to_string()
            } else {
                format!("Location set to {} ({}). Postings located elsewhere are hidden; remote ones open to it stay.", crate::location::country_name(&cfg.location), cfg.location)
            })
        }
        "set_source" => {
            let n = args.str_of("name");
            let on = args.bool_of("enabled").ok_or("enabled (true/false) is required")?;
            scout::set_source(&mut cfg, n.trim(), on)?;
            Ok(format!("{} is now {}.", n.trim(), if on { "on" } else { "off" }))
        }
        "list_excludes" => Ok(if cfg.exclude.is_empty() { "No exclude phrases configured.".to_string() } else { cfg.exclude.join("\n") }),
        "list_bundles" => {
            let limit = int_arg(args, "limit").unwrap_or(10).max(1);
            let files = bundle::list(&cfg.bundles_dir());
            if files.is_empty() {
                return Ok("No bundles yet; run new_jobs first.".into());
            }
            Ok(files.iter().take(limit).filter_map(|f| f.file_stem().map(|x| x.to_string_lossy().into_owned())).collect::<Vec<_>>().join("\n"))
        }
        "get_bundle" => {
            let name = args.str_of("name");
            read_bundle(&cfg, &name)
        }
        "get_config" => {
            let mut v = cfg.to_json();
            v.set("config_path", Json::Str(cfg.config_path().display().to_string()));
            v.set("bundles_dir", Json::Str(cfg.bundles_dir().display().to_string()));
            Ok(v.to_pretty())
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}

fn read_bundle(cfg: &Config, name: &str) -> Result<String, String> {
    let stem = name.trim().trim_end_matches(".md");
    if stem.is_empty() || stem.contains('/') || stem.contains("..") {
        return Err("invalid bundle name".into());
    }
    let path = cfg.bundles_dir().join(format!("{}.md", stem));
    fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))
}

// --------------------------------------------------------------- resources

const BUNDLE_URI: &str = "jobscout://bundle/";
const CONFIG_URI: &str = "jobscout://config";

fn resources_list() -> Result<Json, (i64, String)> {
    let cfg = Config::load().map_err(|e| (-32603, e))?;
    let mut items = vec![obj(vec![
        ("uri", s(CONFIG_URI)),
        ("name", s("config")),
        ("description", s("jobscout configuration")),
        ("mimeType", s("application/json")),
    ])];
    for f in bundle::list(&cfg.bundles_dir()).iter().take(50) {
        let stem = f.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        items.push(obj(vec![
            ("uri", Json::Str(format!("{}{}", BUNDLE_URI, stem))),
            ("name", Json::Str(stem.clone())),
            ("description", Json::Str(format!("Job bundle {}", stem.replace('_', " ")))),
            ("mimeType", s("text/markdown")),
        ]));
    }
    Ok(obj(vec![("resources", Json::Arr(items))]))
}

fn resources_read(params: &Json) -> Result<Json, (i64, String)> {
    let uri = params.str_of("uri");
    let cfg = Config::load().map_err(|e| (-32603, e))?;
    let (mime, text) = if uri == CONFIG_URI {
        ("application/json", cfg.to_json().to_pretty())
    } else if let Some(stem) = uri.strip_prefix(BUNDLE_URI) {
        ("text/markdown", read_bundle(&cfg, stem).map_err(|e| (-32602, e))?)
    } else {
        return Err((-32602, format!("unknown resource: {}", uri)));
    };
    let item = obj(vec![("uri", Json::Str(uri)), ("mimeType", s(mime)), ("text", Json::Str(text))]);
    Ok(obj(vec![("contents", Json::Arr(vec![item]))]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(json: &str) -> Option<Json> {
        handle(&Json::parse(json).unwrap())
    }

    #[test]
    fn initialize_and_list() {
        let r = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#).unwrap();
        assert_eq!(r.path(&["result", "protocolVersion"]).unwrap().as_str(), Some("2025-03-26"));
        assert_eq!(r.path(&["result", "serverInfo", "name"]).unwrap().as_str(), Some("jobscout"));
        assert!(call(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        let t = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        let tools = t.path(&["result", "tools"]).unwrap().as_arr().unwrap();
        assert!(tools.iter().any(|x| x.str_of("name") == "new_jobs"));
        let sch = tools[0].get("inputSchema").unwrap();
        assert_eq!(sch.str_of("type"), "object");
    }

    #[test]
    fn errors() {
        let r = call(r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#).unwrap();
        assert_eq!(r.path(&["error", "code"]).unwrap().as_f64(), Some(-32601.0));
        let r = call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#).unwrap();
        assert_eq!(r.path(&["error", "code"]).unwrap().as_f64(), Some(-32602.0));
        assert!(call(r#"{"jsonrpc":"2.0","id":5}"#).unwrap().get("error").is_some());
    }
}
