//! Bottom status bar: sidebar toggle, mode switch, cursor position,
//! word count, and custom buttons.

use gpui::*;
use unicode_segmentation::UnicodeSegmentation;

use super::Editor;
use crate::config::preferences::{StatusBarButton, StatusBarPreferences};
use crate::i18n::I18nStrings;
use crate::theme::Theme;

#[derive(Default)]
pub(super) struct StatusBarState {
    pub sidebar_hovered: bool,
    pub mode_hovered: bool,
    custom_button_hovered: Option<String>,
}

impl Editor {
    pub(super) fn render_status_bar(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prefs = self.status_bar_preferences(cx);
        if !prefs.enabled {
            return None;
        }

        let c = &theme.colors;
        let d = &theme.dimensions;

        let mut left_items: Vec<AnyElement> = Vec::new();

        if prefs.show_sidebar_toggle {
            left_items.push(render_sidebar_toggle(
                &mut self.status_bar,
                self.workspace.is_open,
                theme,
                strings,
                cx,
            ));
        }

        if prefs.show_mode_switch {
            left_items.push(render_mode_switch(
                &mut self.status_bar,
                self.view_mode,
                theme,
                strings,
                cx,
            ));
        }

        let mut right_items: Vec<AnyElement> = Vec::new();

        if prefs.show_cursor_position && self.view_mode == super::ViewMode::Source {
            right_items.push(render_cursor(
                self.compute_source_cursor_position(cx),
                theme,
            ));
        }

        if prefs.show_word_count {
            let text = self.serialized_document_text(cx);
            let total_count = count_words(&text);
            let selection_count = self.selected_markdown_text(cx).as_deref().map(count_words);
            right_items.push(render_word_count(
                selection_count,
                total_count,
                theme,
                strings,
            ));
        }

        for button in &prefs.custom_buttons {
            right_items.push(render_custom_button(
                &mut self.status_bar,
                button,
                theme,
                cx,
            ));
        }

        let bar = div()
            .id("status-bar")
            .h(px(d.status_bar_height))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px(px(d.status_bar_padding_x))
            .bg(c.status_bar_background)
            .border_t(px(1.0))
            .border_color(c.dialog_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(d.status_bar_item_gap))
                    .children(left_items),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(d.status_bar_item_gap))
                    .children(right_items),
            )
            .into_any_element();

        Some(bar)
    }

    fn status_bar_preferences(&self, cx: &App) -> StatusBarPreferences {
        crate::config::preferences::EditorSettings::status_bar_preferences(cx)
    }

    /// Returns (line, col), both 1-based, from the source-mode selection snapshot.
    fn compute_source_cursor_position(&self, cx: &App) -> (usize, usize) {
        let snapshot = self.capture_source_selection_snapshot(cx);
        let cursor_offset = snapshot.range.end;
        let text = self.document.raw_source_text(cx);
        let clamped = cursor_offset.min(text.len());

        let line = text[..clamped].matches('\n').count() + 1;
        let last_newline = text[..clamped].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = text[last_newline..clamped].graphemes(true).count() + 1;
        (line, col)
    }
}

fn render_sidebar_toggle(
    state: &mut StatusBarState,
    _is_open: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    div()
        .id("status-bar-sidebar-toggle")
        .h(px(d.status_bar_height - 4.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if state.sidebar_hovered {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(strings.status_bar_files.clone())
        .on_hover(cx.listener(
            |editor: &mut Editor,
             hovered: &bool,
             _window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.status_bar.sidebar_hovered = *hovered;
                cx.notify();
            },
        ))
        .on_click(cx.listener(
            |editor: &mut Editor,
             _: &gpui::ClickEvent,
             window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.toggle_workspace_drawer(window, cx);
            },
        ))
        .into_any_element()
}

fn render_mode_switch(
    state: &mut StatusBarState,
    view_mode: super::ViewMode,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let label = match view_mode {
        super::ViewMode::Source => strings.status_bar_mode_rendered.clone(),
        super::ViewMode::Rendered => strings.status_bar_mode_source.clone(),
    };

    div()
        .id("status-bar-mode-switch")
        .h(px(d.status_bar_height - 4.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if state.mode_hovered {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(label)
        .on_hover(cx.listener(
            |editor: &mut Editor,
             hovered: &bool,
             _window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.status_bar.mode_hovered = *hovered;
                cx.notify();
            },
        ))
        .on_click(cx.listener(
            |editor: &mut Editor,
             _: &gpui::ClickEvent,
             _window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.toggle_view_mode_from_ui(cx);
            },
        ))
        .into_any_element()
}

fn render_cursor((line, col): (usize, usize), theme: &Theme) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let label = format!("{} : {}", &line.to_string(), &col.to_string());

    div()
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(label)
        .into_any_element()
}

fn render_word_count(
    selection_count: Option<usize>,
    total_count: usize,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let label = if let Some(sel) = selection_count {
        format!(
            "{} / {} {}",
            sel, total_count, strings.status_bar_word_count_suffix
        )
    } else {
        format!("{} {}", total_count, strings.status_bar_word_count_suffix)
    };

    div()
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text_dim)
        .child(label)
        .into_any_element()
}

fn render_custom_button(
    state: &mut StatusBarState,
    button: &StatusBarButton,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let id = button.id.clone();
    let hovered = state.custom_button_hovered.as_deref() == Some(&button.id);

    div()
        .id(ElementId::Name(
            format!("status-bar-custom-button-{}", button.id).into(),
        ))
        .h(px(d.status_bar_height - 4.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if hovered {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(button.label.clone())
        .on_hover(cx.listener(
            move |editor: &mut Editor,
                  hovered: &bool,
                  _window: &mut Window,
                  cx: &mut Context<Editor>| {
                if *hovered {
                    editor.status_bar.custom_button_hovered = Some(id.clone());
                } else if editor.status_bar.custom_button_hovered.as_deref() == Some(&id) {
                    editor.status_bar.custom_button_hovered = None;
                }
                cx.notify();
            },
        ))
        .into_any_element()
}

/// Count words in mixed CJK / Latin text.
///
/// Every CJK character counts as one word. Latin words are split on whitespace.
pub fn count_words(text: &str) -> usize {
    let mut count = 0;
    let mut in_latin_word = false;

    for ch in text.chars() {
        if is_cjk_char(ch) {
            if in_latin_word {
                count += 1;
                in_latin_word = false;
            }
            count += 1;
        } else if ch.is_whitespace() {
            if in_latin_word {
                count += 1;
                in_latin_word = false;
            }
        } else {
            in_latin_word = true;
        }
    }
    if in_latin_word {
        count += 1;
    }
    count
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF
        // CJK Unified Ideographs Extension A
        | 0x3400..=0x4DBF
        // CJK Unified Ideographs Extension B
        | 0x20000..=0x2A6DF
        // CJK Compatibility Ideographs
        | 0xF900..=0xFAFF
        // CJK Radicals Supplement / Kangxi Radicals
        | 0x2E80..=0x2EFF
        | 0x2F00..=0x2FDF
        // Hiragana / Katakana (Japanese)
        | 0x3040..=0x309F
        | 0x30A0..=0x30FF
        // Hangul Syllables (Korean)
        | 0xAC00..=0xD7AF
    )
}

#[cfg(test)]
mod tests {
    use super::count_words;

    #[test]
    fn empty_text_has_zero_words() {
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn english_words_are_counted() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("one two three four"), 4);
    }

    #[test]
    fn cjk_characters_are_counted_individually() {
        assert_eq!(count_words("你好世界"), 4);
        assert_eq!(count_words("中文"), 2);
    }

    #[test]
    fn mixed_cjk_and_english() {
        assert_eq!(count_words("hello 世界"), 3);
        assert_eq!(count_words("你好 world foo"), 4);
    }

    #[test]
    fn whitespace_handling() {
        assert_eq!(count_words("  hello   world  "), 2);
        assert_eq!(count_words("   "), 0);
    }
}
