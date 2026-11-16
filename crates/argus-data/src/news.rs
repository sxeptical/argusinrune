//! Singapore RSS feeds (The Straits Times / CNA) with bounded, abortable
//! downloads. Ports `lib/api-clients/news.ts`; feeds fail independently and
//! deduplicated, Singapore-relevant items are sorted newest-first.

use std::time::Duration;

use argus_core::types::NewsItem;
use regex::Regex;

use crate::{
    Client, Result,
    error::Error,
    health::Source,
    time::{iso_from_millis, now_millis, parse_timestamp},
};

const RSS_TIMEOUT_MS: u64 = 10_000;
const MAX_RSS_BYTES: usize = 512 * 1024;
const NEWS_TTL: Duration = Duration::from_secs(15 * 60);

const FEEDS: &[(&str, &str)] = &[
    (
        "The Straits Times",
        "https://www.straitstimes.com/news/singapore/rss.xml",
    ),
    (
        "CNA",
        "https://www.channelnewsasia.com/api/v1/rss-outbound-feed?_format=xml",
    ),
];

pub type ParsedRssItem = NewsItemWithCategories;

#[derive(Clone, Debug, PartialEq)]
pub struct NewsItemWithCategories {
    pub title: String,
    pub source: String,
    pub url: String,
    pub published_at: String,
    pub categories: Vec<String>,
}

fn entity_replacer(caps: &regex::Captures<'_>) -> String {
    let entity = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        return u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| caps[0].to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        return dec
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| caps[0].to_string());
    }
    match entity.to_ascii_lowercase().as_str() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => "\u{a0}".into(),
        "rsquo" => "’".into(),
        "lsquo" => "‘".into(),
        "rdquo" => "”".into(),
        "ldquo" => "“".into(),
        "ndash" => "–".into(),
        "mdash" => "—".into(),
        "hellip" => "…".into(),
        _ => caps[0].to_string(),
    }
}

pub fn decode_html_entities(text: &str) -> String {
    let cdata = Regex::new(r"(?s)<!\[CDATA\[([\s\S]*?)\]\]>").unwrap();
    let entities = Regex::new(r"(?i)&(#\d+|#x[0-9a-f]+|[a-z]+);").unwrap();
    let unwrapped = cdata.replace_all(text, "$1");
    entities
        .replace_all(&unwrapped, entity_replacer)
        .into_owned()
}

fn strip_html_tags(text: &str) -> String {
    Regex::new(r"<[^>]*>")
        .unwrap()
        .replace_all(text, "")
        .trim()
        .to_string()
}

/// Read a tag, tolerating attributes and namespaces (`<dc:date>`, etc.).
pub fn extract_rss_tag(xml: &str, tag: &str) -> String {
    let escaped = regex::escape(tag);
    let pattern = format!(r"(?i)<(?:\w+:)?{escaped}(?:\s[^>]*)?>([\s\S]*?)</(?:\w+:)?{escaped}>");
    if let Some(caps) = Regex::new(&pattern).unwrap().captures(xml) {
        return strip_html_tags(&decode_html_entities(&caps[1]));
    }
    if tag.eq_ignore_ascii_case("link") {
        // Atom-style self-closed link: <link href="https://…" />.
        if let Some(caps) = Regex::new(r#"(?i)<link[^>]*href=["']([^"']+)["']"#)
            .unwrap()
            .captures(xml)
        {
            return decode_html_entities(&caps[1]).trim().to_string();
        }
    }
    String::new()
}

pub fn extract_rss_categories(xml: &str) -> Vec<String> {
    let re =
        Regex::new(r"(?is)<(?:\w+:)?category(?:\s[^>]*)?>([\s\S]*?)</(?:\w+:)?category>").unwrap();
    re.captures_iter(xml)
        .filter_map(|caps| {
            let value = strip_html_tags(&decode_html_entities(&caps[1]));
            if value.is_empty() { None } else { Some(value) }
        })
        .collect()
}

fn to_iso_date(value: &str) -> String {
    let ms = if value.is_empty() {
        now_millis()
    } else {
        parse_timestamp(value).unwrap_or_else(now_millis)
    };
    iso_from_millis(ms)
}

/// Parse both RSS `<item>` and Atom `<entry>`; drop items missing a title, a
/// safe http(s) link, or with an unsafe scheme.
pub fn parse_rss_items(xml: &str, source: &str) -> Vec<ParsedRssItem> {
    let re = Regex::new(r"(?is)<(?:item|entry)(?:\s[^>]*)?>([\s\S]*?)</(?:item|entry)>").unwrap();
    re.captures_iter(xml)
        .filter_map(|caps| {
            let item = &caps[1];
            let title = extract_rss_tag(item, "title");
            let link = {
                let l = extract_rss_tag(item, "link");
                if l.is_empty() {
                    extract_rss_tag(item, "guid")
                } else {
                    l
                }
            };
            let published = ["pubDate", "pubdate", "updated", "published", "date"]
                .iter()
                .map(|tag| extract_rss_tag(item, tag))
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            if title.is_empty() || link.is_empty() {
                return None;
            }
            let lower = link.to_ascii_lowercase();
            if !lower.starts_with("http://") && !lower.starts_with("https://") {
                return None;
            }
            Some(ParsedRssItem {
                title,
                source: source.to_string(),
                url: link,
                published_at: to_iso_date(&published),
                categories: extract_rss_categories(item),
            })
        })
        .collect()
}

/// CNA publishes a global feed — keep items with any Singapore signal.
pub fn is_singapore_relevant(item: &ParsedRssItem) -> bool {
    if item.source != "CNA" {
        return true;
    }
    item.url.to_ascii_lowercase().contains("singapore")
        || item.title.to_ascii_lowercase().contains("singapore")
        || item
            .categories
            .iter()
            .any(|c| c.to_ascii_lowercase().contains("singapore"))
}

impl Client {
    pub fn get_news(&self) -> Result<Vec<NewsItem>> {
        let result = self.cache.get("news", NEWS_TTL, || {
            let mut succeeded = 0usize;
            let mut items: Vec<ParsedRssItem> = Vec::new();
            for (source, url) in FEEDS {
                match self.http.get_text_limited(
                    "rss",
                    *url,
                    &[("Accept", "application/rss+xml, application/xml, text/xml")],
                    RSS_TIMEOUT_MS,
                    MAX_RSS_BYTES,
                ) {
                    Ok(xml) => {
                        succeeded += 1;
                        items.extend(parse_rss_items(&xml, source));
                    }
                    Err(_) => continue,
                }
            }
            if succeeded == 0 {
                return Err(Error::external("rss", 502, "All news feeds failed"));
            }
            let mut seen = std::collections::HashSet::new();
            items.retain(|item| is_singapore_relevant(item) && seen.insert(item.url.clone()));
            items.sort_by_key(|item| {
                std::cmp::Reverse(parse_timestamp(&item.published_at).unwrap_or(0))
            });
            Ok(items
                .into_iter()
                .take(20)
                .map(|item| NewsItem {
                    title: item.title,
                    source: item.source,
                    url: item.url,
                    published_at: item.published_at,
                })
                .collect())
        });
        self.finish(Source::News, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_named_decimal_and_hex_entities() {
        assert_eq!(decode_html_entities("a &amp; b"), "a & b");
        assert_eq!(decode_html_entities("&ldquo;hi&rdquo;"), "“hi”");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(
            decode_html_entities("<![CDATA[hello &amp; world]]>"),
            "hello & world"
        );
    }

    #[test]
    fn extracts_tags_attributes_namespaces_atom_href_and_categories() {
        assert_eq!(extract_rss_tag("<title>Hello</title>", "title"), "Hello");
        assert_eq!(
            extract_rss_tag("<title type=\"text\">Hi</title>", "title"),
            "Hi"
        );
        assert_eq!(
            extract_rss_tag("<dc:date>2026-01-01</dc:date>", "date"),
            "2026-01-01"
        );
        assert_eq!(
            extract_rss_tag("<link href=\"https://example.test/a\" />", "link"),
            "https://example.test/a"
        );
        assert_eq!(
            extract_rss_tag("<title><b>Bold</b> news</title>", "title"),
            "Bold news"
        );
        assert_eq!(
            extract_rss_categories(
                "<item><category>Singapore</category><category type=\"x\">Politics</category></item>"
            ),
            ["Singapore", "Politics"]
        );
    }

    #[test]
    fn parses_rss_items_and_filters_unsafe() {
        let xml = r#"
          <rss><channel>
            <item><title>A</title><link>https://example.test/a</link><pubDate>Mon, 01 Jan 2026 00:00:00 GMT</pubDate><category>Singapore</category></item>
            <item><title></title><link>https://example.test/b</link></item>
            <item><title>C</title><link>javascript:alert(1)</link></item>
          </channel></rss>"#;
        let items = parse_rss_items(xml, "Test");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "A");
        assert_eq!(items[0].categories, ["Singapore"]);
        assert!(items[0].published_at.starts_with("2026-01-01"));
    }

    #[test]
    fn singapore_relevance_rules() {
        let relevant = |title: &str, url: &str, source: &str, cats: &[&str]| ParsedRssItem {
            title: title.into(),
            url: url.into(),
            source: source.into(),
            published_at: String::new(),
            categories: cats.iter().map(|s| s.to_string()).collect(),
        };
        assert!(is_singapore_relevant(&relevant(
            "Budget debate",
            "https://example.test/world/123",
            "CNA",
            &["Singapore"]
        )));
        assert!(!is_singapore_relevant(&relevant(
            "Global markets",
            "https://example.test/world/123",
            "CNA",
            &["World"]
        )));
        assert!(is_singapore_relevant(&relevant(
            "Anything",
            "https://example.test/x",
            "The Straits Times",
            &[]
        )));
    }
}
