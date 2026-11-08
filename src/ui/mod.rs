//! GPUI desktop interface for Legenda.

mod components;
mod text_input;
pub mod theme;
mod view;

use std::sync::LazyLock;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, Focusable, KeyBinding, Timer, TitlebarOptions, WindowBounds,
    WindowOptions, actions, prelude::*, px, size,
};
use regex::Regex;

pub use text_input::TextInput;
pub use view::LegendaView;

/// Application ID used for desktop integration and credential storage.
pub const APP_ID: &str = "io.github.desenrolado_s.Legenda";

static LANGUAGE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]{2,3}(?:-[a-z]{2})?$").expect("valid language regex"));

/// Validates a language tag such as `pt-br`, `pt`, `en`, `es`, or `ja`.
pub fn is_valid_language(lang: &str) -> bool {
    LANGUAGE_REGEX.is_match(lang)
}

actions!(
    legenda_ui,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Submit,
        Escape,
        Quit,
    ]
);

/// Registers keyboard shortcuts for text editing and application navigation.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("ctrl-a", SelectAll, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("ctrl-v", Paste, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("ctrl-x", Cut, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("enter", Submit, None),
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}

/// Launches the GPUI desktop application.
///
/// When `smoke` is `true` (`--smoke-test`), the window opens, initializes GPUI rendering,
/// and exits cleanly after a short timer.
pub fn run_app(smoke: bool) {
    Application::new().run(move |cx: &mut App| {
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(1000.0), px(760.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("LEGENDA".into()),
                        ..Default::default()
                    }),
                    app_id: Some(APP_ID.into()),
                    window_min_size: Some(size(px(640.0), px(480.0))),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| LegendaView::new(cx, smoke));
                    let query_focus = view.read(cx).query.read(cx).focus_handle(cx);
                    window.focus(&query_focus);
                    view
                },
            )
            .expect("Não foi possível abrir a janela gráfica GPUI.");

        let _ = window;
        cx.activate(true);

        if smoke {
            cx.spawn(async move |cx| {
                Timer::after(Duration::from_millis(300)).await;
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::text_input::{
        next_grapheme_boundary, offset_from_utf16, offset_to_utf16, previous_grapheme_boundary,
    };
    use super::*;

    #[test]
    fn test_language_validation() {
        assert!(is_valid_language("pt-br"));
        assert!(is_valid_language("pt"));
        assert!(is_valid_language("en"));
        assert!(is_valid_language("ja"));
        assert!(is_valid_language("jpn"));
        assert!(!is_valid_language(""));
        assert!(!is_valid_language("PT-BR"));
        assert!(!is_valid_language("invalid_lang"));
    }

    #[test]
    fn test_utf16_offset_conversions_and_grapheme_boundaries() {
        let text = "Ação 🎬!";
        // 'A' (1 byte, 1 utf16), 'ç' (2 bytes, 1 utf16), 'ã' (2 bytes, 1 utf16),
        // 'o' (1 byte, 1 utf16), ' ' (1 byte, 1 utf16), '🎬' (4 bytes, 2 utf16), '!' (1 byte, 1 utf16)
        assert_eq!(offset_to_utf16(text, 0), 0);
        assert_eq!(offset_to_utf16(text, 1), 1);
        assert_eq!(offset_to_utf16(text, 3), 2);
        assert_eq!(offset_to_utf16(text, 7), 5);
        assert_eq!(offset_to_utf16(text, 11), 7);
        assert_eq!(offset_to_utf16(text, 12), 8);

        assert_eq!(offset_from_utf16(text, 0), 0);
        assert_eq!(offset_from_utf16(text, 2), 3);
        assert_eq!(offset_from_utf16(text, 5), 7);
        assert_eq!(offset_from_utf16(text, 7), 11);
        assert_eq!(offset_from_utf16(text, 8), 12);

        assert_eq!(next_grapheme_boundary(text, 0), 1);
        assert_eq!(next_grapheme_boundary(text, 1), 3);
        assert_eq!(next_grapheme_boundary(text, 7), 11);
        assert_eq!(previous_grapheme_boundary(text, 11), 7);
        assert_eq!(previous_grapheme_boundary(text, 3), 1);
        assert_eq!(previous_grapheme_boundary(text, 0), 0);
    }
}
