//! GPUI desktop interface for Legenda.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, PathPromptOptions, Pixels, Point, ShapedLine, SharedString, Style, TextRun, Timer,
    TitlebarOptions, UTF16Selection, UnderlineStyle, Window, WindowBounds, WindowOptions, actions,
    div, fill, hsla, point, prelude::*, px, relative, rgb, rgba, size,
};
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::credentials::{ACCOUNT, SERVICE, decode_credentials, encode_credentials};
use crate::service::{ProviderKind, SubtitleResult, download as download_subtitle, search as search_subtitles};

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

/// Single-line interactive text input component built on GPUI's `EntityInputHandler`.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    masked: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    /// Creates a new [`TextInput`] with initial text, placeholder, and optional password masking.
    pub fn new(
        cx: &mut Context<Self>,
        initial: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        masked: bool,
    ) -> Self {
        let content: SharedString = initial.into();
        let end = content.len();
        Self {
            focus_handle: cx.focus_handle(),
            content,
            placeholder: placeholder.into(),
            masked,
            selected_range: end..end,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    /// Returns the current text content.
    pub fn text(&self) -> &str {
        self.content.as_ref()
    }

    /// Replaces the text content and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn display_text(&self) -> SharedString {
        if self.masked && !self.content.is_empty() {
            "•".repeat(self.content.graphemes(true).count()).into()
        } else {
            self.content.clone()
        }
    }

    fn display_offset_for_byte(&self, byte_offset: usize) -> usize {
        if !self.masked || self.content.is_empty() {
            return byte_offset.min(self.content.len());
        }
        let clamped = byte_offset.min(self.content.len());
        let graphemes = self.content[..clamped].graphemes(true).count();
        graphemes * "•".len()
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let display_idx = line.closest_index_for_x(position.x - bounds.left());
        if self.masked {
            let bullet_count = display_idx / "•".len();
            self.content
                .grapheme_indices(true)
                .nth(bullet_count)
                .map(|(i, _)| i)
                .unwrap_or(self.content.len())
        } else {
            display_idx.min(self.content.len())
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start_disp = self.display_offset_for_byte(range.start);
        let end_disp = self.display_offset_for_byte(range.end);
        Some(Bounds::from_corners(
            point(bounds.left() + last_layout.x_for_index(start_disp), bounds.top()),
            point(bounds.left() + last_layout.x_for_index(end_disp), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index.min(self.content.len())))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.display_offset_for_byte(input.cursor_offset());
        let sel_start = input.display_offset_for_byte(selected_range.start);
        let sel_end = input.display_offset_for_byte(selected_range.end);
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), rgb(0x767882).into())
        } else {
            (input.display_text(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref()
            && !input.masked
        {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0xded8ee),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + line.x_for_index(sel_start), bounds.top()),
                        point(bounds.left() + line.x_for_index(sel_end), bounds.bottom()),
                    ),
                    rgba(0xded8ee38),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let border_color = if is_focused {
            rgb(0xded8ee)
        } else {
            rgb(0x35373d)
        };

        div()
            .flex()
            .items_center()
            .w_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .bg(rgb(0x1b1c20))
            .border_1()
            .border_color(border_color)
            .rounded(px(9.0))
            .px(px(14.0))
            .py(px(10.0))
            .line_height(px(20.0))
            .text_size(px(14.0))
            .text_color(rgb(0xececf0))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Main GPUI root view for the Legenda desktop application.
pub struct LegendaView {
    pub query: Entity<TextInput>,
    pub language: Entity<TextInput>,
    pub key: Entity<TextInput>,
    pub user: Entity<TextInput>,
    pub password: Entity<TextInput>,
    pub video: Option<PathBuf>,
    pub search_video: Option<PathBuf>,
    pub search_language: String,
    pub results: Vec<SubtitleResult>,
    pub selected_index: Option<usize>,
    pub busy: bool,
    pub status: SharedString,
    pub show_account: bool,
    pub account_status: SharedString,
    focus_handle: FocusHandle,
}

impl LegendaView {
    /// Creates a new [`LegendaView`], loading initial environment variables and keychain credentials.
    pub fn new(cx: &mut Context<Self>, smoke: bool) -> Self {
        let initial_key = std::env::var("OPENSUBTITLES_API_KEY").unwrap_or_default();
        let initial_user = std::env::var("OPENSUBTITLES_USERNAME").unwrap_or_default();

        let query = cx.new(|cx| TextInput::new(cx, "", "Nome do filme ou série", false));
        let language = cx.new(|cx| TextInput::new(cx, "pt-br", "pt-br", false));
        let key = cx.new(|cx| TextInput::new(cx, initial_key, "Chave API", true));
        let user = cx.new(|cx| TextInput::new(cx, initial_user, "Usuário", false));
        let password = cx.new(|cx| TextInput::new(cx, "", "Senha", true));

        let view = Self {
            query,
            language,
            key,
            user,
            password,
            video: None,
            search_video: None,
            search_language: "pt-br".to_string(),
            results: Vec::new(),
            selected_index: None,
            busy: false,
            status: "Digite um título para começar.".into(),
            show_account: false,
            account_status: "Salve para lembrar a conta na próxima abertura.".into(),
            focus_handle: cx.focus_handle(),
        };

        if !smoke {
            let read_task = cx.read_credentials(SERVICE);
            cx.spawn(async move |this, cx| {
                match read_task.await {
                    Ok(Some((_, bytes))) => {
                        if let Ok(raw) = String::from_utf8(bytes)
                            && let Ok(Some(creds)) = decode_credentials(&raw)
                        {
                            let _ = this.update(cx, |this, cx| {
                                this.key.update(cx, |k, cx| k.set_text(creds.key, cx));
                                this.user.update(cx, |u, cx| u.set_text(creds.user, cx));
                                this.password
                                    .update(cx, |p, cx| p.set_text(creds.password, cx));
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        let _ = this.update(cx, |this, cx| {
                            this.status =
                                "Cofre indisponível. Você pode informar as credenciais em Conta."
                                    .into();
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        }

        view
    }

    /// Links a local video file and populates the search query from its filename stem.
    pub fn set_video(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let normalized = stem.replace('.', " ");
            self.query.update(cx, |q, cx| q.set_text(normalized, cx));
        }
        self.video = Some(path);
        cx.notify();
    }

    /// Removes the currently linked video file.
    pub fn clear_video(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.video = None;
            cx.notify();
        }
    }

    /// Opens the native platform file picker to choose a local video file.
    pub fn choose_video(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Selecionar vídeo".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                match result {
                    Ok(Some(mut paths)) => {
                        if let Some(first) = paths.pop() {
                            let _ = this.update(cx, |this, cx| {
                                this.set_video(first, cx);
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        let _ = this.update(cx, |this, cx| {
                            this.status = err.to_string().into();
                            cx.notify();
                        });
                    }
                }
            }
        })
        .detach();
    }

    /// Opens the OpenSubtitles account settings modal.
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.show_account = true;
        cx.notify();
    }

    /// Closes the OpenSubtitles account settings modal.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.show_account = false;
        cx.notify();
    }

    /// Saves the current API key, username, and password to the system credential vault.
    pub fn save_credentials(&mut self, cx: &mut Context<Self>) {
        let key_val = self.key.read(cx).text().trim().to_string();
        let user_val = self.user.read(cx).text().trim().to_string();
        let pass_val = self.password.read(cx).text().to_string();

        if key_val.is_empty() || user_val.is_empty() || pass_val.is_empty() {
            self.account_status = "Preencha chave API, usuário e senha.".into();
            cx.notify();
            return;
        }

        let payload = encode_credentials(&key_val, &user_val, &pass_val);
        let write_task = cx.write_credentials(SERVICE, ACCOUNT, payload.as_bytes());
        cx.spawn(async move |this, cx| {
            let outcome = write_task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(()) => {
                        this.account_status = "Credenciais salvas no cofre do sistema.".into();
                    }
                    Err(_) => {
                        this.account_status = "Não foi possível salvar. Desbloqueie o cofre de credenciais do sistema e tente novamente.".into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Deletes stored credentials from the system vault and clears the current session fields.
    pub fn forget_credentials(&mut self, cx: &mut Context<Self>) {
        let delete_task = cx.delete_credentials(SERVICE);
        cx.spawn(async move |this, cx| {
            let outcome = delete_task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(()) => {
                        this.key.update(cx, |k, cx| k.set_text("", cx));
                        this.user.update(cx, |u, cx| u.set_text("", cx));
                        this.password.update(cx, |p, cx| p.set_text("", cx));
                        this.account_status =
                            "Credenciais removidas do cofre e desta sessão.".into();
                    }
                    Err(_) => {
                        this.account_status =
                            "Não foi possível remover as credenciais. Confira o cofre do sistema."
                                .into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Validates search inputs and returns `(title, video, language, key)` if valid.
    pub fn validate_search(&mut self, cx: &mut Context<Self>) -> Option<(String, Option<PathBuf>, String, String)> {
        if self.busy {
            return None;
        }
        let title = self.query.read(cx).text().trim().to_string();
        let lang = self.language.read(cx).text().trim().to_string();
        let key = self.key.read(cx).text().trim().to_string();

        if (title.is_empty() && self.video.is_none()) || !is_valid_language(&lang) {
            self.status = "Informe título ou vídeo e um idioma válido.".into();
            cx.notify();
            return None;
        }

        let video = self.video.clone();
        self.results.clear();
        self.selected_index = None;
        Some((title, video, lang, key))
    }

    /// Applies completed search results to the view state.
    pub fn apply_search_results(
        &mut self,
        video: Option<PathBuf>,
        lang: String,
        rows: Vec<SubtitleResult>,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        self.search_video = video;
        self.search_language = lang;
        let count = rows.len();
        self.selected_index = if count > 0 { Some(0) } else { None };
        self.results = rows;
        self.status = if count > 0 {
            format!("{count} arquivos encontrados. Selecione uma versão.").into()
        } else {
            "Nenhum resultado. Tente outro título ou idioma.".into()
        };
        cx.notify();
    }

    /// Runs a subtitle search in the background executor.
    pub fn search(&mut self, cx: &mut Context<Self>) {
        let Some((title, video, lang, key)) = self.validate_search(cx) else {
            return;
        };

        self.busy = true;
        self.status = "Aguarde…".into();
        cx.notify();

        let video_for_task = video.clone();
        let lang_for_task = lang.clone();
        let bg_task = cx.background_executor().spawn(async move {
            search_subtitles(&title, video_for_task.as_deref(), &lang_for_task, &key)
        });

        cx.spawn(async move |this, cx| {
            let outcome = bg_task.await;
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(rows) => this.apply_search_results(video, lang, rows, cx),
                Err(err) => {
                    this.busy = false;
                    this.status = err.into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Downloads the currently selected subtitle result.
    pub fn download(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(idx) = self.selected_index else {
            return;
        };
        let Some(selected) = self.results.get(idx).cloned() else {
            return;
        };

        let key = self.key.read(cx).text().to_string();
        let user = self.user.read(cx).text().to_string();
        let password = self.password.read(cx).text().to_string();

        if selected.provider == ProviderKind::OpenSubtitles
            && (key.is_empty() || user.is_empty() || password.is_empty())
        {
            self.status = "Para esta fonte, preencha chave, usuário e senha em Conta.".into();
            cx.notify();
            return;
        }

        if let Some(video_path) = &self.search_video {
            let target = video_path.with_extension(format!("{}.srt", self.search_language));
            self.save_to_path(selected, target, key, user, password, cx);
        } else {
            let suggested = format!("legenda.{}.srt", self.search_language);
            let initial_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let rx = cx.prompt_for_new_path(&initial_dir, Some(&suggested));
            cx.spawn(async move |this, cx| {
                if let Ok(result) = rx.await {
                    match result {
                        Ok(Some(path)) => {
                            let _ = this.update(cx, |this, cx| {
                                this.save_to_path(selected, path, key, user, password, cx);
                            });
                        }
                        Ok(None) => {}
                        Err(err) => {
                            let _ = this.update(cx, |this, cx| {
                                this.status = err.to_string().into();
                                cx.notify();
                            });
                        }
                    }
                }
            })
            .detach();
        }
    }

    fn save_to_path(
        &mut self,
        selected: SubtitleResult,
        path: PathBuf,
        key: String,
        user: String,
        password: String,
        cx: &mut Context<Self>,
    ) {
        if path.exists() {
            self.status = "O arquivo já existe. Escolha outro nome.".into();
            cx.notify();
            return;
        }

        self.busy = true;
        self.status = "Aguarde…".into();
        cx.notify();

        let bg_task = cx.background_executor().spawn(async move {
            download_subtitle(&selected, &path, &key, &user, &password)
        });

        cx.spawn(async move |this, cx| {
            let outcome = bg_task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match outcome {
                    Ok(saved) => {
                        this.status = format!("Salva: {}", saved.display()).into();
                    }
                    Err(err) => {
                        this.status = err.into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_button(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (bg_color, text_color, border_color, hover_bg, hover_border) = if primary {
            (
                rgb(0xded8ee),
                rgb(0x24212c),
                rgb(0xded8ee),
                rgb(0xeee9f8),
                rgb(0xeee9f8),
            )
        } else {
            (
                rgb(0x1d1f23),
                rgb(0xececf0),
                rgb(0x35373d),
                rgb(0x292b31),
                rgb(0x484a52),
            )
        };

        div()
            .id(id)
            .flex_none()
            .cursor_pointer()
            .rounded(px(9.0))
            .border_1()
            .border_color(border_color)
            .bg(bg_color)
            .text_color(text_color)
            .px(px(14.0))
            .py(px(10.0))
            .text_size(px(14.0))
            .hover(move |style| style.bg(hover_bg).border_color(hover_border))
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            .child(label.into())
    }

    fn render_account_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("account-backdrop")
            .absolute()
            .inset_0()
            .bg(hsla(0.0, 0.0, 0.0, 0.65))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(440.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(0x292b30))
                    .bg(rgb(0x17181b))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(rgb(0xececf0))
                            .mb(px(4.0))
                            .child("Conta OpenSubtitles"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xececf0))
                            .child("Chave API"),
                    )
                    .child(self.key.clone())
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xececf0))
                            .child("Usuário"),
                    )
                    .child(self.user.clone())
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xececf0))
                            .child("Senha"),
                    )
                    .child(self.password.clone())
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa2a4ad))
                            .child(self.account_status.clone()),
                    )
                    .child(Self::render_button(
                        "btn-save-credentials",
                        "Salvar no cofre",
                        true,
                        |this, _, cx| this.save_credentials(cx),
                        cx,
                    ))
                    .child(Self::render_button(
                        "btn-forget-credentials",
                        "Esquecer credenciais salvas",
                        false,
                        |this, _, cx| this.forget_credentials(cx),
                        cx,
                    ))
                    .child(Self::render_button(
                        "btn-close-account",
                        "Concluído",
                        false,
                        |this, _, cx| this.close_settings(cx),
                        cx,
                    )),
            )
    }
}

impl Focusable for LegendaView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LegendaView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let video_text: SharedString = self
            .video
            .as_ref()
            .map(|p| p.display().to_string().into())
            .unwrap_or_else(|| "Nenhum vídeo vinculado".into());

        let selected_index = self.selected_index;
        let result_rows: Vec<_> = self.results.iter().enumerate().map(|(ix, item)| {
            let is_selected = selected_index == Some(ix);
            let row_bg = if is_selected {
                rgb(0x302d39)
            } else {
                rgb(0x17181b)
            };
            let title_text: SharedString = if item.file_name.is_empty() {
                "Legenda".into()
            } else {
                item.file_name.clone().into()
            };
            let meta_text: SharedString = format!(
                "{} · {} · {} downloads",
                item.source, item.language, item.download_count
            )
            .into();

            div()
                .id(("subtitle-row", ix))
                .w_full()
                .cursor_pointer()
                .px(px(14.0))
                .py(px(12.0))
                .bg(row_bg)
                .border_b_1()
                .border_color(rgb(0x292b30))
                .hover(move |style| {
                    if is_selected {
                        style.bg(rgb(0x302d39))
                    } else {
                        style.bg(rgb(0x202227))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected_index = Some(ix);
                    cx.notify();
                }))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(0xececf0))
                        .child(title_text),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(0xa2a4ad))
                        .child(meta_text),
                )
        }).collect();

        div()
            .size_full()
            .relative()
            .bg(rgb(0x17181b))
            .text_color(rgb(0xececf0))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _: &Submit, _, cx| {
                if !this.show_account {
                    this.search(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                if this.show_account {
                    this.close_settings(cx);
                }
            }))
            .flex()
            .flex_col()
            .child(
                // HeaderBar
                div()
                    .w_full()
                    .px(px(24.0))
                    .py(px(12.0))
                    .bg(rgb(0x17181b))
                    .border_b_1()
                    .border_color(rgb(0x292b30))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(rgb(0xececf0))
                            .child("LEGENDA"),
                    )
                    .child(Self::render_button(
                        "btn-account",
                        "Conta",
                        false,
                        |this, _, cx| this.open_settings(cx),
                        cx,
                    )),
            )
            .child(
                // Main Body
                div()
                    .flex_1()
                    .w_full()
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .overflow_hidden()
                    .child(self.query.clone())
                    .child(
                        // Action bar
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(div().w(px(108.0)).child(self.language.clone()))
                            .child(Self::render_button(
                                "btn-choose-video",
                                "+ Vincular vídeo",
                                false,
                                |this, _, cx| this.choose_video(cx),
                                cx,
                            ))
                            .child(Self::render_button(
                                "btn-clear-video",
                                "Remover vídeo",
                                false,
                                |this, _, cx| this.clear_video(cx),
                                cx,
                            ))
                            .child(Self::render_button(
                                "btn-search",
                                "Buscar legendas →",
                                true,
                                |this, _, cx| this.search(cx),
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa2a4ad))
                            .child(video_text),
                    )
                    .child(
                        // Scrollable Results List
                        div()
                            .id("results-scroll")
                            .flex_1()
                            .w_full()
                            .overflow_y_scroll()
                            .rounded(px(9.0))
                            .border_1()
                            .border_color(rgb(0x292b30))
                            .flex()
                            .flex_col()
                            .children(result_rows),
                    )
                    .child(Self::render_button(
                        "btn-download",
                        "Baixar selecionada ↓",
                        false,
                        |this, _, cx| this.download(cx),
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa2a4ad))
                            .child(self.status.clone()),
                    ),
            )
            .when(self.show_account, |this| {
                this.child(self.render_account_modal(cx))
            })
    }
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
}
