use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{AnyWindowHandle, AppContext, TestAppContext, VisualTestContext};

use super::{Editor, ViewMode};
use crate::components::{
    BlockKind, ImageResolvedSource, InlineTextTree, QuitApplication, SaveDocument,
    TableColumnAlignment, superscript_ordinal,
};
use crate::export::ExportFormat;
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::{Theme, ThemeManager};
fn init_editor_test_app(cx: &mut TestAppContext) {
    cx.update(|cx| {
        I18nManager::init(cx);
        ThemeManager::init(cx);
        crate::components::init(cx);
    });
}

fn temp_markdown_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "velotype-{test_name}-{}-{nanos}.md",
        std::process::id()
    ))
}

fn temp_export_path(test_name: &str, extension: &str) -> PathBuf {
    let mut path = temp_markdown_path(test_name);
    path.set_extension(extension);
    path
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
}

fn activate_visual_window(cx: &mut VisualTestContext) -> AnyWindowHandle {
    cx.update(|window, _cx| window.activate_window());
    cx.run_until_parked();
    cx.cx
        .update(|cx| cx.active_window().expect("window should be active"))
}

#[test]
fn centered_column_ratio_stays_full_before_shrink_start() {
    let theme = Theme::default_theme();
    assert_eq!(Editor::centered_column_ratio(900.0, &theme.dimensions), 1.0);
    assert_eq!(
        Editor::centered_column_ratio(theme.dimensions.centered_shrink_start, &theme.dimensions),
        1.0
    );
}

#[test]
fn centered_column_ratio_reaches_new_minimum() {
    let theme = Theme::default_theme();
    let ratio =
        Editor::centered_column_ratio(theme.dimensions.centered_shrink_end, &theme.dimensions);
    assert!((ratio - 0.58).abs() < f32::EPSILON);
}

#[test]
fn scrollbar_geometry_and_inverse_mapping_stay_aligned() {
    let geometry = Editor::scrollbar_geometry(400.0, 600.0, 300.0);
    assert_eq!(geometry.track_height, 400.0);
    assert!(geometry.thumb_height >= 28.0);
    assert!((geometry.thumb_top - (400.0 - geometry.thumb_height) * 0.5).abs() < 0.001);

    let scroll_y = Editor::scroll_offset_for_thumb_top(
        geometry.thumb_top,
        geometry.track_height,
        geometry.thumb_height,
        geometry.max_scroll_y,
    );
    assert!((scroll_y - 300.0).abs() < 0.001);
}

#[test]
fn scrollbar_offset_mapping_clamps_to_track_bounds() {
    let geometry = Editor::scrollbar_geometry(300.0, 450.0, 0.0);
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            -25.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        0.0
    );
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            999.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        geometry.max_scroll_y
    );
}

#[test]
fn about_dialog_body_lines_include_repository_and_star_message() {
    let strings = I18nStrings::zh_cn();
    let lines = Editor::about_dialog_body_lines(&strings);

    assert_eq!(lines[0], format!("Velotype {}", env!("CARGO_PKG_VERSION")));
    assert_eq!(
        lines[2],
        format!("GitHub: {}", super::render::ABOUT_GITHUB_URL)
    );
    assert_eq!(
        lines[3],
        "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
    );
}

#[gpui::test]
async fn about_github_link_uses_gpui_url_opening(cx: &mut TestAppContext) {
    cx.update(|cx| {
        super::render::open_about_github_url(cx);
    });

    assert_eq!(
        cx.opened_url(),
        Some(super::render::ABOUT_GITHUB_URL.to_string())
    );
}

#[gpui::test]
async fn ctrl_s_saves_rendered_mode_edit_to_existing_file(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let path = temp_markdown_path("ctrl-s-rendered-save");
    fs::write(&path, "alpha").expect("write initial markdown");
    let cleanup_path = path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) = cx.add_window_view({
        let path = path.clone();
        move |_window, cx| Editor::from_markdown(cx, "alpha".to_string(), Some(path))
    });

    cx.simulate_input("!");
    redraw(cx);
    let expected = editor.read_with(cx, |editor, cx| {
        assert!(editor.document_dirty);
        assert!(!editor.pending_save);
        editor.document.markdown_text(cx)
    });
    assert_ne!(expected, "alpha");

    cx.simulate_keystrokes("ctrl-s");
    redraw(cx);

    assert_eq!(
        fs::read_to_string(&path).expect("read saved markdown"),
        expected
    );
    editor.read_with(cx, |editor, _cx| {
        assert!(!editor.document_dirty);
        assert!(!editor.pending_save);
    });
}

#[gpui::test]
async fn window_save_action_saves_current_editor_without_global_menu_route(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let path = temp_markdown_path("window-action-save");
    fs::write(&path, "alpha").expect("write initial markdown");
    let cleanup_path = path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) = cx.add_window_view({
        let path = path.clone();
        move |_window, cx| Editor::from_markdown(cx, "alpha".to_string(), Some(path))
    });

    cx.simulate_input(" action");
    redraw(cx);
    let expected = editor.read_with(cx, |editor, cx| {
        assert!(editor.document_dirty);
        editor.document.markdown_text(cx)
    });
    assert_ne!(expected, "alpha");

    cx.dispatch_action(SaveDocument);
    redraw(cx);

    assert_eq!(
        fs::read_to_string(&path).expect("read saved markdown"),
        expected
    );
    editor.read_with(cx, |editor, _cx| {
        assert!(!editor.document_dirty);
        assert!(!editor.pending_save);
    });
}

#[gpui::test]
async fn export_html_writes_rendered_document_without_changing_editor_state(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let export_path = temp_export_path("rendered-export-html", "html");
    let cleanup_path = export_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# Title\n\nbody".to_string(), None)
    });

    editor.update(cx, |editor, cx| {
        editor.mark_dirty(cx);
        assert!(editor.document_dirty);
        assert!(editor.file_path.is_none());
        editor
            .export_document_to_path(ExportFormat::Html, &export_path, cx)
            .expect("html export should write");
        assert!(editor.document_dirty);
        assert!(editor.file_path.is_none());
    });

    let html = fs::read_to_string(&export_path).expect("read exported html");
    assert!(html.contains("<h1>Title</h1>"));
    assert!(html.contains("<p>body</p>"));
}

#[gpui::test]
async fn export_html_uses_source_mode_raw_text(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let export_path = temp_export_path("source-export-html", "html");
    let cleanup_path = export_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "rendered".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        let source_block = editor
            .document
            .first_root()
            .expect("source mode should keep one root block")
            .clone();
        source_block.update(cx, |block, _cx| {
            block.record.set_title(InlineTextTree::plain(
                "# Source\n\n<!--\n<strong>visible</strong>\n-->".to_string(),
            ));
            block.sync_render_cache();
        });
        editor
            .export_document_to_path(ExportFormat::Html, &export_path, cx)
            .expect("source html export should write");
    });

    let html = fs::read_to_string(&export_path).expect("read exported html");
    assert!(html.contains("<h1>Source</h1>"));
    assert!(html.contains("class=\"vlt-comment\""));
    assert!(html.contains("&lt;strong&gt;visible&lt;/strong&gt;"));
}

#[gpui::test]
async fn dropped_markdown_replaces_clean_editor_in_current_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let dropped_path = temp_markdown_path("drop-clean-replace");
    fs::write(
        &dropped_path,
        "# Dropped\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n",
    )
    .expect("write dropped markdown");
    let cleanup_path = dropped_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "old".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(editor.view_mode == ViewMode::Source);
    });

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.request_dropped_markdown_replace(dropped_path.clone(), window, cx);
        });
    });
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.file_path.as_ref(), Some(&dropped_path));
        assert!(editor.view_mode == ViewMode::Rendered);
        assert!(!editor.document_dirty);
        assert!(!editor.show_drop_replace_dialog);
        assert_eq!(editor.document.root_count(), 2);
        assert_eq!(
            editor
                .document
                .root_blocks()
                .last()
                .expect("table block")
                .read(cx)
                .kind(),
            BlockKind::Table
        );
        assert!(editor.document.markdown_text(cx).contains("# Dropped"));
    });
    assert_eq!(cx.cx.windows().len(), 1);
}

#[gpui::test]
async fn dropped_paths_pick_first_valid_markdown_file(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let text_path = temp_export_path("drop-ignore-non-markdown", "txt");
    let markdown_path = temp_export_path("drop-pick-markdown", "markdown");
    fs::write(&text_path, "plain").expect("write text");
    fs::write(&markdown_path, "markdown").expect("write markdown");
    let cleanup_text = text_path.clone();
    let cleanup_markdown = markdown_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_text);
        let _ = fs::remove_file(&cleanup_markdown);
    });

    assert_eq!(
        Editor::first_dropped_markdown_path(&[text_path, markdown_path.clone()]),
        Some(markdown_path)
    );
}

#[gpui::test]
async fn dirty_drop_waits_for_replace_decision_and_cancel_preserves_document(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let dropped_path = temp_markdown_path("drop-dirty-cancel");
    fs::write(&dropped_path, "dropped").expect("write dropped markdown");
    let cleanup_path = dropped_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "current".to_string(), None));
    editor.update(cx, |editor, cx| editor.mark_dirty(cx));

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.request_dropped_markdown_replace(dropped_path, window, cx);
        });
    });
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        assert!(editor.document_dirty);
        assert!(editor.show_drop_replace_dialog);
        assert_eq!(editor.document.markdown_text(cx), "current");
        assert!(editor.pending_drop_replace_path.is_some());
    });

    editor.update(cx, |editor, cx| editor.cancel_drop_replace_dialog(cx));

    editor.read_with(cx, |editor, cx| {
        assert!(editor.document_dirty);
        assert!(!editor.show_drop_replace_dialog);
        assert!(editor.pending_drop_replace_path.is_none());
        assert_eq!(editor.document.markdown_text(cx), "current");
    });
}

#[gpui::test]
async fn dirty_drop_can_replace_without_saving(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let dropped_path = temp_markdown_path("drop-dirty-discard");
    fs::write(&dropped_path, "dropped").expect("write dropped markdown");
    let cleanup_path = dropped_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_path);
    });

    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "current".to_string(), None));
    editor.update(cx, |editor, cx| editor.mark_dirty(cx));

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.request_dropped_markdown_replace(dropped_path.clone(), window, cx);
            editor.discard_pending_drop_replace(window, cx);
        });
    });
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.file_path.as_ref(), Some(&dropped_path));
        assert_eq!(editor.document.markdown_text(cx), "dropped");
        assert!(!editor.document_dirty);
        assert!(!editor.show_drop_replace_dialog);
    });
}

#[gpui::test]
async fn dirty_drop_saves_existing_document_before_replace(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let current_path = temp_markdown_path("drop-save-current");
    let dropped_path = temp_markdown_path("drop-save-replace");
    fs::write(&current_path, "original").expect("write current markdown");
    fs::write(&dropped_path, "dropped").expect("write dropped markdown");
    let cleanup_current = current_path.clone();
    let cleanup_dropped = dropped_path.clone();
    cx.on_quit(move || {
        let _ = fs::remove_file(&cleanup_current);
        let _ = fs::remove_file(&cleanup_dropped);
    });

    let (editor, cx) = cx.add_window_view({
        let current_path = current_path.clone();
        move |_window, cx| Editor::from_markdown(cx, "original".to_string(), Some(current_path))
    });

    editor.update(cx, |editor, cx| {
        let first = editor.document.first_root().expect("current root").clone();
        first.update(cx, |block, _cx| {
            block
                .record
                .set_title(InlineTextTree::plain("edited".to_string()));
            block.sync_render_cache();
        });
        editor.mark_dirty(cx);
    });

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.request_dropped_markdown_replace(dropped_path.clone(), window, cx);
            editor.save_and_replace_pending_drop(window, cx);
        });
    });
    redraw(cx);

    assert_eq!(
        fs::read_to_string(&current_path).expect("read saved current"),
        "edited"
    );
    editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.file_path.as_ref(), Some(&dropped_path));
        assert_eq!(editor.document.markdown_text(cx), "dropped");
        assert!(!editor.document_dirty);
        assert!(!editor.pending_drop_replace_after_save);
    });
}

#[gpui::test]
async fn quit_menu_action_closes_only_active_editor_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let (_first_editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_string(), None));
    let first_window = activate_visual_window(cx);

    let (_second_editor, cx) = cx
        .cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "second".to_string(), None));
    let second_window = activate_visual_window(cx);

    assert_ne!(first_window.window_id(), second_window.window_id());
    assert_eq!(cx.cx.windows().len(), 2);

    cx.cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    let remaining = cx.cx.windows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].window_id(), first_window.window_id());
    assert_ne!(remaining[0].window_id(), second_window.window_id());
}

#[gpui::test]
async fn app_menu_opened_windows_activate_and_close_independently(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let first_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "first".to_string(), None));
    cx.run_until_parked();
    let second_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "second".to_string(), None));
    cx.run_until_parked();

    let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
    assert_eq!(active_window.window_id(), second_window.window_id());
    assert_ne!(first_window.window_id(), second_window.window_id());
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);

    assert!(
        second_window
            .update(cx, |editor, _window, _cx| editor.close_guard_installed)
            .expect("second editor window should be open")
    );

    cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    let remaining = cx.update(|cx| cx.windows());
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].window_id(), first_window.window_id());

    cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    assert!(cx.update(|cx| cx.windows().is_empty()));
}

#[gpui::test]
async fn app_menu_opened_file_window_reinstalls_close_guard_after_registration(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let opened_path = temp_markdown_path("app-menu-opened-file-window-close");
    fs::write(&opened_path, "opened from file").expect("write opened markdown");

    let first_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "first".to_string(), None));
    cx.run_until_parked();
    let second_window = cx.update(|cx| {
        crate::app_menu::open_editor_window(
            cx,
            fs::read_to_string(&opened_path).expect("read opened markdown"),
            Some(opened_path.clone()),
        )
    });
    cx.run_until_parked();

    let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
    assert_eq!(active_window.window_id(), second_window.window_id());
    assert_ne!(first_window.window_id(), second_window.window_id());

    second_window
        .update(cx, |editor, window, cx| {
            assert!(editor.close_guard_installed);
            assert!(editor.on_window_should_close(window, cx));
        })
        .expect("second editor window should be open");

    cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    let remaining = cx.update(|cx| cx.windows());
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].window_id(), first_window.window_id());
    assert_ne!(remaining[0].window_id(), second_window.window_id());

    let _ = fs::remove_file(opened_path);
}

#[gpui::test]
async fn app_menu_opened_dirty_file_window_prompts_only_that_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let opened_path = temp_markdown_path("app-menu-opened-dirty-file-window-close");
    fs::write(&opened_path, "opened from file").expect("write opened markdown");

    let first_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "first".to_string(), None));
    let second_window = cx.update(|cx| {
        crate::app_menu::open_editor_window(
            cx,
            fs::read_to_string(&opened_path).expect("read opened markdown"),
            Some(opened_path.clone()),
        )
    });
    cx.run_until_parked();

    second_window
        .update(cx, |editor, window, cx| {
            editor.mark_dirty(cx);
            assert!(!editor.on_window_should_close(window, cx));
        })
        .expect("second editor window should be open");

    first_window
        .update(cx, |editor, _window, _cx| {
            assert!(!editor.show_unsaved_changes_dialog);
        })
        .expect("first editor window should be open");
    second_window
        .update(cx, |editor, _window, _cx| {
            assert!(editor.show_unsaved_changes_dialog);
        })
        .expect("second editor window should be open");

    let _ = fs::remove_file(opened_path);
}

#[gpui::test]
async fn app_menu_opened_dirty_window_close_guard_prompts_only_that_window(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let first_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "first".to_string(), None));
    let second_window =
        cx.update(|cx| crate::app_menu::open_editor_window(cx, "second".to_string(), None));
    cx.run_until_parked();

    second_window
        .update(cx, |editor, window, cx| {
            editor.mark_dirty(cx);
            assert!(!editor.on_window_should_close(window, cx));
        })
        .expect("second editor window should be open");

    first_window
        .update(cx, |editor, _window, _cx| {
            assert!(!editor.show_unsaved_changes_dialog);
        })
        .expect("first editor window should be open");
    second_window
        .update(cx, |editor, _window, _cx| {
            assert!(editor.show_unsaved_changes_dialog);
        })
        .expect("second editor window should be open");
}

#[gpui::test]
async fn quit_menu_action_prompts_only_dirty_active_editor(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let (first_editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_string(), None));
    let first_window = activate_visual_window(cx);

    let (second_editor, cx) = cx
        .cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "second".to_string(), None));
    let second_window = activate_visual_window(cx);

    second_editor.update(cx, |editor, cx| editor.mark_dirty(cx));
    assert_eq!(cx.cx.windows().len(), 2);

    cx.cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    let open_windows = cx.cx.windows();
    assert_eq!(open_windows.len(), 2);
    assert!(
        open_windows
            .iter()
            .any(|window| window.window_id() == first_window.window_id())
    );
    assert!(
        open_windows
            .iter()
            .any(|window| window.window_id() == second_window.window_id())
    );
    first_editor.read_with(cx, |editor, _cx| {
        assert!(!editor.show_unsaved_changes_dialog);
    });
    second_editor.read_with(cx, |editor, _cx| {
        assert!(editor.show_unsaved_changes_dialog);
    });
}

#[gpui::test]
async fn windows_fallback_quit_dispatch_closes_target_editor_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "target".to_string(), None));
    let target_window = activate_visual_window(cx);

    cx.update(|window, cx| {
        let editor = editor.downgrade();
        crate::app_menu::dispatch_menu_action_for_editor(&QuitApplication, &editor, window, cx);
    });
    cx.run_until_parked();

    assert!(
        cx.cx
            .windows()
            .iter()
            .all(|window| window.window_id() != target_window.window_id())
    );
}

#[gpui::test]
async fn window_quit_action_closes_current_editor_before_global_menu_route(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let (_first_editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_string(), None));
    let first_window = activate_visual_window(cx);

    let (_second_editor, cx) = cx
        .cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "second".to_string(), None));
    let second_window = activate_visual_window(cx);

    cx.dispatch_action(QuitApplication);
    cx.run_until_parked();

    let remaining = cx.cx.windows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].window_id(), first_window.window_id());
    assert_ne!(remaining[0].window_id(), second_window.window_id());
}

#[gpui::test]
async fn dismissing_menu_bar_from_body_clears_open_state(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.set_menu_bar_hovered(true, cx);
        editor.set_menu_panel_hovered(true, cx);
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.dismiss_menu_bar_from_body(cx);
        assert_eq!(editor.menu_bar_open, None);
        assert!(!editor.menu_bar_hovered);
        assert!(!editor.menu_panel_hovered);
        assert!(editor.menu_close_task.is_none());
    });
}

#[gpui::test]
async fn starting_and_ending_scrollbar_drag_updates_editor_state(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.pending_scroll_active_block_into_view = true;
        editor.pending_scroll_recheck_after_layout = true;

        editor.start_scrollbar_drag(12.0, 320.0, 64.0, 500.0, cx);
        assert_eq!(
            editor.scrollbar_drag,
            Some(super::ScrollbarDragSession {
                pointer_offset_y: 12.0,
                track_height: 320.0,
                thumb_height: 64.0,
                max_scroll_y: 500.0,
            })
        );
        assert!(!editor.pending_scroll_active_block_into_view);
        assert!(!editor.pending_scroll_recheck_after_layout);

        editor.update_scrollbar_drag(172.0, cx);
        let offset_y = -f32::from(editor.scroll_handle.offset().y);
        assert!(offset_y > 0.0);

        editor.end_scrollbar_drag(cx);
        assert!(editor.scrollbar_drag.is_none());
    });
}

#[gpui::test]
async fn parsed_table_runtime_installs_column_alignment_on_cells(cx: &mut TestAppContext) {
    let markdown = [
        "| Left | Center | Right |",
        "| :--- | :---: | ---: |",
        "| a | b | c |",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        assert_eq!(table.read(cx).kind(), BlockKind::Table);
        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime");
        assert_eq!(
            runtime.header[0].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Left)
        );
        assert_eq!(
            runtime.header[1].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Center)
        );
        assert_eq!(
            runtime.rows[0][2].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Right)
        );
    });
}

#[gpui::test]
async fn append_column_updates_table_and_focuses_new_header_cell(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | ---: |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.append_table_column(&table, cx);

        let record = table
            .read(cx)
            .record
            .table
            .as_ref()
            .expect("table record after append");
        assert_eq!(record.header.len(), 3);
        assert_eq!(record.rows[0].len(), 3);
        assert_eq!(
            record.alignments,
            vec![
                TableColumnAlignment::Left,
                TableColumnAlignment::Right,
                TableColumnAlignment::Right,
            ]
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        let focused = runtime.header[2].entity_id();
        assert_eq!(editor.pending_focus, Some(focused));
    });
}

#[gpui::test]
async fn append_row_updates_table_and_focuses_first_cell_of_new_row(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | :---: |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.append_table_row(&table, cx);

        let record = table
            .read(cx)
            .record
            .table
            .as_ref()
            .expect("table record after append");
        assert_eq!(record.rows.len(), 2);
        assert_eq!(record.rows[1].len(), 2);
        assert!(
            record.rows[1]
                .iter()
                .all(|cell| cell.serialize_markdown().is_empty())
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        let focused = runtime.rows[1][0].entity_id();
        assert_eq!(editor.pending_focus, Some(focused));
    });
}

#[gpui::test]
async fn setting_column_alignment_updates_record_and_selection(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.set_table_column_alignment(&table, 1, TableColumnAlignment::Right, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(
            record.alignments,
            vec![TableColumnAlignment::Left, TableColumnAlignment::Right]
        );
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Column,
                index: 1,
            })
        );
    });
}

#[gpui::test]
async fn moving_table_row_updates_focus_and_selection(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.move_table_row(&table, 1, -1, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.rows[0][0].serialize_markdown(), "3");
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Row,
                index: 0,
            })
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        assert_eq!(editor.pending_focus, Some(runtime.rows[0][0].entity_id()));
    });
}

#[gpui::test]
async fn deleting_table_column_moves_selection_to_nearest_survivor(cx: &mut TestAppContext) {
    let markdown = ["| A | B | C |", "| --- | --- | --- |", "| 1 | 2 | 3 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.delete_table_column(&table, 2, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.header.len(), 2);
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Column,
                index: 1,
            })
        );
    });
}

#[gpui::test]
async fn standalone_root_image_installs_runtime_and_resolves_relative_path(
    cx: &mut TestAppContext,
) {
    let markdown = "![diagram](./assets/diagram.png \"System diagram\")".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.title.as_deref(), Some("System diagram"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn indented_root_images_install_runtime_before_indented_code(cx: &mut TestAppContext) {
    let url1 = "https://gitee.com/jikeyang/typera_picgo/raw/master/sias/202508201435626.png";
    let url2 = "https://gitee.com/jikeyang/typera_picgo/raw/master/sias/202508201438742.png";
    let url3 = "https://gitee.com/jikeyang/typera_picgo/raw/master/sias/202508201439288.png";
    let url4 = "https://gitee.com/jikeyang/typera_picgo/raw/master/sias/202508201419865.png";
    let markdown = [
        format!("![image-1]({})", url1.replace("_", "\\_")),
        String::new(),
        format!("   ![image-2]({})", url2.replace("_", "\\_")),
        String::new(),
        format!("        ![image-3]({})", url3.replace("_", "\\_")),
        String::new(),
        "   所有组或用户名均对**Anaconda安装目录**的权限设置为**完全控制**后，如下图所示："
            .to_string(),
        String::new(),
        format!("![image-4]({})", url4.replace("_", "\\_")),
        String::new(),
        "    plain indented code".to_string(),
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let roots = editor.document.root_blocks();
        let image_sources = roots
            .iter()
            .filter_map(|block| {
                block
                    .read(cx)
                    .image_runtime()
                    .map(|runtime| runtime.src.clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(image_sources, vec![url1, url2, url3, url4]);
        assert!(
            roots
                .iter()
                .any(|block| matches!(block.read(cx).kind(), BlockKind::CodeBlock { .. }))
        );
    });
}

#[gpui::test]
async fn mixed_text_does_not_activate_image_runtime(cx: &mut TestAppContext) {
    let markdown = "before ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        assert!(block.read(cx).image_runtime().is_none());
    });
}

#[gpui::test]
async fn reference_style_root_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown =
        "![reference image][ref-image]\n\n[ref-image]: ./assets/ref-image.png \"Caption\""
            .to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "reference image");
        assert_eq!(runtime.src, "./assets/ref-image.png");
        assert_eq!(runtime.title.as_deref(), Some("Caption"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/ref-image.png")
            )
        );
    });
}

#[gpui::test]
async fn quote_child_standalone_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = ">     ![diagram](./assets/diagram.png \"Caption\")".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let quote = editor.document.first_root().expect("quote root").clone();
        let image_block = quote
            .read(cx)
            .children
            .first()
            .expect("quote image child")
            .clone();
        let runtime = image_block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Caption"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn bulleted_list_item_standalone_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = "-     ![diagram](./assets/diagram.png \"Caption\")".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Caption"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn html_fallback_before_image_does_not_swallow_standalone_image(cx: &mut TestAppContext) {
    let image_url = "https://gitee.com/jikeyang/typera_picgo/raw/master/sias/202508200941158.png";
    let markdown = format!(
        "<span style='color:blue;'>Anaconda下载地址</span>：https://mirrors.tuna.tsinghua.edu.cn/anaconda/archive/\n\n![image-20250820094109009]({image_url})"
    );
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.document.root_count(), 2);
        {
            let html = editor.document.root_blocks()[0].read(cx);
            assert_eq!(html.kind(), BlockKind::HtmlBlock);
            assert!(
                html.display_text()
                    .starts_with("<span style='color:blue;'>")
            );
            assert!(
                html.record
                    .html
                    .as_ref()
                    .is_some_and(|html| html.is_semantic())
            );
        }

        let image = editor.document.root_blocks()[1].read(cx);
        let runtime = image.image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "image-20250820094109009");
        assert_eq!(runtime.src, image_url);
        match &runtime.resolved_source {
            ImageResolvedSource::Remote(uri) => assert_eq!(uri.to_string(), image_url),
            other => panic!("expected remote image, got {other:?}"),
        }
    });
}

#[gpui::test]
async fn unclosed_html_fallback_stops_before_standalone_image_without_blank(
    cx: &mut TestAppContext,
) {
    let image_url = "https://example.com/image.png";
    let markdown = format!("<span>unclosed html\n![image]({image_url})");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.document.root_count(), 2);
        assert_eq!(
            editor.document.root_blocks()[0].read(cx).kind(),
            BlockKind::RawMarkdown
        );
        let image = editor.document.root_blocks()[1].read(cx);
        let runtime = image.image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "image");
        assert_eq!(runtime.src, image_url);
    });
}

#[gpui::test]
async fn numbered_list_item_standalone_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = "1. ![diagram](https://example.com/diagram.gif \"Caption\")".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.title.as_deref(), Some("Caption"));
        match &runtime.resolved_source {
            ImageResolvedSource::Remote(uri) => {
                assert_eq!(uri.to_string(), "https://example.com/diagram.gif");
            }
            other => panic!("expected remote source, got {other:?}"),
        }
    });
}

#[gpui::test]
async fn task_list_item_reference_style_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = "- [ ] ![diagram][cover]\n\n[cover]: ./assets/diagram.png \"Cover\"".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("task list item root")
            .clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Cover"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn mixed_list_item_title_does_not_activate_image_runtime(cx: &mut TestAppContext) {
    let markdown = "- text ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        assert!(block.read(cx).image_runtime().is_none());
    });
}

#[gpui::test]
async fn list_child_reference_style_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = [
        "- item",
        "  ![diagram][cover]",
        "",
        "[cover]: ./assets/diagram.png \"Cover\"",
    ]
    .join("\n");
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let list_item = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let image_block = list_item
            .read(cx)
            .children
            .first()
            .expect("list child image")
            .clone();
        let runtime = image_block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Cover"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn list_scoped_reference_definition_supports_list_item_image_runtime(
    cx: &mut TestAppContext,
) {
    let markdown = [
        "- ![diagram][cover]",
        "  [cover]: ./assets/diagram.png \"Cover\"",
    ]
    .join("\n");
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let list_item = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let runtime = list_item.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Cover"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
        assert_eq!(
            list_item
                .read(cx)
                .children
                .first()
                .expect("reference definition child")
                .read(cx)
                .kind(),
            BlockKind::RawMarkdown
        );
    });
}

#[gpui::test]
async fn quote_list_item_standalone_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = "> - ![diagram](./assets/diagram.png)".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let quote = editor.document.first_root().expect("quote root").clone();
        let list_item = quote
            .read(cx)
            .children
            .first()
            .expect("quote list child")
            .clone();
        let runtime = list_item.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn callout_task_list_reference_style_image_uses_container_scoped_definition(
    cx: &mut TestAppContext,
) {
    let markdown = [
        "> [!NOTE]",
        "> - [ ] ![diagram][cover]",
        ">",
        "> [cover]: ./assets/diagram.png \"Cover\"",
    ]
    .join("\n");
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let callout = editor.document.first_root().expect("callout root").clone();
        let list_item = callout
            .read(cx)
            .children
            .first()
            .expect("callout list child")
            .clone();
        let runtime = list_item.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Cover"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn callout_list_child_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = [
        "> [!NOTE]",
        "> - item",
        ">   ![diagram](./assets/diagram.png)",
    ]
    .join("\n");
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let callout = editor.document.first_root().expect("callout root").clone();
        let list_item = callout
            .read(cx)
            .children
            .first()
            .expect("callout list child")
            .clone();
        let image_block = list_item
            .read(cx)
            .children
            .first()
            .expect("list child image")
            .clone();
        let runtime = image_block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn callout_child_reference_style_image_uses_container_scoped_definition(
    cx: &mut TestAppContext,
) {
    let markdown = [
        "> [!NOTE]",
        ">     ![diagram][anim]",
        ">",
        "> [anim]: ./assets/diagram.png \"Animated\"",
    ]
    .join("\n");
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let callout = editor.document.first_root().expect("callout root").clone();
        let image_block = callout
            .read(cx)
            .children
            .iter()
            .find(|child| {
                child.read(cx).kind() == BlockKind::Paragraph
                    && child.read(cx).image_runtime().is_some()
            })
            .expect("callout image child")
            .clone();
        let runtime = image_block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.src, "./assets/diagram.png");
        assert_eq!(runtime.title.as_deref(), Some("Animated"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn table_cell_with_standalone_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = [
        "| Preview |",
        "| --- |",
        "|    ![diagram](https://example.com/diagram.gif \"Animated\") |",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime");
        let cell_runtime = runtime.rows[0][0]
            .read(cx)
            .image_runtime()
            .expect("cell image runtime");
        assert_eq!(cell_runtime.alt, "diagram");
        assert_eq!(cell_runtime.title.as_deref(), Some("Animated"));
        match &cell_runtime.resolved_source {
            ImageResolvedSource::Remote(uri) => {
                assert_eq!(uri.to_string(), "https://example.com/diagram.gif");
            }
            other => panic!("expected remote source, got {other:?}"),
        }
    });
}

#[gpui::test]
async fn table_cell_with_reference_style_image_installs_runtime(cx: &mut TestAppContext) {
    let markdown = [
        "| Preview |",
        "| --- |",
        "| ![diagram][anim] |",
        "",
        "[anim]: https://example.com/diagram.gif \"Animated\"",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime");
        let cell_runtime = runtime.rows[0][0]
            .read(cx)
            .image_runtime()
            .expect("cell image runtime");
        assert_eq!(cell_runtime.alt, "diagram");
        assert_eq!(cell_runtime.title.as_deref(), Some("Animated"));
        match &cell_runtime.resolved_source {
            ImageResolvedSource::Remote(uri) => {
                assert_eq!(uri.to_string(), "https://example.com/diagram.gif");
            }
            other => panic!("expected remote source, got {other:?}"),
        }
    });
}

#[gpui::test]
async fn reference_style_link_in_root_paragraph_resolves_document_wide(cx: &mut TestAppContext) {
    let markdown = [
        "[reference link][ref-link]",
        "",
        "[ref-link]: https://example.com",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        assert_eq!(block.read(cx).display_text(), "reference link");
        assert_eq!(
            block.read(cx).inline_link_at(0),
            Some("https://example.com")
        );
    });
}

#[gpui::test]
async fn reference_style_link_in_table_cell_resolves_document_wide(cx: &mut TestAppContext) {
    let markdown = [
        "| Link |",
        "| --- |",
        "| [reference link][ref-link] |",
        "",
        "[ref-link]: https://example.com",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime");
        let cell = runtime.rows[0][0].clone();
        assert_eq!(cell.read(cx).display_text(), "reference link");
        assert_eq!(cell.read(cx).inline_link_at(0), Some("https://example.com"));
    });
}

#[gpui::test]
async fn root_level_footnotes_number_by_first_reference_and_render_in_place(
    cx: &mut TestAppContext,
) {
    let markdown = [
        "Here is a footnote reference.[^1]",
        "",
        "Here is another footnote reference.[^longnote]",
        "",
        "A footnote can appear after multiple paragraphs, lists, and code blocks.",
        "",
        "[^1]: Footnote text.",
        "",
        "[^longnote]: Footnote text with **bold**, `code`, and a nested list:",
        "    - item 1",
        "    - item 2",
        "    ",
        "    Second paragraph in the footnote.",
    ]
    .join("\n");
    let canonical_markdown = [
        "Here is a footnote reference.[^1]",
        "",
        "Here is another footnote reference.[^longnote]",
        "",
        "A footnote can appear after multiple paragraphs, lists, and code blocks.",
        "",
        "[^1]: Footnote text.",
        "",
        "[^longnote]: Footnote text with **bold**, `code`, and a nested list:",
        "",
        "    - item 1",
        "    - item 2",
        "",
        "    Second paragraph in the footnote.",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();

        let first_ref = visible
            .iter()
            .find(|visible| {
                visible
                    .entity
                    .read(cx)
                    .display_text()
                    .contains("Here is a footnote reference.")
            })
            .expect("first footnote reference")
            .entity
            .clone();
        assert_eq!(
            first_ref.read(cx).display_text(),
            format!("Here is a footnote reference.{}", superscript_ordinal(1))
        );

        let second_ref = visible
            .iter()
            .find(|visible| {
                visible
                    .entity
                    .read(cx)
                    .display_text()
                    .contains("Here is another footnote reference.")
            })
            .expect("second footnote reference")
            .entity
            .clone();
        assert_eq!(
            second_ref.read(cx).display_text(),
            format!(
                "Here is another footnote reference.{}",
                superscript_ordinal(2)
            )
        );

        let footnote_defs = visible
            .iter()
            .filter_map(|visible| {
                let block = visible.entity.read(cx);
                (block.kind() == BlockKind::FootnoteDefinition).then_some(visible.entity.clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(footnote_defs.len(), 2);
        assert_eq!(footnote_defs[0].read(cx).display_text(), "1");
        assert_eq!(
            footnote_defs[0].read(cx).footnote_definition_ordinal(),
            Some(1)
        );
        assert_eq!(footnote_defs[1].read(cx).display_text(), "longnote");
        assert_eq!(
            footnote_defs[1].read(cx).footnote_definition_ordinal(),
            Some(2)
        );

        assert_eq!(editor.document.markdown_text(cx), canonical_markdown);
    });
}

#[gpui::test]
async fn callout_footnotes_number_and_render_in_place(cx: &mut TestAppContext) {
    let markdown = [
        "> [!WARNING]",
        "> Callout footnote reference.[^final]",
        "> ",
        "> [^final]: Nested footnote text.",
        "> Tail paragraph.",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();

        let reference_block = visible
            .iter()
            .find(|visible| {
                visible
                    .entity
                    .read(cx)
                    .display_text()
                    .contains("Callout footnote reference.")
            })
            .expect("callout footnote reference")
            .entity
            .clone();
        assert_eq!(
            reference_block.read(cx).display_text(),
            format!("Callout footnote reference.{}", superscript_ordinal(1))
        );

        let definition = visible
            .iter()
            .find(|visible| visible.entity.read(cx).kind() == BlockKind::FootnoteDefinition)
            .expect("callout footnote definition")
            .entity
            .clone();
        assert_eq!(definition.read(cx).display_text(), "final");
        assert_eq!(definition.read(cx).quote_depth, 1);
        assert_eq!(definition.read(cx).footnote_definition_ordinal(), Some(1));
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn root_reference_binds_to_nested_quote_footnote_definition(cx: &mut TestAppContext) {
    let markdown = "Root reference.[^note]\n\n> [^note]: Nested quote footnote".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();

        let root_reference = visible
            .iter()
            .find(|visible| visible.entity.read(cx).quote_depth == 0)
            .expect("root reference block")
            .entity
            .clone();
        assert_eq!(
            root_reference.read(cx).display_text(),
            format!("Root reference.{}", superscript_ordinal(1))
        );

        let definition = visible
            .iter()
            .find(|visible| visible.entity.read(cx).kind() == BlockKind::FootnoteDefinition)
            .expect("nested quote footnote definition")
            .entity
            .clone();
        assert_eq!(definition.read(cx).display_text(), "note");
        assert_eq!(definition.read(cx).quote_depth, 1);
        assert_eq!(definition.read(cx).footnote_definition_ordinal(), Some(1));
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn unresolved_footnote_reference_stays_literal_and_unlinked(cx: &mut TestAppContext) {
    let markdown = "Missing footnote[^missing].".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("root paragraph")
            .clone();
        assert_eq!(block.read(cx).display_text(), markdown);
        assert!(
            block
                .read(cx)
                .inline_footnote_hit_at("Missing footnote".len())
                .is_none()
        );
        assert!(editor.footnote_registry.binding("missing").is_none());
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_root_image_runtime(cx: &mut TestAppContext) {
    let markdown = "![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        assert!(block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_reference_style_root_image_runtime(
    cx: &mut TestAppContext,
) {
    let markdown = "![diagram][ref]\n\n[ref]: ./assets/diagram.png".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.src, "./assets/diagram.png");
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_quote_child_image_runtime(cx: &mut TestAppContext) {
    let markdown = "> ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let quote = editor.document.first_root().expect("quote root").clone();
        let image_block = quote
            .read(cx)
            .children
            .first()
            .expect("quote image child")
            .clone();
        assert!(image_block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_list_item_image_runtime(cx: &mut TestAppContext) {
    let markdown = "- ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        assert!(block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_list_child_image_runtime(cx: &mut TestAppContext) {
    let markdown = "- item\n  ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let list_item = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let image_block = list_item
            .read(cx)
            .children
            .first()
            .expect("list child image")
            .clone();
        assert!(image_block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn undo_reverts_recent_rendered_typing(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.active_entity_id = Some(block.entity_id());
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(5..5, " beta", None, false, cx);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.document.markdown_text(cx), "alpha beta");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "alpha");
    });
}

#[gpui::test]
async fn consecutive_text_edits_within_window_coalesce_into_one_undo(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "a".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.active_entity_id = Some(block.entity_id());

        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(1..1, "b", None, false, cx);
        });
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(2..2, "c", None, false, cx);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.document.markdown_text(cx), "abc");
        assert_eq!(editor.undo_history.len(), 1);

        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "a");
    });
}

#[gpui::test]
async fn toggle_view_mode_preserves_paragraph_caret_position(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha\n\nbeta".to_string(), None));

    editor.update(cx, |editor, cx| {
        let target = editor.document.visible_blocks()[1].entity.clone();
        target.update(cx, |block, _cx| {
            block.selected_range = 2..2;
        });
        editor.active_entity_id = Some(target.entity_id());

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        let source = editor.document.first_root().expect("source root").clone();
        assert_eq!(source.read(cx).selected_range, 9..9);

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
        let visible = editor.document.visible_blocks();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[1].entity.read(cx).display_text(), "beta");
        assert_eq!(visible[1].entity.read(cx).selected_range, 2..2);
        assert_eq!(editor.pending_focus, Some(visible[1].entity.entity_id()));
    });
}

#[gpui::test]
async fn toggle_view_mode_preserves_table_cell_position(cx: &mut TestAppContext) {
    let markdown = ["| Name | Value |", "| --- | --- |", "| alpha | beta |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        let cell = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime")
            .rows[0][1]
            .clone();
        cell.update(cx, |block, _cx| {
            block.selected_range = 2..2;
        });
        editor.active_entity_id = Some(cell.entity_id());

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
        let restored_table = editor
            .document
            .first_root()
            .expect("restored table")
            .clone();
        let restored_cell = restored_table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("restored runtime")
            .rows[0][1]
            .clone();
        assert_eq!(restored_cell.read(cx).display_text(), "beta");
        assert_eq!(restored_cell.read(cx).selected_range, 2..2);
        assert_eq!(editor.pending_focus, Some(restored_cell.entity_id()));
    });
}

#[gpui::test]
async fn toggle_view_mode_preserves_callout_table_cell_position(cx: &mut TestAppContext) {
    let markdown = [
        "> [!NOTE]",
        "> | Name | Value |",
        "> | --- | --- |",
        "> | alpha | beta |",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let callout = editor.document.first_root().expect("callout root").clone();
        let table = callout
            .read(cx)
            .children
            .iter()
            .find(|child| child.read(cx).kind() == BlockKind::Table)
            .expect("nested table child")
            .clone();
        let cell = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime")
            .rows[0][1]
            .clone();
        cell.update(cx, |block, _cx| {
            block.selected_range = 2..2;
        });
        editor.active_entity_id = Some(cell.entity_id());

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
        let restored_callout = editor
            .document
            .first_root()
            .expect("restored callout")
            .clone();
        let restored_table = restored_callout
            .read(cx)
            .children
            .iter()
            .find(|child| child.read(cx).kind() == BlockKind::Table)
            .expect("restored nested table")
            .clone();
        let restored_cell = restored_table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("restored runtime")
            .rows[0][1]
            .clone();
        assert_eq!(restored_cell.read(cx).display_text(), "beta");
        assert_eq!(restored_cell.read(cx).selected_range, 2..2);
        assert_eq!(editor.pending_focus, Some(restored_cell.entity_id()));
    });
}
