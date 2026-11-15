# jobscout

A small command-line agent that watches job boards for new postings, filters
them against your keywords, and bundles the new ones into Markdown + JSON
digests. Written in Rust with **zero third-party crates**: the JSON parser,
HTML/RSS extraction, date handling, config and storage are all in this repo.

```
$ jobscout run
  remoteok        97 postings
  remotive        31 postings
  arbeitnow       472 postings
  weworkremotely  100 postings
  hackernews      244 postings
[hackernews] Senior Rust Engineer
    Acme · Remote (US) · remote · 2026-11-14
    https://news.ycombinator.com/item?id=...
...
bundle written: ~/.jobscout/bundles/2026-11-15_10-30-00.md
944 fetched, 23 matched, 23 new
```

## Install

With Homebrew, from your own tap (see *Publishing to Homebrew* below):

```sh
brew tap juniorinyang-spec/tap
brew install jobscout
```

From source (needs a Rust toolchain from <https://rustup.rs>):

```sh
cargo install --path .
```

The only runtime requirement is `curl`, which macOS ships with.

## Usage

```sh
jobscout init                 # writes ~/.jobscout/config.json
jobscout query add rust       # what to look for (repeat for more)
jobscout company add stripe   # track a company; its ATS is auto-detected
jobscout company add https://jobs.lever.co/mistral https://jobs.ashbyhq.com/linear

jobscout search rust backend  # one-off search, ignores memory
jobscout run                  # new postings only; writes a bundle; remembers them
jobscout run --dry-run        # preview without saving anything
jobscout watch --every 1h     # keep running
jobscout bundles              # list digests written so far
jobscout sources              # list boards and whether they are enabled
jobscout forget               # clear memory of seen postings
```

Useful flags: `--source NAME` (repeatable), `--remote`, `--limit N`, `--json`.

### Config

`~/.jobscout/config.json` (override the directory with `JOBSCOUT_HOME`):

```json
{
  "queries": ["rust", "backend engineer", "typescript react"],
  "exclude": ["staff", "principal"],
  "remote_only": false,
  "max_age_days": 14,
  "max_per_source": 500,
  "sources": {
    "remoteok": true,
    "remotive": true,
    "arbeitnow": true,
    "weworkremotely": true,
    "hackernews": true,
    "companies": true
  },
  "companies": [
    { "name": "stripe", "ats": "greenhouse", "slug": "stripe" },
    { "name": "Linear", "ats": "ashby", "slug": "linear" }
  ]
}
```

- A posting matches if **any** query matches. A query matches when **all** of
  its words appear as whole words somewhere in the title, company, location,
  tags, salary or summary (`rust` does not match `trust`).
- `exclude` phrases drop a posting when present.
- `companies` is easiest to manage with `jobscout company add`, which accepts
  a company name (it probes each supported ATS for a matching board) or the
  careers URL (exact: the hostname names the ATS). Supported systems:
  Greenhouse, Lever, Ashby, Workable and SmartRecruiters. Companies on a
  custom careers site without one of these are not supported yet.

### Bundles

Each `run` with new postings writes two files into `~/.jobscout/bundles/`:

- `<timestamp>.md`, grouped by board, ready to read or paste anywhere;
- `<timestamp>.json`, the same postings as structured data for other tools.

### Sources

| name             | what                                                     |
|------------------|----------------------------------------------------------|
| `remoteok`       | remoteok.com public JSON feed                            |
| `remotive`       | remotive.com API, searched per query                     |
| `arbeitnow`      | arbeitnow.com API (Europe-heavy, remote + on-site)       |
| `weworkremotely` | weworkremotely.com RSS                                   |
| `hackernews`     | the monthly "Ask HN: Who is hiring?" thread via Algolia  |
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

## Publishing to Homebrew

1. Push this repo to GitHub and tag a release: `git tag v0.1.0 && git push --tags`.
2. Create a repo named `homebrew-tap` and copy `Formula/jobscout.rb` into
   `Formula/` there.
3. Fill in `url` (the tag tarball) and `sha256`
   (`curl -sL <url> | shasum -a 256`); the CI workflow prints both on tag push.
4. `brew tap juniorinyang-spec/tap && brew install jobscout`.

Before any release exists, `brew install --HEAD juniorinyang-spec/tap/jobscout`
builds straight from `main`.

## Development

```sh
cargo test
cargo run -- search rust --limit 10
JOBSCOUT_HOME=/tmp/js cargo run -- run --dry-run
```
