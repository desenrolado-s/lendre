<div align="center">

# Legenda

**Fast, GPU-accelerated subtitle downloader for desktop, built in Rust with GPUI.**

[![Build and test](https://github.com/desenrolado-s/Subtitle-Downloader/actions/workflows/build.yml/badge.svg)](https://github.com/desenrolado-s/Subtitle-Downloader/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/desenrolado-s/Subtitle-Downloader?label=release)](https://github.com/desenrolado-s/Subtitle-Downloader/releases/latest)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![GPUI](https://img.shields.io/badge/GPUI-0.2-084CCF)](https://gpui.rs/)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-3776AB)](#requirements)

</div>

---

**Legenda** is a lightweight desktop app for Linux, macOS, and Windows that searches and downloads subtitles from OpenSubtitles and public providers. Powered by [GPUI](https://gpui.rs/) (the GPU-accelerated UI framework behind Zed), it delivers instant responsiveness with minimal resource usage.

## Features

- **Hash & Title Search**: Match local video files by 64-bit hash or search by title.
- **Multiple Providers**: Supports OpenSubtitles plus public sources (no API key required): Gestdown/Addic7ed, Subtitulamos, Podnapisi, and Kitsunekko/AJATT (anime).
- **Multi-language**: Filter by `pt-br`, `pt`, `en`, `es`, `ja`, and more.
- **Safe Downloads**: Automatic SRT validation, atomic writes without accidental overwriting, and auto-naming next to linked video files.
- **Secure Credentials**: OpenSubtitles account credentials are encrypted in your OS keychain/vault.
- **Fast & Native**: GPU-accelerated dark theme powered by GPUI.

## Installation

### Pre-built Binaries

Download the latest binary or AppImage for Linux, macOS, or Windows from [Releases](https://github.com/desenrolado-s/Subtitle-Downloader/releases/latest).

### Building from Source

**Requirements:**
- Rust 1.88+ (`cargo` and `rustc`)
- On Linux, install development packages:
  - **Arch Linux**: `sudo pacman -S base-devel pkgconf libxcb libxkbcommon wayland fontconfig freetype2 vulkan-icd-loader`
  - **Ubuntu / Debian**: `sudo apt install build-essential pkg-config libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libfontconfig1-dev libfreetype6-dev libssl-dev`
  - **Fedora**: `sudo dnf install gcc pkgconf-pkg-config libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel wayland-devel fontconfig-devel freetype-devel openssl-devel`

Clone and run:

```sh
git clone https://github.com/desenrolado-s/Subtitle-Downloader.git
cd Subtitle-Downloader
cargo run --release
```

## Usage

1. **Enter Title or Link Video**: Type the movie or show title. Optionally link a local video file for automatic hash matching.
2. **Select Language**: Set your target language code (e.g., `pt-br`, `en`, `es`, `ja`).
3. **Search & Download**: Press `Enter` or click **Search subtitles**. Select a result and click **Download selected**.
   - If a video file was linked, the `.srt` is saved next to it automatically.
   - Otherwise, a native Save dialog will ask where to save.

## Subtitle Providers

| Provider | Type | Description |
| --- | --- | --- |
| **OpenSubtitles** | Account / API | Full movie & TV catalog. Configure API key and account in the **Account** dialog. |
| **Gestdown** | Public | TV show subtitles from Addic7ed (matched by episode patterns like `S01E02`). |
| **Subtitulamos** | Public | TV show subtitles in English and Portuguese. |
| **Podnapisi** | Public | Multilingual movies and series (`en`, `pt`, `ja`, `es`, `fr`, etc.). |
| **Kitsunekko / AJATT** | Public | Japanese subtitles (`ja`) for anime. |

> Public providers work out of the box without any account or API key.

## Development

Run tests:

```sh
cargo test
```

Run smoke test:

```sh
cargo run -- --smoke-test
```

## License

This project is licensed under the [MIT License](LICENSE).
