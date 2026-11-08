//! Native file dialog fallbacks for Linux desktops without an xdg-desktop-portal FileChooser.
//!
//! GPUI opens file pickers exclusively through `org.freedesktop.portal.FileChooser`. Many
//! tiling compositors (Hyprland, Sway, niri…) ship a portal without that interface, so GPUI
//! fails with "missing xdg-desktop-portal implementation". In that case we fall back to the
//! `zenity` or `kdialog` helpers, which are commonly installed on Linux desktops.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns `true` when a GPUI file picker error means the portal FileChooser is unavailable.
pub fn is_portal_missing(error: &str) -> bool {
    error.contains("xdg-desktop-portal")
}

/// Helper programs supported as file picker fallbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogTool {
    Zenity,
    Kdialog,
}

impl DialogTool {
    fn program(self) -> &'static str {
        match self {
            DialogTool::Zenity => "zenity",
            DialogTool::Kdialog => "kdialog",
        }
    }
}

/// Which kind of dialog to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogRequest {
    /// Choose an existing file.
    Open { title: String },
    /// Choose a destination path for a new file.
    Save { title: String, suggested: PathBuf },
}

/// Builds the command-line arguments for `tool` and `request`.
pub fn dialog_args(tool: DialogTool, request: &DialogRequest) -> Vec<String> {
    match (tool, request) {
        (DialogTool::Zenity, DialogRequest::Open { title }) => {
            vec!["--file-selection".into(), format!("--title={title}")]
        }
        (DialogTool::Zenity, DialogRequest::Save { title, suggested }) => vec![
            "--file-selection".into(),
            "--save".into(),
            format!("--title={title}"),
            format!("--filename={}", suggested.display()),
        ],
        (DialogTool::Kdialog, DialogRequest::Open { title }) => vec![
            "--title".into(),
            title.clone(),
            "--getopenfilename".into(),
            home_dir().display().to_string(),
        ],
        (DialogTool::Kdialog, DialogRequest::Save { title, suggested }) => vec![
            "--title".into(),
            title.clone(),
            "--getsavefilename".into(),
            suggested.display().to_string(),
        ],
    }
}

/// Interprets the exit status and standard output of a dialog helper.
///
/// Both helpers exit with status `0` and print the chosen path on success, and exit with
/// status `1` when the user cancels.
pub fn parse_dialog_output(exit_code: Option<i32>, stdout: &str) -> Result<Option<PathBuf>, String> {
    match exit_code {
        Some(0) => {
            let path = stdout.lines().next().unwrap_or("").trim();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(PathBuf::from(path)))
            }
        }
        Some(1) => Ok(None),
        Some(code) => Err(format!("O seletor de arquivos encerrou com código {code}.")),
        None => Err("O seletor de arquivos foi interrompido.".to_string()),
    }
}

/// Opens a file dialog using the first available helper (`zenity`, then `kdialog`).
///
/// Returns `Ok(None)` if the user cancels, and an error if no helper is installed.
pub fn run_fallback_dialog(request: &DialogRequest) -> Result<Option<PathBuf>, String> {
    if !cfg!(target_os = "linux") {
        return Err("Seletor de arquivos indisponível neste sistema.".to_string());
    }
    for tool in [DialogTool::Zenity, DialogTool::Kdialog] {
        let output = match Command::new(tool.program())
            .args(dialog_args(tool, request))
            .output()
        {
            Ok(output) => output,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("Não foi possível abrir o seletor de arquivos: {err}")),
        };
        return parse_dialog_output(output.status.code(), &String::from_utf8_lossy(&output.stdout));
    }
    Err(NO_PICKER_MESSAGE.to_string())
}

/// Message shown when neither the portal nor a fallback helper is available.
pub const NO_PICKER_MESSAGE: &str = "Seletor de arquivos indisponível. Instale o zenity ou configure o FileChooser do xdg-desktop-portal.";

/// Default folder used to save subtitles when no file picker can be opened.
pub fn default_download_dir() -> PathBuf {
    let home = home_dir();
    let downloads = home.join("Downloads");
    if downloads.is_dir() { downloads } else { home }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Joins `dir` and `name` into the suggested destination path for a save dialog.
pub fn suggested_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_missing_portal_error() {
        assert!(is_portal_missing(
            "Couldn't open file picker due to missing xdg-desktop-portal implementation."
        ));
        assert!(!is_portal_missing("permission denied"));
    }

    #[test]
    fn test_zenity_arguments() {
        let open = DialogRequest::Open { title: "Selecionar vídeo".into() };
        assert_eq!(
            dialog_args(DialogTool::Zenity, &open),
            vec!["--file-selection", "--title=Selecionar vídeo"]
        );
        let save = DialogRequest::Save {
            title: "Salvar legenda".into(),
            suggested: PathBuf::from("/home/u/Downloads/legenda.pt-br.srt"),
        };
        assert_eq!(
            dialog_args(DialogTool::Zenity, &save),
            vec![
                "--file-selection",
                "--save",
                "--title=Salvar legenda",
                "--filename=/home/u/Downloads/legenda.pt-br.srt",
            ]
        );
    }

    #[test]
    fn test_kdialog_save_arguments() {
        let save = DialogRequest::Save {
            title: "Salvar legenda".into(),
            suggested: PathBuf::from("/tmp/legenda.en.srt"),
        };
        assert_eq!(
            dialog_args(DialogTool::Kdialog, &save),
            vec!["--title", "Salvar legenda", "--getsavefilename", "/tmp/legenda.en.srt"]
        );
    }

    #[test]
    fn test_parse_dialog_output() {
        assert_eq!(
            parse_dialog_output(Some(0), "/home/u/Filme.mkv\n").unwrap(),
            Some(PathBuf::from("/home/u/Filme.mkv"))
        );
        assert_eq!(parse_dialog_output(Some(0), "\n").unwrap(), None);
        assert_eq!(parse_dialog_output(Some(1), "").unwrap(), None);
        assert!(parse_dialog_output(Some(5), "").is_err());
        assert!(parse_dialog_output(None, "").is_err());
    }
}
