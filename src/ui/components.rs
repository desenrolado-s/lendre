//! Reusable UI sections and component builders for [`LegendaView`].

use gpui::{Context, ElementId, IntoElement, SharedString, Window, div, prelude::*, px, rgb};

use crate::service::SubtitleResult;

use super::{theme, view::LegendaView};

impl LegendaView {
    pub(crate) fn render_button(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (bg_color, text_color, border_color, hover_bg, hover_border) = if primary {
            (
                rgb(theme::ACCENT),
                rgb(theme::ACCENT_TEXT),
                rgb(theme::ACCENT),
                rgb(theme::ACCENT_HOVER),
                rgb(theme::ACCENT_HOVER),
            )
        } else {
            (
                rgb(theme::BG_BUTTON),
                rgb(theme::TEXT_PRIMARY),
                rgb(theme::BORDER_DEFAULT),
                rgb(theme::BG_BUTTON_HOVER),
                rgb(theme::BORDER_HOVER),
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

    pub(crate) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .w_full()
            .px(px(24.0))
            .py(px(12.0))
            .bg(rgb(theme::BG_APP))
            .border_b_1()
            .border_color(rgb(theme::BORDER_SUBTLE))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(15.0))
                    .text_color(rgb(theme::TEXT_PRIMARY))
                    .child("LEGENDA"),
            )
            .child(Self::render_button(
                "btn-account",
                "Conta",
                false,
                |this, _, cx| this.open_settings(cx),
                cx,
            ))
    }

    pub(crate) fn render_action_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            ))
    }

    pub(crate) fn render_result_row(
        ix: usize,
        item: &SubtitleResult,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let row_bg = if is_selected {
            rgb(theme::BG_ROW_SELECTED)
        } else {
            rgb(theme::BG_APP)
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
            .border_color(rgb(theme::BORDER_SUBTLE))
            .hover(move |style| {
                if is_selected {
                    style.bg(rgb(theme::BG_ROW_SELECTED))
                } else {
                    style.bg(rgb(theme::BG_ROW_HOVER))
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
                    .text_color(rgb(theme::TEXT_PRIMARY))
                    .child(title_text),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(meta_text),
            )
    }

    pub(crate) fn render_results_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let selected_index = self.selected_index;
        let result_rows: Vec<_> = self
            .results
            .iter()
            .enumerate()
            .map(|(ix, item)| Self::render_result_row(ix, item, selected_index == Some(ix), cx))
            .collect();

        div()
            .id("results-scroll")
            .flex_1()
            .w_full()
            .overflow_y_scroll()
            .rounded(px(9.0))
            .border_1()
            .border_color(rgb(theme::BORDER_SUBTLE))
            .flex()
            .flex_col()
            .children(result_rows)
    }

    pub(crate) fn render_account_modal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("account-backdrop")
            .absolute()
            .inset_0()
            .bg(theme::modal_backdrop())
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(440.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(theme::BORDER_SUBTLE))
                    .bg(rgb(theme::BG_APP))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(rgb(theme::TEXT_PRIMARY))
                            .mb(px(4.0))
                            .child("Conta OpenSubtitles"),
                    )
                    .child(Self::render_field_label("Chave API"))
                    .child(self.key.clone())
                    .child(Self::render_field_label("Usuário"))
                    .child(self.user.clone())
                    .child(Self::render_field_label("Senha"))
                    .child(self.password.clone())
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(theme::TEXT_MUTED))
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

    fn render_field_label(label: &'static str) -> impl IntoElement {
        div()
            .text_size(px(13.0))
            .text_color(rgb(theme::TEXT_PRIMARY))
            .child(label)
    }
}
