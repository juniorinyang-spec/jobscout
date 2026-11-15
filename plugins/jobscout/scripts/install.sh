#!/bin/sh
# Install the latest jobscout release. No sudo, no toolchain.
#
#   curl -fsSL https://raw.githubusercontent.com/juniorinyang-spec/jobscout/main/install.sh | sh
#
# Picks the right build for this machine (macOS or Linux, Apple Silicon/arm64
# or Intel/x64), verifies its SHA-256 checksum, and installs it into the first
# writable directory of: $JOBSCOUT_INSTALL_DIR, /usr/local/bin, ~/.local/bin.
# Set JOBSCOUT_VERSION=v0.1.10 to install a specific release.
set -eu

REPO="juniorinyang-spec/jobscout"
VERSION="${JOBSCOUT_VERSION:-latest}"

say() { printf '%s\n' "$*"; }
die() { printf 'jobscout install: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Darwin) os_part="apple-darwin" ;;
  Linux)  os_part="unknown-linux-gnu" ;;
  *) die "unsupported OS '$os' (macOS and Linux only); build from source with cargo" ;;
esac
case "$arch" in
  arm64|aarch64) arch_part="aarch64" ;;
  x86_64|amd64)  arch_part="x86_64" ;;
  *) die "unsupported CPU '$arch'; build from source with cargo" ;;
esac
# An Intel shell under Rosetta on Apple Silicon still gets the native build.
if [ "$os" = "Darwin" ] && [ "$arch_part" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
  arch_part="aarch64"
fi
target="${arch_part}-${os_part}"

if [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi
name="jobscout-$target"

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t jobscout)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $name ($VERSION)..."
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz" || die "download failed: $base/$name.tar.gz"
curl -fsSL "$base/$name.tar.gz.sha256" -o "$tmp/$name.tar.gz.sha256" || die "checksum download failed"

want=$(cut -d' ' -f1 "$tmp/$name.tar.gz.sha256")
if command -v shasum >/dev/null 2>&1; then
  got=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
elif command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
else
  die "need shasum or sha256sum to verify the download"
fi
[ "$want" = "$got" ] || die "checksum mismatch (expected $want, got $got)"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
[ -f "$tmp/$name/jobscout" ] || die "archive did not contain jobscout"

dest=""
for d in "${JOBSCOUT_INSTALL_DIR:-}" /usr/local/bin "$HOME/.local/bin"; do
  [ -n "$d" ] || continue
  if [ -d "$d" ] && [ -w "$d" ]; then dest="$d"; break; fi
  if [ "$d" = "$HOME/.local/bin" ] || [ "$d" = "${JOBSCOUT_INSTALL_DIR:-}" ]; then
    mkdir -p "$d" 2>/dev/null && [ -w "$d" ] && { dest="$d"; break; }
  fi
done
[ -n "$dest" ] || die "no writable install directory; set JOBSCOUT_INSTALL_DIR"

install -m 755 "$tmp/$name/jobscout" "$dest/jobscout" 2>/dev/null || { cp "$tmp/$name/jobscout" "$dest/jobscout" && chmod 755 "$dest/jobscout"; }
say "installed $("$dest/jobscout" --version) to $dest/jobscout"

case ":$PATH:" in
  *":$dest:"*) ;;
  *)
    say ""
    say "$dest is not on your PATH. Add it, then open a new terminal:"
    say "  echo 'export PATH=\"$dest:\$PATH\"' >> ~/.zshrc    # zsh (macOS default)"
    say "  echo 'export PATH=\"$dest:\$PATH\"' >> ~/.bashrc   # bash"
    ;;
esac

say ""
say "next steps:"
say "  jobscout init"
say "  jobscout location set \"United States\""
say "  jobscout company add stripe"
say "  jobscout run"
say "use it from Claude Code (tools + skill):  claude plugin install jobscout --marketplace juniorinyang-spec/jobscout"
say "  or register this binary directly:     claude mcp add --scope user jobscout -- \"$dest/jobscout\" mcp"
