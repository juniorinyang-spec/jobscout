# jobscout plugin

Find new job postings at the companies you choose, straight from their own
hiring systems, so new roles show up before they reach the big job boards.
Ask Claude to track companies, set where you can work from, and show what was
posted today, this week, or on a given day. jobscout reads Greenhouse, Lever,
Ashby, Workable, SmartRecruiters, Workday, Phenom, TikTok, and any careers
site that publishes schema.org job data.

## What you get

- **MCP tools**: `search_jobs`, `new_jobs`, `add_company`, `remove_company`,
  `list_companies`, `add_query`, `remove_query`, `list_queries`,
  `add_exclude`, `remove_exclude`, `list_excludes`, `set_location`,
  `set_source`, `list_bundles`, `get_bundle`, `get_config`.
- **A skill** that teaches Claude the setup steps, the date and location
  filters, and the limits to mention.

Try: *"set up jobscout for me"*, *"track Stripe, Ramp and DoorDash"*, *"AI
engineer jobs at my companies posted this week"*, *"what's new since last
time?"*.

## What it runs, downloads and connects to

- **On first start**, `scripts/jobscout-mcp` installs the jobscout command
  line program, version-pinned to this plugin, into the plugin's data
  directory (`~/.claude/plugins/data/...`). It uses the bundled
  `scripts/install.sh`, which downloads the release tarball for your machine
  from this project's GitHub Releases
  (`github.com/juniorinyang-spec/jobscout/releases`) and checks its SHA-256
  checksum before installing. The program is built in public by GitHub
  Actions from the open-source Rust code in this repository, with no
  third-party dependencies.
- **When you use the tools**, jobscout makes HTTPS requests to the job systems
  and careers sites of the companies you track (for example
  `boards-api.greenhouse.io` or a company's careers domain) and, only if you
  turn them on, to public job boards (RemoteOK, Remotive, Arbeitnow, We Work
  Remotely). For careers sites read through schema.org data, it follows the
  site's robots.txt and reads slowly.
- **What it stores**: your settings, the postings it has already shown you,
  digests and a page cache, all in `~/.jobscout` on your computer.
- **What it never does**: it sends nothing about you anywhere. No account, no
  telemetry, and no credentials needed.

Requirements: macOS or Linux, `curl`, and internet access. Chat on claude.ai
can't run local tools like this one; Claude Code can, and so can Cowork when
the session runs on your computer.

Project, docs and command-line use: <https://github.com/juniorinyang-spec/jobscout>.
MIT licensed.
