//! HTTP fetching.
//!
//! Writing a TLS stack from scratch is the one place where "no libraries"
//! stops being sensible, so HTTPS is delegated to the system `curl` binary
//! (shipped with every macOS and most Linux installs; Homebrew can declare it
//! as a dependency). Everything above this function is our own code, and this
//! is the single seam to swap for a different transport later.

use std::process::Command;

const USER_AGENT: &str = "jobscout/0.1 (+https://github.com/juniorinyang-spec/jobscout)";

pub fn curl_available() -> bool {
    Command::new("curl").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// GET `url` and return the response body as text.
pub fn get(url: &str) -> Result<String, String> {
    let out = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--fail",
            "--compressed",
            "--max-time",
            "45",
            "--retry",
            "1",
            "--user-agent",
            USER_AGENT,
            "--header",
            "Accept: application/json, application/rss+xml, application/xml, text/html;q=0.9, */*;q=0.8",
            "--",
            url,
        ])
        .output()
        .map_err(|e| format!("could not run curl: {}", e))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() { format!("curl exited with {}", out.status) } else { err };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Percent-encode a query-string value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes() {
        assert_eq!(super::url_encode("rust dev/é"), "rust+dev%2F%C3%A9");
    }
}
