# jobscout

A small command-line agent that watches the careers pages of companies you
choose for new postings, filters them by your keywords and location, and
bundles the new ones into Markdown + JSON digests. Public job boards can be
switched on too. Written in Rust with **zero third-party crates**: the JSON
parser, HTML/RSS extraction, date handling, config and storage are all in this
repo.

```
$ jobscout run
  companies       412 postings
[greenhouse] Senior Backend Engineer, Payments
    stripe · New York, NY · 2026-11-14
    https://stripe.com/jobs/search?gh_jid=...
[lever] Machine Learning Engineer
    spotify · New York, NY · 2026-11-13
    https://jobs.lever.co/spotify/...
...
bundle written: ~/.jobscout/bundles/2026-11-15_10-30-00.md
412 fetched, 18 matched, 18 new
```

## Install

**Homebrew** (macOS or Linux). The repo doubles as a tap:

```sh
brew tap juniorinyang-spec/jobscout https://github.com/juniorinyang-spec/jobscout
brew install juniorinyang-spec/jobscout/jobscout        # latest release
brew install --HEAD juniorinyang-spec/jobscout/jobscout # or straight from main
```

**Prebuilt binary**, no toolchain needed. Each release on the
[Releases page](https://github.com/juniorinyang-spec/jobscout/releases) has
tarballs for macOS (Apple Silicon and Intel) and Linux (x64 and arm64):

```sh
# pick one: aarch64-apple-darwin (Apple Silicon), x86_64-apple-darwin (Intel),
#           x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu
T=aarch64-apple-darwin
curl -fL https://github.com/juniorinyang-spec/jobscout/releases/latest/download/jobscout-$T.tar.gz | tar xz
sudo mkdir -p /usr/local/bin && sudo mv jobscout-$T/jobscout /usr/local/bin/
jobscout --version
```

Linux binaries need glibc 2.35 or newer (Ubuntu 22.04, Debian 12, RHEL 9).

**With Rust** (<https://rustup.rs>):

```sh
cargo install --git https://github.com/juniorinyang-spec/jobscout --locked
```

The only runtime requirement is `curl`, which macOS ships with.

## Usage

```sh
jobscout init                 # writes ~/.jobscout/config.json
jobscout location set "United States"   # where you can work from
jobscout company add stripe   # track a company; its ATS is auto-detected
jobscout company add https://jobs.lever.co/spotify https://jobs.ashbyhq.com/linear
jobscout query add rust       # what to look for (repeat for more)
jobscout exclude add go to market  # drop postings containing a phrase
jobscout source on remoteok   # optionally add a public job board (off by default)

jobscout search rust backend  # one-off search, ignores memory
jobscout run                  # new postings only; writes a bundle; remembers them
jobscout run --dry-run        # preview without saving anything
jobscout watch --every 1h     # keep running
jobscout bundles              # list digests written so far
jobscout sources              # list sources and whether they are on
jobscout forget               # clear memory of seen postings
```

Useful flags: `--source NAME` (repeatable), `--remote`, `--limit N`, `--json`,
`--since DUR`.

```sh
jobscout search --since 24h                 # everything posted in the last day
jobscout search --since 24h -s companies    # only your tracked companies, last day
jobscout run --since 3d                     # new postings from the last 3 days
jobscout search --on friday                 # posted on the most recent Friday
jobscout search --on 2026-11-13 -n 500      # posted on that day (local time)
jobscout search --on yesterday -s companies # your tracked companies, yesterday
```

`--since` and `--on` apply to every source, tracked companies included, and
replace `max_age_days` for that call. Postings that carry no date are left out.
`--on` takes `YYYY-MM-DD`, `today`, `yesterday` or a weekday name (`friday`,
`fri`: the most recent one, today included), read in your Mac's time zone.
Over MCP the same filters are `posted_within_hours` and `posted_on`.

### Config

`~/.jobscout/config.json` (override the directory with `JOBSCOUT_HOME`):

```json
{
  "queries": ["rust", "backend engineer", "typescript react"],
  "exclude": ["staff", "principal"],
  "location": "US",
  "remote_only": false,
  "max_age_days": 14,
  "max_per_source": 500,
  "sources": {
    "remoteok": false,
    "remotive": false,
    "arbeitnow": false,
    "weworkremotely": false,
    "companies": true
  },
  "companies": [
    { "name": "stripe", "ats": "greenhouse", "slug": "stripe" },
    { "name": "Linear", "ats": "ashby", "slug": "linear" }
  ]
}
```

- `location` is the country you search from (`jobscout location set
  "United States"`, or a state/city inside it). Postings located clearly
  elsewhere are hidden: on-site/hybrid jobs in other countries, and remote jobs
  limited to other regions ("EMEA Only", "Remote - Europe"). Remote jobs open
  to your country ("USA Only", "Americas", "Worldwide") stay, and so do
  postings whose location is blank or unclear, so a real match is not lost.
  `--location Germany` or `--anywhere` overrides it for one search or run.
- Tracked companies are the main source. The public boards are off unless you
  turn them on with `jobscout source on <name>`.
- A posting matches if **any** query matches. A query matches when **all** of
  its words appear as whole words somewhere in the title, company, location,
  tags, salary or summary (`rust` does not match `trust`).
- `exclude` phrases drop a posting when present, matched as whole words
  (`intern` drops "Go intern" but not "internal tools"). Manage them with
  `jobscout exclude add|remove|list`. Short queries such as `go` also match
  "Go To Market" sales roles; `jobscout exclude add go to market` fixes that.
- `max_age_days` drops job-board postings older than that (0 = no limit).
  Roles from tracked companies are shown however long they have been open.
- `run --limit N` shows and remembers only the N newest; the summary line
  says how many were new in total (`24 new, 3 shown`).
- `companies` is easiest to manage with `jobscout company add`, which accepts
  a company name or a careers URL.
  - **By name** (`stripe`, `public storage`): jobscout probes Greenhouse,
    Lever, Ashby, Workable and SmartRecruiters for a matching board.
  - **By URL**: a board URL on one of those systems is recognised from its
    hostname. A URL on the company's own domain (`https://jobs.cvshealth.com/`)
    is fetched and fingerprinted: Phenom-powered careers sites and Workday
    boards (`<tenant>.wd<N>.myworkdayjobs.com/<site>`) are supported, and a
    page that merely links to a Workday board is followed to it.
  - Large catalogues (Phenom, Workday) are searched server-side with your
    queries rather than downloaded whole.
  - Sites behind bot protection (a Cloudflare "Just a moment" page, HTTP 403
    for non-browsers) cannot be read; `company add` says so.

### Use it from Claude (MCP)

`jobscout mcp` serves the same engine as an [MCP](https://modelcontextprotocol.io)
tool server over stdio, so Claude Desktop, Claude Code, Cursor and other MCP
clients can call it. No extra dependencies; the JSON-RPC layer is in-house.

Claude Code:

```sh
claude mcp add jobscout -- jobscout mcp
```

Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "jobscout": { "command": "/opt/homebrew/bin/jobscout", "args": ["mcp"] }
  }
}
```

Use the absolute path from `which jobscout`; GUI apps don't share your shell's
`PATH`. Then ask Claude things like *"anything new at Stripe or Linear that
mentions Rust?"* or *"start tracking Home Depot and Nike"*.

Tools exposed: `search_jobs`, `new_jobs`, `list_companies`, `add_company`,
`remove_company`, `list_queries`, `add_query`, `remove_query`,
`list_excludes`, `add_exclude`, `remove_exclude`, `set_location`,
`set_source`, `list_bundles`,
`get_bundle`, `get_config`. Bundles and the config are also exposed as MCP
resources (`jobscout://bundle/<name>`, `jobscout://config`).

### Bundles

Each `run` with new postings writes two files into `~/.jobscout/bundles/`:

- `<timestamp>.md`, grouped by board, ready to read or paste anywhere;
- `<timestamp>.json`, the same postings as structured data for other tools.

### Sources

Only `companies` is on by default; turn boards on with `jobscout source on NAME`.

| name             | what                                                     |
|------------------|----------------------------------------------------------|
| `remoteok`       | remoteok.com public JSON feed                            |
| `remotive`       | remotive.com API, searched per query                     |
| `arbeitnow`      | arbeitnow.com API (Europe-heavy, remote + on-site)       |
| `weworkremotely` | weworkremotely.com RSS                                   |
| `companies`      | companies you track, on Greenhouse, Lever, Ashby, Workable or SmartRecruiters |

Adding a board is one file in `src/sources/` implementing the `Source` trait
and one line in `sources::all()`.

## Design notes

- `src/json.rs`: recursive-descent JSON parser and pretty-printer.
- `src/html.rs`: entity decoding, tag stripping, RSS element extraction.
- `src/time.rs`: civil-date arithmetic, ISO-8601 and RFC 2822 parsing.
- `src/http.rs`: the one deliberate exception to "from scratch". A TLS stack
  is not something to hand-roll, so HTTPS is delegated to the system `curl`
  binary. It is a single function, easy to swap later.
- `src/store.rs`: memory of seen postings, written atomically.

## Releasing

1. Bump `version` in `Cargo.toml`, commit, then `git tag vX.Y.Z && git push --tags`.
2. CI creates the GitHub release and attaches binaries for four targets. Its
   `create-release` job log prints the `url` and `sha256` lines for the
   formula.
3. Paste those into `Formula/jobscout.rb` and push to `main`; `brew upgrade`
   picks it up.

## Development

```sh
cargo test
cargo run -- search rust --limit 10
JOBSCOUT_HOME=/tmp/js cargo run -- run --dry-run
```
