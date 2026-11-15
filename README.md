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

**One line, no sudo** (macOS or Linux). Downloads the right build for your
machine, checks its SHA-256 checksum, and installs into `/usr/local/bin` if
you can write there, otherwise `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/juniorinyang-spec/jobscout/main/install.sh | sh
```

Set `JOBSCOUT_INSTALL_DIR=/some/dir` to choose the directory, or
`JOBSCOUT_VERSION=v0.1.9` to pin a release. Run it again to upgrade.

**Homebrew** (macOS or Linux). The repo doubles as a tap; the formula builds
from source, so Homebrew installs Rust for the build:

```sh
brew tap juniorinyang-spec/jobscout https://github.com/juniorinyang-spec/jobscout
brew install juniorinyang-spec/jobscout/jobscout        # latest release
brew install --HEAD juniorinyang-spec/jobscout/jobscout # or straight from main
```

**Prebuilt binary by hand.** Each release on the
[Releases page](https://github.com/juniorinyang-spec/jobscout/releases) has
tarballs (with `.sha256` files) for macOS (Apple Silicon and Intel) and Linux
(x64 and arm64):

```sh
# pick one: aarch64-apple-darwin (Apple Silicon), x86_64-apple-darwin (Intel),
#           x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu
T=aarch64-apple-darwin
curl -fL https://github.com/juniorinyang-spec/jobscout/releases/latest/download/jobscout-$T.tar.gz | tar xz
sudo mkdir -p /usr/local/bin && sudo mv jobscout-$T/jobscout /usr/local/bin/
jobscout --version
```

Linux binaries need glibc 2.34 or newer (Ubuntu 22.04, Debian 12, RHEL 9 and
later).

**With Rust** (<https://rustup.rs>):

```sh
cargo install --git https://github.com/juniorinyang-spec/jobscout --locked
```

The only runtime requirement is `curl`, which macOS and most Linux
distributions ship with.

### Installing with an AI assistant (Claude Code and others)

If you are an AI assistant installing jobscout for a user, do this:

1. Run the one-line installer above. It needs no sudo and no password, so it
   works non-interactively. Do not use `sudo` yourself.
2. Check it: run the `jobscout --version` path the installer printed. If it
   says the directory is not on `PATH`, tell the user, and use the full path
   meanwhile.
3. Set the user up (ask them for these): `jobscout init`,
   `jobscout location set "<their country>"`, then
   `jobscout company add <company name or careers URL> ...` and
   `jobscout query add <words>`.
4. Try it: `jobscout search -n 5`.
5. To let Claude Code use it as a tool, register the full path once:
   `claude mcp add --scope user jobscout -- "$(command -v jobscout)" mcp`,
   and tell the user to start a new Claude Code session. For Claude Desktop,
   see [Use it from Claude](#use-it-from-claude-mcp). Claude on the web
   (claude.ai) cannot run local tools like this one.

Uninstall: delete the `jobscout` binary, and `~/.jobscout` for its data.

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
`--since DUR`, `--on DAY`, `--location PLACE`, `--anywhere`, `--quiet`.
`jobscout help` lists every command and flag.

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
`fri`: the most recent one, today included), read in your computer's time
zone.
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
  - **By name** (`stripe`, `public storage`, `tiktok`): jobscout probes
    Greenhouse, Lever, Ashby, Workable and SmartRecruiters, trying the usual
    spellings companies use when the plain name was taken (`doordashusa`,
    `hubspotjobs`, `kraken.com`), and keeps the board with the most open
    roles. If a name is not found, paste the careers page URL instead.
  - **By URL**: a board URL on one of those systems is recognised from its
    hostname. A URL on the company's own domain (`https://jobs.cvshealth.com/`)
    is fetched and fingerprinted: Phenom-powered careers sites and Workday
    boards (`<tenant>.wd<N>.myworkdayjobs.com/<site>`) are supported, and a
    page that merely links to a Workday board is followed to it.
  - Large catalogues (Phenom, Workday, TikTok) are searched server-side with
    your queries rather than downloaded whole. Phenom postings link to the
    company's own job page.
  - **Any other careers site that publishes schema.org `JobPosting` data**
    (the structured data Google for Jobs reads), for example
    `jobscout company add https://careers.marriott.com/`. jobscout finds job
    pages through the site's links and sitemaps (respecting robots.txt),
    reads each page's JobPosting once and caches it in `~/.jobscout/cache/`,
    and drops postings that leave the sitemap or pass their `validThrough`
    date. To stay polite it reads at most 120 new pages per company per run
    (newest first), 2 at a time, so a large site fills in over a few runs;
    through MCP it reads 40 per call to answer quickly. Sites that build their
    job list in the browser with no structured data (Amazon, Google, Apple,
    Meta, Microsoft) cannot be read this way.
  - **TikTok** (`jobscout company add tiktok`) runs its own careers site,
    which jobscout reads through its public search. TikTok gives no posting
    dates; jobscout reads the creation time built into each posting id.
  - Sites behind bot protection (a Cloudflare "Just a moment" page, HTTP 403
    for non-browsers) cannot be read; `company add` says so.

### Use it from Claude (MCP)

`jobscout mcp` serves the same engine as an [MCP](https://modelcontextprotocol.io)
tool server over stdio, so Claude Desktop, Claude Code, Cursor and other MCP
clients can call it. No extra dependencies; the JSON-RPC layer is in-house.

Claude Code (available in every project; start a new session afterwards):

```sh
claude mcp add --scope user jobscout -- "$(command -v jobscout)" mcp
```

Claude Desktop: add this to
`~/Library/Application Support/Claude/claude_desktop_config.json`, with the
path that `command -v jobscout` prints (`/usr/local/bin/jobscout`,
`~/.local/bin/jobscout` written out in full, or `/opt/homebrew/bin/jobscout`
for Homebrew), then quit and reopen the app:

```json
{
  "mcpServers": {
    "jobscout": { "command": "/usr/local/bin/jobscout", "args": ["mcp"] }
  }
}
```

Use an absolute path: GUI apps don't share your shell's `PATH`. Claude on the
web (claude.ai) cannot use local tools such as this one.

Then ask Claude things like *"anything new at Stripe or Linear that mentions
Rust?"*, *"start tracking Ramp and DoorDash"*, *"AI engineer jobs at my
companies posted this week"* or *"what was posted yesterday?"*.

Tools exposed: `search_jobs`, `new_jobs`, `list_companies`, `add_company`,
`remove_company`, `list_queries`, `add_query`, `remove_query`,
`list_excludes`, `add_exclude`, `remove_exclude`, `set_location`,
`set_source`, `list_bundles`,
`get_bundle`, `get_config`. Bundles and the config are also exposed as MCP
resources (`jobscout://bundle/<name>`, `jobscout://config`).

### Bundles

Each `run` with new postings writes two files into `~/.jobscout/bundles/`:

- `<timestamp>.md`, grouped by source, ready to read or paste anywhere;
- `<timestamp>.json`, the same postings as structured data for other tools.

### Sources

Only `companies` is on by default; turn boards on with `jobscout source on NAME`.

| name             | what                                                     |
|------------------|----------------------------------------------------------|
| `remoteok`       | remoteok.com public JSON feed                            |
| `remotive`       | remotive.com API, searched per query                     |
| `arbeitnow`      | arbeitnow.com API (Europe-heavy, remote + on-site)       |
| `weworkremotely` | weworkremotely.com RSS                                   |
| `companies`      | companies you track: on Greenhouse, Lever, Ashby, Workable, SmartRecruiters, Phenom or Workday, TikTok, or any careers site with schema.org JobPosting data |

Adding a board is one file in `src/sources/` implementing the `Source` trait
and one line in `sources::all()`.

## Design notes

- `src/json.rs`: recursive-descent JSON parser and pretty-printer.
- `src/html.rs`: entity decoding, tag stripping, RSS element extraction.
- `src/time.rs`: civil-date arithmetic, ISO-8601 and RFC 2822 parsing, local
  calendar days for `--on`.
- `src/filter.rs`: whole-word keyword, exclude, age and location filtering.
- `src/location.rs`: countries, cities, US states and regions in free-form
  location text, and whether a posting is open to the user's country.
- `src/sources/companies.rs`: the job-system adapters (Greenhouse, Lever,
  Ashby, Workable, SmartRecruiters, Phenom, Workday, TikTok) and detection.
- `src/sources/schema.rs`: the schema.org JobPosting reader (sitemaps,
  robots.txt, JSON-LD, page cache).
- `src/scout.rs`: the engine shared by the CLI (`src/main.rs`) and the MCP
  server (`src/mcp.rs`).
- `src/http.rs`: the one deliberate exception to "from scratch". A TLS stack
  is not something to hand-roll, so HTTPS is delegated to the system `curl`
  binary. It is a single function, easy to swap later.
- `src/store.rs`: memory of seen postings. It and the config are written
  atomically (temp file + rename).

## Releasing

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows,
   commit both, then `git tag -a vX.Y.Z -m "jobscout vX.Y.Z"` and push the
   branch and the tag.
2. CI creates the GitHub release and attaches binaries for four targets. Its
   `create-release` job log prints the `url` and `sha256` lines for the
   formula.
3. Paste those into `Formula/jobscout.rb` and push to `main`; `brew upgrade`
   picks it up.

## Development

```sh
cargo test
cargo run -- search rust --limit 10
JOBSCOUT_HOME=/tmp/js cargo run -- run --dry-run   # a throwaway data dir
```

No third-party crates: everything is built on the standard library, and HTTPS
goes through the system `curl`. Keep `[dependencies]` empty.
