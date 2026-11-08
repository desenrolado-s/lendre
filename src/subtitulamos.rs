//! Keyless TV subtitle search through Subtitulamos.tv's public HTTPS site.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::gestdown::episode_query;
use crate::service::{ProviderKind, SubtitleResult};

/// Subtitulamos.tv HTTPS base URL.
pub const BASE: &str = "https://www.subtitulamos.tv/";

/// Allowed origins for Subtitulamos.tv requests.
pub const ALLOWED_HOSTS: &[&str] = &["www.subtitulamos.tv", "subtitulamos.tv"];

/// Public catalog User-Agent.
pub const USER_AGENT: &str =
    "LegendasLinux/2.0 (+https://github.com/desenrolado-s/Subtitle-Downloader)";

const MAX_RESPONSE: usize = 5 * 1024 * 1024;

/// Resolves `url` against `base` and ensures it uses HTTPS on an allowed Subtitulamos host.
pub fn resolve_url(url: &str, base: &str) -> Result<String, String> {
    let base_url =
        Url::parse(base).map_err(|_| "Link de download do Subtitulamos inválido.".to_string())?;
    let resolved = base_url
        .join(url)
        .map_err(|_| "Link de download do Subtitulamos inválido.".to_string())?;
    let Some(host) = resolved.host_str() else {
        return Err("Link de download do Subtitulamos inválido.".to_string());
    };
    if resolved.scheme() != "https" || !ALLOWED_HOSTS.contains(&host) {
        return Err("Link de download do Subtitulamos inválido.".to_string());
    }
    Ok(resolved.into())
}

fn wanted_languages(language: &str) -> Option<&'static [&'static str]> {
    let primary = language.split('-').next().unwrap_or(language).to_lowercase();
    match primary.as_str() {
        "en" => Some(&["english", "english (us)", "english (uk)"]),
        "pt" => Some(&["portuguese", "brazilian"]),
        _ => None,
    }
}

fn fetch_bytes(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let valid_url = resolve_url(url, BASE)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .get(&valid_url)
        .set("Accept", accept)
        .set("Referer", BASE)
        .set("User-Agent", USER_AGENT)
        .call();

    match response {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take((MAX_RESPONSE + 1) as u64)
                .read_to_end(&mut buf)
                .map_err(|_| "Não foi possível conectar ao Subtitulamos.".to_string())?;
            Ok(buf)
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("Subtitulamos retornou HTTP {code}.")),
        Err(ureq::Error::Transport(_)) => {
            Err("Não foi possível conectar ao Subtitulamos.".to_string())
        }
    }
}

fn extract_attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(pos) = rest.to_ascii_lowercase().find(&name.to_ascii_lowercase()) {
        let after_name = &rest[pos + name.len()..];
        let trimmed = after_name.trim_start();
        if let Some(after_eq) = trimmed.strip_prefix('=') {
            let val_trimmed = after_eq.trim_start();
            if let Some(quote) = val_trimmed.chars().next()
                && (quote == '"' || quote == '\'')
            {
                let content = &val_trimmed[quote.len_utf8()..];
                if let Some(end) = content.find(quote) {
                    return Some(&content[..end]);
                }
            }
        }
        rest = after_name;
    }
    None
}

/// Parses an episode HTML page to extract `(language_name, download_href)` pairs.
pub fn parse_episode_items(html: &str) -> Vec<(Option<String>, String)> {
    let mut items = Vec::new();
    let mut current_language: Option<String> = None;
    let mut language_depth: usize = 0;
    let mut language_text = String::new();
    let mut progress_depth: usize = 0;
    let mut current_href: Option<String> = None;

    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        let text_before = &rest[..lt];
        if language_depth > 0 {
            language_text.push_str(text_before);
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
            if tag_name == "div" {
                if language_depth > 0 {
                    language_depth -= 1;
                    if language_depth == 0 {
                        current_language = Some(language_text.trim().to_lowercase());
                        language_text.clear();
                    }
                }
                if progress_depth > 0 {
                    progress_depth -= 1;
                    if progress_depth == 0
                        && let Some(href) = current_href.take()
                    {
                        items.push((current_language.clone(), href));
                    }
                }
            }
            continue;
        }

        let is_self_closing = tag_content.ends_with('/');
        let clean_tag = tag_content.trim_end_matches('/');
        let mut parts = clean_tag.splitn(2, char::is_whitespace);
        let tag_name = parts.next().unwrap_or("").to_ascii_lowercase();
        let attrs = parts.next().unwrap_or("");

        if tag_name == "div" && !is_self_closing {
            if language_depth > 0 {
                language_depth += 1;
            } else if let Some(class_val) = extract_attr(attrs, "class")
                && class_val
                    .split_whitespace()
                    .any(|c| c == "subtitle_language")
            {
                language_depth = 1;
                language_text.clear();
            }

            if progress_depth > 0 {
                progress_depth += 1;
            } else if extract_attr(attrs, "id") == Some("progress_buttons_row") {
                progress_depth = 1;
                current_href = None;
            }
        } else if tag_name == "a"
            && progress_depth > 0
            && current_href.is_none()
            && let Some(href) = extract_attr(attrs, "href")
        {
            current_href = Some(href.to_string());
        }
    }

    items
}

/// Extracts matching subtitle rows from an episode HTML page for the requested `language`.
pub fn subtitle_rows(page: &[u8], language: &str) -> Vec<SubtitleResult> {
    let Some(wanted) = wanted_languages(language) else {
        return Vec::new();
    };
    let html = String::from_utf8_lossy(page);
    let mut rows = Vec::new();

    for (current_language, href) in parse_episode_items(&html) {
        let Some(lang) = current_language else {
            continue;
        };
        if !wanted.contains(&lang.as_str()) {
            continue;
        }
        let Ok(download_url) = resolve_url(&href, BASE) else {
            continue;
        };
        let file_name = Url::parse(&download_url)
            .ok()
            .and_then(|u| {
                u.path()
                    .rsplit('/')
                    .next()
                    .map(|seg| percent_decode_str(seg))
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Legenda".to_string());

        rows.push(SubtitleResult {
            provider: ProviderKind::Subtitulamos,
            file_id: None,
            subtitle_id: None,
            download_url: Some(download_url),
            file_name,
            language: language.to_string(),
            download_count: 0,
            source: "Subtitulamos.tv".to_string(),
        });
    }

    rows
}

fn percent_decode_str(input: &str) -> String {
    url::form_urlencoded::parse(input.as_bytes())
        .map(|(k, v)| {
            if v.is_empty() {
                k.into_owned()
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn find_episode_url<F>(
    title: &str,
    season: u32,
    episode: u32,
    mut fetcher: F,
) -> Result<Option<String>, String>
where
    F: FnMut(&str, &str) -> Result<Vec<u8>, String>,
{
    let query = format!("{title} {season}x{episode}");
    let encoded_query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", &query)
        .finish();
    let body = fetcher(&format!("search/query?{encoded_query}"), "application/json")?;
    if body.len() > MAX_RESPONSE {
        return Err(
            "Resposta do Subtitulamos grande demais para consultar com segurança.".to_string(),
        );
    }
    let shows: Vec<Value> = serde_json::from_slice(&body)
        .map_err(|_| "Resposta inválida do Subtitulamos.".to_string())?;

    let normalized = title.to_lowercase();
    for show in shows {
        let name = show
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !name.contains(&normalized) {
            continue;
        }
        if let Some(episodes) = show.get("episodes").and_then(|v| v.as_array()) {
            for item in episodes {
                let s = item.get("season").and_then(|v| v.as_u64());
                let n = item.get("number").and_then(|v| v.as_u64());
                if s == Some(u64::from(season))
                    && n == Some(u64::from(episode))
                    && let Some(id_val) = item.get("id")
                {
                    let id_str = match id_val {
                        Value::Number(num) => num.to_string(),
                        Value::String(s) if !s.is_empty() => s.clone(),
                        _ => continue,
                    };
                    return Ok(Some(resolve_url(&format!("episodes/{id_str}"), BASE)?));
                }
            }
        }
    }
    Ok(None)
}

/// Searches Subtitulamos.tv using custom episode URL resolver and page fetcher.
pub fn search_with<E, F>(
    title: &str,
    video: Option<&Path>,
    language: &str,
    episode_url_fn: E,
    page_fetcher: F,
) -> Result<Vec<SubtitleResult>, String>
where
    E: FnOnce(&str, u32, u32) -> Result<Option<String>, String>,
    F: FnOnce(&str, &str) -> Result<Vec<u8>, String>,
{
    if wanted_languages(language).is_none() {
        return Ok(Vec::new());
    }
    let Some((show_title, season, episode)) = episode_query(title, video) else {
        return Ok(Vec::new());
    };
    if show_title.chars().count() < 3 {
        return Ok(Vec::new());
    }
    let Some(url) = episode_url_fn(&show_title, season, episode)? else {
        return Ok(Vec::new());
    };
    let page = page_fetcher(&url, "text/html")?;
    Ok(subtitle_rows(&page, language))
}

/// Searches Subtitulamos.tv for English or Portuguese TV episode subtitles without credentials.
pub fn search(
    title: &str,
    video: Option<&Path>,
    language: &str,
) -> Result<Vec<SubtitleResult>, String> {
    search_with(
        title,
        video,
        language,
        |show, season, episode| find_episode_url(show, season, episode, fetch_bytes),
        fetch_bytes,
    )
}

/// Downloads a Subtitulamos subtitle using a custom byte fetcher.
pub fn download_with<F>(result: &SubtitleResult, fetcher: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&str, &str) -> Result<Vec<u8>, String>,
{
    let raw_url = result
        .download_url
        .as_deref()
        .ok_or_else(|| "Link de download do Subtitulamos inválido.".to_string())?;
    let valid_url = resolve_url(raw_url, BASE)?;
    fetcher(&valid_url, "text/srt, text/plain, */*")
}

/// Downloads a subtitle file from Subtitulamos.tv over HTTPS.
pub fn download(result: &SubtitleResult) -> Result<Vec<u8>, String> {
    download_with(result, fetch_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ignores_unsupported_language() {
        let mut called = false;
        let rows = search_with(
            "Show S01E02",
            None,
            "ja",
            |_, _, _| {
                called = true;
                Ok(None)
            },
            |_, _| Ok(Vec::new()),
        )
        .unwrap();
        assert!(rows.is_empty());
        assert!(!called);
    }

    #[test]
    fn test_search_finds_portuguese_finished_subtitle() {
        let page = br#"<div class="subtitle_language">Brazilian</div>
        <div id="progress_buttons_row"><div><a href="/download/subtitle.srt">Baixar</a></div></div>
        <div class="subtitle_language">English</div>
        <div id="progress_buttons_row"><span>Em andamento</span></div>"#;

        let rows = search_with(
            "The Show S01E02",
            None,
            "pt-br",
            |_, _, _| Ok(Some("https://www.subtitulamos.tv/episodes/42".to_string())),
            |_, _| Ok(page.to_vec()),
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, ProviderKind::Subtitulamos);
        assert_eq!(
            rows[0].download_url.as_deref(),
            Some("https://www.subtitulamos.tv/download/subtitle.srt")
        );
        assert_eq!(rows[0].source, "Subtitulamos.tv");
    }

    #[test]
    fn test_download_rejects_off_origin_url() {
        let item = SubtitleResult {
            provider: ProviderKind::Subtitulamos,
            file_id: None,
            subtitle_id: None,
            download_url: Some("https://example.invalid/file.srt".to_string()),
            file_name: "file.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 0,
            source: "Subtitulamos.tv".to_string(),
        };
        assert!(download(&item).is_err());
    }

    #[test]
    fn test_download_uses_allowed_https_url() {
        let url = "https://www.subtitulamos.tv/download/file.srt";
        let item = SubtitleResult {
            provider: ProviderKind::Subtitulamos,
            file_id: None,
            subtitle_id: None,
            download_url: Some(url.to_string()),
            file_name: "file.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 0,
            source: "Subtitulamos.tv".to_string(),
        };
        let mut requested_url = String::new();
        let bytes = download_with(&item, |u, _| {
            requested_url = u.to_string();
            Ok(b"subtitle".to_vec())
        })
        .unwrap();
        assert_eq!(bytes, b"subtitle");
        assert_eq!(requested_url, url);
    }
}
