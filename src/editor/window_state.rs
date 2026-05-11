//! Window-level editor state such as scrolling, mode switching, and menus.

use super::*;

impl Editor {
    pub(super) fn scrollbar_geometry(
        viewport_height: f32,
        max_scroll_y: f32,
        current_scroll_y: f32,
    ) -> ScrollbarGeometry {
        let track_height = viewport_height.max(20.0);
        let content_height = viewport_height + max_scroll_y;
        let thumb_height = if max_scroll_y > 0.5 {
            (track_height * (viewport_height / content_height)).clamp(28.0, track_height)
        } else {
            track_height
        };
        let progress = if max_scroll_y > 0.0 {
            current_scroll_y.clamp(0.0, max_scroll_y) / max_scroll_y
        } else {
            0.0
        };
        let thumb_top = (track_height - thumb_height).max(0.0) * progress;
        ScrollbarGeometry {
            track_height,
            thumb_height,
            thumb_top,
            max_scroll_y,
        }
    }

    pub(super) fn scroll_offset_for_thumb_top(
        thumb_top: f32,
        track_height: f32,
        thumb_height: f32,
        max_scroll_y: f32,
    ) -> f32 {
        if max_scroll_y <= 0.0 {
            return 0.0;
        }

        let travel = (track_height - thumb_height).max(0.0);
        if travel <= 0.0 {
            return 0.0;
        }

        let progress = (thumb_top / travel).clamp(0.0, 1.0);
        max_scroll_y * progress
    }

    /// Linearly interpolates the editor content width ratio based on viewport
    /// width. The column stays full-width until `centered_shrink_start`, then
    /// shrinks to `centered_min_ratio` at `centered_shrink_end`.
    pub(super) fn centered_column_ratio(
        viewport_width: f32,
        dimensions: &crate::theme::ThemeDimensions,
    ) -> f32 {
        if viewport_width <= dimensions.centered_shrink_start {
            return 1.0;
        }

        let t = ((viewport_width - dimensions.centered_shrink_start)
            / (dimensions.centered_shrink_end - dimensions.centered_shrink_start))
            .clamp(0.0, 1.0);
        1.0 - t * (1.0 - dimensions.centered_min_ratio)
    }

    pub(crate) fn centered_column_width(
        viewport_width: f32,
        dimensions: &crate::theme::ThemeDimensions,
    ) -> f32 {
        let available_content_width = (viewport_width - dimensions.editor_padding * 2.0).max(1.0);
        let centered_ratio = Self::centered_column_ratio(viewport_width, dimensions);
        (available_content_width * centered_ratio)
            .max(320.0)
            .min(available_content_width)
    }

    /// Builds the OS window title, including the dirty marker when the
    /// document has unsaved changes.
    pub(super) fn window_title(
        file_path: Option<&Path>,
        is_dirty: bool,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        let base_title = if let Some(path) = file_path {
            format!(
                "Velotype - {}",
                path.file_name().map_or_else(
                    || path.to_string_lossy().to_string(),
                    |name| name.to_string_lossy().to_string()
                )
            )
        } else {
            "Velotype".to_string()
        };

        if is_dirty && !strings.dirty_title_marker.is_empty() {
            format!("{} {}", strings.dirty_title_marker, base_title)
        } else {
            base_title
        }
    }

    pub(crate) fn on_toggle_view_mode(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.toggle_view_mode(cx);
    }

    pub(crate) fn on_undo(
        &mut self,
        _: &crate::components::Undo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_document(cx);
    }

    pub(crate) fn on_save_document(
        &mut self,
        _: &crate::components::SaveDocument,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_save_document(cx);
    }

    pub(crate) fn on_save_document_as(
        &mut self,
        _: &crate::components::SaveDocumentAs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_save_document_as(cx);
    }

    pub(crate) fn on_export_html(
        &mut self,
        _: &crate::components::ExportHtml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document_via_prompt(crate::export::ExportFormat::Html, window, cx);
    }

    pub(crate) fn on_export_pdf(
        &mut self,
        _: &crate::components::ExportPdf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document_via_prompt(crate::export::ExportFormat::Pdf, window, cx);
    }

    pub(crate) fn on_quit_application(
        &mut self,
        _: &crate::components::QuitApplication,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close_current_window(window, cx);
    }

    pub(crate) fn toggle_view_mode(&mut self, cx: &mut Context<Self>) {
        let selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.clear_cross_block_selection(cx);
        match self.view_mode {
            ViewMode::Rendered => {
                let markdown = self.document.markdown_text(cx);
                let block = Self::new_block(cx, BlockRecord::paragraph(markdown));
                block.update(cx, |block, _cx| block.set_source_raw_mode());
                self.document.replace_roots(vec![block], cx);
                self.view_mode = ViewMode::Source;
                self.table_cells.clear();
            }
            ViewMode::Source => {
                let source = self.document.raw_source_text(cx);
                let mut roots = Self::build_root_blocks_from_markdown(cx, &source);
                if roots.is_empty() {
                    roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
                }
                self.document.replace_roots(roots, cx);
                self.view_mode = ViewMode::Rendered;
                self.rebuild_table_runtimes(cx);
                self.rebuild_image_runtimes(cx);
            }
        }

        self.apply_selection_snapshot_in_current_mode(&selection_snapshot, cx);
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        self.last_scroll_viewport_size = None;
        self.pending_window_title_refresh = true;
        self.close_dialog_restore_focus = None;
        self.table_axis_preview = None;
        self.table_axis_selection = None;
        self.dismiss_contextual_overlays(cx);
        self.sync_table_axis_visuals(cx);
        self.refresh_stable_document_snapshot(cx);
        cx.notify();
    }

    /// Marks the document dirty and schedules window-title and edited-state
    /// refresh for the next render frame.
    pub(super) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        if !self.document_dirty {
            self.document_dirty = true;
            self.pending_window_edited = true;
            self.pending_window_title_refresh = true;
            cx.notify();
        }
    }

    pub(super) fn request_active_block_scroll_into_view(&mut self, cx: &mut Context<Self>) {
        self.pending_scroll_recheck_after_layout = true;
        if !self.pending_scroll_active_block_into_view {
            self.pending_scroll_active_block_into_view = true;
            cx.notify();
        }
    }

    pub(super) fn viewport_size_changed(previous: Size<Pixels>, current: Size<Pixels>) -> bool {
        const EPSILON: f32 = 0.5;

        (f32::from(previous.width) - f32::from(current.width)).abs() > EPSILON
            || (f32::from(previous.height) - f32::from(current.height)).abs() > EPSILON
    }

    pub(crate) fn show_info_dialog(&mut self, kind: InfoDialogKind, cx: &mut Context<Self>) {
        if self.show_unsaved_changes_dialog {
            return;
        }

        self.menu_bar_open = None;
        self.menu_submenu_open = None;
        self.menu_submenu_panel_hovered = false;
        self.info_dialog = Some(kind);
        cx.notify();
    }

    pub(crate) fn hide_info_dialog(&mut self, cx: &mut Context<Self>) {
        if self.info_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn open_menu_bar(&mut self, index: usize, cx: &mut Context<Self>) {
        self.menu_close_task = None;
        if self.menu_bar_open != Some(index) {
            self.menu_bar_open = Some(index);
            self.menu_submenu_open = None;
            self.menu_submenu_panel_hovered = false;
            cx.notify();
        }
    }

    pub(crate) fn open_menu_submenu(&mut self, index: usize, cx: &mut Context<Self>) {
        self.menu_close_task = None;
        if self.menu_submenu_open != Some(index) {
            self.menu_submenu_open = Some(index);
            cx.notify();
        }
    }

    pub(crate) fn close_menu_submenu(&mut self, cx: &mut Context<Self>) {
        let had_open_submenu = self.menu_submenu_open.take().is_some();
        let had_submenu_hover = self.menu_submenu_panel_hovered;
        self.menu_submenu_panel_hovered = false;
        if had_open_submenu || had_submenu_hover {
            cx.notify();
        }
    }

    pub(super) fn schedule_menu_bar_close(&mut self, cx: &mut Context<Self>) {
        if self.menu_bar_open.is_none() {
            return;
        }

        let weak_editor = cx.entity().downgrade();
        self.menu_close_task = Some(cx.spawn(
            async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = weak_editor.update(cx, |editor, cx| {
                    editor.menu_close_task = None;
                    if !editor.menu_bar_hovered
                        && !editor.menu_panel_hovered
                        && !editor.menu_submenu_panel_hovered
                    {
                        editor.close_menu_bar(cx);
                    }
                });
            },
        ));
    }

    pub(crate) fn set_menu_bar_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.menu_bar_hovered = hovered;
        if hovered {
            self.menu_close_task = None;
        } else if !self.menu_panel_hovered && !self.menu_submenu_panel_hovered {
            self.schedule_menu_bar_close(cx);
        }
    }

    pub(crate) fn set_menu_panel_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.menu_panel_hovered = hovered;
        if hovered {
            self.menu_close_task = None;
        } else if !self.menu_bar_hovered && !self.menu_submenu_panel_hovered {
            self.schedule_menu_bar_close(cx);
        }
    }

    pub(crate) fn set_menu_submenu_panel_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.menu_submenu_panel_hovered = hovered;
        if hovered {
            self.menu_close_task = None;
        } else if !self.menu_bar_hovered && !self.menu_panel_hovered {
            self.schedule_menu_bar_close(cx);
        }
    }

    pub(crate) fn dismiss_menu_bar_from_body(&mut self, cx: &mut Context<Self>) {
        if self.menu_bar_open.is_some() {
            self.close_menu_bar(cx);
        }
    }

    pub(crate) fn request_save_document(&mut self, cx: &mut Context<Self>) {
        if !self.pending_save {
            self.pending_save = true;
            cx.notify();
        }
    }

    pub(crate) fn request_save_document_as(&mut self, cx: &mut Context<Self>) {
        if !self.pending_save_as {
            self.pending_save_as = true;
            cx.notify();
        }
    }

    pub(crate) fn request_open_link_prompt(
        &mut self,
        prompt_target: String,
        open_target: String,
        cx: &mut Context<Self>,
    ) {
        self.pending_open_link = Some(PendingOpenLink {
            prompt_target,
            open_target,
        });
        cx.notify();
    }

    pub(crate) fn set_view_mode_toggle_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.view_mode_toggle_hovered != hovered {
            self.view_mode_toggle_hovered = hovered;
            cx.notify();
        }
    }

    pub(crate) fn close_menu_bar(&mut self, cx: &mut Context<Self>) {
        let had_open_menu = self.menu_bar_open.take().is_some();
        let had_open_submenu = self.menu_submenu_open.take().is_some();
        let had_hover_state =
            self.menu_bar_hovered || self.menu_panel_hovered || self.menu_submenu_panel_hovered;
        let had_pending_close = self.menu_close_task.take().is_some();
        self.menu_bar_hovered = false;
        self.menu_panel_hovered = false;
        self.menu_submenu_panel_hovered = false;
        if had_open_menu || had_open_submenu || had_hover_state || had_pending_close {
            cx.notify();
        }
    }
}
