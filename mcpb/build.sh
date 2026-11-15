#!/bin/sh
# Build jobscout.mcpb, the one-click Claude Desktop extension (macOS).
#
#   mcpb/build.sh <version> <aarch64-apple-darwin binary> <x86_64-apple-darwin binary> <out.mcpb>
#
# Combines both Mac builds into one universal binary with lipo, writes
# manifest.json (the tool list comes from the binary's own tools/list, so it
# cannot drift), and zips the bundle. An .mcpb is a zip with manifest.json at
# its root. Needs: lipo (macOS), python3, zip.
set -eu
[ $# -eq 4 ] || { echo "usage: $0 <version> <arm64-bin> <x86_64-bin> <out.mcpb>" >&2; exit 2; }
VERSION="${1#v}"; ARM="$2"; X86="$3"; OUT="$4"
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT

mkdir -p "$work/server"
lipo -create -output "$work/server/jobscout" "$ARM" "$X86"
chmod 755 "$work/server/jobscout"
lipo -info "$work/server/jobscout" >&2
cp "$here/../LICENSE" "$work/LICENSE"

# Ask the bundled binary for its tools.
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mcpb-build","version":"1"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | JOBSCOUT_HOME="$work/.home" "$work/server/jobscout" mcp 2>/dev/null > "$work/tools.jsonl"
rm -rf "$work/.home"

python3 -I - "$VERSION" "$work/tools.jsonl" "$work/manifest.json" <<'PY'
import json, sys
version, tools_path, out = sys.argv[1:4]
tools = []
for line in open(tools_path):
    msg = json.loads(line)
    if msg.get("id") == 2:
        tools = [{"name": t["name"], "description": t["description"].split(". ")[0].rstrip(".") + "."} for t in msg["result"]["tools"]]
assert tools, "binary returned no tools"
manifest = {
    "manifest_version": "0.3",
    "name": "jobscout",
    "display_name": "jobscout",
    "version": version,
    "description": "Find new job postings at the companies you choose, filtered by keywords, location and date.",
    "long_description": "jobscout reads job postings straight from companies' own hiring systems (Greenhouse, Lever, Ashby, Workable, SmartRecruiters, Workday, Phenom, TikTok, and any careers site that publishes schema.org job data), so new roles show up before they reach the big job boards. Ask Claude to track companies, set where you can work from, and show what was posted today, this week or on a given day. Settings and digests live in ~/.jobscout.",
    "author": {"name": "juniorinyang-spec", "url": "https://github.com/juniorinyang-spec"},
    "homepage": "https://github.com/juniorinyang-spec/jobscout",
    "documentation": "https://github.com/juniorinyang-spec/jobscout#readme",
    "support": "https://github.com/juniorinyang-spec/jobscout/issues",
    "repository": {"type": "git", "url": "https://github.com/juniorinyang-spec/jobscout"},
    "license": "MIT",
    "keywords": ["jobs", "careers", "job-search"],
    "server": {
        "type": "binary",
        "entry_point": "server/jobscout",
        "mcp_config": {"command": "${__dirname}/server/jobscout", "args": ["mcp"], "env": {}},
    },
    "tools": tools,
    "compatibility": {"platforms": ["darwin"]},
}
json.dump(manifest, open(out, "w"), indent=2)
print(f"manifest: jobscout {version}, {len(tools)} tools", file=sys.stderr)
PY
rm -f "$work/tools.jsonl"

mkdir -p "$(dirname "$OUT")"
OUT_ABS="$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"
rm -f "$OUT_ABS"
(cd "$work" && zip -q -X -r "$OUT_ABS" manifest.json server LICENSE)
echo "wrote $OUT_ABS" >&2
