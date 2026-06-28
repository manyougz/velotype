//! A pill-shaped toggle switch component with slide animation.

use std::time::Duration;

use gpui::{prelude::FluentBuilder, *};

use crate::theme::ThemeManager;

/// A toggle switch that can be checked or unchecked.
#[derive(IntoElement)]
pub(crate) struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            checked: false,
            disabled: false,
            on_click: None,
        }
    }

    /// Set the checked state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let c = &theme.colors;

        let checked = self.checked;
        let disabled = self.disabled;

        let track_color = if disabled {
            c.dialog_secondary_button_bg
        } else if checked {
            c.dialog_primary_button_bg
        } else {
            c.dialog_secondary_button_bg
        };
        let thumb_color = if disabled {
            c.dialog_secondary_button_text
        } else if checked {
            c.dialog_primary_button_text
        } else {
            c.dialog_secondary_button_text
        };

        // Keep the visual position across renders so we can detect changes.
        let toggle_state = window.use_keyed_state::<bool>(self.id.clone(), cx, |_, _| checked);
        let prev_checked = *toggle_state.read(cx);
        let target: f32 = if checked { 16.0 } else { 0.0 };
        let origin: f32 = if prev_checked { 16.0 } else { 0.0 };
        let needs_animation = prev_checked != checked;
        let duration = Duration::from_secs_f64(0.18);

        if needs_animation {
            cx.spawn({
                let toggle_state = toggle_state.clone();
                async move |cx| {
                    cx.background_executor().timer(duration).await;
                    _ = toggle_state.update(cx, |state, _| *state = checked);
                }
            })
            .detach();
        }

        let thumb = div()
            .w(px(16.0))
            .h(px(16.0))
            .rounded(px(8.0))
            .bg(thumb_color)
            .map(|mut this| {
                if needs_animation {
                    this.with_animation(
                        ElementId::NamedInteger("switch-move".into(), checked as u64),
                        Animation::new(duration),
                        move |mut this, delta| {
                            let margin = origin + (target - origin) * delta;
                            this.style().margin.left =
                                Some(Length::Definite(DefiniteLength::from(px(margin))));
                            this
                        },
                    )
                    .into_any_element()
                } else {
                    this.style().margin.left =
                        Some(Length::Definite(DefiniteLength::from(px(target))));
                    this.into_any_element()
                }
            });

        div()
            .id(self.id)
            .w(px(36.0))
            .h(px(20.0))
            .px(px(2.0))
            .flex()
            .items_center()
            .rounded(px(10.0))
            .bg(track_color)
            .when(!disabled, |this| this.cursor_pointer())
            .child(thumb)
            .when_some(self.on_click, |this, on_click| this.on_click(on_click))
    }
}
