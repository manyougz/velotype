//! Editor window rendering: centered scrollable block column,
//! unsaved-changes overlay dialog, custom scrollbar, and deferred
//! operations (focus, scroll, save, window title).

use std::time::Instant;

use gpui::*;

use super::{Editor, InfoDialogKind};
use crate::app_menu::dispatch_menu_action_for_editor;
use crate::components::Block;
use crate::components::CalloutVariant;
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::{Theme, ThemeDimensions, ThemeManager};

pub(crate) const ABOUT_GITHUB_URL: &str = "https://github.com/manyougz/velotype";

pub(crate) fn open_about_github_url(cx: &mut App) {
    cx.open_url(ABOUT_GITHUB_URL);
}

/// Adjacent-row metadata used to collapse spacing inside visual groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderedRowSpacingInfo {
    quote_group_anchor: Option<uuid::Uuid>,
    visible_quote_group_anchor: Option<uuid::Uuid>,
    callout_anchor: Option<uuid::Uuid>,
    callout_variant: Option<CalloutVariant>,
    is_callout_header: bool,
    footnote_anchor: Option<uuid::Uuid>,
    is_footnote_header: bool,
}

impl RenderedRowSpacingInfo {
    fn from_block(block: &Block) -> Self {
        Self {
            quote_group_anchor: block.quote_group_anchor,
            visible_quote_group_anchor: block.visible_quote_group_anchor,
            callout_anchor: block.callout_anchor,
            callout_variant: block.callout_variant,
            is_callout_header: block.kind().is_callout(),
            footnote_anchor: block.footnote_anchor,
            is_footnote_header: block.kind().is_footnote_definition(),
        }
    }
}

fn rendered_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    default_gap: f32,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.quote_group_anchor.is_some()
        && previous.quote_group_anchor == current.quote_group_anchor
    {
        0.0
    } else {
        default_gap
    }
}

fn callout_colors(variant: CalloutVariant, theme: &Theme) -> (Hsla, Hsla) {
    let c = &theme.colors;
    match variant {
        CalloutVariant::Note => (c.callout_note_border, c.callout_note_bg),
        CalloutVariant::Tip => (c.callout_tip_border, c.callout_tip_bg),
        CalloutVariant::Important => (c.callout_important_border, c.callout_important_bg),
        CalloutVariant::Warning => (c.callout_warning_border, c.callout_warning_bg),
        CalloutVariant::Caution => (c.callout_caution_border, c.callout_caution_bg),
    }
}

fn callout_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    dimensions: &ThemeDimensions,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.visible_quote_group_anchor.is_some()
        && previous.visible_quote_group_anchor == current.visible_quote_group_anchor
    {
        return 0.0;
    }

    if previous.is_callout_header {
        dimensions.callout_header_margin_bottom
    } else {
        dimensions.callout_body_gap
    }
}

fn footnote_row_top_gap(previous: Option<RenderedRowSpacingInfo>, default_gap: f32) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.is_footnote_header {
        default_gap * 0.75
    } else {
        default_gap
    }
}

fn is_wide_menu_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x11ff
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
    )
}

fn estimated_menu_label_width(label: &str, text_size: f32) -> f32 {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_whitespace() {
                text_size * 0.35
            } else if ch.is_ascii_punctuation() {
                text_size * 0.45
            } else if ch.is_ascii() {
                text_size * 0.62
            } else if is_wide_menu_char(ch) {
                text_size
            } else {
                text_size * 0.85
            }
        })
        .sum()
}

fn menu_bar_button_width(label: &str, dimensions: &ThemeDimensions) -> f32 {
    let content_width = estimated_menu_label_width(label, dimensions.menu_text_size)
        + dimensions.menu_bar_button_padding_x * 2.0;
    dimensions.menu_bar_button_width.max(content_width.ceil())
}

fn menu_panel_left(open_index: usize, menu_labels: &[String], dimensions: &ThemeDimensions) -> f32 {
    let prior_width: f32 = menu_labels
        .iter()
        .take(open_index)
        .map(|label| menu_bar_button_width(label, dimensions))
        .sum();
    dimensions.menu_bar_padding_x + prior_width + dimensions.menu_bar_gap * open_index as f32
}

fn footnote_group_shell(
    children: Vec<AnyElement>,
    theme: &Theme,
    dimensions: &ThemeDimensions,
) -> AnyElement {
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(0.0))
        .px(px(dimensions.footnote_padding_x))
        .py(px(dimensions.footnote_padding_y))
        .rounded(px(dimensions.footnote_radius))
        .border(px(1.0))
        .border_color(theme.colors.footnote_border)
        .bg(theme.colors.footnote_bg)
        .children(children)
        .into_any_element()
}

impl Editor {
    pub(crate) fn install_close_guard(&mut self, cx: &mut Context<Self>, window: &mut Window) {
        if self.close_guard_installed {
            return;
        }

        self.force_install_close_guard(cx, window);
    }

    pub(crate) fn force_install_close_guard(
        &mut self,
        cx: &mut Context<Self>,
        window: &mut Window,
    ) {
        let editor = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            editor
                .update(cx, |this, cx| this.on_window_should_close(window, cx))
                .unwrap_or(true)
        });
        self.close_guard_installed = true;
    }

    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entity_id) = self.pending_focus.take() {
            if let Some(block) = self.focusable_entity_by_id(entity_id) {
                block.read(cx).focus_handle.focus(window);
            }
        }
    }

    fn ensure_focused_caret_visible(&mut self, window: &Window, cx: &App) -> bool {
        let Some(focused_block) = self.focused_edit_target(window, cx) else {
            return false;
        };
        let Some(active_bounds) =
            focused_block.read_with(cx, |block, _cx| block.active_range_or_cursor_bounds())
        else {
            return false;
        };

        let viewport = self.scroll_handle.bounds();
        let padding = px(20.0);
        let top_limit = viewport.top() + padding;
        let bottom_limit = viewport.bottom() - padding;
        let mut offset = self.scroll_handle.offset();
        let mut changed = false;

        if active_bounds.top() < top_limit {
            offset.y += top_limit - active_bounds.top();
            changed = true;
        } else if active_bounds.bottom() > bottom_limit {
            offset.y -= active_bounds.bottom() - bottom_limit;
            changed = true;
        }

        if changed {
            let max_offset_y = self.scroll_handle.max_offset().height.max(px(0.0));
            offset.y = offset.y.min(px(0.0)).max(-max_offset_y);
            self.scroll_handle.set_offset(offset);
        }

        true
    }

    fn should_use_item_scroll(&self, window: &Window, cx: &App) -> bool {
        if self.view_mode == super::ViewMode::Source {
            return false;
        }

        let Some(focused_id) = self.focused_edit_target_entity_id(window, cx) else {
            return true;
        };
        if self.table_cell_binding(focused_id).is_some() {
            return false;
        }
        let Some(focused_block) = self.document.block_entity_by_id(focused_id) else {
            return true;
        };

        let Some(block_bounds) = focused_block.read_with(cx, |block, _cx| block.last_bounds) else {
            return true;
        };

        let viewport = self.scroll_handle.bounds();
        block_bounds.size.height <= viewport.size.height
    }

    fn apply_pending_scroll_into_view(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.scrollbar_drag.is_some() {
            return;
        }

        if !self.pending_scroll_active_block_into_view {
            return;
        }

        let use_item_scroll = self.should_use_item_scroll(window, cx);
        if use_item_scroll {
            if let Some(focused_id) = self.focused_edit_target_entity_id(window, cx) {
                if let Some(focused_idx) = self.document.visible_index_for_entity_id(focused_id) {
                    self.scroll_handle.scroll_to_item(focused_idx);
                }
            }
        }

        let has_bounds = self.ensure_focused_caret_visible(window, cx);
        if self.pending_scroll_recheck_after_layout {
            self.pending_scroll_recheck_after_layout = false;
            cx.notify();
            return;
        }

        if !has_bounds {
            cx.notify();
            return;
        }

        self.pending_scroll_active_block_into_view = false;
    }

    fn sync_pending_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_save {
            self.pending_save = false;
            self.save_document(window, cx);
        }
    }

    fn sync_pending_save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_save_as {
            self.pending_save_as = false;
            self.save_document_as(window, cx);
        }
    }

    fn sync_pending_open_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.pending_open_link.take() else {
            return;
        };

        let strings = cx.global::<I18nManager>().strings().clone();
        let buttons = [
            strings.open_link_open.as_str(),
            strings.open_link_cancel.as_str(),
        ];
        let prompt = window.prompt(
            PromptLevel::Info,
            &strings.open_link_title,
            Some(&link.prompt_target),
            &buttons,
            cx,
        );
        let window_handle = window.window_handle();
        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let Ok(choice) = prompt.await else {
                return;
            };
            if choice == 0 {
                let _ = cx.update_window(window_handle, |_view: AnyView, _window, cx| {
                    cx.open_url(&link.open_target);
                });
            }
        })
        .detach();
    }

    fn sync_window_edited_state(&mut self, window: &mut Window) {
        if self.pending_window_edited {
            self.pending_window_edited = false;
            window.set_window_edited(true);
        }
    }

    fn sync_scroll_viewport(&mut self, viewport_size: Size<Pixels>, cx: &mut Context<Self>) {
        match self.last_scroll_viewport_size {
            Some(previous) if Self::viewport_size_changed(previous, viewport_size) => {
                self.last_scroll_viewport_size = Some(viewport_size);
                self.request_active_block_scroll_into_view(cx);
            }
            Some(_) => {}
            None => {
                self.last_scroll_viewport_size = Some(viewport_size);
            }
        }
    }

    fn sync_window_title(&mut self, window: &mut Window, strings: &I18nStrings) {
        if self.pending_window_title_refresh {
            self.pending_window_title_refresh = false;
            let title = Self::window_title(self.file_path.as_deref(), self.document_dirty, strings);
            window.set_window_title(&title);
        }
    }

    /// Renders a Windows-only fallback menu bar backed by the app menus
    /// registered through `App::set_menus`.
    fn render_windows_menu_bar(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !cfg!(target_os = "windows") {
            return None;
        }

        let menus = cx.get_menus()?;
        if menus.is_empty() {
            return None;
        }

        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let menu_labels = menus
            .iter()
            .map(|menu| menu.name.to_string())
            .collect::<Vec<_>>();
        let button_widths = menu_labels
            .iter()
            .map(|label| menu_bar_button_width(label, d))
            .collect::<Vec<_>>();

        Some(
            div()
                .id("windows-menu-bar")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(d.menu_bar_height))
                .occlude()
                .flex()
                .items_center()
                .gap(px(d.menu_bar_gap))
                .px(px(d.menu_bar_padding_x))
                .py(px(d.menu_bar_padding_y))
                .bg(c.dialog_surface)
                .border_b(px(theme.dimensions.dialog_border_width))
                .border_color(c.dialog_border)
                .on_hover(cx.listener(Self::on_menu_bar_hover))
                .children(menu_labels.iter().enumerate().map(|(index, label)| {
                    let label = label.clone();
                    let is_open = self.menu_bar_open == Some(index);
                    let button_editor = editor.clone();
                    let button_width = button_widths[index];

                    div()
                        .id(("windows-menu-button", index))
                        .h(px(d.menu_bar_button_height))
                        .w(px(button_width))
                        .px(px(d.menu_bar_button_padding_x))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(px(d.menu_bar_button_radius))
                        .bg(if is_open {
                            c.dialog_secondary_button_hover
                        } else {
                            c.dialog_surface
                        })
                        .hover(|this| this.bg(c.dialog_secondary_button_hover))
                        .active(|this| this.opacity(0.92))
                        .cursor_pointer()
                        .text_size(px(d.menu_text_size))
                        .font_weight(t.dialog_button_weight.to_font_weight())
                        .text_color(c.dialog_secondary_button_text)
                        .whitespace_nowrap()
                        .child(label)
                        .on_hover(move |hovered, _window, cx| {
                            if *hovered {
                                let _ = button_editor
                                    .update(cx, |editor, cx| editor.open_menu_bar(index, cx));
                            }
                        })
                }))
                .into_any_element(),
        )
    }

    /// Renders the currently open Windows fallback menu as a floating panel.
    fn render_windows_menu_panel(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !cfg!(target_os = "windows") {
            return None;
        }

        let open_index = self.menu_bar_open?;
        let menus = cx.get_menus()?;
        let menu = menus.get(open_index)?.clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let menu_labels = menus
            .iter()
            .map(|menu| menu.name.to_string())
            .collect::<Vec<_>>();

        let items = menu
            .items
            .into_iter()
            .enumerate()
            .map(|(item_index, item)| match item {
                OwnedMenuItem::Separator => div()
                    .id(("windows-menu-separator", item_index))
                    .mx(px(d.menu_separator_margin_x))
                    .my(px(d.menu_separator_margin_y))
                    .h(px(d.menu_separator_height))
                    .bg(c.dialog_border)
                    .into_any_element(),
                OwnedMenuItem::Action { name, action, .. } => {
                    let editor = editor.clone();
                    div()
                        .id(("windows-menu-item", item_index))
                        .h(px(d.menu_item_height))
                        .px(px(d.menu_item_padding_x))
                        .flex()
                        .items_center()
                        .rounded(px(d.menu_item_radius))
                        .bg(c.dialog_surface)
                        .hover(|this| this.bg(c.dialog_secondary_button_hover))
                        .active(|this| this.opacity(0.92))
                        .cursor_pointer()
                        .text_size(px(d.menu_text_size))
                        .font_weight(t.dialog_body_weight.to_font_weight())
                        .text_color(c.dialog_secondary_button_text)
                        .child(name)
                        .on_click(move |_, window, cx| {
                            let _ = editor.update(cx, |editor, cx| editor.close_menu_bar(cx));
                            dispatch_menu_action_for_editor(action.as_ref(), &editor, window, cx);
                        })
                        .into_any_element()
                }
                OwnedMenuItem::Submenu(submenu) => div()
                    .id(("windows-menu-submenu", item_index))
                    .h(px(d.menu_item_height))
                    .px(px(d.menu_item_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(d.menu_item_radius))
                    .bg(c.dialog_surface)
                    .text_size(px(d.menu_text_size))
                    .text_color(c.dialog_muted)
                    .child(submenu.name.to_string())
                    .into_any_element(),
                OwnedMenuItem::SystemMenu(os_menu) => div()
                    .id(("windows-menu-system", item_index))
                    .h(px(d.menu_item_height))
                    .px(px(d.menu_item_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(d.menu_item_radius))
                    .bg(c.dialog_surface)
                    .text_size(px(d.menu_text_size))
                    .text_color(c.dialog_muted)
                    .child(os_menu.name.to_string())
                    .into_any_element(),
            });

        Some(
            div()
                .id(("windows-menu-panel", open_index))
                .absolute()
                .occlude()
                .top(px(d.menu_panel_top))
                .left(px(menu_panel_left(open_index, &menu_labels, d)))
                .w(px(d.menu_panel_width))
                .p(px(d.menu_panel_padding))
                .flex()
                .flex_col()
                .gap(px(d.menu_panel_gap))
                .bg(c.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .rounded(px(d.menu_panel_radius))
                .shadow_lg()
                .on_hover(cx.listener(Self::on_menu_panel_hover))
                .children(items)
                .into_any_element(),
        )
    }

    /// Builds the unsaved-changes dialog with backdrop, message, and three
    /// action buttons (cancel, discard, save-and-close).
    fn render_unsaved_changes_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let strings = cx.global::<I18nManager>().strings();

        div()
            .id("unsaved-changes-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(c.dialog_backdrop)
            .child(
                div()
                    .w_full()
                    .px(px(d.editor_padding))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("unsaved-changes-dialog")
                            .w(px(d.dialog_width))
                            .max_w(relative(1.0))
                            .flex()
                            .flex_col()
                            .gap(px(d.dialog_gap))
                            .p(px(d.dialog_padding))
                            .bg(c.dialog_surface)
                            .border(px(d.dialog_border_width))
                            .border_color(c.dialog_border)
                            .rounded(px(d.dialog_radius))
                            .shadow_lg()
                            .child(
                                div()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(strings.unsaved_changes_title.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(d.dialog_gap * 0.5))
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .font_weight(t.dialog_body_weight.to_font_weight())
                                            .line_height(rems(t.text_line_height))
                                            .text_color(c.dialog_body)
                                            .child(strings.unsaved_changes_message.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .font_weight(t.dialog_body_weight.to_font_weight())
                                            .line_height(rems(t.text_line_height))
                                            .text_color(c.dialog_muted)
                                            .child(strings.unsaved_changes_hint.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(d.dialog_button_gap))
                                    .child(
                                        div()
                                            .id("cancel-close-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .bg(c.dialog_secondary_button_bg)
                                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_secondary_button_text)
                                            .child(strings.unsaved_changes_cancel.clone())
                                            .on_click(cx.listener(Self::on_cancel_close_dialog)),
                                    )
                                    .child(
                                        div()
                                            .id("discard-and-close-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .bg(c.dialog_danger_button_bg)
                                            .hover(|this| this.bg(c.dialog_danger_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_danger_button_text)
                                            .child(
                                                strings.unsaved_changes_discard_and_close.clone(),
                                            )
                                            .on_click(cx.listener(Self::on_discard_and_close)),
                                    )
                                    .child(
                                        div()
                                            .id("save-and-close-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .bg(c.dialog_primary_button_bg)
                                            .hover(|this| this.bg(c.dialog_primary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_primary_button_text)
                                            .child(strings.unsaved_changes_save_and_close.clone())
                                            .on_click(cx.listener(Self::on_save_and_close)),
                                    ),
                            ),
                    ),
            )
    }

    /// Builds the dropped-file replacement dialog shown when the current
    /// document has unsaved changes.
    fn render_drop_replace_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let strings = cx.global::<I18nManager>().strings();

        div()
            .id("drop-replace-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(c.dialog_backdrop)
            .child(
                div()
                    .w_full()
                    .px(px(d.editor_padding))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("drop-replace-dialog")
                            .w(px(d.dialog_width))
                            .max_w(relative(1.0))
                            .flex()
                            .flex_col()
                            .gap(px(d.dialog_gap))
                            .p(px(d.dialog_padding))
                            .bg(c.dialog_surface)
                            .border(px(d.dialog_border_width))
                            .border_color(c.dialog_border)
                            .rounded(px(d.dialog_radius))
                            .shadow_lg()
                            .child(
                                div()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(strings.drop_replace_title.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(d.dialog_gap * 0.5))
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .font_weight(t.dialog_body_weight.to_font_weight())
                                            .line_height(rems(t.text_line_height))
                                            .text_color(c.dialog_body)
                                            .child(strings.drop_replace_message.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .font_weight(t.dialog_body_weight.to_font_weight())
                                            .line_height(rems(t.text_line_height))
                                            .text_color(c.dialog_muted)
                                            .child(strings.drop_replace_hint.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(d.dialog_button_gap))
                                    .child(
                                        div()
                                            .id("cancel-drop-replace-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .bg(c.dialog_secondary_button_bg)
                                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_secondary_button_text)
                                            .child(strings.drop_replace_cancel.clone())
                                            .on_click(
                                                cx.listener(Self::on_cancel_drop_replace_dialog),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("discard-and-replace-drop-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .bg(c.dialog_danger_button_bg)
                                            .hover(|this| this.bg(c.dialog_danger_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_danger_button_text)
                                            .child(strings.drop_replace_discard_and_replace.clone())
                                            .on_click(
                                                cx.listener(Self::on_discard_and_replace_drop),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("save-and-replace-drop-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .bg(c.dialog_primary_button_bg)
                                            .hover(|this| this.bg(c.dialog_primary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_primary_button_text)
                                            .child(strings.drop_replace_save_and_replace.clone())
                                            .on_click(cx.listener(Self::on_save_and_replace_drop)),
                                    ),
                            ),
                    ),
            )
    }

    fn info_dialog_title<'a>(&self, strings: &'a I18nStrings, kind: InfoDialogKind) -> &'a str {
        match kind {
            InfoDialogKind::CheckForUpdates => &strings.help_check_updates_title,
            InfoDialogKind::About => &strings.help_about_title,
        }
    }

    pub(crate) fn about_dialog_body_lines(strings: &I18nStrings) -> Vec<String> {
        vec![
            format!("Velotype {}", env!("CARGO_PKG_VERSION")),
            strings.help_about_message.clone(),
            format!("{}: {}", strings.help_about_github_label, ABOUT_GITHUB_URL),
            strings.help_about_star_message.clone(),
        ]
    }

    fn info_dialog_body(&self, strings: &I18nStrings, kind: InfoDialogKind) -> String {
        match kind {
            InfoDialogKind::CheckForUpdates => strings.help_check_updates_message.clone(),
            InfoDialogKind::About => Self::about_dialog_body_lines(strings).join("\n"),
        }
    }

    fn render_info_dialog_body(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        kind: InfoDialogKind,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let body_style = |this: Div| {
            this.text_size(px(t.dialog_body_size))
                .font_weight(t.dialog_body_weight.to_font_weight())
                .line_height(rems(t.text_line_height))
                .text_color(c.dialog_body)
        };

        match kind {
            InfoDialogKind::CheckForUpdates => div()
                .flex()
                .flex_col()
                .gap(px(d.dialog_gap * 0.5))
                .child(
                    body_style(div()).children(
                        self.info_dialog_body(strings, kind)
                            .lines()
                            .map(|line| div().child(line.to_string())),
                    ),
                )
                .into_any_element(),
            InfoDialogKind::About => div()
                .flex()
                .flex_col()
                .gap(px(d.dialog_gap * 0.5))
                .child(body_style(div()).child(format!("Velotype {}", env!("CARGO_PKG_VERSION"))))
                .child(body_style(div()).child(strings.help_about_message.clone()))
                .child(
                    body_style(div())
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .child(format!("{}:", strings.help_about_github_label))
                        .child(
                            div()
                                .id("about-github-link")
                                .cursor_pointer()
                                .text_color(c.text_link)
                                .underline()
                                .child(ABOUT_GITHUB_URL)
                                .on_click(move |_, _, cx| {
                                    open_about_github_url(cx);
                                }),
                        ),
                )
                .child(body_style(div()).child(strings.help_about_star_message.clone()))
                .into_any_element(),
        }
    }

    fn on_dismiss_info_dialog(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_info_dialog(cx);
    }

    fn render_info_dialog_overlay(
        &self,
        theme: &Theme,
        kind: InfoDialogKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let strings = cx.global::<I18nManager>().strings();

        div()
            .id("info-dialog-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(c.dialog_backdrop)
            .child(
                div()
                    .w_full()
                    .px(px(d.editor_padding))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("info-dialog")
                            .w(px(d.dialog_width))
                            .max_w(relative(1.0))
                            .flex()
                            .flex_col()
                            .gap(px(d.dialog_gap))
                            .p(px(d.dialog_padding))
                            .bg(c.dialog_surface)
                            .border(px(d.dialog_border_width))
                            .border_color(c.dialog_border)
                            .rounded(px(d.dialog_radius))
                            .shadow_lg()
                            .child(
                                div()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(self.info_dialog_title(strings, kind).to_string()),
                            )
                            .child(self.render_info_dialog_body(theme, strings, kind))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(d.dialog_button_gap))
                                    .child(
                                        div()
                                            .id("dismiss-info-dialog")
                                            .h(px(d.dialog_button_height))
                                            .px(px(d.dialog_button_padding_x))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                            .bg(c.dialog_primary_button_bg)
                                            .hover(|this| this.bg(c.dialog_primary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_button_size))
                                            .font_weight(t.dialog_button_weight.to_font_weight())
                                            .text_color(c.dialog_primary_button_text)
                                            .child(strings.info_dialog_ok.clone())
                                            .on_click(cx.listener(Self::on_dismiss_info_dialog)),
                                    ),
                            ),
                    ),
            )
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.install_close_guard(cx, window);
        self.apply_pending_focus(window, cx);
        self.apply_pending_scroll_into_view(window, cx);
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.sync_pending_save(window, cx);
        self.sync_pending_save_as(window, cx);
        self.sync_pending_open_link(window, cx);
        self.sync_window_edited_state(window);

        let viewport_bounds = self.scroll_handle.bounds();
        let viewport_size = viewport_bounds.size;
        self.sync_scroll_viewport(viewport_size, cx);

        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        self.sync_window_title(window, &strings);

        let d = &theme.dimensions;
        let c = &theme.colors;
        let visible_blocks = self.document.visible_blocks().to_vec();
        let editor = cx.entity().downgrade();
        let has_windows_menu = cfg!(target_os = "windows")
            && cx
                .get_menus()
                .map(|menus| !menus.is_empty())
                .unwrap_or(false);
        let menu_bar_height = if has_windows_menu {
            px(d.menu_bar_height)
        } else {
            px(0.0)
        };
        let scroll_trigger_padding = (d.block_min_height * 0.75).max(16.0);
        let max_scroll_y = f32::from(self.scroll_handle.max_offset().height.max(px(0.0)));
        let viewport_height = f32::from(viewport_bounds.size.height.max(px(1.0)));
        let viewport_width = f32::from(viewport_bounds.size.width.max(px(1.0)));
        let has_overflow = max_scroll_y > 0.5;

        let centered_width = Self::centered_column_width(viewport_width, &theme.dimensions);
        let current_scroll_y = (-f32::from(self.scroll_handle.offset().y)).clamp(0.0, max_scroll_y);
        let scrollbar_geometry =
            Self::scrollbar_geometry(viewport_height, max_scroll_y, current_scroll_y);
        let track_height = scrollbar_geometry.track_height;
        let thumb_height = scrollbar_geometry.thumb_height;
        let thumb_top = scrollbar_geometry.thumb_top;

        let show_custom_scrollbar = has_overflow
            && (self.scrollbar_drag.is_some()
                || self.scrollbar_hovered
                || Instant::now() <= self.scrollbar_visible_until);

        let spacing_infos = visible_blocks
            .iter()
            .map(|visible| {
                visible
                    .entity
                    .read_with(cx, |block, _cx| RenderedRowSpacingInfo::from_block(block))
            })
            .collect::<Vec<_>>();
        let mut previous_row_spacing = None;
        let mut block_rows = Vec::new();
        let mut index = 0usize;
        while index < visible_blocks.len() {
            let first_visible = visible_blocks[index].clone();
            let first_spacing = spacing_infos[index];
            let top_gap = rendered_row_top_gap(previous_row_spacing, first_spacing, d.block_gap);

            if let (Some(callout_anchor), Some(callout_variant)) =
                (first_spacing.callout_anchor, first_spacing.callout_variant)
            {
                let mut group_children = Vec::new();
                let mut group_end = index;
                let mut previous_callout_row = None;
                while group_end < visible_blocks.len()
                    && spacing_infos[group_end].callout_anchor == Some(callout_anchor)
                {
                    let row_spacing = spacing_infos[group_end];
                    if let Some(footnote_anchor) = row_spacing.footnote_anchor {
                        let mut footnote_children = Vec::new();
                        let mut footnote_end = group_end;
                        let mut previous_footnote_row = None;
                        while footnote_end < visible_blocks.len()
                            && spacing_infos[footnote_end].callout_anchor == Some(callout_anchor)
                            && spacing_infos[footnote_end].footnote_anchor == Some(footnote_anchor)
                        {
                            let footnote_spacing = spacing_infos[footnote_end];
                            let entity = visible_blocks[footnote_end].entity.clone();
                            let row = div()
                                .w_full()
                                .flex_shrink_0()
                                .mt(px(footnote_row_top_gap(previous_footnote_row, d.block_gap)))
                                .child(entity.clone());
                            let row = if self.view_mode == super::ViewMode::Rendered {
                                let row_editor = editor.clone();
                                let entity_id = entity.entity_id();
                                row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                    let _ = row_editor.update(cx, |editor, cx| {
                                        editor.on_block_context_menu_mouse_down(
                                            entity_id, event, window, cx,
                                        );
                                    });
                                })
                            } else {
                                row
                            };
                            footnote_children.push(row.into_any_element());
                            previous_footnote_row = Some(footnote_spacing);
                            footnote_end += 1;
                        }

                        group_children.push(
                            div()
                                .w_full()
                                .flex_shrink_0()
                                .mt(px(callout_row_top_gap(
                                    previous_callout_row,
                                    row_spacing,
                                    d,
                                )))
                                .child(footnote_group_shell(footnote_children, &theme, d))
                                .into_any_element(),
                        );
                        previous_callout_row = Some(spacing_infos[footnote_end - 1]);
                        group_end = footnote_end;
                        continue;
                    }

                    let entity = visible_blocks[group_end].entity.clone();
                    let row = div()
                        .w_full()
                        .flex_shrink_0()
                        .mt(px(callout_row_top_gap(
                            previous_callout_row,
                            row_spacing,
                            d,
                        )))
                        .child(entity.clone());
                    let row = if self.view_mode == super::ViewMode::Rendered {
                        let row_editor = editor.clone();
                        let entity_id = entity.entity_id();
                        row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            let _ = row_editor.update(cx, |editor, cx| {
                                editor
                                    .on_block_context_menu_mouse_down(entity_id, event, window, cx);
                            });
                        })
                    } else {
                        row
                    };
                    group_children.push(row.into_any_element());
                    previous_callout_row = Some(row_spacing);
                    group_end += 1;
                }

                let (accent, background) = callout_colors(callout_variant, &theme);
                block_rows.push(
                    div()
                        .w(px(centered_width))
                        .max_w(relative(1.0))
                        .flex_shrink_0()
                        .mt(px(top_gap))
                        .flex()
                        .flex_col()
                        .gap(px(0.0))
                        .px(px(d.callout_padding_x))
                        .py(px(d.callout_padding_y))
                        .rounded(px(d.callout_radius))
                        .border_l(px(d.callout_border_width))
                        .border_color(accent)
                        .bg(background)
                        .children(group_children)
                        .into_any_element(),
                );
                previous_row_spacing = Some(spacing_infos[group_end - 1]);
                index = group_end;
                continue;
            }

            if let Some(footnote_anchor) = first_spacing.footnote_anchor {
                let mut group_children = Vec::new();
                let mut group_end = index;
                let mut previous_footnote_row = None;
                while group_end < visible_blocks.len()
                    && spacing_infos[group_end].footnote_anchor == Some(footnote_anchor)
                {
                    let row_spacing = spacing_infos[group_end];
                    let entity = visible_blocks[group_end].entity.clone();
                    let row = div()
                        .w_full()
                        .flex_shrink_0()
                        .mt(px(footnote_row_top_gap(previous_footnote_row, d.block_gap)))
                        .child(entity.clone());
                    let row = if self.view_mode == super::ViewMode::Rendered {
                        let row_editor = editor.clone();
                        let entity_id = entity.entity_id();
                        row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            let _ = row_editor.update(cx, |editor, cx| {
                                editor
                                    .on_block_context_menu_mouse_down(entity_id, event, window, cx);
                            });
                        })
                    } else {
                        row
                    };
                    group_children.push(row.into_any_element());
                    previous_footnote_row = Some(row_spacing);
                    group_end += 1;
                }

                block_rows.push(
                    div()
                        .w(px(centered_width))
                        .max_w(relative(1.0))
                        .flex_shrink_0()
                        .mt(px(top_gap))
                        .child(footnote_group_shell(group_children, &theme, d))
                        .into_any_element(),
                );
                previous_row_spacing = Some(spacing_infos[group_end - 1]);
                index = group_end;
                continue;
            }

            let entity = first_visible.entity.clone();
            let row = div()
                .w(px(centered_width))
                .max_w(relative(1.0))
                .flex_shrink_0()
                .mt(px(top_gap))
                .child(entity.clone());
            let row = if self.view_mode == super::ViewMode::Rendered {
                let row_editor = editor.clone();
                let entity_id = entity.entity_id();
                row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    let _ = row_editor.update(cx, |editor, cx| {
                        editor.on_block_context_menu_mouse_down(entity_id, event, window, cx);
                    });
                })
            } else {
                row
            };
            block_rows.push(row.into_any_element());
            previous_row_spacing = Some(first_spacing);
            index += 1;
        }

        let scroll_content = div()
            .id("editor-scroll-inner")
            .flex()
            .flex_col()
            .flex_grow()
            .h_full()
            .items_center()
            .bg(theme.colors.editor_background)
            .overflow_y_scroll()
            .scrollbar_width(px(0.0))
            .track_scroll(&self.scroll_handle)
            .on_hover(cx.listener(Self::on_editor_hover))
            .capture_any_mouse_down(cx.listener(Self::on_editor_capture_mouse_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_editor_mouse_down))
            .on_mouse_move(cx.listener(Self::on_editor_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_editor_scroll_wheel))
            .p(px(d.editor_padding))
            .pb(px(d.editor_padding + scroll_trigger_padding))
            .children(block_rows);
        let scroll_content = if self.view_mode == super::ViewMode::Rendered {
            scroll_content.on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_editor_context_menu_mouse_down),
            )
        } else {
            scroll_content
        };

        let content_area = div()
            .id("editor-scroll")
            .w_full()
            .h_full()
            .bg(theme.colors.editor_background)
            .relative()
            .child(scroll_content);

        let content_area = if show_custom_scrollbar {
            let scrollbar_editor = editor.clone();
            let track_origin_y = f32::from(viewport_bounds.origin.y);
            content_area.child(
                div()
                    .id("editor-scrollbar-thumb")
                    .absolute()
                    .occlude()
                    .top(px(thumb_top))
                    .right(px(d.scrollbar_right))
                    .w(px(d.scrollbar_width))
                    .h(px(thumb_height))
                    .rounded(px(999.0))
                    .bg(theme.colors.scrollbar_thumb)
                    .cursor_pointer()
                    .on_hover(cx.listener(Self::on_editor_hover))
                    .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                        let pointer_offset_y =
                            f32::from(event.position.y) - track_origin_y - thumb_top;
                        let _ = scrollbar_editor.update(cx, |editor, cx| {
                            cx.stop_propagation();
                            editor.start_scrollbar_drag(
                                pointer_offset_y,
                                track_height,
                                thumb_height,
                                max_scroll_y,
                                cx,
                            );
                        });
                    })
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_thumb_bounds, _, window, _| {
                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |_event: &MouseUpEvent, phase, _window, cx| {
                                        if !phase.bubble() {
                                            return;
                                        }
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.end_scrollbar_drag(cx);
                                        });
                                    }
                                });

                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |event: &MouseMoveEvent, phase, _window, cx| {
                                        if !phase.bubble() || !event.dragging() {
                                            return;
                                        }

                                        let pointer_y_in_track =
                                            f32::from(event.position.y) - track_origin_y;
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.update_scrollbar_drag(pointer_y_in_track, cx);
                                        });
                                    }
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
        } else {
            content_area
        };

        // View-mode toggle button — bottom-left corner of the editor.
        let toggle_label = match (self.view_mode, self.view_mode_toggle_hovered) {
            (super::ViewMode::Rendered, false) => strings.view_mode_source.clone(),
            (super::ViewMode::Rendered, true) => strings.view_mode_switch_to_source.clone(),
            (super::ViewMode::Source, false) => strings.view_mode_rendered.clone(),
            (super::ViewMode::Source, true) => strings.view_mode_switch_to_rendered.clone(),
        };
        let view_mode_toggle = div()
            .id("view-mode-toggle")
            .absolute()
            .left(px(d.view_mode_toggle_left))
            .bottom(px(d.view_mode_toggle_bottom))
            .occlude()
            .px(px(d.view_mode_toggle_padding_x))
            .py(px(d.view_mode_toggle_padding_y))
            .rounded(px(d.view_mode_toggle_radius))
            .bg(if self.view_mode_toggle_hovered {
                c.dialog_secondary_button_hover
            } else {
                c.dialog_surface
            })
            .border(px(d.view_mode_toggle_border_width))
            .border_color(c.dialog_border.opacity(0.65))
            .cursor_pointer()
            .text_size(px(d.view_mode_toggle_text_size))
            .text_color(if self.view_mode_toggle_hovered {
                c.dialog_secondary_button_text
            } else {
                c.dialog_muted
            })
            .on_hover(cx.listener(Self::on_view_mode_toggle_hover))
            .child(SharedString::from(toggle_label))
            .on_click(cx.listener(Self::on_toggle_view_mode));

        let base = div()
            .w_full()
            .h_full()
            .relative()
            .bg(theme.colors.editor_background)
            .capture_action(cx.listener(Self::on_copy_capture))
            .capture_action(cx.listener(Self::on_cut_capture))
            .capture_action(cx.listener(Self::on_delete_capture))
            .capture_action(cx.listener(Self::on_delete_back_capture))
            .can_drop(|dragged, _window, _cx| dragged.is::<ExternalPaths>())
            .on_drop::<ExternalPaths>(cx.listener(Self::on_external_paths_drop))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_save_document))
            .on_action(cx.listener(Self::on_save_document_as))
            .on_action(cx.listener(Self::on_export_html))
            .on_action(cx.listener(Self::on_export_pdf))
            .on_action(cx.listener(Self::on_quit_application))
            .on_action(cx.listener(Self::on_dismiss_transient_ui));
        let base = if let Some(menu_bar) = self.render_windows_menu_bar(&theme, cx) {
            base.child(menu_bar)
        } else {
            base
        };
        let base = base.child(
            div()
                .w_full()
                .h_full()
                .pt(menu_bar_height)
                .child(content_area),
        );
        let base = if let Some(menu_panel) = self.render_windows_menu_panel(&theme, cx) {
            base.child(menu_panel)
        } else {
            base
        };
        let base = if let Some(context_menu) = self.render_context_menu_overlay(&theme, cx) {
            base.child(context_menu)
        } else {
            base
        };
        let base = if let Some(table_dialog) = self.render_table_insert_dialog_overlay(&theme, cx) {
            base.child(table_dialog)
        } else {
            base
        };
        let base = base.child(view_mode_toggle);

        if let Some(kind) = self.info_dialog {
            base.child(self.render_info_dialog_overlay(&theme, kind, cx))
        } else if self.show_drop_replace_dialog {
            base.child(self.render_drop_replace_overlay(&theme, cx))
        } else if self.show_unsaved_changes_dialog {
            base.child(self.render_unsaved_changes_overlay(&theme, cx))
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RenderedRowSpacingInfo, callout_row_top_gap, menu_bar_button_width, menu_panel_left,
        rendered_row_top_gap,
    };
    use crate::theme::Theme;
    use uuid::Uuid;

    #[test]
    fn contiguous_quote_rows_collapse_inter_row_gap() {
        let group = Uuid::new_v4();
        let gap = rendered_row_top_gap(
            Some(RenderedRowSpacingInfo {
                quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo {
                quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            },
            4.0,
        );
        assert_eq!(gap, 0.0);
    }

    #[test]
    fn nested_quote_separator_row_keeps_outer_group_gap_collapsed() {
        let group = Uuid::new_v4();
        let gap = rendered_row_top_gap(
            Some(RenderedRowSpacingInfo {
                quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo {
                quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            },
            4.0,
        );
        assert_eq!(gap, 0.0);
    }

    #[test]
    fn distinct_quote_groups_keep_default_gap() {
        let gap = rendered_row_top_gap(
            Some(RenderedRowSpacingInfo {
                quote_group_anchor: Some(Uuid::new_v4()),
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo {
                quote_group_anchor: Some(Uuid::new_v4()),
                ..RenderedRowSpacingInfo::default()
            },
            4.0,
        );
        assert_eq!(gap, 4.0);
    }

    #[test]
    fn non_quote_rows_keep_default_gap() {
        let gap = rendered_row_top_gap(
            Some(RenderedRowSpacingInfo {
                quote_group_anchor: None,
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo {
                quote_group_anchor: Some(Uuid::new_v4()),
                ..RenderedRowSpacingInfo::default()
            },
            4.0,
        );
        assert_eq!(gap, 4.0);
    }

    #[test]
    fn callout_inner_spacing_uses_header_and_body_tokens() {
        let theme = Theme::default_theme();
        let dimensions = &theme.dimensions;

        let header_gap = callout_row_top_gap(
            Some(RenderedRowSpacingInfo {
                is_callout_header: true,
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo::default(),
            dimensions,
        );
        let body_gap = callout_row_top_gap(
            Some(RenderedRowSpacingInfo {
                is_callout_header: false,
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo::default(),
            dimensions,
        );

        assert_eq!(header_gap, dimensions.callout_header_margin_bottom);
        assert_eq!(body_gap, dimensions.callout_body_gap);
    }

    #[test]
    fn nested_quote_rows_inside_callout_collapse_body_gap() {
        let theme = Theme::default_theme();
        let dimensions = &theme.dimensions;
        let group = Uuid::new_v4();

        let gap = callout_row_top_gap(
            Some(RenderedRowSpacingInfo {
                is_callout_header: false,
                visible_quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            }),
            RenderedRowSpacingInfo {
                visible_quote_group_anchor: Some(group),
                ..RenderedRowSpacingInfo::default()
            },
            dimensions,
        );

        assert_eq!(gap, 0.0);
    }

    #[test]
    fn menu_button_width_expands_for_long_ascii_labels() {
        let theme = Theme::default_theme();
        let dimensions = &theme.dimensions;

        assert_eq!(
            menu_bar_button_width("文件", dimensions),
            dimensions.menu_bar_button_width
        );
        assert!(menu_bar_button_width("Language", dimensions) > dimensions.menu_bar_button_width);
    }

    #[test]
    fn menu_panel_left_uses_accumulated_dynamic_button_widths() {
        let theme = Theme::default_theme();
        let dimensions = &theme.dimensions;
        let labels = vec![
            "File".to_string(),
            "Language".to_string(),
            "Theme".to_string(),
            "Help".to_string(),
        ];

        let left = menu_panel_left(2, &labels, dimensions);
        let expected = dimensions.menu_bar_padding_x
            + menu_bar_button_width("File", dimensions)
            + dimensions.menu_bar_gap
            + menu_bar_button_width("Language", dimensions)
            + dimensions.menu_bar_gap;
        let old_fixed_left = dimensions.menu_bar_padding_x
            + 2.0 * (dimensions.menu_bar_button_width + dimensions.menu_bar_gap);

        assert_eq!(left, expected);
        assert!(left > old_fixed_left);
    }
}
