//! Keyless multilingual subtitle search through Podnapisi's public XML API.

use std::io::{Cursor, Read};
use std::path::Path;
use std::time::Duration;

use url::Url;
use zip::ZipArchive;

use crate::gestdown::episode_query;
use crate::service::{ProviderKind, SubtitleResult};

/// Podnapisi HTTPS origin.
pub const BASE: &str = "https://www.podnapisi.net";

/// Public catalog User-Agent.
pub const USER_AGENT: &str =
    "LegendasLinux/2.0 (+https://github.com/desenrolado-s/Subtitle-Downloader)";

const MAX_RESPONSE: usize = 5 * 1024 * 1024;
const MAX_SUBTITLE: usize = 10 * 1024 * 1024;

/// Maps primary ISO language codes to Podnapisi numeric language IDs.
pub fn language_id(code: &str) -> Option<u32> {
    match code {
        "en" => Some(2),
        "pt" => Some(26),
        "ja" => Some(11),
        "es" => Some(28),
        "fr" => Some(8),
        "de" => Some(14),
        "it" => Some(9),
        "ko" => Some(4),
        "ru" => Some(27),
        "zh" => Some(17),
        _ => None,
    }
}

/// Resolves `path` against `https://www.podnapisi.net/` and verifies the host and HTTPS scheme.
pub fn resolve_url(path: &str) -> Result<String, String> {
    let base = Url::parse("https://www.podnapisi.net/").expect("valid base URL");
    let resolved = base
        .join(path)
        .map_err(|_| "Link de download do Podnapisi inválido.".to_string())?;
    if resolved.scheme() != "https" || resolved.host_str() != Some("www.podnapisi.net") {
        return Err("Link de download do Podnapisi inválido.".to_string());
    }
    Ok(resolved.into())
}

fn fetch_bytes(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let valid_url = resolve_url(url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .get(&valid_url)
        .set("Accept", accept)
        .set("User-Agent", USER_AGENT)
        .call();

    match response {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take((MAX_RESPONSE + 1) as u64)
                .read_to_end(&mut buf)
                .map_err(|_| "Não foi possível conectar ao Podnapisi.".to_string())?;
            Ok(buf)
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("Podnapisi retornou HTTP {code}.")),
        Err(ureq::Error::Transport(_)) => {
            Err("Não foi possível conectar ao Podnapisi.".to_string())
        }
    }
}

/// Infers `(query, season, episode)` from the title or linked video path.
pub fn coordinates(title: &str, video: Option<&Path>) -> (String, Option<u32>, Option<u32>) {
    if let Some((show, season, episode)) = episode_query(title, video) {
        return (show, Some(season), Some(episode));
    }
    let raw = if let Some(path) = video {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(title)
    } else {
        title
    };
    let clean = raw.replace(['.', '_'], " ").trim().to_string();
    (clean, None, None)
}

/// Validates that an XML payload is within size limits and contains no DTD/entity declarations.
pub fn validate_xml(body: &[u8]) -> Result<String, String> {
    if body.len() > MAX_RESPONSE {
        return Err("Resposta do Podnapisi grande demais para consultar com segurança.".to_string());
    }
    let upper = body.to_ascii_uppercase();
    if upper.windows(9).any(|w| w == b"<!DOCTYPE") || upper.windows(8).any(|w| w == b"<!ENTITY") {
        return Err("Resposta XML inválida do Podnapisi.".to_string());
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| "Resposta XML inválida do Podnapisi.".to_string())?
        .to_string();
    roxmltree::Document::parse(&text)
        .map_err(|_| "Resposta XML inválida do Podnapisi.".to_string())?;
    Ok(text)
}

fn child_text<'a>(node: roxmltree::Node<'a, 'a>, tag: &str) -> &'a str {
    node.children()
        .find(|n| n.is_element() && n.has_tag_name(tag))
        .and_then(|n| n.text())
        .map(str::trim)
        .unwrap_or("")
}

/// Searches Podnapisi using a custom byte fetcher.
pub fn search_with<F>(
    title: &str,
    video: Option<&Path>,
    language: &str,
    fetcher: F,
) -> Result<Vec<SubtitleResult>, String>
where
    F: FnOnce(&str, &str) -> Result<Vec<u8>, String>,
{
    let language_code = language
        .split('-')
        .next()
        .unwrap_or(language)
        .to_lowercase();
    let Some(lang_id) = language_id(&language_code) else {
        return Ok(Vec::new());
    };

    let (query, season, episode) = coordinates(title, video);
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("sXML", "1")
        .append_pair("sK", &query)
        .append_pair("sL", &lang_id.to_string());
    if let (Some(s), Some(e)) = (season, episode) {
        serializer
            .append_pair("sTS", &s.to_string())
            .append_pair("sTE", &e.to_string());
    }
    let query_str = serializer.finish();

    let raw_xml = fetcher(
        &format!("subtitles/search/old?{query_str}"),
        "application/xml, text/xml",
    )?;
    let xml_text = validate_xml(&raw_xml)?;
    let doc = roxmltree::Document::parse(&xml_text)
        .map_err(|_| "Resposta XML inválida do Podnapisi.".to_string())?;

    let mut rows = Vec::new();
    let lang_id_str = lang_id.to_string();

    for subtitle in doc
        .descendants()
        .filter(|n| n.is_element() && n.has_tag_name("subtitle"))
    {
        let pid = child_text(subtitle, "pid");
        let id = if !pid.is_empty() {
            pid
        } else {
            child_text(subtitle, "id")
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let sub_lang = child_text(subtitle, "language").to_lowercase();
        if !sub_lang.is_empty() && sub_lang != lang_id_str && sub_lang != language_code {
            continue;
        }

        let release_raw = child_text(subtitle, "release");
        let title_raw = child_text(subtitle, "title");
        let file_name = if !release_raw.is_empty() {
            release_raw.to_string()
        } else if !title_raw.is_empty() {
            title_raw.to_string()
        } else {
            format!("{id}.srt")
        };

        let count: u64 = child_text(subtitle, "downloads").parse().unwrap_or(0);
        let download_url = resolve_url(&format!("subtitles/{id}/download"))?;

        rows.push(SubtitleResult {
            provider: ProviderKind::Podnapisi,
            file_id: None,
            subtitle_id: Some(id.to_string()),
            download_url: Some(download_url),
            file_name,
            language: language.to_string(),
            download_count: count,
            source: "Podnapisi".to_string(),
        });
    }

    Ok(rows)
}

/// Searches Podnapisi for movie or episode subtitles in supported languages without credentials.
pub fn search(
    title: &str,
    video: Option<&Path>,
    language: &str,
) -> Result<Vec<SubtitleResult>, String> {
    search_with(title, video, language, fetch_bytes)
}

/// Extracts a single `.srt` subtitle from a Podnapisi ZIP archive payload.
pub fn srt_from_archive(content: &[u8]) -> Result<Vec<u8>, String> {
    let reader = Cursor::new(content);
    let mut archive = ZipArchive::new(reader)
        .map_err(|_| "Arquivo inválido recebido do Podnapisi.".to_string())?;

    let mut srt_indices = Vec::new();
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|_| "Arquivo inválido recebido do Podnapisi.".to_string())?;
        if !entry.is_dir() && entry.name().to_lowercase().ends_with(".srt") {
            srt_indices.push((idx, entry.size()));
        }
    }

    if srt_indices.len() != 1 || srt_indices[0].1 > MAX_SUBTITLE as u64 {
        return Err("Arquivo do Podnapisi não contém uma legenda SRT segura.".to_string());
    }

    let subtitle_entry = archive
        .by_index(srt_indices[0].0)
        .map_err(|_| "Arquivo inválido recebido do Podnapisi.".to_string())?;
    let mut body = Vec::new();
    subtitle_entry
        .take((MAX_SUBTITLE + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| "Arquivo inválido recebido do Podnapisi.".to_string())?;

    if body.len() > MAX_SUBTITLE {
        return Err("Legenda do Podnapisi grande demais.".to_string());
    }
    Ok(body)
}

/// Downloads a Podnapisi subtitle using a custom byte fetcher.
pub fn download_with<F>(result: &SubtitleResult, fetcher: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&str, &str) -> Result<Vec<u8>, String>,
{
    let raw_url = result
        .download_url
        .as_deref()
        .ok_or_else(|| "Link de download do Podnapisi inválido.".to_string())?;
    let valid_url = resolve_url(raw_url)?;
    let content = fetcher(&valid_url, "application/zip, text/srt, text/plain, */*")?;
    if content.starts_with(b"PK\x03\x04") {
        return srt_from_archive(&content);
    }
    if content.len() > MAX_SUBTITLE {
        return Err("Legenda do Podnapisi grande demais.".to_string());
    }
    Ok(content)
}

/// Downloads and unpacks a subtitle from Podnapisi over HTTPS.
pub fn download(result: &SubtitleResult) -> Result<Vec<u8>, String> {
    download_with(result, fetch_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn test_search_parses_public_xml_results() {
        let xml = br#"<results><subtitle><pid>42</pid><release>Show.S01E02.srt</release><language>26</language><downloads>7</downloads></subtitle></results>"#;
        let rows = search_with("Show S01E02", None, "pt-br", |_, _| Ok(xml.to_vec())).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].download_url.as_deref(),
            Some("https://www.podnapisi.net/subtitles/42/download")
        );
        assert_eq!(rows[0].source, "Podnapisi");
        assert_eq!(rows[0].download_count, 7);
    }

    #[test]
    fn test_search_rejects_xml_entities() {
        let err = validate_xml(b"<!DOCTYPE x [<!ENTITY x \"x\">]><x/>").unwrap_err();
        assert!(err.contains("XML inválida"));
    }

    #[test]
    fn test_download_extracts_single_srt_from_zip() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file("subtitle.srt", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"1\n00:00:01,000 --> 00:00:02,000\nHello\n")
                .unwrap();
            zip.finish().unwrap();
        }
        let archive_bytes = buf.into_inner();
        let item = SubtitleResult {
            provider: ProviderKind::Podnapisi,
            file_id: None,
            subtitle_id: Some("42".to_string()),
            download_url: Some("https://www.podnapisi.net/subtitles/42/download".to_string()),
            file_name: "Show.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 1,
            source: "Podnapisi".to_string(),
        };
        let body = download_with(&item, |_, _| Ok(archive_bytes)).unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Hello"));
    }

    #[test]
    fn test_download_rejects_off_origin_url() {
        let item = SubtitleResult {
            provider: ProviderKind::Podnapisi,
            file_id: None,
            subtitle_id: None,
            download_url: Some("https://example.invalid/file.srt".to_string()),
            file_name: "file.srt".to_string(),
            language: "pt-br".to_string(),
            download_count: 0,
            source: "Podnapisi".to_string(),
        };
        assert!(download(&item).is_err());
    }
}
