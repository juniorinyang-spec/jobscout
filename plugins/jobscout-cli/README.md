![jobscout](.claude-plugin/icon.png)

# jobscout

Find new job postings at the companies you choose, straight from their own
hiring systems, so new roles show up before they reach the big job boards.
Ask Claude to track companies, set where you can work from, and show what was
posted today, this week, or on a given day. jobscout reads Greenhouse, Lever,
Ashby, Workable, SmartRecruiters, Workday, Phenom, TikTok, and any careers
site that publishes schema.org job data.

Try: *"set up jobscout for me"*, *"track Stripe, Ramp and DoorDash"*, *"AI
engineer jobs at my companies posted this week"*, *"what's new since last
time?"*.

## How it works

This plugin is a single skill. It contains no scripts and starts no programs
by itself. The skill teaches Claude to use the open-source
[jobscout](https://github.com/juniorinyang-spec/jobscout) command line
through the terminal, so it needs a terminal: Claude Code, or Cowork running
on your computer.

## What it runs, downloads and connects to

- **Installing jobscout**: if jobscout isn't installed, Claude asks you
  first. With your OK it downloads the project's `install.sh` from GitHub,
  which fetches the jobscout release for your machine from the project's
  GitHub Releases, checks its SHA-256 checksum, and installs it without sudo
  into `/usr/local/bin` or `~/.local/bin`. jobscout is MIT-licensed Rust with
  no third-party dependencies, built in public by GitHub Actions.
- **Searching**: jobscout makes HTTPS requests to the job systems and careers
  sites of the companies you track (for example `boards-api.greenhouse.io` or
  a company's careers domain) and, only if you turn them on, to public job
  boards. For careers sites read through schema.org data, it follows the
  site's robots.txt and reads slowly.
- **Storage**: your settings, the postings it has already shown you, digests
  and a page cache, all in `~/.jobscout` on your computer.
- **Never**: it sends nothing about you anywhere. No account, no telemetry,
  and no credentials needed.

To uninstall jobscout itself, delete the `jobscout` binary, and
`~/.jobscout` for its data.

MIT licensed. Project and docs: <https://github.com/juniorinyang-spec/jobscout>.
