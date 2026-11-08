//! Japanese anime subtitle search through AJATT's public catalog.

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use url::Url;

use crate::service::{ProviderKind, SubtitleResult};

/// AJATT public catalog root URL.
pub const CATALOG: &str = "https://subtitles.ajatt.top/";

/// Allowed origins for AJATT catalog pages and raw subtitle files.
pub const ALLOWED_HOSTS: &[&str] = &["subtitles.ajatt.top", "raw.githubusercontent.com"];

/// Public catalog User-Agent.
pub const USER_AGENT: &str =
    "LegendasLinux/2.0 (+https://github.com/desenrolado-s/Subtitle-Downloader)";

const MAX_RESPONSE: usize = 5 * 1024 * 1024;
const MAX_DOWNLOAD_READ: u64 = 10 * 1024 * 1024 + 1;

static EPISODE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:s\d{1,2}e\d{1,3}|\d{1,2}x\d{1,3})\b").expect("valid episode regex")
});

/// Resolves `url` against `base` and enforces HTTPS and trusted AJATT/Kitsunekko hosts/paths.
pub fn resolve_url(url: &str, base: &str) -> Result<String, String> {
    let base_url =
        Url::parse(base).map_err(|_| "Link de download do AJATT inválido.".to_string())?;
    let resolved = base_url
        .join(url)
        .map_err(|_| "Link de download do AJATT inválido.".to_string())?;
    let Some(host) = resolved.host_str() else {
        return Err("Link de download do AJATT inválido.".to_string());
    };
    if resolved.scheme() != "https" || !ALLOWED_HOSTS.contains(&host) {
        return Err("Link de download do AJATT inválido.".to_string());
    }
    if host == "raw.githubusercontent.com"
        && !resolved
            .path()
            .starts_with("/Ajatt-Tools/kitsunekko-mirror/main/subtitles/")
    {
        return Err("Link de download do AJATT inválido.".to_string());
    }
    Ok(resolved.into())
}

fn extract_href(attrs: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(pos) = rest.to_ascii_lowercase().find("href") {
        let after = &rest[pos + 4..];
        let trimmed = after.trim_start();
        if let Some(after_eq) = trimmed.strip_prefix('=') {
            let val_trimmed = after_eq.trim_start();
            if let Some(quote) = val_trimmed.chars().next()
                && (quote == '"' || quote == '\'')
            {
                let content = &val_trimmed[quote.len_utf8()..];
                if let Some(end) = content.find(quote) {
                    return Some(content[..end].to_string());
                }
            }
        }
        rest = after;
    }
    None
}

/// Parses `<a href="...">text</a>` anchor pairs from an HTML document.
pub fn parse_links(html: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let mut current_href: Option<String> = None;
    let mut current_text = String::new();

    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        let before = &rest[..lt];
        if current_href.is_some() {
            current_text.push_str(before);
        }
        let after_lt = &rest[lt + 1..];
        let Some(gt) = after_lt.find('>') else {
            break;
        };
        let tag_content = after_lt[..gt].trim();
        rest = &after_lt[gt + 1..];

        if let Some(closing) = tag_content.strip_prefix('/') {
            let tag_name = closing
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if tag_name == "a"
                && let Some(href) = current_href.take()
            {
                links.push((href, current_text.trim().to_string()));
                current_text.clear();
            }
            continue;
        }

        let clean_tag = tag_content.trim_end_matches('/');
        let mut parts = clean_tag.splitn(2, char::is_whitespace);
        let tag_name = parts.next().unwrap_or("").to_ascii_lowercase();
        let attrs = parts.next().unwrap_or("");
        if tag_name == "a" {
            current_href = extract_href(attrs);
            current_text.clear();
        }
    }

    links
}

/// Fetches and parses anchor links from a validated AJATT catalog URL using a custom byte fetcher.
pub fn fetch_links_with<F>(url: &str, fetcher: F) -> Result<Vec<(String, String)>, String>
where
    F: FnOnce(&str) -> Result<Vec<u8>, String>,
{
    let valid_url = resolve_url(url, CATALOG)?;
    let body = fetcher(&valid_url)?;
    if body.len() > MAX_RESPONSE {
        return Err("Catálogo AJATT grande demais para consultar com segurança.".to_string());
    }
    let html = String::from_utf8_lossy(&body);
    Ok(parse_links(&html))
}

fn fetch_catalog_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .get(url)
        .set("Accept", "text/html")
        .set("User-Agent", USER_AGENT)
        .call();

    match response {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take((MAX_RESPONSE + 1) as u64)
                .read_to_end(&mut buf)
                .map_err(|_| "Não foi possível conectar ao catálogo AJATT.".to_string())?;
            Ok(buf)
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("AJATT retornou HTTP {code}.")),
        Err(ureq::Error::Transport(_)) => {
            Err("Não foi possível conectar ao catálogo AJATT.".to_string())
        }
    }
}

/// Tokenizes a search title into normalized terms, stripping episode markers.
pub fn terms(title: &str) -> Vec<String> {
    let clean = EPISODE_REGEX.replace_all(title, " ").to_lowercase();
    clean
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|part| part.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

/// Searches AJATT using a custom link provider closure.
pub fn search_with<F>(
    title: &str,
    video: Option<&Path>,
    language: &str,
    mut links_provider: F,
) -> Result<Vec<SubtitleResult>, String>
where
    F: FnMut(&str) -> Result<Vec<(String, String)>, String>,
{
    let primary = language.split('-').next().unwrap_or(language).to_lowercase();
    if primary != "ja" && primary != "jpn" {
        return Ok(Vec::new());
    }

    let raw_query = if !title.trim().is_empty() {
        title
    } else if let Some(path) = video {
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
    } else {
        ""
    };
    let query_terms = terms(raw_query);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }

    let catalog_links = links_provider(CATALOG)?;
    let candidates: Vec<(String, String)> = catalog_links
        .into_iter()
        .filter(|(href, name)| {
            let lower_name = name.to_lowercase();
            href.ends_with(".html") && query_terms.iter().all(|t| lower_name.contains(t))
        })
        .collect();

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let (chosen_href, _) = candidates
        .iter()
        .find(|(_, name)| terms(name) == query_terms)
        .unwrap_or(&candidates[0]);

    let page_url = resolve_url(chosen_href, CATALOG)?;
    let page_links = links_provider(&page_url)?;

    let mut rows = Vec::new();
    for (file_href, file_name) in page_links {
        if !file_href.to_lowercase().ends_with(".srt") {
            continue;
        }
        let Ok(download_url) = resolve_url(&file_href, &page_url) else {
            continue;
        };
        let resolved_name = if !file_name.trim().is_empty() {
            file_name
        } else {
            Url::parse(&download_url)
                .ok()
                .and_then(|u| u.path().rsplit('/').next().map(str::to_string))
                .unwrap_or_else(|| "Legenda.srt".to_string())
        };

        rows.push(SubtitleResult {
            provider: ProviderKind::Ajatt,
            file_id: None,
            subtitle_id: None,
            download_url: Some(download_url),
            file_name: resolved_name,
            language: "ja".to_string(),
            download_count: 0,
            source: "AJATT / Kitsunekko".to_string(),
        });
    }

    Ok(rows)
}

/// Searches AJATT for Japanese anime `.srt` subtitles without requiring credentials.
pub fn search(
    title: &str,
    video: Option<&Path>,
    language: &str,
) -> Result<Vec<SubtitleResult>, String> {
    search_with(title, video, language, |url| {
        fetch_links_with(url, fetch_catalog_bytes)
    })
}

/// Downloads an AJATT subtitle using a custom byte fetcher.
pub fn download_with<F>(result: &SubtitleResult, fetcher: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&str) -> Result<Vec<u8>, String>,
{
    let raw_url = result
        .download_url
        .as_deref()
        .ok_or_else(|| "Link de download do AJATT inválido.".to_string())?;
    let valid_url = resolve_url(raw_url, CATALOG)?;
    fetcher(&valid_url)
}

/// Downloads an `.srt` subtitle file from AJATT / Kitsunekko over HTTPS.
pub fn download(result: &SubtitleResult) -> Result<Vec<u8>, String> {
    download_with(result, |url| {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        let response = agent
            .get(url)
            .set("Accept", "text/srt, text/plain, */*")
            .set("User-Agent", USER_AGENT)
            .call();

        match response {
            Ok(resp) => {
                let mut buf = Vec::new();
                resp.into_reader()
                    .take(MAX_DOWNLOAD_READ)
                    .read_to_end(&mut buf)
                    .map_err(|_| "Não foi possível baixar a legenda do AJATT.".to_string())?;
                Ok(buf)
            }
            Err(ureq::Error::Status(code, _)) => {
                Err(format!("AJATT retornou HTTP {code} ao baixar a legenda."))
            }
            Err(ureq::Error::Transport(_)) => {
                Err("Não foi possível baixar a legenda do AJATT.".to_string())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn test_ignores_non_japanese_language() {
        let mut called = false;
        let rows = search_with("One Piece", None, "pt-br", |_| {
            called = true;
            Ok(Vec::new())
        })
        .unwrap();
        assert!(rows.is_empty());
        assert!(!called);
    }

    #[test]
    fn test_search_returns_srt_from_matching_anime_page() {
        let catalog = vec![
            ("anime_tv/one-piece.html".to_string(), "ONE PIECE".to_string()),
            ("anime_tv/naruto.html".to_string(), "Naruto".to_string()),
        ];
        let page = vec![
            (
                "https://raw.githubusercontent.com/Ajatt-Tools/kitsunekko-mirror/main/subtitles/anime_tv/ONE%20PIECE/One%20Piece%20-%20E01.srt".to_string(),
                "One Piece - E01.srt".to_string(),
            ),
            ("episode.ass".to_string(), "Episode.ass".to_string()),
        ];
        let mut responses = VecDeque::from(vec![catalog, page]);

        let rows = search_with("One Piece", None, "ja", |_| {
            Ok(responses.pop_front().unwrap())
        })
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, ProviderKind::Ajatt);
        assert_eq!(rows[0].source, "AJATT / Kitsunekko");
    }

    #[test]
    fn test_rejects_untrusted_download_origin() {
        let bad1 = SubtitleResult {
            provider: ProviderKind::Ajatt,
            file_id: None,
            subtitle_id: None,
            download_url: Some("https://example.invalid/file.srt".to_string()),
            file_name: "file.srt".to_string(),
            language: "ja".to_string(),
            download_count: 0,
            source: "AJATT / Kitsunekko".to_string(),
        };
        assert!(download(&bad1).is_err());

        let bad2 = SubtitleResult {
            download_url: Some(
                "https://raw.githubusercontent.com/other/source/main/file.srt".to_string(),
            ),
            ..bad1
        };
        assert!(download(&bad2).is_err());
    }

    #[test]
    fn test_download_uses_allowed_https_url() {
        let url =
            "https://raw.githubusercontent.com/Ajatt-Tools/kitsunekko-mirror/main/subtitles/x.srt";
        let item = SubtitleResult {
            provider: ProviderKind::Ajatt,
            file_id: None,
            subtitle_id: None,
            download_url: Some(url.to_string()),
            file_name: "x.srt".to_string(),
            language: "ja".to_string(),
            download_count: 0,
            source: "AJATT / Kitsunekko".to_string(),
        };
        let mut requested_url = String::new();
        let bytes = download_with(&item, |u| {
            requested_url = u.to_string();
            Ok(b"subtitle".to_vec())
        })
        .unwrap();
        assert_eq!(bytes, b"subtitle");
        assert_eq!(requested_url, url);
    }

    #[test]
    fn test_catalog_network_error_is_clear() {
        let err = fetch_links_with(CATALOG, |_| {
            Err("Não foi possível conectar ao catálogo AJATT.".to_string())
        })
        .unwrap_err();
        assert!(err.contains("conectar ao catálogo"));
    }
}
