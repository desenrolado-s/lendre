//! Video identification and atomic subtitle storage.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::bytes::Regex;
use tempfile::Builder;

/// Maximum valid subtitle size (10 MiB).
pub const MAX_SUBTITLE_BYTES: usize = 10 * 1024 * 1024;

/// Minimum video file size for OpenSubtitles hash calculation (128 KiB).
pub const MIN_VIDEO_HASH_BYTES: u64 = 131_072;

const HASH_CHUNK_BYTES: usize = 65_536;

static SRT_TIMECODE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\d{2}:\d{2}:\d{2}[,.]\d{3}\s*-->\s*\d{2}:\d{2}:\d{2}[,.]\d{3}")
        .expect("valid SRT timecode regex")
});

/// Errors that can occur when validating and saving a subtitle file.
#[derive(Debug)]
pub enum SaveError {
    /// Content exceeds 10 MiB or does not contain a valid SRT timecode arrow.
    InvalidSrt,
    /// Target file already exists; existing files are never overwritten.
    AlreadyExists,
    /// Underlying filesystem error.
    Io(io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSrt => write!(f, "Resposta não parece uma legenda SRT válida."),
            Self::AlreadyExists => write!(f, "O arquivo já existe. Escolha outro nome."),
            Self::Io(err) => write!(f, "Erro ao salvar a legenda: {err}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<io::Error> for SaveError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::AlreadyExists {
            Self::AlreadyExists
        } else {
            Self::Io(err)
        }
    }
}

/// Computes the 64-bit OpenSubtitles movie hash for a video file.
///
/// Returns `Ok(None)` when the file is smaller than 128 KiB.
pub fn movie_hash(path: &Path) -> io::Result<Option<String>> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size < MIN_VIDEO_HASH_BYTES {
        return Ok(None);
    }

    let mut total = size;
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];

    for offset in [0, size - HASH_CHUNK_BYTES as u64] {
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buf)?;
        for chunk in buf.chunks_exact(8) {
            let word = u64::from_le_bytes(chunk.try_into().expect("8-byte slice"));
            total = total.wrapping_add(word);
        }
    }

    Ok(Some(format!("{total:016x}")))
}

/// Validates that `content` is a well-formed SRT payload and atomically publishes
/// it to `target` without overwriting existing files.
pub fn save_subtitle(target: &Path, content: &[u8]) -> Result<PathBuf, SaveError> {
    if content.len() > MAX_SUBTITLE_BYTES || !SRT_TIMECODE_REGEX.is_match(content) {
        return Err(SaveError::InvalidSrt);
    }

    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut named_temp = Builder::new().prefix(".legenda-").tempfile_in(parent)?;
    named_temp.write_all(content)?;
    named_temp.flush()?;
    named_temp.as_file().sync_all()?;

    let temp_path = named_temp.into_temp_path();
    let link_result = fs::hard_link(&temp_path, target);
    // Explicitly remove the temporary path regardless of hard_link outcome.
    let _ = temp_path.close();

    link_result?;
    Ok(target.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const SRT: &[u8] = b"1\n00:00:01,000 --> 00:00:02,000\nOla!\n";

    #[test]
    fn test_save_and_no_overwrite() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("movie.pt-br.srt");

        save_subtitle(&path, SRT).unwrap();
        assert_eq!(fs::read(&path).unwrap(), SRT);

        let err = save_subtitle(&path, b"1\n00:00:01,000 --> 00:00:02,000\nOla!changed\n")
            .unwrap_err();
        assert!(matches!(err, SaveError::AlreadyExists));
        assert_eq!(fs::read(&path).unwrap(), SRT);

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries, vec!["movie.pt-br.srt".to_string()]);
    }

    #[test]
    fn test_reject_html() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("movie.srt");

        let err = save_subtitle(&path, b"<html><!-- error -->Unavailable</html>").unwrap_err();
        assert!(matches!(err, SaveError::InvalidSrt));
        assert_eq!(err.to_string(), "Resposta não parece uma legenda SRT válida.");
        assert!(!path.exists());
    }

    #[test]
    fn test_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("video");

        fs::write(&path, vec![0u8; 131_072]).unwrap();
        assert_eq!(movie_hash(&path).unwrap(), Some("0000000000020000".to_string()));

        fs::write(&path, b"tiny").unwrap();
        assert_eq!(movie_hash(&path).unwrap(), None);
    }
}
