//! Isolated toast notification component with smooth fade-in and upward fade-out animations.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, IntoElement, Render, SharedString, Timer,
    Window, div, ease_in_out, prelude::*, px, rgb,
};

use super::theme;

/// Duration the notification remains visible before auto-dismissing.
pub const TOAST_DURATION: Duration = Duration::from_millis(4500);
/// Duration of the upward fade-out exit animation.
pub const TOAST_DISMISS_DURATION: Duration = Duration::from_millis(260);
/// Duration of the subtle entrance animation.
pub const TOAST_ENTER_DURATION: Duration = Duration::from_millis(200);

/// Visual style of a toast notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    /// Neutral information or progress message.
    Info,
    /// A completed action, such as a saved subtitle.
    Success,
    /// A failure the user should notice.
    Error,
}

/// Floating toast notification view with isolated rendering and smooth animations.
pub struct ToastNotification {
    pub status: SharedString,
    pub status_kind: ToastKind,
    pub visible: bool,
    pub dismissing: bool,
    pub generation: u64,
}

impl ToastNotification {
    /// Creates a new inactive [`ToastNotification`].
    pub fn new() -> Self {
        Self {
            status: SharedString::default(),
            status_kind: ToastKind::Info,
            visible: false,
            dismissing: false,
            generation: 0,
        }
    }

    /// Whether a toast is currently visible (including while dismissing).
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Shows `message` as a toast notification in the top-right corner.
    pub fn show(
        &mut self,
        message: impl Into<SharedString>,
        kind: ToastKind,
        persistent: bool,
        cx: &mut Context<Self>,
    ) {
        self.status = message.into();
        self.status_kind = kind;
        self.visible = true;
        self.dismissing = false;
        self.generation = self.generation.wrapping_add(1);
        cx.notify();

        if persistent {
            return;
        }

        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            Timer::after(TOAST_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.dismiss(cx);
                }
            });
        })
        .detach();
    }

    /// Starts the upward fade-out exit animation.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.visible || self.dismissing {
            return;
        }
        self.dismissing = true;
        self.generation = self.generation.wrapping_add(1);
        cx.notify();

        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            Timer::after(TOAST_DISMISS_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.visible = false;
                    this.dismissing = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for ToastNotification {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("toast-hidden").absolute().into_any_element();
        }

        let accent = match self.status_kind {
            ToastKind::Info => rgb(theme::ACCENT),
            ToastKind::Success => rgb(theme::TOAST_SUCCESS),
            ToastKind::Error => rgb(theme::TOAST_ERROR),
        };

        let card = div()
            .id("toast-card")
            .absolute()
            .top(px(72.0))
            .right(px(24.0))
            .w(px(380.0))
            .max_w(px(460.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(theme::BORDER_DEFAULT))
            .bg(rgb(theme::BG_TOAST))
            .shadow_lg()
            .px(px(14.0))
            .py(px(12.0))
            .child(
                div()
                    .flex_none()
                    .w(px(4.0))
                    .h(px(19.0))
                    .rounded(px(2.0))
                    .bg(accent),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .text_size(px(13.5))
                    .line_height(px(19.0))
                    .text_color(rgb(theme::TEXT_PRIMARY))
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .id("toast-close")
                    .flex_none()
                    .cursor_pointer()
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .hover(|style| style.text_color(rgb(theme::TEXT_PRIMARY)))
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx)))
                    .child("✕"),
            );

        if self.dismissing {
            card.with_animation(
                ("toast-exit", self.generation),
                Animation::new(TOAST_DISMISS_DURATION).with_easing(ease_in_out),
                |el, delta| {
                    let offset = delta * 24.0;
                    el.opacity((1.0 - delta).max(0.0)).top(px(72.0 - offset))
                },
            )
            .into_any_element()
        } else {
            card.with_animation(
                ("toast-enter", self.generation),
                Animation::new(TOAST_ENTER_DURATION).with_easing(ease_in_out),
                |el, delta| {
                    let offset = (1.0 - delta) * 16.0;
                    el.opacity(delta).top(px(72.0 - offset))
                },
            )
            .into_any_element()
        }
    }
}
