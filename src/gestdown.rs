//! Keyless TV subtitle search through Gestdown's public HTTPS API.

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use url::Url;

use crate::service::{ProviderKind, SubtitleResult};

/// Gestdown HTTPS base origin.
pub const BASE: &str = "https://api.gestdown.info";

/// Public catalog User-Agent.
pub const USER_AGENT: &str =
    "LegendasLinux/2.0 (+https://github.com/desenrolado-s/Subtitle-Downloader)";

const MAX_DOWNLOAD_READ: u64 = 10 * 1024 * 1024 + 1;

static EPISODE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\bs(\d{1,2})e(\d{1,3})\b|\b(\d{1,2})x(\d{1,3})\b)")
        .expect("valid episode regex")
});

/// Extracts `(show_title, season, episode)` from a search query or video filename stem.
pub fn episode_query(title: &str, video: Option<&Path>) -> Option<(String, u32, u32)> {
    let raw = if let Some(path) = video {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(title)
    } else {
        title
    };
    let text = raw.replace(['.', '_'], " ");
    let captures = EPISODE_REGEX.captures(&text)?;
    let full_match = captures.get(0)?;

    let (season_str, episode_str) = if let (Some(s), Some(e)) = (captures.get(1), captures.get(2)) {
        (s.as_str(), e.as_str())
    } else {
        (captures.get(3)?.as_str(), captures.get(4)?.as_str())
    };

    let season: u32 = season_str.parse().ok()?;
    let episode: u32 = episode_str.parse().ok()?;
    let show = text[..full_match.start()]
        .trim_matches(|c: char| c.is_whitespace() || c == '-')
        .to_string();

    Some((show, season, episode))
}

/// Resolves and validates a Gestdown download URI, ensuring it stays on `https://api.gestdown.info`.
pub fn download_url(uri: &str) -> Result<String, String> {
    let base = Url::parse(BASE).expect("valid BASE URL");
    let resolved = base
        .join(uri)
        .map_err(|_| "Link de download do Gestdown inválido.".to_string())?;
    if resolved.scheme() != "https" || resolved.host_str() != Some("api.gestdown.info") {
        return Err("Link de download do Gestdown inválido.".to_string());
    }
    Ok(resolved.into())
}

fn fetch_json(path: &str) -> Result<Value, String> {
    let url = format!("{BASE}{path}");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let response = agent
        .get(&url)
        .set("Accept", "application/json")
        .set("User-Agent", USER_AGENT)
        .call();

    match response {
        Ok(resp) => {
            let mut body = Vec::new();
            resp.into_reader()
                .take(5 * 1024 * 1024)
                .read_to_end(&mut body)
                .map_err(|_| {
                    "Não foi possível conectar ao Gestdown. Confira a internet e tente novamente."
                        .to_string()
                })?;
            serde_json::from_slice(&body).map_err(|_| "Resposta inválida do Gestdown.".to_string())
        }
        Err(ureq::Error::Status(404 | 423, _)) => Ok(serde_json::json!({})),
        Err(ureq::Error::Status(code, _)) => Err(format!("Gestdown retornou HTTP {code}.")),
        Err(ureq::Error::Transport(_)) => {
            Err("Não foi possível conectar ao Gestdown. Confira a internet e tente novamente."
                .to_string())
        }
    }
}

/// Searches Gestdown using a custom JSON fetcher (used for unit testing and live requests).
pub fn search_with<F>(
    title: &str,
    video: Option<&Path>,
    language: &str,
    mut json_fetcher: F,
) -> Result<Vec<SubtitleResult>, String>
where
    F: FnMut(&str) -> Result<Value, String>,
{
    let Some((show_title, season, episode)) = episode_query(title, video) else {
        return Ok(Vec::new());
    };
    if show_title.chars().count() < 3 {
        return Ok(Vec::new());
    }

    let encoded_show = url::form_urlencoded::byte_serialize(show_title.as_bytes()).collect::<String>();
    let shows_payload = json_fetcher(&format!("/shows/search/{encoded_show}"))?;
    let Some(shows) = shows_payload.get("shows").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    if shows.is_empty() {
        return Ok(Vec::new());
    }

    let exact = shows
        .iter()
        .find(|show| {
            show.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|name| name.to_lowercase() == show_title.to_lowercase())
        })
        .unwrap_or(&shows[0]);

    let Some(show_id) = exact.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    let show_name = exact
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&show_title);

    let language_code = language
        .split('-')
        .next()
        .unwrap_or(language)
        .to_lowercase();
    let encoded_lang =
        url::form_urlencoded::byte_serialize(language_code.as_bytes()).collect::<String>();

    let subtitles_payload = json_fetcher(&format!(
        "/subtitles/get/{show_id}/{season}/{episode}/{encoded_lang}"
    ))?;

    let mut rows = Vec::new();
    if let Some(items) = subtitles_payload
        .get("matchingSubtitles")
        .and_then(|v| v.as_array())
    {
        for subtitle in items {
            if subtitle.get("completed").and_then(|v| v.as_bool()) == Some(false) {
                continue;
            }
            let Some(subtitle_id) = subtitle
                .get("subtitleId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(uri) = subtitle
                .get("downloadUri")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Ok(resolved_url) = download_url(uri) else {
                continue;
            };

            let version = subtitle
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file_name = format!("{show_name} S{season:02}E{episode:02} {version}")
                .trim()
                .to_string();
            let download_count = subtitle
                .get("downloadCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            rows.push(SubtitleResult {
                provider: ProviderKind::Gestdown,
                file_id: None,
                subtitle_id: Some(subtitle_id.to_string()),
                download_url: Some(resolved_url),
                file_name,
                language: language.to_string(),
                download_count,
                source: "Gestdown (Addic7ed)".to_string(),
            });
        }
    }

    Ok(rows)
}

/// Searches Gestdown for matching TV episode subtitles without requiring an API key.
pub fn search(
    title: &str,
    video: Option<&Path>,
    language: &str,
) -> Result<Vec<SubtitleResult>, String> {
    search_with(title, video, language, fetch_json)
}

/// Downloads a Gestdown subtitle using a custom byte fetcher.
pub fn download_with<F>(result: &SubtitleResult, fetcher: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&str) -> Result<Vec<u8>, String>,
{
    let raw_url = result
        .download_url
        .as_deref()
        .ok_or_else(|| "Link de download do Gestdown inválido.".to_string())?;
    let valid_url = download_url(raw_url)?;
    fetcher(&valid_url)
}

/// Downloads a subtitle payload from Gestdown over HTTPS.
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
                    .map_err(|_| "Não foi possível baixar a legenda do Gestdown.".to_string())?;
                Ok(buf)
            }
            Err(ureq::Error::Status(code, _)) => {
                Err(format!("Gestdown retornou HTTP {code} ao baixar a legenda."))
            }
            Err(ureq::Error::Transport(_)) => {
                Err("Não foi possível baixar a legenda do Gestdown.".to_string())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    #[test]
    fn test_episode_query_reads_standard_release_name() {
        let (show, season, episode) =
            episode_query("", Some(Path::new("The.Show.S02E03.1080p.mkv"))).unwrap();
        assert_eq!((show.as_str(), season, episode), ("The Show", 2, 3));
    }

    #[test]
    fn test_search_ignores_incomplete_and_off_origin_results() {
        let mut responses = VecDeque::from(vec![
            json!({"shows": [{"id": "show-id", "name": "The Show"}]}),
            json!({"matchingSubtitles": [
                {"subtitleId": "good", "downloadUri": "/subtitles/download/good", "version": "WEB", "completed": true, "downloadCount": 4},
                {"subtitleId": "bad", "downloadUri": "https://example.invalid/sub", "completed": true},
                {"subtitleId": "partial", "downloadUri": "/subtitles/download/partial", "completed": false}
            ]}),
        ]);

        let rows = search_with("The Show S01E02", None, "pt-br", |_| {
            Ok(responses.pop_front().unwrap())
        })
        .unwrap();

        let ids: Vec<_> = rows
            .iter()
            .filter_map(|r| r.subtitle_id.as_deref())
            .collect();
        assert_eq!(ids, vec!["good"]);
    }

    #[test]
    fn test_search_builds_result() {
        let mut responses = VecDeque::from(vec![
            json!({"shows": [{"id": "show-id", "name": "The Show"}]}),
            json!({"matchingSubtitles": [
                {"subtitleId": "good", "downloadUri": "/subtitles/download/good", "version": "WEB", "completed": true, "downloadCount": 4}
            ]}),
        ]);

        let rows = search_with("The Show S01E02", None, "pt-br", |_| {
            Ok(responses.pop_front().unwrap())
        })
        .unwrap();

        assert_eq!(
            rows[0].download_url.as_deref(),
            Some("https://api.gestdown.info/subtitles/download/good")
        );
        assert_eq!(rows[0].source, "Gestdown (Addic7ed)");
    }

    #[test]
    fn test_download_uses_https_url() {
        let item = SubtitleResult {
            provider: ProviderKind::Gestdown,
            file_id: None,
            subtitle_id: Some("x".to_string()),
            download_url: Some("https://api.gestdown.info/subtitles/download/x".to_string()),
            file_name: "Show.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 1,
            source: "Gestdown (Addic7ed)".to_string(),
        };
        let mut requested_url = String::new();
        let data = download_with(&item, |url| {
            requested_url = url.to_string();
            Ok(b"subtitle".to_vec())
        })
        .unwrap();
        assert_eq!(data, b"subtitle");
        assert_eq!(
            requested_url,
            "https://api.gestdown.info/subtitles/download/x"
        );
    }

    #[test]
    fn test_download_rejects_off_origin_url() {
        let item = SubtitleResult {
            provider: ProviderKind::Gestdown,
            file_id: None,
            subtitle_id: None,
            download_url: Some("https://example.invalid/subtitle".to_string()),
            file_name: "Show.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 0,
            source: "Gestdown (Addic7ed)".to_string(),
        };
        assert!(download(&item).is_err());
    }
}
