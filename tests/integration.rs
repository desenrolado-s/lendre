//! Integration tests for release versioning and end-to-end subtitle workflows.

use std::path::PathBuf;
use std::process::Command;

use legenda::credentials::{Vault, forget_with, load_with, save_with};
use legenda::files::{movie_hash, save_subtitle};
use legenda::service::{ProviderKind, SubtitleResult, search_with};
use legenda::ui::is_valid_language;
use serde_json::json;
use tempfile::tempdir;

#[cfg(not(target_os = "windows"))]
fn run_next_version(current: &str) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest_dir.join("scripts").join("next-version.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(current)
        .output()
        .expect("failed to execute scripts/next-version.sh");
    assert!(
        output.status.success(),
        "next-version.sh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("valid UTF-8")
        .trim()
        .to_string()
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_version_increments_patch() {
    assert_eq!(run_next_version("v2.0.8"), "2.0.9");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_version_patch_rolls_over_to_minor() {
    assert_eq!(run_next_version("v2.0.9"), "2.1.0");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_version_existing_two_digit_patch_rolls_over() {
    assert_eq!(run_next_version("v2.0.10"), "2.1.0");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_version_minor_rolls_over_to_major() {
    assert_eq!(run_next_version("v2.9.9"), "3.0.0");
}

#[test]
fn test_end_to_end_multi_provider_aggregation_and_atomic_save() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("The.Show.S01E02.mkv");
    std::fs::write(&video_path, vec![0u8; 131_072]).unwrap();
    assert_eq!(
        movie_hash(&video_path).unwrap().as_deref(),
        Some("0000000000020000")
    );

    let results = search_with(
        "The Show S01E02",
        Some(&video_path),
        "pt-br",
        "api-key",
        |_, _, _| {
            Ok(vec![SubtitleResult {
                provider: ProviderKind::Gestdown,
                file_id: None,
                subtitle_id: Some("g1".to_string()),
                download_url: Some("https://api.gestdown.info/subtitles/download/g1".to_string()),
                file_name: "The Show S01E02 WEB".to_string(),
                language: "pt-br".to_string(),
                download_count: 12,
                source: "Gestdown (Addic7ed)".to_string(),
            }])
        },
        |_, _, _| {
            Ok(vec![SubtitleResult {
                provider: ProviderKind::Subtitulamos,
                file_id: None,
                subtitle_id: None,
                download_url: Some("https://www.subtitulamos.tv/download/s1.srt".to_string()),
                file_name: "s1.srt".to_string(),
                language: "pt-br".to_string(),
                download_count: 0,
                source: "Subtitulamos.tv".to_string(),
            }])
        },
        |_, _, _| {
            Ok(vec![SubtitleResult {
                provider: ProviderKind::Podnapisi,
                file_id: None,
                subtitle_id: Some("42".to_string()),
                download_url: Some("https://www.podnapisi.net/subtitles/42/download".to_string()),
                file_name: "The.Show.S01E02.srt".to_string(),
                language: "pt-br".to_string(),
                download_count: 7,
                source: "Podnapisi".to_string(),
            }])
        },
        |_, _, _| Ok(vec![]),
        |_, _| {
            Ok(json!({
                "data": [{
                    "attributes": {
                        "language": "pt-br",
                        "download_count": 99,
                        "files": [{"file_id": 1001, "file_name": "The.Show.S01E02.OS.srt"}]
                    }
                }]
            }))
        },
    )
    .unwrap();

    assert_eq!(results.len(), 4);
    assert_eq!(results[0].provider, ProviderKind::Gestdown);
    assert_eq!(results[1].provider, ProviderKind::Subtitulamos);
    assert_eq!(results[2].provider, ProviderKind::Podnapisi);
    assert_eq!(results[3].provider, ProviderKind::OpenSubtitles);
    assert!(results[3].display_text().contains("OpenSubtitles · pt-br · 99 downloads"));

    let target = video_path.with_extension("pt-br.srt");
    let srt_bytes = b"1\n00:00:01,000 --> 00:00:03,500\nLegenda de teste.\n";
    let saved = save_subtitle(&target, srt_bytes).unwrap();
    assert_eq!(saved, target);
    assert_eq!(std::fs::read(&target).unwrap(), srt_bytes);
    assert!(is_valid_language("pt-br"));
}

#[test]
fn test_vault_roundtrip_integration() {
    struct InMemoryVault(std::cell::RefCell<Option<String>>);
    impl Vault for InMemoryVault {
        fn get_password(&self, _: &str, _: &str) -> Result<Option<String>, String> {
            Ok(self.0.borrow().clone())
        }
        fn set_password(&self, _: &str, _: &str, password: &str) -> Result<(), String> {
            *self.0.borrow_mut() = Some(password.to_string());
            Ok(())
        }
        fn delete_password(&self, _: &str, _: &str) -> Result<(), String> {
            *self.0.borrow_mut() = None;
            Ok(())
        }
    }

    let vault = InMemoryVault(std::cell::RefCell::new(None));
    save_with(&vault, "k1", "u1", "p1").unwrap();
    let creds = load_with(&vault).unwrap().unwrap();
    assert_eq!((creds.key.as_str(), creds.user.as_str(), creds.password.as_str()), ("k1", "u1", "p1"));
    forget_with(&vault).unwrap();
    assert!(load_with(&vault).unwrap().is_none());
}
