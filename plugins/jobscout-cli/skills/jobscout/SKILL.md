---
name: jobscout
description: Find, track and filter job postings at companies the user chooses by running the open-source jobscout command line in the terminal. Use when the user asks about job openings, new postings, jobs at specific companies, jobs posted on a date or within a time window, tracking or untracking companies, or setting their job-search location, keywords or exclusions.
---

# jobscout (command line)

jobscout reads job postings straight from companies' own hiring systems
(Greenhouse, Lever, Ashby, Workable, SmartRecruiters, Workday, Phenom,
TikTok, and careers sites that publish schema.org job data), so new roles
show up before they reach LinkedIn or Indeed. You run it with the Bash tool.
The user's settings persist in `~/.jobscout`.

This skill needs a terminal: Claude Code, or Cowork running on the user's
computer. In chat without a terminal, say that jobscout needs one, and point
to <https://github.com/juniorinyang-spec/jobscout>.

## 1. Make sure jobscout is installed

Run `command -v jobscout || ls ~/.local/bin/jobscout`. If you find it, run
`jobscout --version` and continue.

If it isn't installed, **ask the user before installing anything.** Tell them
it is an open-source program (MIT license, no dependencies), installed without
sudo into `/usr/local/bin` if writable, otherwise `~/.local/bin`. The
installer downloads the release for their machine from the project's GitHub
Releases and checks its SHA-256 checksum. With their OK, download the
installer to a file and read it before running it:

```sh
curl -fsSL -o /tmp/jobscout-install.sh https://raw.githubusercontent.com/juniorinyang-spec/jobscout/main/install.sh
sh /tmp/jobscout-install.sh
```

Never use `sudo`. If the installer reports that its directory isn't on
`PATH`, use the full path it printed (for example `~/.local/bin/jobscout`)
and tell the user how to add it.

## 2. Set the user up (first use)

Run `jobscout config`. If no companies are tracked or the location is
"anywhere", ask the user, then run:

- `jobscout location set "<country, or a state or city in it>"`. Jobs located
  elsewhere are then hidden. Remote jobs open to that country stay, and so do
  jobs whose location is unclear.
- `jobscout company add <name or careers URL> ...`. Names work for most
  startups and tech companies ("stripe", "doordash", "tiktok"). For a large
  company, ask for its careers page URL if the name isn't found.
- `jobscout query add <words>`, once per phrase. Every word must appear as a
  whole word. Add `jobscout exclude add <phrase>` for noise; for example,
  "go" also matches "Go To Market" sales jobs, which `exclude add go to
  market` removes.

## 3. Answer questions

Use `--json` when you need to read the results, and `-q` to drop progress
messages.

| The user asks | Run |
|---|---|
| "Anything new?" | `jobscout run -q --json`: only postings not shown before; it remembers them and writes a digest. Add `--dry-run` to preview without marking them seen |
| A search | `jobscout search -q --json [words]`: never changes what counts as new |
| Posted recently | add `--since 24h` (or `3d`, `7d`) |
| Posted on a day | add `--on yesterday`, `--on friday` (the most recent one) or `--on 2026-11-13` |
| Only tracked companies | add `-s companies` |
| Another country, once | add `--location Canada`, or `--anywhere` |
| More results | add `-n 200` (the default shows 50) |

`jobscout help` lists every command. Show results as a short list with title,
company, location, date and link. Then add judgment: fit with the user's
experience, seniority, whether "remote" includes them, and the same role
listed in several cities.

## Limits to mention

- Some careers sites can't be read: those that build their job list in the
  browser without structured data (Amazon, Google, Apple, Meta, Microsoft,
  Walmart), and those behind bot protection. `company add` says so; don't
  retry them.
- Sites read through schema.org data (Netflix, Marriott and others) fill in
  over a few runs: about 120 new job pages per company per run, read politely.
  The first runs on such a site take about a minute.
- TikTok publishes no dates. Its dates come from the posting id, so they are
  close estimates.
- Links lead to the company's own job page. If one looks closed, the role was
  probably filled.
