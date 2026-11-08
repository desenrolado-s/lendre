//! GUI-independent subtitle search and download operations.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

use crate::files::{movie_hash, save_subtitle};
use crate::provider::{TransportError, USER_AGENT, api};
use crate::{ajatt, gestdown, podnapisi, subtitulamos};

const MAX_DOWNLOAD_READ: u64 = 10 * 1024 * 1024 + 1;

/// Supported subtitle provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "opensubtitles")]
    OpenSubtitles,
    #[serde(rename = "gestdown")]
    Gestdown,
    #[serde(rename = "subtitulamos")]
    Subtitulamos,
    #[serde(rename = "podnapisi")]
    Podnapisi,
    #[serde(rename = "ajatt")]
    Ajatt,
}

/// Normalized subtitle search result across all providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleResult {
    pub provider: ProviderKind,
    pub file_id: Option<u64>,
    pub subtitle_id: Option<String>,
    pub download_url: Option<String>,
    pub file_name: String,
    pub language: String,
    pub download_count: u64,
    pub source: String,
}

impl SubtitleResult {
    /// Formats the result row text displayed in the subtitle list.
    pub fn display_text(&self) -> String {
        let name = if self.file_name.is_empty() {
            "Legenda"
        } else {
            &self.file_name
        };
        format!(
            "{name}\n{} · {} · {} downloads",
            self.source, self.language, self.download_count
        )
    }
}

/// Searches public providers and (when `key` is non-empty) OpenSubtitles using injectable provider callbacks.
pub fn search_with<G, S, P, A, O>(
    title: &str,
    video: Option<&Path>,
    language: &str,
    key: &str,
    gestdown_fn: G,
    subtitulamos_fn: S,
    podnapisi_fn: P,
    ajatt_fn: A,
    mut opensubtitles_api: O,
) -> Result<Vec<SubtitleResult>, String>
where
    G: FnOnce(&str, Option<&Path>, &str) -> Result<Vec<SubtitleResult>, String>,
    S: FnOnce(&str, Option<&Path>, &str) -> Result<Vec<SubtitleResult>, String>,
    P: FnOnce(&str, Option<&Path>, &str) -> Result<Vec<SubtitleResult>, String>,
    A: FnOnce(&str, Option<&Path>, &str) -> Result<Vec<SubtitleResult>, String>,
    O: FnMut(&str, &str) -> Result<Value, String>,
{
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for outcome in [
        gestdown_fn(title, video, language),
        subtitulamos_fn(title, video, language),
        podnapisi_fn(title, video, language),
        ajatt_fn(title, video, language),
    ] {
        match outcome {
            Ok(items) => results.extend(items),
            Err(err) => errors.push(err),
        }
    }

    if key.trim().is_empty() {
        if !errors.is_empty() && results.is_empty() {
            return Err(errors.remove(0));
        }
        return Ok(results);
    }

    let mut rows: Vec<Value> = Vec::new();
    let fingerprint = if let Some(v) = video {
        movie_hash(v).ok().flatten()
    } else {
        None
    };

    if let Some(hash) = fingerprint {
        let qs = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("languages", language)
            .append_pair("order_by", "download_count")
            .append_pair("order_direction", "desc")
            .append_pair("moviehash", &hash)
            .append_pair("moviehash_match", "only")
            .finish();
        let resp = opensubtitles_api(&format!("subtitles?{qs}"), key)?;
        if let Some(arr) = resp.get("data").and_then(|v| v.as_array()) {
            rows = arr.clone();
        }
    }

    if rows.is_empty() {
        let fallback_query = if !title.trim().is_empty() {
            title.trim().to_string()
        } else if let Some(v) = video {
            v.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        let qs = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("languages", language)
            .append_pair("order_by", "download_count")
            .append_pair("order_direction", "desc")
            .append_pair("query", &fallback_query)
            .finish();
        let resp = opensubtitles_api(&format!("subtitles?{qs}"), key)?;
        if let Some(arr) = resp.get("data").and_then(|v| v.as_array()) {
            rows = arr.clone();
        }
    }

    for row in rows {
        let Some(attrs) = row.get("attributes") else {
            continue;
        };
        let Some(files) = attrs.get("files").and_then(|v| v.as_array()) else {
            continue;
        };
        let row_lang = attrs
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or(language);
        let download_count = attrs
            .get("download_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let release_fallback = attrs
            .get("release")
            .and_then(|v| v.as_str())
            .unwrap_or("Legenda");

        for file in files {
            let file_id = file.get("file_id").and_then(|v| v.as_u64());
            let file_name = file
                .get("file_name")
                .and_then(|v| v.as_str())
                .unwrap_or(release_fallback)
                .to_string();

            results.push(SubtitleResult {
                provider: ProviderKind::OpenSubtitles,
                file_id,
                subtitle_id: None,
                download_url: None,
                file_name,
                language: row_lang.to_string(),
                download_count,
                source: "OpenSubtitles".to_string(),
            });
        }
    }

    Ok(results)
}

/// Searches all public subtitle providers and OpenSubtitles (when `key` is provided).
pub fn search(
    title: &str,
    video: Option<&Path>,
    language: &str,
    key: &str,
) -> Result<Vec<SubtitleResult>, String> {
    search_with(
        title,
        video,
        language,
        key,
        gestdown::search,
        subtitulamos::search,
        podnapisi::search,
        ajatt::search,
        |route, k| api(route, k, None, ""),
    )
}

/// Downloads an OpenSubtitles subtitle using custom API and CDN transport functions.
pub fn download_opensubtitles_with<A, C>(
    result: &SubtitleResult,
    target: &Path,
    key: &str,
    user: &str,
    password: &str,
    mut api_fn: A,
    cdn_fetcher: C,
) -> Result<PathBuf, String>
where
    A: FnMut(&str, &str, Option<&Value>, &str) -> Result<Value, String>,
    C: FnOnce(&str, &[(&str, &str)]) -> Result<Vec<u8>, TransportError>,
{
    let file_id = result
        .file_id
        .ok_or_else(|| "Identificador de legenda ausente.".to_string())?;
    let login_payload = json!({
        "username": user,
        "password": password,
    });
    let login_resp = api_fn("login", key, Some(&login_payload), "")?;
    let token = login_resp
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Resposta de login inválida do OpenSubtitles.".to_string())?;

    let download_payload = json!({
        "file_id": file_id,
        "sub_format": "srt",
    });
    let download_resp = api_fn("download", key, Some(&download_payload), token)?;
    let link = download_resp
        .get("link")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Link de download inválido.".to_string())?;

    let parsed = Url::parse(link).map_err(|_| "Link de download inválido.".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Link de download inválido.".to_string());
    }

    let headers = [("User-Agent", USER_AGENT), ("Accept", "*/*")];
    let content = match cdn_fetcher(link, &headers) {
        Ok(bytes) => bytes,
        Err(TransportError::Status(403)) => {
            return Err(
                "O OpenSubtitles recusou o download (HTTP 403). Tente novamente mais tarde."
                    .to_string(),
            );
        }
        Err(TransportError::Status(code)) => {
            return Err(format!(
                "OpenSubtitles retornou HTTP {code} ao baixar a legenda."
            ));
        }
        Err(TransportError::Network(_)) => {
            return Err(
                "Não foi possível baixar a legenda do OpenSubtitles. Confira a internet e tente novamente."
                    .to_string(),
            );
        }
    };

    save_subtitle(target, &content).map_err(|e| e.to_string())
}

fn fetch_cdn_bytes(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, TransportError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .build();
    let mut req = agent.get(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    match req.call() {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take(MAX_DOWNLOAD_READ)
                .read_to_end(&mut buf)
                .map_err(|e| TransportError::Network(e.to_string()))?;
            Ok(buf)
        }
        Err(ureq::Error::Status(code, _)) => Err(TransportError::Status(code)),
        Err(ureq::Error::Transport(err)) => Err(TransportError::Network(err.to_string())),
    }
}

/// Downloads the selected subtitle from its provider and saves it atomically to `target`.
pub fn download(
    result: &SubtitleResult,
    target: &Path,
    key: &str,
    user: &str,
    password: &str,
) -> Result<PathBuf, String> {
    match result.provider {
        ProviderKind::Gestdown => {
            let bytes = gestdown::download(result)?;
            save_subtitle(target, &bytes).map_err(|e| e.to_string())
        }
        ProviderKind::Ajatt => {
            let bytes = ajatt::download(result)?;
            save_subtitle(target, &bytes).map_err(|e| e.to_string())
        }
        ProviderKind::Subtitulamos => {
            let bytes = subtitulamos::download(result)?;
            save_subtitle(target, &bytes).map_err(|e| e.to_string())
        }
        ProviderKind::Podnapisi => {
            let bytes = podnapisi::download(result)?;
            save_subtitle(target, &bytes).map_err(|e| e.to_string())
        }
        ProviderKind::OpenSubtitles => {
            download_opensubtitles_with(result, target, key, user, password, api, fetch_cdn_bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use tempfile::tempdir;

    const VALID_SRT: &[u8] = b"1\n00:00:01,000 --> 00:00:02,000\nHello\n";

    #[test]
    fn test_title_search_without_video() {
        let mut called_route = String::new();
        let rows = search_with(
            "Dune",
            None,
            "pt-br",
            "key",
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |route, _| {
                called_route = route.to_string();
                Ok(json!({"data": []}))
            },
        )
        .unwrap();

        assert!(rows.is_empty());
        assert!(called_route.contains("query=Dune"));
        assert!(!called_route.contains("moviehash"));
    }

    #[test]
    fn test_hash_miss_falls_back_to_title() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("movie.mkv");
        fs::write(&video, vec![0u8; 131_072]).unwrap();

        let mut routes = Vec::new();
        let mut responses = VecDeque::from(vec![
            json!({"data": []}),
            json!({"data": [{"attributes": {"files": [{"file_id": 5, "file_name": "movie.srt"}]}}]}),
        ]);

        let rows = search_with(
            "movie",
            Some(&video),
            "en",
            "key",
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |route, _| {
                routes.push(route.to_string());
                Ok(responses.pop_front().unwrap())
            },
        )
        .unwrap();

        assert_eq!(rows[0].file_id, Some(5));
        assert!(routes[0].contains("moviehash="));
        assert!(routes[1].contains("query=movie"));
    }

    #[test]
    fn test_download_saves_srt() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("movie.srt");
        let item = SubtitleResult {
            provider: ProviderKind::OpenSubtitles,
            file_id: Some(5),
            subtitle_id: None,
            download_url: None,
            file_name: "movie.srt".to_string(),
            language: "en".to_string(),
            download_count: 10,
            source: "OpenSubtitles".to_string(),
        };

        let mut api_responses = VecDeque::from(vec![
            json!({"token": "session"}),
            json!({"link": "https://example.org/sub"}),
        ]);

        download_opensubtitles_with(
            &item,
            &target,
            "key",
            "user",
            "password",
            |_, _, _, _| Ok(api_responses.pop_front().unwrap()),
            |_, _| Ok(VALID_SRT.to_vec()),
        )
        .unwrap();

        assert_eq!(fs::read(&target).unwrap(), VALID_SRT);
    }

    #[test]
    fn test_opensubtitles_download_sends_user_agent() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("movie.srt");
        let item = SubtitleResult {
            provider: ProviderKind::OpenSubtitles,
            file_id: Some(5),
            subtitle_id: None,
            download_url: None,
            file_name: "movie.srt".to_string(),
            language: "en".to_string(),
            download_count: 10,
            source: "OpenSubtitles".to_string(),
        };

        let mut api_responses = VecDeque::from(vec![
            json!({"token": "session"}),
            json!({"link": "https://dl.opensubtitles.com/en/download/sub/1"}),
        ]);
        let mut captured_ua = String::new();

        download_opensubtitles_with(
            &item,
            &target,
            "key",
            "user",
            "password",
            |_, _, _, _| Ok(api_responses.pop_front().unwrap()),
            |_, headers| {
                for (k, v) in headers {
                    if k.eq_ignore_ascii_case("User-Agent") {
                        captured_ua = (*v).to_string();
                    }
                }
                Ok(VALID_SRT.to_vec())
            },
        )
        .unwrap();

        assert!(captured_ua.contains("LegendasLinux"));
    }

    #[test]
    fn test_opensubtitles_download_403_is_reported_clearly() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("movie.srt");
        let item = SubtitleResult {
            provider: ProviderKind::OpenSubtitles,
            file_id: Some(5),
            subtitle_id: None,
            download_url: None,
            file_name: "movie.srt".to_string(),
            language: "en".to_string(),
            download_count: 10,
            source: "OpenSubtitles".to_string(),
        };

        let mut api_responses = VecDeque::from(vec![
            json!({"token": "session"}),
            json!({"link": "https://dl.opensubtitles.com/en/download/sub/1"}),
        ]);

        let err = download_opensubtitles_with(
            &item,
            &target,
            "key",
            "user",
            "password",
            |_, _, _, _| Ok(api_responses.pop_front().unwrap()),
            |_, _| Err(TransportError::Status(403)),
        )
        .unwrap_err();

        assert!(err.contains("HTTP 403"));
        assert!(!target.exists());
    }

    #[test]
    fn test_insecure_link_rejected() {
        let item = SubtitleResult {
            provider: ProviderKind::OpenSubtitles,
            file_id: Some(5),
            subtitle_id: None,
            download_url: None,
            file_name: "movie.srt".to_string(),
            language: "en".to_string(),
            download_count: 10,
            source: "OpenSubtitles".to_string(),
        };

        let mut api_responses = VecDeque::from(vec![
            json!({"token": "session"}),
            json!({"link": "http://example.org/sub"}),
        ]);

        let err = download_opensubtitles_with(
            &item,
            Path::new("unused.srt"),
            "key",
            "user",
            "password",
            |_, _, _, _| Ok(api_responses.pop_front().unwrap()),
            |_, _| Ok(VALID_SRT.to_vec()),
        )
        .unwrap_err();

        assert_eq!(err, "Link de download inválido.");
    }

    #[test]
    fn test_keyless_provider_searches_without_opensubtitles() {
        let expected = vec![SubtitleResult {
            provider: ProviderKind::Gestdown,
            file_id: None,
            subtitle_id: Some("x".to_string()),
            download_url: Some("https://api.gestdown.info/subtitles/download/x".to_string()),
            file_name: "Show S01E02".to_string(),
            language: "pt-br".to_string(),
            download_count: 1,
            source: "Gestdown (Addic7ed)".to_string(),
        }];
        let expected_clone = expected.clone();
        let mut os_called = false;

        let rows = search_with(
            "Show S01E02",
            None,
            "pt-br",
            "",
            |_, _, _| Ok(expected_clone),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _| {
                os_called = true;
                Ok(json!({}))
            },
        )
        .unwrap();

        assert_eq!(rows, expected);
        assert!(!os_called);
    }

    #[test]
    fn test_gestdown_outage_does_not_hide_opensubtitles() {
        let rows = search_with(
            "Show S01E02",
            None,
            "en",
            "key",
            |_, _, _| Err("offline".to_string()),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _, _| Ok(vec![]),
            |_, _| Ok(json!({"data": [{"attributes": {"files": [{"file_id": 5}]}}]})),
        )
        .unwrap();

        assert_eq!(rows[0].provider, ProviderKind::OpenSubtitles);
        assert_eq!(rows[0].file_id, Some(5));
    }
}
