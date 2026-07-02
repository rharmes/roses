//! OPML generation for `roses export` (TASK-38).
//!
//! A minimal, dependency-free writer. The *import* direction needs no parser at
//! all — Feedbin parses the uploaded OPML server-side — and export is a handful
//! of well-understood tags, so we hand-roll the XML (with proper attribute
//! escaping) rather than pull in an XML crate, keeping the dep tree lean and the
//! static musl build clean (the same reason `tui` hand-rolls HTML→text).

/// One feed to serialize as an OPML `<outline>`. `xml_url` (the feed URL) is
/// required for a valid outline; `text` is the display label (the caller falls
/// back to the URL when a subscription has no title); `html_url` (the site URL)
/// is optional.
pub struct OpmlFeed {
    pub text: String,
    pub xml_url: String,
    pub html_url: Option<String>,
}

/// Serialize `feeds` to an OPML 2.0 document with `title` in the head. Feeds are
/// written in the order given (the caller sorts them). Every attribute value and
/// the head title is XML-escaped, so hostile or punctuation-heavy titles/URLs
/// can't break the document.
pub fn to_opml(title: &str, feeds: &[OpmlFeed]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"2.0\">\n");
    out.push_str("  <head>\n");
    out.push_str(&format!("    <title>{}</title>\n", xml_escape(title)));
    out.push_str("  </head>\n");
    out.push_str("  <body>\n");
    for feed in feeds {
        out.push_str(&format!(
            "    <outline type=\"rss\" text=\"{}\" xmlUrl=\"{}\"",
            xml_escape(&feed.text),
            xml_escape(&feed.xml_url),
        ));
        if let Some(html_url) = &feed.html_url {
            out.push_str(&format!(" htmlUrl=\"{}\"", xml_escape(html_url)));
        }
        out.push_str("/>\n");
    }
    out.push_str("  </body>\n");
    out.push_str("</opml>\n");
    out
}

/// Escape the five XML predefined entities so a value is safe inside a
/// double-quoted attribute (and as element text).
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_opml_writes_a_golden_document_with_escaping() {
        let feeds = vec![
            OpmlFeed {
                text: "Rust Blog".to_string(),
                xml_url: "https://blog.rust-lang.org/feed.xml".to_string(),
                html_url: Some("https://blog.rust-lang.org".to_string()),
            },
            // No htmlUrl, and a title with characters that must be escaped
            // (ampersand, angle brackets, quote) to prove the writer is safe.
            OpmlFeed {
                text: "Tom & Jerry <\"news\">".to_string(),
                xml_url: "https://example.com/feed?a=1&b=2".to_string(),
                html_url: None,
            },
        ];
        let opml = to_opml("roses subscriptions", &feeds);
        let expected = "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<opml version=\"2.0\">
  <head>
    <title>roses subscriptions</title>
  </head>
  <body>
    <outline type=\"rss\" text=\"Rust Blog\" xmlUrl=\"https://blog.rust-lang.org/feed.xml\" htmlUrl=\"https://blog.rust-lang.org\"/>
    <outline type=\"rss\" text=\"Tom &amp; Jerry &lt;&quot;news&quot;&gt;\" xmlUrl=\"https://example.com/feed?a=1&amp;b=2\"/>
  </body>
</opml>
";
        assert_eq!(opml, expected);
    }

    #[test]
    fn to_opml_with_no_feeds_is_still_valid() {
        let opml = to_opml("empty", &[]);
        assert!(opml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(opml.contains("<title>empty</title>"));
        assert!(opml.contains("<body>\n  </body>"));
        assert!(opml.trim_end().ends_with("</opml>"));
    }
}
