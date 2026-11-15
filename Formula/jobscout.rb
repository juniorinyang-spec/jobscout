# Homebrew formula for jobscout. The repo itself is the tap:
#   brew tap juniorinyang-spec/jobscout https://github.com/juniorinyang-spec/jobscout
#   brew install juniorinyang-spec/jobscout/jobscout
#
# On each release, update `url` and `sha256` (the CI create-release job
# prints both). `brew install --HEAD ...` builds from the main branch.
class Jobscout < Formula
  desc "Zero-dependency job posting scout that bundles new listings into digests"
  homepage "https://github.com/juniorinyang-spec/jobscout"
  url "https://github.com/juniorinyang-spec/jobscout/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "159647b721c57884049ec07fb95dc952711e673680d6795701f5107bba971f7d"
  license "MIT"
  head "https://github.com/juniorinyang-spec/jobscout.git", branch: "main"

  depends_on "rust" => :build
  # HTTPS is delegated to curl; macOS ships one, Linuxbrew users get this.
  uses_from_macos "curl"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    # Not "jobscout #{version}": for --HEAD installs version is "HEAD-<sha>".
    assert_match(/^jobscout \d+\.\d+\.\d+/, shell_output("#{bin}/jobscout --version"))
    assert_match "remoteok", shell_output("#{bin}/jobscout sources")
  end
end
