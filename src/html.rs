//! Minimal HTML / XML text utilities: entity decoding, tag stripping, and
//! element extraction for RSS feeds. Not a full parser, but enough for feeds
//! and job descriptions.

/// Decode the common named entities plus numeric (`&#123;`, `&#x1F;`) ones.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..].find(';').filter(|&e| e > 0 && e <= 10);
        let replaced = end.and_then(|e| {
            let name = &rest[1..1 + e];
            let rep: Option<String> = match name {
                "amp" => Some("&".into()),
                "lt" => Some("<".into()),
                "gt" => Some(">".into()),
                "quot" => Some("\"".into()),
                "apos" => Some("'".into()),
                "nbsp" => Some(" ".into()),
                "mdash" => Some("—".into()),
                "ndash" => Some("–".into()),
                "hellip" => Some("…".into()),
                "rsquo" => Some("’".into()),
                "lsquo" => Some("‘".into()),
                "rdquo" => Some("”".into()),
                "ldquo" => Some("“".into()),
                "bull" => Some("•".into()),
                "copy" => Some("©".into()),
                "euro" => Some("€".into()),
                "pound" => Some("£".into()),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32).map(String::from)
                }
                _ if name.starts_with('#') => {
                    name[1..].parse::<u32>().ok().and_then(char::from_u32).map(String::from)
                }
                _ => None,
            };
            rep.map(|r| (r, e + 2))
        });
        match replaced {
            Some((r, skip)) => {
                out.push_str(&r);
                rest = &rest[skip..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Case-insensitive ASCII substring search.
fn find_ci(hay: &str, needle_lower: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(needle_lower)
}

/// Remove tags, dropping `<script>`/`<style>` bodies and turning block-level
/// tags into newlines.
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];

        // A tag starts with a letter, '/', '!' or '?'; anything else is a literal '<'.
        let is_tag = rest[1..].chars().next().map(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')).unwrap_or(false);
        if !is_tag || !rest.contains('>') {
            out.push('<');
            rest = &rest[1..];
            continue;
        }

        let head: String = rest.chars().take(8).collect::<String>().to_ascii_lowercase();
        if head.starts_with("<script") || head.starts_with("<style") {
            let close = if head.starts_with("<script") { "</script" } else { "</style" };
            match find_ci(rest, close) {
                Some(j) => rest = &rest[j..],
                None => return out,
            }
        }

        match rest.find('>') {
            Some(j) => {
                let tag = &rest[1..j];
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "ul" | "ol" | "blockquote" | "pre" | "section"
                ) {
                    out.push('\n');
                }
                rest = &rest[j + 1..];
            }
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Collapse all runs of whitespace (including newlines) into single spaces.
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// HTML -> one line of plain text. Some APIs ship HTML that is itself
/// entity-encoded (`&lt;p&gt;...`), so if decoding reveals tags, strip again.
pub fn text(s: &str) -> String {
    let once = decode_entities(&strip_tags(s));
    let plain = if looks_like_markup(&once) { decode_entities(&strip_tags(&once)) } else { once };
    collapse_ws(&plain)
}

fn looks_like_markup(s: &str) -> bool {
    let b = s.as_bytes();
    s.match_indices('<')
        .any(|(i, _)| matches!(b.get(i + 1), Some(c) if c.is_ascii_alphabetic() || *c == b'/') && s[i..].contains('>'))
        && (s.contains("</") || s.contains("/>") || s.contains("<p") || s.contains("<br"))
}

/// HTML -> plain text, preserving line breaks (used for HN comments).
pub fn text_lines(s: &str) -> String {
    decode_entities(&strip_tags(s))
        .lines()
        .map(|l| collapse_ws(l))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate to at most `max` chars, adding an ellipsis.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Inner text of every `<tag ...>...</tag>` element, in document order.
/// Does not handle nested elements of the same name (fine for RSS).
pub fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        // Must be a complete tag name: `<item>` or `<item attr=..>` but not `<items>`.
        let boundary = after.chars().next().map(|c| c == '>' || c == '/' || c.is_whitespace()).unwrap_or(false);
        if !boundary {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            // Self-closing element, no content.
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        found.push(&body[..end]);
        rest = &body[end + close.len()..];
    }
    found
}

/// First `<tag>` inside `xml`, CDATA unwrapped and entities decoded.
pub fn element(xml: &str, tag: &str) -> Option<String> {
    elements(xml, tag).first().map(|raw| decode_entities(&unwrap_cdata(raw)))
}

pub fn unwrap_cdata(s: &str) -> String {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")) {
        inner.to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt; &#39;q&#39; &#x41; &unknown; & x"), "a & b <c> 'q' A &unknown; & x");
    }

    #[test]
    fn strip() {
        let html = "<p>Hello <b>world</b></p><script>var x = '<p>';</script><br/>bye";
        assert_eq!(text(html), "Hello world bye");
    }

    #[test]
    fn double_encoded_html() {
        assert_eq!(text("&lt;p&gt;&amp;nbsp;&lt;/p&gt; &lt;p&gt;Hi &amp;amp; bye&lt;/p&gt;"), "Hi & bye");
        // Literal angle brackets in plain text survive.
        assert_eq!(text("a < b and c > d, 5 <= 6"), "a < b and c > d, 5 <= 6");
        assert_eq!(strip_tags("x <b>y</b> < z"), "x y < z");
    }

    #[test]
    fn rss_elements() {
        let xml = "<rss><item><title><![CDATA[A &amp; B]]></title></item><items>no</items><item attr=\"1\"><title>C</title></item></rss>";
        let items = elements(xml, "item");
        assert_eq!(items.len(), 2);
        assert_eq!(element(items[0], "title").as_deref(), Some("A & B"));
        assert_eq!(element(items[1], "title").as_deref(), Some("C"));
    }
}
