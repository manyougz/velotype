//! A pill-shaped toggle switch component.

use gpui::*;

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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::prelude::FluentBuilder as _;

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
        // Track: 36×20. px(2) leaves 32px of inner width.
        // Thumb: 16×16. When unchecked: ml=0 (2px from left edge).
        // When checked: ml=16 (2px from right edge).
        let thumb_margin: f32 = if checked { 16.0 } else { 0.0 };

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
            .child(
                div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .ml(px(thumb_margin))
                    .rounded(px(8.0))
                    .bg(thumb_color),
            )
            .when_some(self.on_click, |this, on_click| this.on_click(on_click))
    }
}
