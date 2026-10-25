//! Action handlers dispatched by GPUI's action system when bound keys are
//! pressed on a focused block.  Each handler maps to a named action declared
//! in [`crate::components::actions`] and delegates structural changes to the
//! parent editor via `BlockEvent` emissions.

use std::time::Duration;

use gpui::*;
use unicode_segmentation::UnicodeSegmentation;

use super::CollapsedCaretAffinity;
use super::{
    Block, BlockEvent, BlockKind, InlineFormat, InlineTextTree, PastedImageSource, UndoCaptureKind,
};
use crate::components::markdown::paste::should_split_plain_multiline_paste;
use crate::components::{
    BlockDown, BlockUp, BoldSelection, CodeSelection, Copy, Cut, Delete, DeleteBack,
    DismissTransientUi, End, ExitCodeBlock, FocusNext, FocusPrev, Heading1, Heading2, Heading3,
    Heading4, Heading5, Heading6, Home, IndentBlock, ItalicSelection, MoveLeft, MoveRight, Newline,
    OutdentBlock, Paste, SelectAll, SelectEnd, SelectHome, SelectLeft, SelectRight,
    UnderlineSelection, WordDeleteBack, WordDeleteForward, WordMoveLeft, WordMoveRight,
    WordSelectLeft, WordSelectRight,
};

impl Block {
    /// Extracts a complete image batch without mixing duplicate clipboard formats.
    fn pasted_image_sources_from_clipboard(item: &ClipboardItem) -> Option<Vec<PastedImageSource>> {
        let images = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => {
                    Some(PastedImageSource::ClipboardImage(image.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !images.is_empty() {
            return Some(images);
        }
        item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths)
                if !paths.paths().is_empty()
                    && paths
                        .paths()
                        .iter()
                        .all(|path| Self::is_supported_local_image_path(path)) =>
            {
                Some(
                    paths
                        .paths()
                        .iter()
                        .cloned()
                        .map(PastedImageSource::LocalPath)
                        .collect(),
                )
            }
            _ => None,
        })
    }

    /// Accepts a single-line clipboard string naming an existing local image.
    fn pasted_image_source_from_text(text: &str) -> Option<PastedImageSource> {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
            return None;
        }

        Self::pasted_image_path_from_text_item(trimmed).map(PastedImageSource::LocalPath)
    }

    /// Parses a single clipboard text item as a local image path.
    ///
    /// Windows file-copy paste reaches GPUI as a plain drive-letter path; that
    /// must be tested as a path before URL parsing, because `url::Url` treats
    /// the drive letter as a URL scheme.
    fn pasted_image_path_from_text_item(text: &str) -> Option<std::path::PathBuf> {
        let unquoted = text
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(text);
        let direct_path = std::path::PathBuf::from(unquoted);
        let path = if Self::is_supported_local_image_path(&direct_path) {
            direct_path
        } else if let Ok(url) = url::Url::parse(unquoted) {
            if url.scheme() == "file" {
                url.to_file_path().ok()?
            } else {
                return None;
            }
        } else {
            return None;
        };
        if !Self::is_supported_local_image_path(&path) {
            return None;
        }
        Some(path)
    }

    /// Checks that a path is an existing file with a supported image extension.
    fn is_supported_local_image_path(path: &std::path::Path) -> bool {
        if !path.is_file() {
            return false;
        }
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tif" | "tiff"
        )
    }

    /// Splits the title around the clean selection for image insertion.
    pub(crate) fn paste_image_split(&self) -> (InlineTextTree, InlineTextTree) {
        let clean_selected = self.selection_clean_range();
        let (leading, tail) = self.record.title.split_at(clean_selected.start);
        let (_, trailing) = tail.split_at(clean_selected.end.saturating_sub(clean_selected.start));
        (leading, trailing)
    }

    /// Returns whether a quote has no children or embedded newline.
    fn is_leaf_quote(&self) -> bool {
        self.kind() == BlockKind::Quote
            && self.children.is_empty()
            && !self.display_text().contains('\n')
    }

    /// Returns whether a callout has no child blocks.
    fn is_leaf_callout(&self) -> bool {
        matches!(self.kind(), BlockKind::Callout(_)) && self.children.is_empty()
    }

    /// Returns whether an unselected leaf quote has no visible text.
    fn is_empty_leaf_quote(&self) -> bool {
        self.is_leaf_quote() && self.selected_range.is_empty() && self.is_empty()
    }

    /// Converts a leaf callout at its start into a quote containing its header.
    fn downgrade_leaf_callout_to_quote_at_start(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_leaf_callout() || !self.selected_range.is_empty() || self.cursor_offset() != 0 {
            return false;
        }

        let BlockKind::Callout(variant) = self.kind() else {
            return false;
        };
        let header_markdown = variant.header_markdown(&self.record.title.serialize_markdown());
        self.record.kind = BlockKind::Quote;
        self.record
            .set_title(InlineTextTree::from_markdown(&header_markdown));
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        self.assign_collapsed_selection_offset(0, CollapsedCaretAffinity::Default, None);
        self.marked_range = None;
        self.cursor_blink_epoch = std::time::Instant::now();
        cx.emit(BlockEvent::Changed);
        cx.notify();
        true
    }

    /// Converts an empty leaf quote to a paragraph when eligible.
    fn downgrade_empty_leaf_quote_to_paragraph(&mut self, cx: &mut Context<Self>) -> bool {
        if self.is_empty_leaf_quote() {
            self.convert_to_paragraph(cx);
            return true;
        }
        false
    }

    /// Keeps the column append control visible across its edge, zone, or button.
    fn table_append_column_should_stay_visible(&self) -> bool {
        self.table_append_column_edge_hovered
            || self.table_append_column_zone_hovered
            || self.table_append_column_button_hovered
    }

    /// Keeps the row append control visible across its edge, zone, or button.
    fn table_append_row_should_stay_visible(&self) -> bool {
        self.table_append_row_edge_hovered
            || self.table_append_row_zone_hovered
            || self.table_append_row_button_hovered
    }

    /// Hides the column append control after a short hover-exit delay.
    fn schedule_table_append_column_close(&mut self, cx: &mut Context<Self>) {
        if !self.table_append_column_hovered {
            return;
        }

        self.table_append_column_close_task = Some(cx.spawn(
            async |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = this.update(cx, |block, cx| {
                    block.table_append_column_close_task = None;
                    if !block.table_append_column_should_stay_visible() {
                        block.table_append_column_hovered = false;
                        cx.notify();
                    }
                });
            },
        ));
    }

    /// Hides the row append control after a short hover-exit delay.
    fn schedule_table_append_row_close(&mut self, cx: &mut Context<Self>) {
        if !self.table_append_row_hovered {
            return;
        }

        self.table_append_row_close_task = Some(cx.spawn(
            async |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = this.update(cx, |block, cx| {
                    block.table_append_row_close_task = None;
                    if !block.table_append_row_should_stay_visible() {
                        block.table_append_row_hovered = false;
                        cx.notify();
                    }
                });
            },
        ));
    }

    /// Updates one or more column append hover parts and the delayed close.
    fn set_table_append_column_hover_part(
        &mut self,
        edge_hovered: Option<bool>,
        zone_hovered: Option<bool>,
        button_hovered: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        if let Some(edge_hovered) = edge_hovered
            && self.table_append_column_edge_hovered != edge_hovered
        {
            self.table_append_column_edge_hovered = edge_hovered;
            changed = true;
        }
        if let Some(zone_hovered) = zone_hovered
            && self.table_append_column_zone_hovered != zone_hovered
        {
            self.table_append_column_zone_hovered = zone_hovered;
            changed = true;
        }
        if let Some(button_hovered) = button_hovered
            && self.table_append_column_button_hovered != button_hovered
        {
            self.table_append_column_button_hovered = button_hovered;
            changed = true;
        }

        if self.table_append_column_should_stay_visible() {
            self.table_append_column_close_task = None;
            if !self.table_append_column_hovered {
                self.table_append_column_hovered = true;
                changed = true;
            }
        } else if self.table_append_column_hovered && self.table_append_column_close_task.is_none()
        {
            self.schedule_table_append_column_close(cx);
        }

        if changed {
            cx.notify();
        }
    }

    /// Updates one or more row append hover parts and the delayed close.
    fn set_table_append_row_hover_part(
        &mut self,
        edge_hovered: Option<bool>,
        zone_hovered: Option<bool>,
        button_hovered: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        if let Some(edge_hovered) = edge_hovered
            && self.table_append_row_edge_hovered != edge_hovered
        {
            self.table_append_row_edge_hovered = edge_hovered;
            changed = true;
        }
        if let Some(zone_hovered) = zone_hovered
            && self.table_append_row_zone_hovered != zone_hovered
        {
            self.table_append_row_zone_hovered = zone_hovered;
            changed = true;
        }
        if let Some(button_hovered) = button_hovered
            && self.table_append_row_button_hovered != button_hovered
        {
            self.table_append_row_button_hovered = button_hovered;
            changed = true;
        }

        if self.table_append_row_should_stay_visible() {
            self.table_append_row_close_task = None;
            if !self.table_append_row_hovered {
                self.table_append_row_hovered = true;
                changed = true;
            }
        } else if self.table_append_row_hovered && self.table_append_row_close_task.is_none() {
            self.schedule_table_append_row_close(cx);
        }

        if changed {
            cx.notify();
        }
    }

    /// If the code block's last line is a bare fence (three or more backticks
    /// or tildes, no info string), returns the byte offset to cut from so the
    /// whole line is removed; otherwise `None`.
    fn trailing_code_fence_line_start(&self) -> Option<usize> {
        let text = self.display_text();
        let line_start = text.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let is_bare_fence = BlockKind::parse_code_fence_opening(&text[line_start..])
            .is_some_and(|fence| fence.language.is_none());
        // Cut from the preceding newline too, unless the fence is the only line.
        is_bare_fence.then(|| line_start.saturating_sub(1))
    }

    /// Handles Enter using table, source, structural, and rich-text semantics.
    pub(crate) fn on_newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        // Enter is ordered from special editors to rich-text splitting:
        // table/source/code/quote-like blocks keep local newline semantics,
        // while normal rendered blocks emit an editor-level split request.
        if self.is_table_cell() {
            cx.emit(BlockEvent::RequestTableCellMoveVertical { delta: 1 });
            return;
        }

        if self.editor_selection_range.is_some() {
            cx.emit(BlockEvent::RequestReplaceCrossBlockSelection {
                text: "\n".to_string(),
                selected_range_relative: None,
                mark_inserted_text: false,
                undo_kind: UndoCaptureKind::NonCoalescible,
            });
            return;
        }

        if self.is_source_raw_mode() {
            if !self.selected_range.is_empty() {
                self.replace_text_in_range(None, "", window, cx);
            }
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }

        if self.kind() == BlockKind::Paragraph
            && self.selected_range.is_empty()
            && self.cursor_offset() == self.visible_len()
            && BlockKind::parse_separator_line(self.display_text())
            // A dash run is also a setext underline; defer it to the editor so a
            // preceding paragraph can become a heading (the editor falls back to
            // a separator when there is no heading target).
            && BlockKind::parse_setext_underline(self.display_text()).is_none()
        {
            self.convert_to_separator(cx);
            cx.emit(BlockEvent::RequestNewline {
                trailing: InlineTextTree::plain(String::new()),
                source_already_mutated: true,
            });
            return;
        }

        // `$$` then Enter opens a display-math block. Keying off the caret sitting
        // right after a leading `$$` (rather than the line being exactly `$$`)
        // means it also fires after pressing Home on an existing line and typing
        // the fence in front of a formula: the rest of the line becomes the math
        // body instead of being split off into a new paragraph.
        if self.kind() == BlockKind::Paragraph
            && self.selected_range.is_empty()
            && self.cursor_offset() == "$$".len()
            && self.display_text().starts_with("$$")
        {
            let body = self.display_text()["$$".len()..].to_string();
            self.enter_math_block(&body, cx);
            return;
        }

        if self.kind() == BlockKind::Paragraph
            && self.selected_range.is_empty()
            && self.cursor_offset() == self.visible_len()
            && let Some(fence) = BlockKind::parse_code_fence_opening(self.display_text())
        {
            self.enter_code_block(fence.language, cx);
            return;
        }

        if self.kind().is_separator() {
            cx.emit(BlockEvent::RequestNewline {
                trailing: InlineTextTree::plain(String::new()),
                source_already_mutated: false,
            });
            return;
        }

        if self.kind().is_list_item() && self.selected_range.is_empty() && self.is_empty() {
            cx.emit(BlockEvent::RequestOutdent);
            return;
        }

        if self.kind() == BlockKind::Quote {
            if !self.selected_range.is_empty() {
                self.replace_text_in_range(None, "", window, cx);
            }
            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }

        if matches!(self.kind(), BlockKind::Callout(_)) {
            cx.emit(BlockEvent::RequestEnterCalloutBody);
            return;
        }

        // In a code block, Enter inserts a newline into the block content
        // rather than splitting the block.  Pressing Enter on an empty
        // code block exits back to a paragraph.
        if self.kind().is_code_block() {
            if self.selected_range.is_empty() && self.is_empty() {
                self.convert_to_paragraph(cx);
                return;
            }
            // Typing a bare closing fence on the last line and pressing Enter
            // leaves the block, matching source mode.
            if self.selected_range.is_empty()
                && self.cursor_offset() == self.visible_len()
                && let Some(fence_start) = self.trailing_code_fence_line_start()
            {
                let fence_end = self.visible_len();
                self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
                self.replace_text_in_visible_range(fence_start..fence_end, "", None, false, cx);
                cx.emit(BlockEvent::RequestNewline {
                    trailing: InlineTextTree::plain(String::new()),
                    source_already_mutated: true,
                });
                return;
            }
            if !self.selected_range.is_empty() {
                self.replace_text_in_range(None, "", window, cx);
            }
            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }

        if self.collapsed_caret_inherits_inline_code_style() {
            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }

        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }

        let cursor = self.cursor_offset();
        let (leading, trailing) = self.split_title(cursor);
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.record.set_title(leading);
        self.mark_changed(cx);
        let cursor = self.visible_len();
        self.assign_collapsed_selection_offset(cursor, CollapsedCaretAffinity::Default, None);
        self.marked_range = None;
        cx.emit(BlockEvent::RequestNewline {
            trailing,
            source_already_mutated: true,
        });
    }

    /// Deletes backward or requests an editor-level structural change at start.
    pub(crate) fn on_delete_back(
        &mut self,
        _: &DeleteBack,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_table_cell() {
            if self.selected_range.is_empty() {
                let previous = self.previous_boundary(self.cursor_offset());
                if previous == self.cursor_offset() {
                    return;
                }
                self.select_to(previous, cx);
            }
            self.replace_text_in_range(None, "", window, cx);
            return;
        }

        if self.is_source_raw_mode() {
            if self.selected_range.is_empty() {
                self.select_to(self.previous_boundary(self.cursor_offset()), cx);
            }
            self.replace_text_in_range(None, "", window, cx);
            return;
        }

        if self.selected_range.is_empty() && self.cursor_offset() == 0 {
            if self.kind() == BlockKind::Paragraph && self.is_direct_list_child() && self.is_empty()
            {
                cx.emit(BlockEvent::RequestOutdent);
                return;
            }
            if self.is_nested_list_item() {
                cx.emit(BlockEvent::RequestDowngradeNestedListItemToChildParagraph);
                return;
            }
            match self.kind() {
                BlockKind::BulletedListItem
                | BlockKind::TaskListItem { .. }
                | BlockKind::NumberedListItem => {
                    cx.emit(BlockEvent::RequestOutdent);
                    return;
                }
                BlockKind::Heading { .. } => {
                    self.convert_to_paragraph(cx);
                    return;
                }
                BlockKind::Quote => {
                    if self.is_leaf_quote() {
                        self.convert_to_paragraph(cx);
                    }
                    return;
                }
                BlockKind::Callout(_) => {
                    if self.downgrade_leaf_callout_to_quote_at_start(cx) {
                        return;
                    }
                    return;
                }
                BlockKind::Separator => {
                    self.convert_to_paragraph(cx);
                    return;
                }
                BlockKind::CodeBlock { .. } => {
                    self.convert_to_paragraph(cx);
                    return;
                }
                _ => {}
            }
        }

        if self.downgrade_leaf_callout_to_quote_at_start(cx)
            || self.downgrade_empty_leaf_quote_to_paragraph(cx)
        {
            return;
        }

        if self.selected_range.is_empty() && self.display_text().is_empty() {
            cx.emit(BlockEvent::RequestDelete);
            return;
        }

        if self.selected_range.is_empty() && self.cursor_offset() == 0 {
            cx.emit(BlockEvent::RequestMergeIntoPrev {
                content: self.record.title.clone(),
            });
            return;
        }

        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Deletes forward, with kind-specific conversion in rendered mode.
    pub(crate) fn on_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_table_cell() {
            if self.selected_range.is_empty() {
                let next = self.next_boundary(self.cursor_offset());
                if next == self.cursor_offset() {
                    return;
                }
                self.select_to(next, cx);
            }
            self.replace_text_in_range(None, "", window, cx);
            return;
        }

        if self.is_source_raw_mode() {
            if self.selected_range.is_empty() {
                self.select_to(self.next_boundary(self.cursor_offset()), cx);
            }
            self.replace_text_in_range(None, "", window, cx);
            return;
        }

        if self.downgrade_leaf_callout_to_quote_at_start(cx)
            || self.downgrade_empty_leaf_quote_to_paragraph(cx)
        {
            return;
        }

        if self.kind().is_separator() {
            self.convert_to_paragraph(cx);
            return;
        }

        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Deletes to the previous word boundary, deferring start-of-block cases.
    pub(crate) fn on_word_delete_back(
        &mut self,
        _: &WordDeleteBack,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            if self.cursor_offset() == 0 {
                // Nothing to the left in this block; defer to grapheme
                // backspace, which handles block merge and downgrades.
                self.on_delete_back(&DeleteBack, window, cx);
                return;
            }
            self.select_to(self.previous_word_start(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Deletes to the next word boundary, deferring end-of-block cases.
    pub(crate) fn on_word_delete_forward(
        &mut self,
        _: &WordDeleteForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            if self.cursor_offset() == self.visible_len() {
                // Nothing to the right in this block; defer to grapheme
                // delete, which handles block merge and separator removal.
                self.on_delete(&Delete, window, cx);
                return;
            }
            self.select_to(self.next_word_start(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Indents a list, inserts spaces, or advances to the next table cell.
    pub(crate) fn on_indent_block(
        &mut self,
        _: &IndentBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_table_cell() {
            cx.emit(BlockEvent::RequestTableCellMoveHorizontal { delta: 1 });
            return;
        }
        if self.can_adjust_list_nesting() {
            cx.emit(BlockEvent::RequestIndent);
            return;
        }
        if self.kind() == BlockKind::Paragraph || self.kind().is_code_block() {
            self.replace_text_in_range(None, "    ", window, cx);
        }
    }

    /// Outdents a list or moves to the previous table cell.
    pub(crate) fn on_outdent_block(
        &mut self,
        _: &OutdentBlock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_table_cell() {
            cx.emit(BlockEvent::RequestTableCellMoveHorizontal { delta: -1 });
            return;
        }
        if self.can_outdent_list_nesting() {
            cx.emit(BlockEvent::RequestOutdent);
        }
    }

    /// Routes an unmodified Tab or Shift-Tab to indent/outdent handling.
    pub(crate) fn on_block_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "tab" {
            return;
        }

        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return;
        }

        if self.code_language_focus_handle.is_focused(window) {
            return;
        }

        if modifiers.shift {
            self.on_outdent_block(&OutdentBlock, window, cx);
        } else {
            self.on_indent_block(&IndentBlock, window, cx);
        }
        cx.stop_propagation();
    }

    /// Moves up a visual row or asks the editor to focus the previous block.
    pub(crate) fn on_focus_prev(
        &mut self,
        _: &FocusPrev,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preferred_x = self.vertical_anchor_x();
        if !self.move_cursor_vertically(-1, preferred_x, cx) {
            if self.is_table_cell() {
                cx.emit(BlockEvent::RequestTableCellMoveVertical { delta: -1 });
                return;
            }
            cx.emit(BlockEvent::RequestFocusPrev {
                preferred_x: Some(f32::from(preferred_x)),
            });
        }
    }

    /// Moves down a visual row, code-language field, table cell, or block.
    pub(crate) fn on_focus_next(
        &mut self,
        _: &FocusNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preferred_x = self.vertical_anchor_x();
        if !self.move_cursor_vertically(1, preferred_x, cx) {
            if self.is_table_cell() {
                cx.emit(BlockEvent::RequestTableCellMoveVertical { delta: 1 });
                return;
            }
            // In a code block, Down from the last content line steps into the
            // language field rather than leaving the block, so the language is
            // reachable by keyboard. A further Down there exits the block.
            if self.kind().is_code_block() && !self.code_language_focus_handle.is_focused(window) {
                self.code_language_focus_handle.focus(window, cx);
                cx.notify();
                return;
            }
            cx.emit(BlockEvent::RequestFocusNext {
                preferred_x: Some(f32::from(preferred_x)),
            });
        }
    }

    /// Moves left through projected syntax or grapheme boundaries.
    pub(crate) fn on_move_left(
        &mut self,
        _: &MoveLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            if let Some((target, affinity)) = self.projected_move_left_target(self.cursor_offset())
            {
                self.assign_collapsed_selection_offset(target, affinity, None);
                self.cursor_blink_epoch = std::time::Instant::now();
                cx.notify();
            } else {
                let previous = self.previous_boundary(self.cursor_offset());
                // At the start of a table cell, step into the previous cell
                // rather than stalling at the edge (same path as Shift+Tab).
                if previous == self.cursor_offset() && self.is_table_cell() {
                    cx.emit(BlockEvent::RequestTableCellMoveHorizontal { delta: -1 });
                    return;
                }
                self.move_to(previous, cx);
            }
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    /// Moves right through projected syntax or grapheme boundaries.
    pub(crate) fn on_move_right(
        &mut self,
        _: &MoveRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            if let Some((target, affinity)) =
                self.projected_move_right_target(self.selected_range.end)
            {
                self.assign_collapsed_selection_offset(target, affinity, None);
                self.cursor_blink_epoch = std::time::Instant::now();
                cx.notify();
            } else {
                let next = self.next_boundary(self.selected_range.end);
                // At the end of a table cell, step into the next cell rather
                // than stalling at the edge (same path as Tab).
                if next == self.selected_range.end && self.is_table_cell() {
                    cx.emit(BlockEvent::RequestTableCellMoveHorizontal { delta: 1 });
                    return;
                }
                self.move_to(next, cx);
            }
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    /// Moves the caret to the start of visible block text.
    pub(crate) fn on_home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    /// Moves the caret to the end of visible block text.
    pub(crate) fn on_end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visible_len(), cx);
    }

    /// Extends selection left through projection or grapheme boundaries.
    pub(crate) fn on_select_left(
        &mut self,
        _: &SelectLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((target, _)) = self.projected_move_left_target(self.cursor_offset()) {
            self.select_to(target, cx);
        } else {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
    }

    /// Extends selection right through projection or grapheme boundaries.
    pub(crate) fn on_select_right(
        &mut self,
        _: &SelectRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((target, _)) = self.projected_move_right_target(self.cursor_offset()) {
            self.select_to(target, cx);
        } else {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
    }

    /// Moves the caret to the preceding word start.
    pub(crate) fn on_word_move_left(
        &mut self,
        _: &WordMoveLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(self.previous_word_start(self.cursor_offset()), cx);
    }

    /// Moves the caret to the next word start.
    pub(crate) fn on_word_move_right(
        &mut self,
        _: &WordMoveRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(self.next_word_start(self.cursor_offset()), cx);
    }

    /// Extends selection to the preceding word start.
    pub(crate) fn on_word_select_left(
        &mut self,
        _: &WordSelectLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.previous_word_start(self.cursor_offset()), cx);
    }

    /// Extends selection to the next word start.
    pub(crate) fn on_word_select_right(
        &mut self,
        _: &WordSelectRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.next_word_start(self.cursor_offset()), cx);
    }

    /// Requests focus at the start of the previous visible block.
    pub(crate) fn on_block_up(
        &mut self,
        _: &BlockUp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(BlockEvent::RequestBlockUp);
    }

    /// Requests focus at the start of the next visible block.
    pub(crate) fn on_block_down(
        &mut self,
        _: &BlockDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(BlockEvent::RequestBlockDown);
    }

    /// Selects all visible text within this block.
    fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.visible_len(), cx);
    }

    /// Selects local source text or delegates rendered select-all to the editor.
    pub(crate) fn on_select_all(
        &mut self,
        _: &SelectAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_source_line_numbers() {
            self.select_all_text(cx);
        } else {
            cx.emit(BlockEvent::RequestRenderedSelectAll);
        }
    }

    /// Extends selection to the start of visible text.
    pub(crate) fn on_select_home(
        &mut self,
        _: &SelectHome,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    /// Extends selection to the end of visible text.
    pub(crate) fn on_select_end(
        &mut self,
        _: &SelectEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.visible_len(), cx);
    }

    /// Copies the selected visible text to the clipboard.
    pub(crate) fn on_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.display_text()[self.selected_range.clone()].to_string(),
            ));
        }
    }

    /// Copies and removes selected visible text as a discrete edit.
    pub(crate) fn on_cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.display_text()[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    /// Routes clipboard images, multiline text, or local text replacement.
    pub(crate) fn on_paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind().is_separator() && !self.uses_raw_text_editing() {
            return;
        }

        if let Some(item) = cx.read_from_clipboard() {
            if let Some(sources) = Self::pasted_image_sources_from_clipboard(&item) {
                let (leading, trailing) = self.paste_image_split();
                cx.emit(BlockEvent::RequestPasteImage {
                    leading,
                    sources,
                    trailing,
                });
                return;
            }

            let Some(text) = item.text() else {
                return;
            };
            if let Some(source) = Self::pasted_image_source_from_text(&text) {
                let (leading, trailing) = self.paste_image_split();
                cx.emit(BlockEvent::RequestPasteImage {
                    leading,
                    sources: vec![source],
                    trailing,
                });
                return;
            }

            // Only rendered rich-text blocks apply paste correction. Raw/code
            // contexts preserve bytes, and table cells flatten newlines so the
            // surrounding table structure is not accidentally split.
            if self.editor_selection_range.is_some() {
                cx.emit(BlockEvent::RequestReplaceCrossBlockSelection {
                    text,
                    selected_range_relative: None,
                    mark_inserted_text: false,
                    undo_kind: UndoCaptureKind::NonCoalescible,
                });
                return;
            }

            if self.is_table_cell() {
                let flattened = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
                self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
                self.replace_text_in_range(None, &flattened, window, cx);
                return;
            }

            if self.uses_raw_text_editing() {
                self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
                self.replace_text_in_range(None, &text, window, cx);
                return;
            }

            if text.contains('\n') || text.contains('\r') {
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                if self.quote_depth > 0 {
                    self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
                    self.replace_text_in_range(None, &normalized, window, cx);
                    return;
                }
                let clean_selected = self.selection_clean_range();
                let (leading, tail) = self.record.title.split_at(clean_selected.start);
                let (_, trailing) =
                    tail.split_at(clean_selected.end.saturating_sub(clean_selected.start));
                let lines = normalized
                    .split('\n')
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                let split_physical_lines = should_split_plain_multiline_paste(&lines);
                cx.emit(BlockEvent::RequestPasteMultiline {
                    leading,
                    lines,
                    trailing,
                    split_physical_lines,
                });
                return;
            }

            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    /// Returns focus from the code-language field to code content on Enter.
    pub(crate) fn on_code_language_newline(
        &mut self,
        _: &Newline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Returns focus from the code-language field to code content on dismiss.
    pub(crate) fn on_code_language_dismiss(
        &mut self,
        _: &DismissTransientUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Deletes a selected range or previous grapheme in the language field.
    pub(crate) fn on_code_language_delete_back(
        &mut self,
        _: &DeleteBack,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            let previous = self.previous_code_language_boundary(self.code_language_cursor_offset());
            self.select_code_language_to(previous, cx);
        }
        self.replace_code_language_text_in_range(
            self.code_language_selected_range.clone(),
            "",
            None,
            false,
            cx,
        );
    }

    /// Deletes a selected range or next grapheme in the language field.
    pub(crate) fn on_code_language_delete(
        &mut self,
        _: &Delete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            let next = self.next_code_language_boundary(self.code_language_cursor_offset());
            self.select_code_language_to(next, cx);
        }
        self.replace_code_language_text_in_range(
            self.code_language_selected_range.clone(),
            "",
            None,
            false,
            cx,
        );
    }

    /// Moves the language-field caret left or collapses its selection.
    pub(crate) fn on_code_language_move_left(
        &mut self,
        _: &MoveLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            self.move_code_language_to(
                self.previous_code_language_boundary(self.code_language_cursor_offset()),
                cx,
            );
        } else {
            self.move_code_language_to(self.code_language_selected_range.start, cx);
        }
    }

    /// Moves the language-field caret right or collapses its selection.
    pub(crate) fn on_code_language_move_right(
        &mut self,
        _: &MoveRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            self.move_code_language_to(
                self.next_code_language_boundary(self.code_language_cursor_offset()),
                cx,
            );
        } else {
            self.move_code_language_to(self.code_language_selected_range.end, cx);
        }
    }

    /// Moves the language-field caret to its start.
    pub(crate) fn on_code_language_home(
        &mut self,
        _: &Home,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(0, cx);
    }

    /// Moves the language-field caret to its end.
    pub(crate) fn on_code_language_end(
        &mut self,
        _: &End,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(self.code_language_text().len(), cx);
    }

    /// Extends language-field selection one grapheme left.
    pub(crate) fn on_code_language_select_left(
        &mut self,
        _: &SelectLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.select_code_language_to(
            self.previous_code_language_boundary(self.code_language_cursor_offset()),
            cx,
        );
    }

    /// Extends language-field selection one grapheme right.
    pub(crate) fn on_code_language_select_right(
        &mut self,
        _: &SelectRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.select_code_language_to(
            self.next_code_language_boundary(self.code_language_cursor_offset()),
            cx,
        );
    }

    /// Selects the entire code-language field.
    pub(crate) fn on_code_language_select_all(
        &mut self,
        _: &SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(0, cx);
        self.select_code_language_to(self.code_language_text().len(), cx);
    }

    /// Copies the selected code-language text.
    pub(crate) fn on_code_language_copy(
        &mut self,
        _: &Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if !self.code_language_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.code_language_text()[self.code_language_selected_range.clone()].to_string(),
            ));
        }
    }

    /// Copies and removes the selected code-language text.
    pub(crate) fn on_code_language_cut(
        &mut self,
        _: &Cut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if !self.code_language_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.code_language_text()[self.code_language_selected_range.clone()].to_string(),
            ));
            self.replace_code_language_text_in_range(
                self.code_language_selected_range.clone(),
                "",
                None,
                false,
                cx,
            );
        }
    }

    /// Replaces language-field selection with clipboard text.
    pub(crate) fn on_code_language_paste(
        &mut self,
        _: &Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_code_language_text_in_range(
                self.code_language_selected_range.clone(),
                &text,
                None,
                false,
                cx,
            );
        }
    }

    /// Moves focus upward from the language field to code content.
    pub(crate) fn on_code_language_focus_content(
        &mut self,
        _: &FocusPrev,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Requests the next block when moving down from the language field.
    pub(crate) fn on_code_language_focus_next(
        &mut self,
        _: &FocusNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        // Down from the language field leaves the code block: the editor focuses
        // the block below, creating a trailing paragraph first when the code
        // block is the last block. Enter does not exit (see on_code_language_newline).
        cx.emit(BlockEvent::RequestFocusNext { preferred_x: None });
    }

    /// Consumes indent while the language field is focused.
    pub(crate) fn on_code_language_indent(
        &mut self,
        _: &IndentBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_focus_handle.is_focused(window) {
            cx.stop_propagation();
        }
    }

    /// Consumes outdent while the language field is focused.
    pub(crate) fn on_code_language_outdent(
        &mut self,
        _: &OutdentBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_focus_handle.is_focused(window) {
            cx.stop_propagation();
        }
    }

    /// Focuses the language field and starts or extends a pointer selection.
    pub(crate) fn on_code_language_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.code_language_is_selecting = true;
        self.code_language_focus_handle.focus(window, cx);
        let offset = self.code_language_index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_code_language_to(offset, cx);
        } else {
            self.move_code_language_to(offset, cx);
        }
    }

    /// Ends language-field pointer selection and consumes the release.
    pub(crate) fn on_code_language_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.code_language_is_selecting = false;
    }

    /// Clears language-field drag state after a release outside its bounds.
    pub(crate) fn on_code_language_mouse_up_out(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI dispatches mouse_up_out during capture; do not stop propagation
        // here, or controls under the pointer cannot synthesize on_click.
        if self.code_language_is_selecting {
            self.code_language_is_selecting = false;
            cx.notify();
        }
    }

    /// Extends language-field selection only during an active drag.
    pub(crate) fn on_code_language_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_is_selecting {
            // A stale selecting flag can survive a missed mouse-up. Only extend
            // the selection while the platform still reports an active drag.
            if !event.dragging() {
                self.code_language_is_selecting = false;
                cx.notify();
                return;
            }
            cx.stop_propagation();
            self.select_code_language_to(
                self.code_language_index_for_mouse_position(event.position),
                cx,
            );
        }
    }

    /// Handles block focus, caret placement, word selection, or image expansion.
    pub(crate) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.showing_rendered_image() {
            self.is_selecting = false;
            self.request_image_edit_expansion();
            if self.focus_handle.is_focused(window) {
                if self.sync_image_focus_state(true) {
                    cx.notify();
                }
            } else {
                cx.emit(BlockEvent::RequestFocus);
            }
            cx.stop_propagation();
            return;
        }

        let offset = self.index_for_mouse_position(event.position);
        let was_focused = self.focus_handle.is_focused(window);

        // Double click to select a word
        if event.button == MouseButton::Left && event.click_count == 2 {
            let text = self.display_text();

            let range = text
                .split_word_bound_indices()
                .find(|(start, segment)| {
                    let end = start + segment.len();
                    *start <= offset && offset < end
                })
                .map(|(start, segment)| start..start + segment.len());

            if let Some(range) = range {
                self.selected_range = range;
            }

            cx.notify();

            return;
        }

        // Cmd/Ctrl+click follows a rendered link instead of editing it, so the
        // block is neither focused nor selected; the link opens on mouse-up.
        if event.modifiers.secondary() && self.pointer_link_hit(event.position).is_some() {
            self.is_selecting = false;
            cx.stop_propagation();
            return;
        }

        if was_focused {
            self.is_selecting = true;
            if event.modifiers.shift {
                self.select_to(offset, cx);
            } else {
                self.move_to(offset, cx);
            }
        } else {
            self.is_selecting = false;
            self.move_to(offset, cx);
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    /// Resolve the inline link under a pointer position against the most recent
    /// rendered text layout, if any. Returns `None` while the block shows raw
    /// source or when the pointer is not over a link.
    pub(crate) fn pointer_link_hit(&self, position: Point<Pixels>) -> Option<super::InlineLinkHit> {
        self.last_layout
            .as_ref()
            .zip(self.last_bounds)
            .and_then(|(lines, bounds)| {
                super::element::link_at_position(
                    self,
                    lines,
                    bounds,
                    self.last_line_height,
                    position,
                )
            })
            .cloned()
    }

    /// Handle mouse-down on a rendered inline link (in a mixed inline-visual
    /// block). A Cmd/Ctrl+click is claimed here so it follows the link instead
    /// of focusing the block; the destination opens on the matching mouse-up.
    pub(crate) fn on_rendered_link_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only Cmd/Ctrl+click follows the link; a plain click falls through so
        // the block focuses for editing like any other inline text.
        if event.modifiers.secondary() {
            cx.stop_propagation();
        }
    }

    /// Open a rendered inline link's destination through the editor prompt.
    pub(crate) fn open_rendered_link(
        &mut self,
        link: &super::InlineLinkHit,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        cx.emit(BlockEvent::RequestOpenLink {
            prompt_target: link.prompt_target.clone(),
            open_target: link.open_target.clone(),
        });
    }

    /// Ends selection and follows modified-click links or double-click targets.
    pub(crate) fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;

        // Cmd/Ctrl+click follows a rendered link, using the same open-link
        // prompt as the double-click gesture below.
        if event.modifiers.secondary()
            && let Some(link) = self.pointer_link_hit(event.position)
        {
            self.open_rendered_link(&link, cx);
            return;
        }

        if event.click_count >= 2 {
            let footnote = self
                .last_layout
                .as_ref()
                .zip(self.last_bounds)
                .and_then(|(lines, bounds)| {
                    super::element::footnote_at_position(
                        self,
                        lines,
                        bounds,
                        self.last_line_height,
                        event.position,
                    )
                })
                .cloned();
            if let Some(footnote) = footnote {
                cx.stop_propagation();
                cx.emit(BlockEvent::RequestJumpToFootnoteDefinition { id: footnote.id });
                return;
            }

            if let Some(link) = self.pointer_link_hit(event.position) {
                self.open_rendered_link(&link, cx);
            }
        }
    }

    /// Focuses the block before activating its footnote back-reference.
    pub(crate) fn on_footnote_backref_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.focus_handle.is_focused(window) {
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    /// Requests navigation to the first reference of this footnote.
    pub(crate) fn on_footnote_backref_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.footnote_definition_id() else {
            return;
        };
        cx.stop_propagation();
        cx.emit(BlockEvent::RequestJumpToFootnoteBackref { id });
    }

    /// Extends block text selection while an active drag continues.
    pub(crate) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            // A stale selecting flag can survive a missed mouse-up. Only extend
            // the selection while the platform still reports an active drag.
            if !event.dragging() {
                self.is_selecting = false;
                cx.notify();
                return;
            }
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    /// Focuses the block before activating its task checkbox.
    pub(crate) fn on_task_checkbox_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.focus_handle.is_focused(window) {
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    /// Requests a task checked-state toggle in rendered mode.
    pub(crate) fn on_task_checkbox_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.kind().is_task_list_item() || self.is_source_raw_mode() {
            return;
        }

        cx.stop_propagation();
        cx.emit(BlockEvent::ToggleTaskChecked);
    }

    /// Tracks hover over the column append zone.
    pub(crate) fn on_table_append_column_zone_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(None, Some(*hovered), None, cx);
    }

    /// Tracks hover over the column append button.
    pub(crate) fn on_table_append_column_button_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(None, None, Some(*hovered), cx);
    }

    /// Tracks hover over the row append zone.
    pub(crate) fn on_table_append_row_zone_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(None, Some(*hovered), None, cx);
    }

    /// Tracks hover over the row append button.
    pub(crate) fn on_table_append_row_button_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(None, None, Some(*hovered), cx);
    }

    /// Tracks hover over the table's column append edge.
    pub(crate) fn on_table_append_column_edge_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(Some(*hovered), None, None, cx);
    }

    /// Tracks hover over the table's row append edge.
    pub(crate) fn on_table_append_row_edge_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(Some(*hovered), None, None, cx);
    }

    /// Requests another column from the editor for a native table.
    pub(crate) fn on_append_table_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind() == BlockKind::Table {
            cx.emit(BlockEvent::RequestAppendTableColumn);
        }
    }

    /// Requests another body row from the editor for a native table.
    pub(crate) fn on_append_table_row(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind() == BlockKind::Table {
            cx.emit(BlockEvent::RequestAppendTableRow);
        }
    }

    /// Toggles bold formatting on the current inline selection.
    pub(crate) fn on_bold_selection(
        &mut self,
        _: &BoldSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Bold, cx);
    }

    /// Toggles italic formatting on the current inline selection.
    pub(crate) fn on_italic_selection(
        &mut self,
        _: &ItalicSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Italic, cx);
    }

    /// Toggles underline formatting on the current inline selection.
    pub(crate) fn on_underline_selection(
        &mut self,
        _: &UnderlineSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Underline, cx);
    }

    /// Toggles inline-code formatting on the current selection.
    pub(crate) fn on_code_selection(
        &mut self,
        _: &CodeSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Code, cx);
    }

    /// Converts the current block to a first-level heading.
    pub(crate) fn on_heading_1(
        &mut self,
        _: &Heading1,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(1, cx);
    }

    /// Converts the current block to a second-level heading.
    pub(crate) fn on_heading_2(
        &mut self,
        _: &Heading2,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(2, cx);
    }

    /// Converts the current block to a third-level heading.
    pub(crate) fn on_heading_3(
        &mut self,
        _: &Heading3,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(3, cx);
    }

    /// Converts the current block to a fourth-level heading.
    pub(crate) fn on_heading_4(
        &mut self,
        _: &Heading4,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(4, cx);
    }

    /// Converts the current block to a fifth-level heading.
    pub(crate) fn on_heading_5(
        &mut self,
        _: &Heading5,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(5, cx);
    }

    /// Converts the current block to a sixth-level heading.
    pub(crate) fn on_heading_6(
        &mut self,
        _: &Heading6,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_heading_level(6, cx);
    }

    /// Requests a following block or a quote/callout break when applicable.
    pub(crate) fn on_exit_code_block(
        &mut self,
        _: &ExitCodeBlock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exits_multiline_block = self.is_table_cell() || self.kind().is_multiline_text_block();

        if exits_multiline_block {
            cx.emit(BlockEvent::RequestNewline {
                trailing: InlineTextTree::plain(String::new()),
                source_already_mutated: false,
            });
        } else if self.callout_depth > 0 {
            cx.emit(BlockEvent::RequestCalloutBreak);
        } else if self.quote_depth > 0 {
            cx.emit(BlockEvent::RequestQuoteBreak);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Block;
    use crate::components::{BlockKind, BlockRecord, InlineTextTree, PastedImageSource};
    use gpui::{
        AppContext, ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat,
        TestAppContext,
    };
    use std::fs;

    fn temp_image_path(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "velotype-paste-image-path-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("temp image dir should exist");
        let path = root.join(name);
        fs::write(
            &path,
            b"not a real image; extension is enough for paste routing",
        )
        .expect("temp image should be written");
        path
    }

    fn remove_temp_image(path: &std::path::Path) {
        let _ = path.parent().map(|parent| fs::remove_dir_all(parent));
    }

    #[gpui::test]
    async fn append_column_button_stays_visible_while_crossing_hover_gap(cx: &mut TestAppContext) {
        let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

        block.update(cx, |block, cx| {
            block.set_table_append_column_hover_part(Some(true), None, None, cx);
            assert!(block.table_append_column_hovered);

            block.set_table_append_column_hover_part(Some(false), None, Some(true), cx);
            assert!(block.table_append_column_hovered);
            assert!(!block.table_append_column_edge_hovered);
            assert!(!block.table_append_column_zone_hovered);
            assert!(block.table_append_column_button_hovered);
            assert!(block.table_append_column_close_task.is_none());
        });
    }

    #[test]
    fn paste_image_text_accepts_plain_local_image_path() {
        let path = temp_image_path("copied.png");
        let text = path.to_string_lossy().to_string();
        #[cfg(target_os = "windows")]
        assert!(
            text.contains(':'),
            "test should exercise Windows drive-letter paths"
        );

        let source = Block::pasted_image_source_from_text(&text);

        assert_eq!(source, Some(PastedImageSource::LocalPath(path.clone())));
        remove_temp_image(&path);
    }

    #[test]
    fn paste_image_external_path_uses_existing_local_image_route() {
        let path = temp_image_path("copied.png");
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                vec![path.clone()].into(),
            ))],
        };

        assert_eq!(
            Block::pasted_image_sources_from_clipboard(&item),
            Some(vec![PastedImageSource::LocalPath(path.clone())])
        );
        remove_temp_image(&path);
    }

    #[test]
    fn paste_multiple_local_images_is_one_batch() {
        let first = temp_image_path("first.png");
        let second = temp_image_path("second.jpg");
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                vec![first.clone(), second.clone()].into(),
            ))],
        };
        assert_eq!(
            Block::pasted_image_sources_from_clipboard(&item),
            Some(vec![
                PastedImageSource::LocalPath(first.clone()),
                PastedImageSource::LocalPath(second.clone()),
            ])
        );
        remove_temp_image(&first);
        remove_temp_image(&second);
    }

    #[test]
    fn paste_multiple_clipboard_bitmaps_is_one_batch() {
        let images = (0..2)
            .map(|id| Image {
                format: ImageFormat::Png,
                bytes: vec![id as u8].into(),
                id,
            })
            .collect::<Vec<_>>();
        let item = ClipboardItem {
            entries: images.iter().cloned().map(ClipboardEntry::Image).collect(),
        };
        assert_eq!(
            Block::pasted_image_sources_from_clipboard(&item),
            Some(
                images
                    .into_iter()
                    .map(PastedImageSource::ClipboardImage)
                    .collect()
            )
        );
    }

    #[test]
    fn paste_image_text_accepts_quoted_local_image_path() {
        let path = temp_image_path("quoted image.png");
        let text = format!("\"{}\"", path.display());

        let source = Block::pasted_image_source_from_text(&text);

        assert_eq!(source, Some(PastedImageSource::LocalPath(path.clone())));
        remove_temp_image(&path);
    }

    #[test]
    fn paste_image_text_accepts_file_url() {
        let path = temp_image_path("url image.png");
        let url = url::Url::from_file_path(&path).expect("temp image path should form file URL");

        let source = Block::pasted_image_source_from_text(url.as_str());

        assert_eq!(source, Some(PastedImageSource::LocalPath(path.clone())));
        remove_temp_image(&path);
    }

    #[test]
    fn paste_image_text_rejects_non_image_path() {
        let path = temp_image_path("notes.txt");
        let text = path.to_string_lossy().to_string();

        let source = Block::pasted_image_source_from_text(&text);

        assert_eq!(source, None);
        remove_temp_image(&path);
    }

    #[gpui::test]
    async fn append_row_button_stays_visible_while_crossing_hover_gap(cx: &mut TestAppContext) {
        let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

        block.update(cx, |block, cx| {
            block.set_table_append_row_hover_part(Some(true), None, None, cx);
            assert!(block.table_append_row_hovered);

            block.set_table_append_row_hover_part(Some(false), None, Some(true), cx);
            assert!(block.table_append_row_hovered);
            assert!(!block.table_append_row_edge_hovered);
            assert!(!block.table_append_row_zone_hovered);
            assert!(block.table_append_row_button_hovered);
            assert!(block.table_append_row_close_task.is_none());
        });
    }

    #[gpui::test]
    async fn column_edge_hover_reveals_only_column_append_control(cx: &mut TestAppContext) {
        let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

        block.update(cx, |block, cx| {
            block.set_table_append_column_hover_part(Some(true), None, None, cx);
            assert!(block.table_append_column_edge_hovered);
            assert!(block.table_append_column_hovered);
            assert!(!block.table_append_row_hovered);
            assert!(block.table_append_column_close_task.is_none());
            assert!(block.table_append_row_close_task.is_none());
        });
    }

    #[gpui::test]
    async fn row_edge_hover_reveals_only_row_append_control(cx: &mut TestAppContext) {
        let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

        block.update(cx, |block, cx| {
            block.set_table_append_row_hover_part(Some(true), None, None, cx);
            assert!(block.table_append_row_edge_hovered);
            assert!(block.table_append_row_hovered);
            assert!(!block.table_append_column_hovered);
            assert!(block.table_append_column_close_task.is_none());
            assert!(block.table_append_row_close_task.is_none());
        });
    }

    #[gpui::test]
    async fn multiline_quote_is_not_treated_as_leaf(cx: &mut TestAppContext) {
        let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

        block.update(cx, |block, cx| {
            block.record.kind = BlockKind::Quote;
            block.record.set_title(InlineTextTree::plain("first\n"));
            block.sync_edit_mode_from_kind();
            block.sync_render_cache();
            cx.notify();

            assert!(!block.is_leaf_quote());
        });
    }
}
