---
name: jobscout
description: Find, track and filter job postings at companies the user chooses with the jobscout MCP tools. Use when the user asks about job openings, new postings, jobs at specific companies, jobs posted on a date or within a time window, tracking or untracking companies, or setting their job-search location, keywords or exclusions.
---

# jobscout

The jobscout MCP tools (`search_jobs`, `new_jobs`, `add_company`, `set_location`
and the rest) read job postings straight from companies' own hiring systems,
so new roles show up before they reach LinkedIn or Indeed. The user's
settings persist between sessions in `~/.jobscout`.

## First use

Call `get_config`. If no companies are tracked or no location is set, ask the
user, then save their answers:

1. Where they can work from: `set_location` with a country, or a city or state
   inside it ("United States", "Austin, TX", "Germany"). Jobs located
   elsewhere are then hidden. Remote jobs open to that country stay, and so do
   jobs whose location is unclear.
2. Which companies to watch: `add_company` for each one, with a name
   ("stripe", "doordash", "tiktok") or a careers-page URL. If a name isn't
   found, ask for the careers URL, which finds more (Workday, Phenom and other
   careers sites).
3. What to look for: `add_query` per phrase. Every word of a query must appear
   as a whole word. Use `add_exclude` for noise; for example, the query "go"
   also matches "Go To Market" sales jobs, which "go to market" excludes.

## Answering questions

- "Anything new?": `new_jobs`. It returns only postings it hasn't shown before
  and remembers them. Use `dry_run: true` to preview without marking them seen.
- An ad-hoc question: `search_jobs`. It never changes what counts as new.
  - Date windows: `posted_within_hours` (24 = today, 168 = this week) or
    `posted_on` (`"YYYY-MM-DD"`, `"yesterday"`, `"friday"`: the most recent
    such day). Both cover tracked companies, and both leave out postings
    without a date.
  - Another country for one search: `location: "Canada"`; `"anywhere"` turns
    the location filter off.
  - `query` overrides the saved queries for one search.
- Present the results as a short list with title, company, location, date and
  link. Then add judgment the tools can't give: fit with the user's
  experience, seniority, whether "remote" really includes the user, and
  duplicates of one role listed in several cities.

## Limits to tell the user about

- Some careers sites can't be read: those that build their job list in the
  browser without structured data (Amazon, Google, Apple, Meta, Microsoft,
  Walmart), and those behind bot protection. `add_company` says so; don't
  retry them.
- Sites read through their schema.org data (Netflix, Marriott and others)
  fill in gradually: each call reads about 40 new job pages, so a large site
  needs a few calls before it is complete. Say so if results look thin.
- TikTok publishes no posting dates. Its dates come from the posting id, so
  they are close estimates.
- Links lead to the company's own job page. If one looks closed, the role was
  probably filled.

## If the tools are missing

The plugin starts the server with `${CLAUDE_PLUGIN_ROOT}/scripts/jobscout-mcp`,
which installs jobscout on first use and needs internet access. If the tools
don't appear, run that script once in a terminal to see the error. The
`jobscout` command line offers the same features (`jobscout help`).
