//! Sneak Peek scraper: detects new "Chief Concierge: Sneak Peek" articles on
//! whiteoutsurvival.wiki and builds Discord-ready content for them.
//!
//! Deliberately defensive: a theme change or a WAF stub degrades gracefully
//! instead of panicking, so the poller keeps running.

use anyhow::{anyhow, Result};

const ARCHIVE_URL: &str = "https://www.whiteoutsurvival.wiki/sneak-peek/";
const RSS_URL: &str = "https://www.whiteoutsurvival.wiki/sneak-peek/feed/";
const NEWS_FEED_URL: &str = "https://www.whiteoutsurvival.wiki/category/news-en/feed/";
const BASE_URL: &str = "https://www.whiteoutsurvival.wiki";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const FETCH_TIMEOUT_SECS: u64 = 15;

/// Matches the English-language article path, e.g.
/// `https://www.whiteoutsurvival.wiki/sneak-peek/080366853en/`
fn is_article_url(url: &str) -> bool {
    let url = url.trim();
    let prefix = format!("{BASE_URL}/sneak-peek/");
    if !url.starts_with(&prefix) {
        return false;
    }
    let rest = url[prefix.len()..].trim_end_matches('/');
    let last = rest.rsplit('/').next().unwrap_or("");
    // Byte-index slicing could panic on non-ASCII input; strip_suffix cannot.
    match last.to_ascii_lowercase().strip_suffix("en") {
        Some(stem) => !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

async fn fetch(url: &str) -> Result<String> {
    let bust = format!(
        "{}_cb={}",
        if url.contains('?') { "&" } else { "?" },
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let response = crate::net::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(10))
        .gzip(true)
        .brotli(true)
        .build()
        .map_err(|e| anyhow!("building fetch client: {e}"))?
        .get(format!("{url}{bust}"))
        .header(
            "accept",
            "text/html,application/xhtml+xml,application/xml,*/*;q=0.8",
        )
        .header("accept-language", "en-US,en;q=0.9")
        .header("referer", format!("{BASE_URL}/"))
        .send()
        .await
        .map_err(|e| anyhow!("fetching {url}: {e}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP {} fetching {url}",
            response.status().as_u16()
        ));
    }
    let body = response
        .text()
        .await
        .map_err(|e| anyhow!("reading body from {url}: {e}"))?;
    if body.trim().len() < 200 {
        return Err(anyhow!(
            "suspiciously short body ({} bytes) from {url}",
            body.len()
        ));
    }
    Ok(body)
}

/// Latest article URL from RSS (preferred), HTML archive, or news feed.
/// Returns `Ok(None)` when nothing usable is found.
pub async fn fetch_latest_url() -> Result<Option<String>> {
    if let Some(url) = latest_from_rss(RSS_URL).await? {
        return Ok(Some(url));
    }
    if let Some(url) = latest_from_html(ARCHIVE_URL).await? {
        return Ok(Some(url));
    }
    if let Some(url) = latest_from_rss(NEWS_FEED_URL).await? {
        return Ok(Some(url));
    }
    Ok(None)
}

async fn latest_from_rss(url: &str) -> Result<Option<String>> {
    let body = fetch(url).await?;
    for item in body.split("<item>").skip(1) {
        if let Some(link) = extract_tag(item, "link") {
            let link = link.trim();
            if is_article_url(link) {
                return Ok(Some(link.to_string()));
            }
        }
    }
    Ok(None)
}

async fn latest_from_html(url: &str) -> Result<Option<String>> {
    let body = fetch(url).await?;
    let mut cursor = 0;
    while let Some(start) = body[cursor..].find("href=\"") {
        let absolute = cursor + start + 6;
        let end = match body[absolute..].find('"') {
            Some(offset) => absolute + offset,
            None => break,
        };
        let href = &body[absolute..end];
        let href = if href.starts_with('/') {
            format!("{BASE_URL}{href}")
        } else {
            href.to_string()
        };
        if is_article_url(&href) {
            return Ok(Some(href));
        }
        cursor = end;
    }
    Ok(None)
}

fn extract_tag(item: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = item.find(&open)?;
    let content = &item[start + open.len()..];
    let end = content.find(&close)?;
    Some(decode_entities(content[..end].trim()))
}

fn decode_entities(text: &str) -> String {
    fn decode_one(entity: &str) -> String {
        match entity {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" | "#039" => "'".to_string(),
            "nbsp" => " ".to_string(),
            _ => {
                if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| format!("&{entity};"))
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| format!("&{entity};"))
                } else {
                    format!("&{entity};")
                }
            }
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find(';') {
            Some(rel_end) if rel_end <= 12 => {
                out.push_str(&decode_one(&after[1..rel_end]));
                rest = &after[rel_end + 1..];
            }
            _ => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One sneak-peek article, ready to render as an embed.
#[derive(Debug, Clone, Default)]
pub struct SneakPeekPost {
    pub title: String,
    pub url: String,
    pub text: String,
    pub hero_image: Option<String>,
    pub gift_codes: Vec<GiftCodeEntry>,
}

/// Embed acknowledged with a Translate button — kept minimal for translation.
pub(crate) fn stored_embed(post: &SneakPeekPost) -> Option<String> {
    let mut fields = Vec::new();
    if !post.gift_codes.is_empty() {
        let lines = post
            .gift_codes
            .iter()
            .map(|code| {
                let mut line = format!("`{}`", code.code);
                if let Some(req) = &code.requirement {
                    line += &format!(" — {req}");
                }
                if let Some(until) = &code.valid_until {
                    line += &format!("\n_Valid until {until}_");
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        fields.push(crate::translate::StoredField {
            name: "🎁 Gift Code(s)".to_string(),
            value: lines,
            inline: false,
        });
    }

    let stored = crate::translate::StoredEmbed {
        title: Some(format!("❄️ {}", post.title)),
        description: Some(format!(
            "{}\n\n[Read the full sneak peek]({})",
            post.text.chars().take(3800).collect::<String>(),
            post.url
        )),
        fields,
        footer: None,
        color: Some(crate::ui::theme::colors::BRAND),
    };
    serde_json::to_string(&stored).ok()
}

/// Discord embed for a sneak-peek article — used by the poller, the "Send
/// Link" flow and tests.
pub fn embed(post: &SneakPeekPost) -> poise::serenity_prelude::CreateEmbed {
    let mut embed = poise::serenity_prelude::CreateEmbed::new()
        .title(format!("❄️ {}", post.title))
        .url(post.url.clone())
        .description(format!(
            "{}\n\n[Read the full sneak peek]({})",
            post.text.chars().take(3800).collect::<String>(),
            post.url
        ))
        .color(crate::ui::theme::colors::BRAND);

    if let Some(hero) = &post.hero_image {
        embed = embed.image(hero);
    }
    if !post.gift_codes.is_empty() {
        let lines = post
            .gift_codes
            .iter()
            .map(|code| {
                let mut line = format!("`{}`", code.code);
                if let Some(req) = &code.requirement {
                    line += &format!(" — {req}");
                }
                if let Some(until) = &code.valid_until {
                    line += &format!("\n_Valid until {until}_");
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        embed = embed.field("🎁 Gift Code(s)", lines, false);
    }
    embed
}

#[derive(Debug, Clone)]
pub struct GiftCodeEntry {
    pub code: String,
    pub valid_until: Option<String>,
    pub requirement: Option<String>,
    pub exclusive: bool,
}

/// Parse one article page. All parsing is defensive: fields are optional.
pub async fn fetch_post(url: &str) -> Result<SneakPeekPost> {
    let body = fetch(url).await?;

    let title = match extract_tag(&body, "title") {
        Some(title) => title
            .split('|')
            .map(str::trim)
            .next()
            .unwrap_or("Sneak Peek")
            .to_string(),
        None => "Sneak Peek".to_string(),
    };

    let text = strip_html(&body)
        .split("\n\n")
        .find(|chunk| chunk.trim().len() > 80)
        .map(|chunk| chunk.trim().to_string())
        .unwrap_or_else(|| "A new sneak peek is up on the Whiteout Survival Wiki.".to_string());

    let hero_image = body
        .find("<meta property=\"og:image\" content=\"")
        .map(|start_rel| start_rel + "<meta property=\"og:image\" content=\"".len())
        .and_then(|start| {
            body[start..]
                .find('"')
                .map(|end| body[start..start + end].to_string())
        });

    let gift_codes = extract_gift_codes(&body);

    Ok(SneakPeekPost {
        title,
        url: url.to_string(),
        text,
        hero_image,
        gift_codes,
    })
}

fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut in_tag = false;
    while i < bytes.len() {
        if !in_tag && bytes[i] == b'<' {
            let rest = &html[i..];
            if rest.starts_with("<script") {
                if let Some(end) = rest.find("</script>") {
                    i += end + "</script>".len();
                    continue;
                }
            }
            if rest.starts_with("<style") {
                if let Some(end) = rest.find("</style>") {
                    i += end + "</style>".len();
                    continue;
                }
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if bytes[i] == b'>' {
                in_tag = false;
            }
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut pending_newline = false;
    for ch in out.chars() {
        if ch == '\n' {
            pending_newline = true;
        } else if ch.is_ascii_whitespace() {
            if !pending_newline && !collapsed.ends_with(' ') {
                collapsed.push(' ');
            }
        } else {
            collapsed.push(ch);
            pending_newline = false;
        }
    }
    collapsed
}

fn extract_gift_codes(html: &str) -> Vec<GiftCodeEntry> {
    let lower = html.to_ascii_lowercase();
    let mut codes: Vec<GiftCodeEntry> = Vec::new();

    for section in lower.split("gift code").skip(1) {
        let window = &section[..section.len().min(2000)];
        let mut seen = std::collections::HashSet::new();
        for token in window
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| {
                let len = token.len();
                (4..=32).contains(&len)
                    && token.chars().any(|c| c.is_ascii_alphabetic())
                    && token
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
            .take(8)
        {
            if !seen.insert(token.to_string()) {
                continue;
            }
            codes.push(GiftCodeEntry {
                code: token.to_string(),
                valid_until: extract_until(window),
                requirement: extract_requirement(window),
                exclusive: token.starts_with("CC") || window.contains("exclusive"),
            });
        }
    }

    codes.truncate(8);
    codes
}

fn extract_until(window: &str) -> Option<String> {
    let start = window.find("valid until")?;
    let rest = &window[start + "valid until".len()..];
    let end = rest.find(['.', '\n', ';'])?;
    Some(rest[..end].trim().to_string())
}

fn extract_requirement(window: &str) -> Option<String> {
    let start = window.find("requirement:")?;
    let rest = &window[start + "requirement:".len()..];
    let end = rest.find(['.', '\n', ';'])?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_urls() {
        assert!(is_article_url(
            "https://www.whiteoutsurvival.wiki/sneak-peek/080366853en/"
        ));
        assert!(!is_article_url(
            "https://www.whiteoutsurvival.wiki/sneak-peek/"
        ));
        assert!(!is_article_url(
            "https://www.whiteoutsurvival.wiki/sneak-peek/tw/0803/"
        ));
    }

    #[test]
    fn decodes() {
        assert_eq!(decode_entities("a &amp; b &#39;c&#39;"), "a & b 'c'");
    }
}
