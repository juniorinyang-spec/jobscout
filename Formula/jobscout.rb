# Homebrew formula for jobscout.
#
# Publish it from a tap repository named `homebrew-tap` (this file at
# `Formula/jobscout.rb`), then users run:
#   brew tap juniorinyang-spec/tap
#   brew install jobscout
#
# For a tagged release: update `url`, then `sha256` with
#   curl -sL <url> | shasum -a 256
# Until then `brew install --HEAD jobscout` builds from the main branch.
class Jobscout < Formula
  desc "Zero-dependency job posting scout that bundles new listings into digests"
  homepage "https://github.com/juniorinyang-spec/jobscout"
  url "https://github.com/juniorinyang-spec/jobscout/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_TARBALL"
  license "MIT"
  head "https://github.com/juniorinyang-spec/jobscout.git", branch: "main"

  depends_on "rust" => :build
  # HTTPS is delegated to curl; macOS ships one, Linuxbrew users get this.
  uses_from_macos "curl"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "jobscout #{version}", shell_output("#{bin}/jobscout --version")
    assert_match "remoteok", shell_output("#{bin}/jobscout sources")
  end
end
