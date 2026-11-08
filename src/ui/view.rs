//! Main GPUI root view state, async workflows, and top-level layout for Legenda.

use std::path::PathBuf;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, PathPromptOptions, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, div, prelude::*, px, rgb,
};

use crate::credentials::{ACCOUNT, SERVICE, decode_credentials, encode_credentials};
use crate::service::{
    ProviderKind, SubtitleResult, download as download_subtitle, search as search_subtitles,
};

use super::file_dialog::{
    DialogRequest, default_download_dir, is_portal_missing, run_fallback_dialog, suggested_path,
};
use super::toast::{ToastKind, ToastNotification};
use super::{Escape, Submit, TextInput, is_valid_language, theme};

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
    pub toast: Entity<ToastNotification>,
    pub results_scroll_handle: UniformListScrollHandle,
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
        let toast = cx.new(|_| ToastNotification::new());
        let results_scroll_handle = UniformListScrollHandle::new();

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
            toast,
            results_scroll_handle,
            show_account: false,
            account_status: "Salve para lembrar a conta na próxima abertura.".into(),
            focus_handle: cx.focus_handle(),
        };

        if !smoke {
            Self::spawn_load_credentials(cx);
        }

        view
    }

    /// Shows `message` as a toast notification in the top-right corner.
    ///
    /// The toast dismisses itself after a few seconds unless `persistent` is `true`
    /// (used for in-progress messages that are replaced when the task finishes).
    pub fn show_toast(
        &mut self,
        message: impl Into<SharedString>,
        kind: ToastKind,
        persistent: bool,
        cx: &mut Context<Self>,
    ) {
        self.toast
            .update(cx, |toast, cx| toast.show(message, kind, persistent, cx));
    }

    /// Hides the current toast notification with an upward fade-out animation.
    pub fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast.update(cx, |toast, cx| toast.dismiss(cx));
    }

    fn spawn_load_credentials(cx: &mut Context<Self>) {
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
                        this.show_toast(
                            "Cofre indisponível. Você pode informar as credenciais em Conta.",
                            ToastKind::Error,
                            false,
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
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
    ///
    /// Falls back to `zenity`/`kdialog` when the xdg-desktop-portal FileChooser is missing.
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
                    Err(err) if is_portal_missing(&err.to_string()) => {
                        let _ = this.update(cx, |this, cx| this.choose_video_fallback(cx));
                    }
                    Err(err) => {
                        let _ = this.update(cx, |this, cx| {
                            this.show_toast(err.to_string(), ToastKind::Error, false, cx);
                        });
                    }
                }
            }
        })
        .detach();
    }

    fn choose_video_fallback(&mut self, cx: &mut Context<Self>) {
        let request = DialogRequest::Open {
            title: "Selecionar vídeo".to_string(),
        };
        let bg_task = cx
            .background_executor()
            .spawn(async move { run_fallback_dialog(&request) });
        cx.spawn(async move |this, cx| {
            let outcome = bg_task.await;
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(Some(path)) => this.set_video(path, cx),
                Ok(None) => {}
                Err(err) => this.show_toast(err, ToastKind::Error, false, cx),
            });
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
    pub fn validate_search(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, Option<PathBuf>, String, String)> {
        if self.busy {
            return None;
        }
        let title = self.query.read(cx).text().trim().to_string();
        let lang = self.language.read(cx).text().trim().to_string();
        let key = self.key.read(cx).text().trim().to_string();

        if (title.is_empty() && self.video.is_none()) || !is_valid_language(&lang) {
            self.show_toast(
                "Informe título ou vídeo e um idioma válido.",
                ToastKind::Error,
                false,
                cx,
            );
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
        self.results_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        if count > 0 {
            self.show_toast(
                format!("{count} arquivos encontrados. Selecione uma versão."),
                ToastKind::Success,
                false,
                cx,
            );
        } else {
            self.show_toast(
                "Nenhum resultado. Tente outro título ou idioma.",
                ToastKind::Info,
                false,
                cx,
            );
        }
    }

    /// Runs a subtitle search in the background executor.
    pub fn search(&mut self, cx: &mut Context<Self>) {
        let Some((title, video, lang, key)) = self.validate_search(cx) else {
            return;
        };

        self.busy = true;
        self.show_toast("Buscando legendas…", ToastKind::Info, true, cx);

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
                    this.show_toast(err, ToastKind::Error, false, cx);
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
            self.show_toast("Selecione uma legenda primeiro.", ToastKind::Info, false, cx);
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
            self.show_toast(
                "Para esta fonte, preencha chave, usuário e senha em Conta.",
                ToastKind::Error,
                false,
                cx,
            );
            return;
        }

        if let Some(video_path) = &self.search_video {
            let target = video_path.with_extension(format!("{}.srt", self.search_language));
            self.save_to_path(selected, target, key, user, password, cx);
            return;
        }

        let suggested = format!("legenda.{}.srt", self.search_language);
        let initial_dir = default_download_dir();
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
                    Err(err) if is_portal_missing(&err.to_string()) => {
                        let _ = this.update(cx, |this, cx| {
                            let target = suggested_path(&initial_dir, &suggested);
                            this.save_with_fallback_dialog(selected, target, key, user, password, cx);
                        });
                    }
                    Err(err) => {
                        let _ = this.update(cx, |this, cx| {
                            this.show_toast(err.to_string(), ToastKind::Error, false, cx);
                        });
                    }
                }
            }
        })
        .detach();
    }

    /// Asks for a destination with `zenity`/`kdialog`; if no helper is installed, saves the
    /// subtitle directly to `default_target` (inside the Downloads folder).
    fn save_with_fallback_dialog(
        &mut self,
        selected: SubtitleResult,
        default_target: PathBuf,
        key: String,
        user: String,
        password: String,
        cx: &mut Context<Self>,
    ) {
        let request = DialogRequest::Save {
            title: "Salvar legenda".to_string(),
            suggested: default_target.clone(),
        };
        let bg_task = cx
            .background_executor()
            .spawn(async move { run_fallback_dialog(&request) });
        cx.spawn(async move |this, cx| {
            let outcome = bg_task.await;
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(Some(path)) => this.save_to_path(selected, path, key, user, password, cx),
                Ok(None) => {}
                Err(_) => this.save_to_path(selected, default_target, key, user, password, cx),
            });
        })
        .detach();
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
            self.show_toast(
                format!("O arquivo já existe: {}. Escolha outro nome.", path.display()),
                ToastKind::Error,
                false,
                cx,
            );
            return;
        }

        self.busy = true;
        self.show_toast("Baixando legenda…", ToastKind::Info, true, cx);

        let bg_task = cx.background_executor().spawn(async move {
            download_subtitle(&selected, &path, &key, &user, &password)
        });

        cx.spawn(async move |this, cx| {
            let outcome = bg_task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match outcome {
                    Ok(saved) => this.show_toast(
                        format!("Salva: {}", saved.display()),
                        ToastKind::Success,
                        false,
                        cx,
                    ),
                    Err(err) => this.show_toast(err, ToastKind::Error, false, cx),
                }
            });
        })
        .detach();
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

        div()
            .size_full()
            .relative()
            .bg(rgb(theme::BG_APP))
            .text_color(rgb(theme::TEXT_PRIMARY))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _: &Submit, _, cx| {
                if !this.show_account {
                    this.search(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                if this.show_account {
                    this.close_settings(cx);
                } else if this.toast.read(cx).is_visible() {
                    this.toast.update(cx, |t, cx| t.dismiss(cx));
                }
            }))
            .flex()
            .flex_col()
            .child(self.render_header(cx))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .overflow_hidden()
                    .child(self.query.clone())
                    .child(self.render_action_bar(cx))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(video_text),
                    )
                    .child(self.render_results_list(cx))
                    .child(Self::render_button(
                        "btn-download",
                        "Baixar selecionada ↓",
                        false,
                        |this, _, cx| this.download(cx),
                        cx,
                    )),
            )
            .child(self.toast.clone())
            .when(self.show_account, |this| {
                this.child(self.render_account_modal(cx))
            })
    }
}
