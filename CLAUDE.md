# jobscout: notes for Claude

jobscout is a zero-dependency Rust CLI and MCP server that finds new job
postings at companies the user tracks, filters them by keywords, location and
date, and writes Markdown/JSON digests. User docs are in README.md.

## Installing it for a user

Follow "Installing with an AI assistant" in README.md. In short:

```sh
curl -fsSL https://raw.githubusercontent.com/juniorinyang-spec/jobscout/main/install.sh | sh
jobscout --version          # or the full path the installer printed
claude mcp add --scope user jobscout -- "$(command -v jobscout)" mcp
```

Never use `sudo`; the installer falls back to `~/.local/bin`. Claude on the
web (claude.ai) cannot use this local MCP server; Claude Code and Claude
Desktop can.

## Distribution

- `install.sh`: one-line installer (no sudo).
- `.claude-plugin/marketplace.json` + `plugins/jobscout/`: the Claude Code
  plugin (MCP server via `scripts/jobscout-mcp`, which installs the pinned
  release into `${CLAUDE_PLUGIN_DATA}`, plus the `jobscout` skill). Check it
  with `claude plugin validate ./plugins/jobscout` and `claude plugin
  validate .`.
- `plugins/jobscout-cli/`: a skill-only plugin (no scripts, no server) for
  Anthropic's directory, which allows local MCP servers only for Verified
  organizations. The skill drives the `jobscout` CLI through Bash.
- `mcpb/build.sh`: builds the Claude Desktop extension.
- `Formula/jobscout.rb`: Homebrew (this repo is the tap).

## Working on the code

- **No third-party crates.** `[dependencies]` stays empty; JSON, HTML, dates
  and config are hand-written in `src/`. HTTPS goes through the system `curl`
  (`src/http.rs`), the single deliberate exception.
- Build and test: `cargo build --release`, `cargo test`.
- Never touch the user's real `~/.jobscout` while testing: set
  `JOBSCOUT_HOME` to a scratch directory for every command.
- Layout: `src/main.rs` (CLI), `src/mcp.rs` (MCP, JSON-RPC over stdio),
  `src/scout.rs` (engine shared by both), `src/filter.rs` and
  `src/location.rs` (matching), `src/sources/` (boards; `companies.rs` holds
  the job-system adapters, `schema.rs` the schema.org JobPosting reader).
- Nothing in `scout.rs` or the sources may print to stdout: MCP uses stdout
  for protocol messages. Progress goes to stderr.
- Live sites: be polite. The schema.org reader is deliberately slow (2
  requests at a time, a pause, 120 new pages per company per run) after a
  faster version got rate-limited; keep it that way.
- When adding a job system, check links by opening a sample of job URLs; a
  200 response alone proves little (Workday pages can show "doesn't exist").

## Releasing

1. Bump the version in four places: `Cargo.toml` (then `cargo build` to
   update `Cargo.lock`), `plugins/jobscout/.claude-plugin/plugin.json`,
   `plugins/jobscout-cli/.claude-plugin/plugin.json`, and `VERSION=` in
   `plugins/jobscout/scripts/jobscout-mcp`. CI fails if they
   disagree, or if `plugins/jobscout/scripts/install.sh` differs from
   `install.sh` (copy it after editing). Commit, `git tag -a vX.Y.Z -m
   "jobscout vX.Y.Z"`, push branch and tag.
2. CI (`.github/workflows/ci.yml`) tests on macOS and Linux, then builds
   `jobscout-<target>.tar.gz` (+ `.sha256`) for four targets and attaches
   them to the GitHub release, then `mcpb/build.sh` combines the two macOS
   builds into `jobscout.mcpb` (the Claude Desktop extension) and attaches
   it. Asset names carry no version, so `releases/latest/download/...` links
   keep working.
3. Update `Formula/jobscout.rb` `url` and `sha256` to the new tag's source
   tarball (`curl -sL <url> | shasum -a 256`) and push.
