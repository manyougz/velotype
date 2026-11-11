//! Localised UI strings and runtime language selection.
//!
//! This module owns language packs, system-locale matching, and the global
//! manager used by menus and editor UI. Visual styling remains in `theme`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use gpui::{App, Global};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::config::{
    VelotypeConfigDirs, object_without_empty_values, prune_empty_json_values, read_json_or_jsonc,
    sanitize_config_file_stem,
};

/// All localisable UI strings for the editor.
#[derive(Debug, Clone, Serialize)]
pub struct I18nStrings {
    /// Marker prepended to the window title when the document is dirty.
    pub dirty_title_marker: String,
    /// Title of the unsaved-changes dialog.
    pub unsaved_changes_title: String,
    /// Body message of the unsaved-changes dialog.
    pub unsaved_changes_message: String,
    /// Label for the "save and close" button.
    pub unsaved_changes_save_and_close: String,
    /// Label for the "discard and close" button.
    pub unsaved_changes_discard_and_close: String,
    /// Label for the "keep editing" button.
    pub unsaved_changes_cancel: String,
    /// Title of the dropped-file replacement dialog.
    pub drop_replace_title: String,
    /// Body message of the dropped-file replacement dialog.
    pub drop_replace_message: String,
    /// Label for saving before replacing the current document.
    pub drop_replace_save_and_replace: String,
    /// Label for replacing the current document without saving.
    pub drop_replace_discard_and_replace: String,
    /// Label for cancelling a dropped-file replacement.
    pub drop_replace_cancel: String,
    /// Prompt detail shown when no supported Markdown file was dropped.
    pub drop_no_markdown_file_message: String,
    /// Label for dismissing simple informational dialogs.
    pub info_dialog_ok: String,
    /// Title shown while an update check is running.
    pub help_check_updates_title: String,
    /// Body text shown while an update check is running.
    pub help_check_updates_message: String,
    /// Title shown when a newer version is available.
    pub update_available_title: String,
    /// Message template for newer-version prompts. Supports `{current}` and `{latest}`.
    pub update_available_message_template: String,
    /// Title shown when the running app is already current.
    pub update_up_to_date_title: String,
    /// Message template for up-to-date prompts. Supports `{current}` and `{latest}`.
    pub update_up_to_date_message_template: String,
    /// Title shown when an update check fails.
    pub update_failed_title: String,
    /// Message template for update-check failures. Supports `{error}`.
    pub update_failed_message_template: String,
    /// Button label for opening the GitHub Releases page.
    pub update_open_release: String,
    /// Button label for dismissing an available-update prompt.
    pub update_later: String,
    /// Download progress, with a `{version}` placeholder.
    pub update_downloading: String,
    /// Prepared-update explanation, with a `{version}` placeholder.
    pub update_ready_message: String,
    /// Explicit installation confirmation button.
    pub update_restart: String,
    /// Status-bar installation button, with a `{version}` placeholder.
    pub update_status_ready: String,
    /// Explains why installation is paused until documents have been saved.
    pub update_save_first_message: String,
    /// General application settings page.
    pub preferences_nav_settings: String,
    /// Opt-in automatic download switch label and explanation.
    pub preferences_updates_auto_download: String,
    pub preferences_updates_auto_download_hint: String,
    /// System Markdown association controls and read-only status messages.
    pub preferences_file_associations: String,
    pub preferences_associate_formats: String,
    pub preferences_file_associations_checking: String,
    pub preferences_file_associations_associated: String,
    pub preferences_file_associations_pending: String,
    /// Supports `{error}` for native diagnostics.
    pub preferences_file_associations_failed: String,
    /// Title of the About dialog.
    pub help_about_title: String,
    /// Supplemental About dialog text shown below the app name and version.
    pub help_about_message: String,
    /// Label for the project repository link in the About dialog.
    pub help_about_github_label: String,
    /// Star request shown in the About dialog.
    pub help_about_star_message: String,
    /// Top-level File menu label.
    pub menu_file: String,
    /// Top-level Export menu label.
    pub menu_export: String,
    /// Top-level Language menu label.
    pub menu_language: String,
    /// Top-level Theme menu label.
    pub menu_theme: String,
    /// Adaptive system theme label.
    pub theme_system: String,
    /// Top-level Workspace menu label.
    pub menu_workspace: String,
    /// Top-level Help menu label.
    pub menu_help: String,
    /// Language menu item for importing a custom language pack.
    pub menu_add_language_config: String,
    /// Theme menu item for importing a custom theme pack.
    pub menu_add_theme_config: String,
    /// File menu item for opening a new window.
    pub menu_new_window: String,
    /// File menu item for closing the current window.
    pub menu_close_window: String,
    /// File menu item for opening Markdown files.
    pub menu_open_file: String,
    /// File menu item for opening a recent file submenu.
    pub menu_open_recent_file: String,
    /// File menu item for opening app preferences.
    pub menu_preferences: String,
    /// Placeholder item shown when no recent files are recorded.
    pub menu_no_recent_files: String,
    /// File menu item for saving the current document.
    pub menu_save: String,
    /// File menu item for saving the current document to a new path.
    pub menu_save_as: String,
    /// File menu item for quitting the app.
    pub menu_quit: String,
    /// Export menu item for writing an HTML document.
    pub menu_export_html: String,
    /// Export menu item for writing a PDF document.
    pub menu_export_pdf: String,
    /// Help menu item for checking updates.
    pub menu_check_updates: String,
    /// Help menu item for showing About information.
    pub menu_about: String,
    /// Help menu item for installing the CLI tool (symlink to /usr/local/bin).
    pub menu_install_cli_tool: String,
    /// Help menu item for uninstalling the CLI tool.
    pub menu_uninstall_cli_tool: String,
    /// Workspace menu item for opening or closing the workspace drawer.
    pub menu_toggle_workspace: String,
    /// Native file-dialog prompt for opening Markdown files.
    pub open_markdown_files_prompt: String,
    /// Native file-dialog prompt for importing a language pack.
    pub add_language_config_prompt: String,
    /// Native file-dialog prompt for importing a theme pack.
    pub add_theme_config_prompt: String,
    /// Title of the open-file failure prompt.
    pub open_failed_title: String,
    /// Title shown when a recent file path no longer exists.
    pub recent_file_missing_title: String,
    /// Message template for missing recent files. Supports `{path}`.
    pub recent_file_missing_message_template: String,
    /// Title of the save failure prompt.
    pub save_failed_title: String,
    /// Title of the export failure prompt.
    pub export_failed_title: String,
    /// Title of the image-paste failure prompt.
    pub image_paste_failed_title: String,
    /// Title for one PicGo failure notice per paste batch.
    pub image_paste_picgo_failed_title: String,
    /// Message shown after the entire batch uses the saved local policy.
    pub image_paste_picgo_fallback_message: String,
    /// Temporary status while an image batch is uploading to PicGo.
    pub image_paste_picgo_waiting: String,
    /// Title of the custom configuration import failure prompt.
    pub config_import_failed_title: String,
    /// Preferences window title.
    pub preferences_window_title: String,
    /// File preferences navigation label.
    pub preferences_nav_file: String,
    /// Theme preferences navigation label.
    pub preferences_nav_theme: String,
    /// Image preferences navigation label.
    pub preferences_nav_image: String,
    /// Shortcut preferences navigation label.
    pub preferences_nav_shortcuts: String,
    /// Startup option field label.
    pub preferences_startup_option: String,
    /// Startup option for creating a new Markdown document.
    pub preferences_startup_new_file: String,
    /// Startup option for opening the last opened Markdown document.
    pub preferences_startup_last_opened_file: String,
    /// Theme preference field label.
    pub preferences_local_theme: String,
    /// Label for the document body font family list.
    pub preferences_font_body: String,
    /// Label for the code font family list.
    pub preferences_font_code: String,
    /// Label for the application UI font family list.
    pub preferences_font_ui: String,
    /// Help text displayed above the font family inputs.
    pub preferences_font_help: String,
    /// Availability warning displayed verbatim when no requested family is installed.
    pub preferences_font_unavailable_template: String,
    /// Validation message for a font list with no nonblank families.
    pub preferences_font_error_empty: String,
    /// Validation message for an unclosed quote in a font family list.
    pub preferences_font_error_unterminated_quote: String,
    /// Image paste behavior field label.
    pub preferences_image_insert_behavior: String,
    /// Image policy label for leaving existing files in place.
    pub preferences_image_paste_none: String,
    /// Image policy label for copying to the document or working directory.
    pub preferences_image_paste_copy_to_document_folder: String,
    /// Image policy label for copying to an `assets` subdirectory.
    pub preferences_image_paste_copy_to_assets_folder: String,
    /// Image policy label for a document-named assets directory.
    pub preferences_image_paste_copy_to_named_assets_folder: String,
    /// Upload images with the local PicGo Server.
    pub preferences_image_paste_picgo: String,
    /// PicGo Server URL field label.
    pub preferences_picgo_server_url: String,
    /// Non-uploading local server check states.
    pub preferences_picgo_checking: String,
    pub preferences_picgo_available: String,
    pub preferences_picgo_legacy: String,
    pub preferences_picgo_unavailable: String,
    /// Save button label in the preferences window.
    pub preferences_save: String,
    /// Cancel button label in the preferences window.
    pub preferences_cancel: String,
    /// Title shown when preferences cannot be saved.
    pub preferences_save_failed_title: String,
    /// Heading for file-related shortcuts.
    pub preferences_shortcuts_group_file: String,
    /// Heading for editing shortcuts.
    pub preferences_shortcuts_group_edit: String,
    /// Heading for navigation shortcuts.
    pub preferences_shortcuts_group_navigation: String,
    /// Heading for inline formatting shortcuts.
    pub preferences_shortcuts_group_formatting: String,
    /// Heading for block editing shortcuts.
    pub preferences_shortcuts_group_block: String,
    /// Heading for uncategorized shortcuts.
    pub preferences_shortcuts_group_other: String,
    /// Button label for recording a replacement shortcut.
    pub preferences_shortcut_record: String,
    /// Button label for restoring a command's default shortcuts.
    pub preferences_shortcut_reset: String,
    /// Label shown while waiting for a shortcut keystroke.
    pub preferences_shortcut_recording: String,
    /// Conflict message with `{command}` replaced by the conflicting command label.
    pub preferences_shortcut_conflict_template: String,
    /// Invalid-binding message with `{shortcut}` replaced by the captured key.
    pub preferences_shortcut_invalid_template: String,
    /// Command label for inserting a newline.
    pub preferences_shortcut_newline: String,
    /// Command label for deleting backward.
    pub preferences_shortcut_delete_back: String,
    /// Command label for deleting forward.
    pub preferences_shortcut_delete: String,
    /// Command label for deleting the previous word.
    pub preferences_shortcut_word_delete_back: String,
    /// Command label for deleting the next word.
    pub preferences_shortcut_word_delete_forward: String,
    /// Command label for focusing the previous block.
    pub preferences_shortcut_focus_prev: String,
    /// Command label for focusing the next block.
    pub preferences_shortcut_focus_next: String,
    /// Command label for moving the caret left.
    pub preferences_shortcut_move_left: String,
    /// Command label for moving the caret right.
    pub preferences_shortcut_move_right: String,
    /// Command label for moving backward by a word.
    pub preferences_shortcut_word_move_left: String,
    /// Command label for moving forward by a word.
    pub preferences_shortcut_word_move_right: String,
    /// Command label for moving to line start.
    pub preferences_shortcut_home: String,
    /// Command label for moving to line end.
    pub preferences_shortcut_end: String,
    /// Command label for upward block navigation.
    pub preferences_shortcut_block_up: String,
    /// Command label for downward block navigation.
    pub preferences_shortcut_block_down: String,
    /// Command label for paging upward.
    pub preferences_shortcut_page_up: String,
    /// Command label for paging downward.
    pub preferences_shortcut_page_down: String,
    /// Command label for jumping to document start.
    pub preferences_shortcut_jump_to_top: String,
    /// Command label for jumping to document end.
    pub preferences_shortcut_jump_to_bottom: String,
    /// Command label for extending selection left.
    pub preferences_shortcut_select_left: String,
    /// Command label for extending selection right.
    pub preferences_shortcut_select_right: String,
    /// Command label for extending selection to the previous word.
    pub preferences_shortcut_word_select_left: String,
    /// Command label for extending selection to the next word.
    pub preferences_shortcut_word_select_right: String,
    /// Command label for selecting to line start.
    pub preferences_shortcut_select_home: String,
    /// Command label for selecting to line end.
    pub preferences_shortcut_select_end: String,
    /// Command label for selecting all content.
    pub preferences_shortcut_select_all: String,
    /// Command label for copying the selection.
    pub preferences_shortcut_copy: String,
    /// Command label for cutting the selection.
    pub preferences_shortcut_cut: String,
    /// Command label for pasting clipboard content.
    pub preferences_shortcut_paste: String,
    /// Command label for undoing an edit.
    pub preferences_shortcut_undo: String,
    /// Command label for redoing an edit.
    pub preferences_shortcut_redo: String,
    /// Command label for applying bold to the selection.
    pub preferences_shortcut_bold_selection: String,
    /// Command label for applying italics to the selection.
    pub preferences_shortcut_italic_selection: String,
    /// Command label for underlining the selection.
    pub preferences_shortcut_underline_selection: String,
    /// Command label for applying inline code to the selection.
    pub preferences_shortcut_code_selection: String,
    /// Command label for converting the current block to a first-level heading.
    pub preferences_shortcut_heading_1: String,
    /// Command label for converting the current block to a second-level heading.
    pub preferences_shortcut_heading_2: String,
    /// Command label for converting the current block to a third-level heading.
    pub preferences_shortcut_heading_3: String,
    /// Command label for converting the current block to a fourth-level heading.
    pub preferences_shortcut_heading_4: String,
    /// Command label for converting the current block to a fifth-level heading.
    pub preferences_shortcut_heading_5: String,
    /// Command label for converting the current block to a sixth-level heading.
    pub preferences_shortcut_heading_6: String,
    /// Command label for removing a heading and returning to a paragraph.
    pub preferences_shortcut_remove_heading: String,
    /// Command label for indenting a block.
    pub preferences_shortcut_indent_block: String,
    /// Command label for outdenting a block.
    pub preferences_shortcut_outdent_block: String,
    /// Command label for exiting a code block.
    pub preferences_shortcut_exit_code_block: String,
    /// Command label for saving the current document.
    pub preferences_shortcut_save_document: String,
    /// Command label for saving to another path.
    pub preferences_shortcut_save_document_as: String,
    /// Command label for opening a new window.
    pub preferences_shortcut_new_window: String,
    /// Command label for opening a Markdown file.
    pub preferences_shortcut_open_file: String,
    /// Command label for quitting the application.
    pub preferences_shortcut_quit_application: String,
    /// Command label for closing the current window.
    pub preferences_shortcut_close_window: String,
    /// Command label for dismissing transient editor UI.
    pub preferences_shortcut_dismiss_transient_ui: String,
    /// Command label for switching source/rendered mode.
    pub preferences_shortcut_toggle_view_mode: String,
    /// Command label for toggling the workspace drawer.
    pub preferences_shortcut_toggle_workspace: String,
    /// Workspace drawer Files tab.
    pub workspace_tab_files: String,
    /// Workspace drawer Outline tab.
    pub workspace_tab_outline: String,
    /// Title shown when no Markdown file path is available for workspace mode.
    pub workspace_no_file_title: String,
    /// Message shown when no Markdown file path is available for workspace mode.
    pub workspace_no_file_message: String,
    /// Message shown when a workspace directory has no visible Markdown files.
    pub workspace_empty_files: String,
    /// Message shown when the current document has no headings.
    pub workspace_empty_outline: String,
    /// Title shown when the workspace file tree cannot be scanned.
    pub workspace_scan_failed_title: String,
    /// Title of the link-opening confirmation prompt.
    pub open_link_title: String,
    /// Confirm button for the link-opening prompt.
    pub open_link_open: String,
    /// Cancel button for the link-opening prompt.
    pub open_link_cancel: String,
    /// Compact label shown when rendered mode can switch to source mode.
    pub view_mode_source: String,
    /// Hover label shown when rendered mode can switch to source mode.
    pub view_mode_switch_to_source: String,
    /// Compact label shown when source mode can switch to rendered mode.
    pub view_mode_rendered: String,
    /// Hover label shown when source mode can switch to rendered mode.
    pub view_mode_switch_to_rendered: String,
    /// Root context-menu insert label.
    pub context_menu_insert: String,
    /// Insert submenu item for tables.
    pub context_menu_table: String,
    /// Table-axis menu item for left-aligning a column.
    pub table_axis_align_column_left: String,
    /// Table-axis menu item for center-aligning a column.
    pub table_axis_align_column_center: String,
    /// Table-axis menu item for right-aligning a column.
    pub table_axis_align_column_right: String,
    /// Table-axis menu item for moving a column left.
    pub table_axis_move_column_left: String,
    /// Table-axis menu item for moving a column right.
    pub table_axis_move_column_right: String,
    /// Table-axis menu item for deleting a column.
    pub table_axis_delete_column: String,
    /// Table-axis menu item for moving a row up.
    pub table_axis_move_row_up: String,
    /// Table-axis menu item for moving a row down.
    pub table_axis_move_row_down: String,
    /// Table-axis menu item for deleting a row.
    pub table_axis_delete_row: String,
    /// Table header-row menu item that toggles header styling on the top row.
    pub table_header_row: String,
    /// Title of the table-insert dialog.
    pub table_insert_title: String,
    /// Body text of the table-insert dialog.
    pub table_insert_description: String,
    /// Label for table body rows in the table-insert dialog.
    pub table_insert_body_rows: String,
    /// Label for table columns in the table-insert dialog.
    pub table_insert_columns: String,
    /// Cancel button in the table-insert dialog.
    pub table_insert_cancel: String,
    /// Confirm button in the table-insert dialog.
    pub table_insert_confirm: String,
    /// Placeholder label for rendered images without alt text.
    pub image_placeholder: String,
    /// Loading label for rendered images without alt text.
    pub image_loading_without_alt: String,
    /// Loading label template for rendered images with alt text; `{alt}` is replaced.
    pub image_loading_with_alt_template: String,
    /// Placeholder shown in the code-block language input when no language is set.
    pub code_language_placeholder: String,
    /// Label for the sidebar/files toggle button in the status bar.
    pub status_bar_files: String,
    /// Label for source mode in the status bar mode switch.
    pub status_bar_mode_source: String,
    /// Label for rendered mode in the status bar mode switch.
    pub status_bar_mode_rendered: String,
    /// Suffix shown after the word count number.
    pub status_bar_word_count_suffix: String,
    /// Nav label for the status bar preferences tab.
    pub preferences_nav_status_bar: String,
    /// Label for the status bar enabled toggle.
    pub preferences_status_bar_enabled: String,
    /// Label for the word count toggle.
    pub preferences_status_bar_show_word_count: String,
    /// Label for the cursor position toggle.
    pub preferences_status_bar_show_cursor_position: String,
    /// Label for the sidebar toggle visibility.
    pub preferences_status_bar_show_sidebar_toggle: String,
    /// Label for the mode switch visibility.
    pub preferences_status_bar_show_mode_switch: String,
}

/// Partial string input; absent entries inherit the defaults passed to `into_strings`.
/// Present strings, including empty ones, are retained unless pruned by the caller.
#[derive(Debug, Default, Deserialize)]
struct I18nStringsDe {
    /// Optional dirty-document title marker.
    dirty_title_marker: Option<String>,
    /// Optional unsaved-changes dialog title.
    unsaved_changes_title: Option<String>,
    /// Optional unsaved-changes explanation.
    unsaved_changes_message: Option<String>,
    /// Optional save-and-close label.
    unsaved_changes_save_and_close: Option<String>,
    /// Optional discard-and-close label.
    unsaved_changes_discard_and_close: Option<String>,
    /// Optional keep-editing label.
    unsaved_changes_cancel: Option<String>,
    /// Optional dropped-file replacement title.
    drop_replace_title: Option<String>,
    /// Optional dropped-file replacement explanation.
    drop_replace_message: Option<String>,
    /// Optional save-before-replacement label.
    drop_replace_save_and_replace: Option<String>,
    /// Optional discard-before-replacement label.
    drop_replace_discard_and_replace: Option<String>,
    /// Optional replacement cancellation label.
    drop_replace_cancel: Option<String>,
    /// Optional unsupported-drop message.
    drop_no_markdown_file_message: Option<String>,
    /// Optional informational prompt dismissal label.
    info_dialog_ok: Option<String>,
    /// Optional in-progress update-check title.
    help_check_updates_title: Option<String>,
    /// Optional in-progress update-check message.
    help_check_updates_message: Option<String>,
    /// Optional newer-version prompt title.
    update_available_title: Option<String>,
    /// Optional newer-version message with `{current}` and `{latest}` placeholders.
    update_available_message_template: Option<String>,
    /// Optional current-version prompt title.
    update_up_to_date_title: Option<String>,
    /// Optional current-version message with `{current}` and `{latest}` placeholders.
    update_up_to_date_message_template: Option<String>,
    /// Optional update failure title.
    update_failed_title: Option<String>,
    /// Optional update failure message with an `{error}` placeholder.
    update_failed_message_template: Option<String>,
    /// Optional release-page action label.
    update_open_release: Option<String>,
    /// Optional update postponement label.
    update_later: Option<String>,
    update_downloading: Option<String>,
    update_ready_message: Option<String>,
    update_restart: Option<String>,
    update_status_ready: Option<String>,
    update_save_first_message: Option<String>,
    preferences_nav_settings: Option<String>,
    preferences_updates_auto_download: Option<String>,
    preferences_updates_auto_download_hint: Option<String>,
    preferences_file_associations: Option<String>,
    preferences_associate_formats: Option<String>,
    preferences_file_associations_checking: Option<String>,
    preferences_file_associations_associated: Option<String>,
    preferences_file_associations_pending: Option<String>,
    preferences_file_associations_failed: Option<String>,
    /// Optional About dialog title.
    help_about_title: Option<String>,
    /// Optional supplemental About text.
    help_about_message: Option<String>,
    /// Optional About repository-link label.
    help_about_github_label: Option<String>,
    /// Optional About star-request text.
    help_about_star_message: Option<String>,
    /// Optional File menu title.
    menu_file: Option<String>,
    /// Optional Export menu title.
    menu_export: Option<String>,
    /// Optional Language menu title.
    menu_language: Option<String>,
    /// Optional Theme menu title.
    menu_theme: Option<String>,
    /// Optional adaptive system theme label.
    theme_system: Option<String>,
    /// Optional Workspace menu title.
    menu_workspace: Option<String>,
    /// Optional Help menu title.
    menu_help: Option<String>,
    /// Optional language-pack import menu label.
    menu_add_language_config: Option<String>,
    /// Optional theme-pack import menu label.
    menu_add_theme_config: Option<String>,
    /// Optional new-window menu label.
    menu_new_window: Option<String>,
    /// Optional close-window menu label.
    menu_close_window: Option<String>,
    /// Optional open-file menu label.
    menu_open_file: Option<String>,
    /// Optional recent-file submenu label.
    menu_open_recent_file: Option<String>,
    /// Optional preferences menu label.
    menu_preferences: Option<String>,
    /// Optional empty-history menu label.
    menu_no_recent_files: Option<String>,
    /// Optional save menu label.
    menu_save: Option<String>,
    /// Optional save-as menu label.
    menu_save_as: Option<String>,
    /// Optional quit menu label.
    menu_quit: Option<String>,
    /// Optional HTML export menu label.
    menu_export_html: Option<String>,
    /// Optional PDF export menu label.
    menu_export_pdf: Option<String>,
    /// Optional update-check menu label.
    menu_check_updates: Option<String>,
    /// Optional About menu label.
    menu_about: Option<String>,
    /// Optional CLI installation menu label.
    menu_install_cli_tool: Option<String>,
    /// Optional CLI removal menu label.
    menu_uninstall_cli_tool: Option<String>,
    /// Optional workspace drawer menu label.
    menu_toggle_workspace: Option<String>,
    /// Optional native Markdown file-picker prompt.
    open_markdown_files_prompt: Option<String>,
    /// Optional native language-pack picker prompt.
    add_language_config_prompt: Option<String>,
    /// Optional native theme-pack picker prompt.
    add_theme_config_prompt: Option<String>,
    /// Optional file-open failure title.
    open_failed_title: Option<String>,
    /// Optional missing-recent-file title.
    recent_file_missing_title: Option<String>,
    /// Optional missing-file message with a `{path}` placeholder.
    recent_file_missing_message_template: Option<String>,
    /// Optional document-save failure title.
    save_failed_title: Option<String>,
    /// Optional export failure title.
    export_failed_title: Option<String>,
    /// Optional image-paste failure title.
    image_paste_failed_title: Option<String>,
    image_paste_picgo_failed_title: Option<String>,
    image_paste_picgo_fallback_message: Option<String>,
    image_paste_picgo_waiting: Option<String>,
    /// Optional pack-import failure title.
    config_import_failed_title: Option<String>,
    /// Optional preferences window title.
    preferences_window_title: Option<String>,
    /// Optional startup settings navigation label.
    preferences_nav_file: Option<String>,
    /// Optional theme settings navigation label.
    preferences_nav_theme: Option<String>,
    /// Optional image settings navigation label.
    preferences_nav_image: Option<String>,
    /// Optional shortcuts navigation label.
    preferences_nav_shortcuts: Option<String>,
    /// Optional startup policy field label.
    preferences_startup_option: Option<String>,
    /// Optional new-document startup choice label.
    preferences_startup_new_file: Option<String>,
    /// Optional recent-document startup choice label.
    preferences_startup_last_opened_file: Option<String>,
    /// Optional theme selector field label.
    preferences_local_theme: Option<String>,
    /// Optional body font family field label.
    preferences_font_body: Option<String>,
    /// Optional code font family field label.
    preferences_font_code: Option<String>,
    /// Optional UI font family field label.
    preferences_font_ui: Option<String>,
    /// Optional font-list help text.
    preferences_font_help: Option<String>,
    /// Optional font availability warning, displayed without placeholder replacement.
    preferences_font_unavailable_template: Option<String>,
    /// Optional empty-font-list error message.
    preferences_font_error_empty: Option<String>,
    /// Optional unclosed-font-quote error message.
    preferences_font_error_unterminated_quote: Option<String>,
    /// Optional image storage policy field label.
    preferences_image_insert_behavior: Option<String>,
    /// Optional leave-image-files-in-place policy label.
    preferences_image_paste_none: Option<String>,
    /// Optional copy-to-document-directory policy label.
    preferences_image_paste_copy_to_document_folder: Option<String>,
    /// Optional copy-to-assets policy label.
    preferences_image_paste_copy_to_assets_folder: Option<String>,
    /// Optional copy-to-named-assets policy label.
    preferences_image_paste_copy_to_named_assets_folder: Option<String>,
    preferences_image_paste_picgo: Option<String>,
    preferences_picgo_server_url: Option<String>,
    preferences_picgo_checking: Option<String>,
    preferences_picgo_available: Option<String>,
    preferences_picgo_legacy: Option<String>,
    preferences_picgo_unavailable: Option<String>,
    /// Optional preferences save label.
    preferences_save: Option<String>,
    /// Optional preferences cancel label.
    preferences_cancel: Option<String>,
    /// Optional preferences write-failure title.
    preferences_save_failed_title: Option<String>,
    /// Optional file shortcut category heading.
    preferences_shortcuts_group_file: Option<String>,
    /// Optional editing shortcut category heading.
    preferences_shortcuts_group_edit: Option<String>,
    /// Optional navigation shortcut category heading.
    preferences_shortcuts_group_navigation: Option<String>,
    /// Optional formatting shortcut category heading.
    preferences_shortcuts_group_formatting: Option<String>,
    /// Optional block shortcut category heading.
    preferences_shortcuts_group_block: Option<String>,
    /// Optional miscellaneous shortcut category heading.
    preferences_shortcuts_group_other: Option<String>,
    /// Optional shortcut recording button label.
    preferences_shortcut_record: Option<String>,
    /// Optional shortcut reset button label.
    preferences_shortcut_reset: Option<String>,
    /// Optional awaiting-keystroke label.
    preferences_shortcut_recording: Option<String>,
    /// Optional shortcut conflict message with a `{command}` placeholder.
    preferences_shortcut_conflict_template: Option<String>,
    /// Optional invalid shortcut message with a `{shortcut}` placeholder.
    preferences_shortcut_invalid_template: Option<String>,
    /// Optional newline command label.
    preferences_shortcut_newline: Option<String>,
    /// Optional backward deletion command label.
    preferences_shortcut_delete_back: Option<String>,
    /// Optional forward deletion command label.
    preferences_shortcut_delete: Option<String>,
    /// Optional previous-word deletion command label.
    preferences_shortcut_word_delete_back: Option<String>,
    /// Optional next-word deletion command label.
    preferences_shortcut_word_delete_forward: Option<String>,
    /// Optional previous-block focus command label.
    preferences_shortcut_focus_prev: Option<String>,
    /// Optional next-block focus command label.
    preferences_shortcut_focus_next: Option<String>,
    /// Optional leftward caret command label.
    preferences_shortcut_move_left: Option<String>,
    /// Optional rightward caret command label.
    preferences_shortcut_move_right: Option<String>,
    /// Optional previous-word movement command label.
    preferences_shortcut_word_move_left: Option<String>,
    /// Optional next-word movement command label.
    preferences_shortcut_word_move_right: Option<String>,
    /// Optional line-start movement command label.
    preferences_shortcut_home: Option<String>,
    /// Optional line-end movement command label.
    preferences_shortcut_end: Option<String>,
    /// Optional upward block navigation command label.
    preferences_shortcut_block_up: Option<String>,
    /// Optional downward block navigation command label.
    preferences_shortcut_block_down: Option<String>,
    /// Optional page-up command label.
    preferences_shortcut_page_up: Option<String>,
    /// Optional page-down command label.
    preferences_shortcut_page_down: Option<String>,
    /// Optional document-start command label.
    preferences_shortcut_jump_to_top: Option<String>,
    /// Optional document-end command label.
    preferences_shortcut_jump_to_bottom: Option<String>,
    /// Optional leftward selection command label.
    preferences_shortcut_select_left: Option<String>,
    /// Optional rightward selection command label.
    preferences_shortcut_select_right: Option<String>,
    /// Optional previous-word selection command label.
    preferences_shortcut_word_select_left: Option<String>,
    /// Optional next-word selection command label.
    preferences_shortcut_word_select_right: Option<String>,
    /// Optional select-to-line-start command label.
    preferences_shortcut_select_home: Option<String>,
    /// Optional select-to-line-end command label.
    preferences_shortcut_select_end: Option<String>,
    /// Optional select-all command label.
    preferences_shortcut_select_all: Option<String>,
    /// Optional copy command label.
    preferences_shortcut_copy: Option<String>,
    /// Optional cut command label.
    preferences_shortcut_cut: Option<String>,
    /// Optional paste command label.
    preferences_shortcut_paste: Option<String>,
    /// Optional undo command label.
    preferences_shortcut_undo: Option<String>,
    /// Optional redo command label.
    preferences_shortcut_redo: Option<String>,
    /// Optional bold-selection command label.
    preferences_shortcut_bold_selection: Option<String>,
    /// Optional italic-selection command label.
    preferences_shortcut_italic_selection: Option<String>,
    /// Optional underline-selection command label.
    preferences_shortcut_underline_selection: Option<String>,
    /// Optional inline-code-selection command label.
    preferences_shortcut_code_selection: Option<String>,
    /// Optional first-level heading command label.
    preferences_shortcut_heading_1: Option<String>,
    /// Optional second-level heading command label.
    preferences_shortcut_heading_2: Option<String>,
    /// Optional third-level heading command label.
    preferences_shortcut_heading_3: Option<String>,
    /// Optional fourth-level heading command label.
    preferences_shortcut_heading_4: Option<String>,
    /// Optional fifth-level heading command label.
    preferences_shortcut_heading_5: Option<String>,
    /// Optional sixth-level heading command label.
    preferences_shortcut_heading_6: Option<String>,
    /// Optional remove-heading command label.
    preferences_shortcut_remove_heading: Option<String>,
    /// Optional block indentation command label.
    preferences_shortcut_indent_block: Option<String>,
    /// Optional block outdent command label.
    preferences_shortcut_outdent_block: Option<String>,
    /// Optional exit-code-block command label.
    preferences_shortcut_exit_code_block: Option<String>,
    /// Optional document save command label.
    preferences_shortcut_save_document: Option<String>,
    /// Optional save-as command label.
    preferences_shortcut_save_document_as: Option<String>,
    /// Optional new-window command label.
    preferences_shortcut_new_window: Option<String>,
    /// Optional open-file command label.
    preferences_shortcut_open_file: Option<String>,
    /// Optional quit-application command label.
    preferences_shortcut_quit_application: Option<String>,
    /// Optional close-window command label.
    preferences_shortcut_close_window: Option<String>,
    /// Optional transient-UI dismissal command label.
    preferences_shortcut_dismiss_transient_ui: Option<String>,
    /// Optional view-mode toggle command label.
    preferences_shortcut_toggle_view_mode: Option<String>,
    /// Optional workspace toggle command label.
    preferences_shortcut_toggle_workspace: Option<String>,
    /// Optional workspace Files tab label.
    workspace_tab_files: Option<String>,
    /// Optional workspace Outline tab label.
    workspace_tab_outline: Option<String>,
    /// Optional workspace-without-file title.
    workspace_no_file_title: Option<String>,
    /// Optional workspace-without-file message.
    workspace_no_file_message: Option<String>,
    /// Optional empty-workspace-file-list message.
    workspace_empty_files: Option<String>,
    /// Optional empty-document-outline message.
    workspace_empty_outline: Option<String>,
    /// Optional workspace scan failure title.
    workspace_scan_failed_title: Option<String>,
    /// Optional external-link confirmation title.
    open_link_title: Option<String>,
    /// Optional external-link confirmation button label.
    open_link_open: Option<String>,
    /// Optional external-link cancellation label.
    open_link_cancel: Option<String>,
    /// Optional compact source-mode switch label.
    view_mode_source: Option<String>,
    /// Optional source-mode switch hover label.
    view_mode_switch_to_source: Option<String>,
    /// Optional compact rendered-mode switch label.
    view_mode_rendered: Option<String>,
    /// Optional rendered-mode switch hover label.
    view_mode_switch_to_rendered: Option<String>,
    /// Optional root insert-menu label.
    context_menu_insert: Option<String>,
    /// Optional insert-table submenu label.
    context_menu_table: Option<String>,
    /// Optional left-align-column action label.
    table_axis_align_column_left: Option<String>,
    /// Optional center-align-column action label.
    table_axis_align_column_center: Option<String>,
    /// Optional right-align-column action label.
    table_axis_align_column_right: Option<String>,
    /// Optional move-column-left action label.
    table_axis_move_column_left: Option<String>,
    /// Optional move-column-right action label.
    table_axis_move_column_right: Option<String>,
    /// Optional delete-column action label.
    table_axis_delete_column: Option<String>,
    /// Optional move-row-up action label.
    table_axis_move_row_up: Option<String>,
    /// Optional move-row-down action label.
    table_axis_move_row_down: Option<String>,
    /// Optional delete-row action label.
    table_axis_delete_row: Option<String>,
    /// Optional table header styling toggle label.
    table_header_row: Option<String>,
    /// Optional table insertion dialog title.
    table_insert_title: Option<String>,
    /// Optional table insertion explanation.
    table_insert_description: Option<String>,
    /// Optional table body-row count label.
    table_insert_body_rows: Option<String>,
    /// Optional table column count label.
    table_insert_columns: Option<String>,
    /// Optional table insertion cancellation label.
    table_insert_cancel: Option<String>,
    /// Optional table insertion confirmation label.
    table_insert_confirm: Option<String>,
    /// Optional image label when alt text is absent.
    image_placeholder: Option<String>,
    /// Optional loading-image label without alt text.
    image_loading_without_alt: Option<String>,
    /// Optional loading-image message with an `{alt}` placeholder.
    image_loading_with_alt_template: Option<String>,
    /// Optional code language input placeholder.
    code_language_placeholder: Option<String>,
    /// Optional status bar workspace-toggle label.
    status_bar_files: Option<String>,
    /// Optional status bar source-mode label.
    status_bar_mode_source: Option<String>,
    /// Optional status bar rendered-mode label.
    status_bar_mode_rendered: Option<String>,
    /// Optional word-count suffix.
    status_bar_word_count_suffix: Option<String>,
    /// Optional status bar preferences navigation label.
    preferences_nav_status_bar: Option<String>,
    /// Optional status bar visibility switch label.
    preferences_status_bar_enabled: Option<String>,
    /// Optional word-count visibility switch label.
    preferences_status_bar_show_word_count: Option<String>,
    /// Optional cursor-position visibility switch label.
    preferences_status_bar_show_cursor_position: Option<String>,
    /// Optional workspace-toggle visibility switch label.
    preferences_status_bar_show_sidebar_toggle: Option<String>,
    /// Optional mode-switch visibility switch label.
    preferences_status_bar_show_mode_switch: Option<String>,
}

/// String keys retained when normalizing imported packs; unknown keys are dropped.
const I18N_STRING_KEYS: &[&str] = &[
    "dirty_title_marker",
    "unsaved_changes_title",
    "unsaved_changes_message",
    "unsaved_changes_save_and_close",
    "unsaved_changes_discard_and_close",
    "unsaved_changes_cancel",
    "drop_replace_title",
    "drop_replace_message",
    "drop_replace_save_and_replace",
    "drop_replace_discard_and_replace",
    "drop_replace_cancel",
    "drop_no_markdown_file_message",
    "info_dialog_ok",
    "help_check_updates_title",
    "help_check_updates_message",
    "update_available_title",
    "update_available_message_template",
    "update_up_to_date_title",
    "update_up_to_date_message_template",
    "update_failed_title",
    "update_failed_message_template",
    "update_open_release",
    "update_later",
    "update_downloading",
    "update_ready_message",
    "update_restart",
    "update_status_ready",
    "update_save_first_message",
    "preferences_nav_settings",
    "preferences_updates_auto_download",
    "preferences_updates_auto_download_hint",
    "preferences_file_associations",
    "preferences_associate_formats",
    "preferences_file_associations_checking",
    "preferences_file_associations_associated",
    "preferences_file_associations_pending",
    "preferences_file_associations_failed",
    "help_about_title",
    "help_about_message",
    "help_about_github_label",
    "help_about_star_message",
    "menu_file",
    "menu_export",
    "menu_language",
    "menu_theme",
    "theme_system",
    "menu_workspace",
    "menu_help",
    "menu_add_language_config",
    "menu_add_theme_config",
    "menu_new_window",
    "menu_close_window",
    "menu_open_file",
    "menu_open_recent_file",
    "menu_preferences",
    "menu_no_recent_files",
    "menu_save",
    "menu_save_as",
    "menu_quit",
    "menu_export_html",
    "menu_export_pdf",
    "menu_check_updates",
    "menu_about",
    "menu_install_cli_tool",
    "menu_uninstall_cli_tool",
    "menu_toggle_workspace",
    "open_markdown_files_prompt",
    "add_language_config_prompt",
    "add_theme_config_prompt",
    "open_failed_title",
    "recent_file_missing_title",
    "recent_file_missing_message_template",
    "save_failed_title",
    "export_failed_title",
    "image_paste_failed_title",
    "image_paste_picgo_failed_title",
    "image_paste_picgo_fallback_message",
    "image_paste_picgo_waiting",
    "config_import_failed_title",
    "preferences_window_title",
    "preferences_nav_file",
    "preferences_nav_theme",
    "preferences_nav_image",
    "preferences_nav_shortcuts",
    "preferences_startup_option",
    "preferences_startup_new_file",
    "preferences_startup_last_opened_file",
    "preferences_local_theme",
    "preferences_font_body",
    "preferences_font_code",
    "preferences_font_ui",
    "preferences_font_help",
    "preferences_font_unavailable_template",
    "preferences_font_error_empty",
    "preferences_font_error_unterminated_quote",
    "preferences_image_insert_behavior",
    "preferences_image_paste_none",
    "preferences_image_paste_copy_to_document_folder",
    "preferences_image_paste_copy_to_assets_folder",
    "preferences_image_paste_copy_to_named_assets_folder",
    "preferences_image_paste_picgo",
    "preferences_picgo_server_url",
    "preferences_picgo_checking",
    "preferences_picgo_available",
    "preferences_picgo_legacy",
    "preferences_picgo_unavailable",
    "preferences_save",
    "preferences_cancel",
    "preferences_save_failed_title",
    "preferences_shortcuts_group_file",
    "preferences_shortcuts_group_edit",
    "preferences_shortcuts_group_navigation",
    "preferences_shortcuts_group_formatting",
    "preferences_shortcuts_group_block",
    "preferences_shortcuts_group_other",
    "preferences_shortcut_record",
    "preferences_shortcut_reset",
    "preferences_shortcut_recording",
    "preferences_shortcut_conflict_template",
    "preferences_shortcut_invalid_template",
    "preferences_shortcut_newline",
    "preferences_shortcut_delete_back",
    "preferences_shortcut_delete",
    "preferences_shortcut_word_delete_back",
    "preferences_shortcut_word_delete_forward",
    "preferences_shortcut_focus_prev",
    "preferences_shortcut_focus_next",
    "preferences_shortcut_move_left",
    "preferences_shortcut_move_right",
    "preferences_shortcut_word_move_left",
    "preferences_shortcut_word_move_right",
    "preferences_shortcut_home",
    "preferences_shortcut_end",
    "preferences_shortcut_block_up",
    "preferences_shortcut_block_down",
    "preferences_shortcut_page_up",
    "preferences_shortcut_page_down",
    "preferences_shortcut_jump_to_top",
    "preferences_shortcut_jump_to_bottom",
    "preferences_shortcut_select_left",
    "preferences_shortcut_select_right",
    "preferences_shortcut_word_select_left",
    "preferences_shortcut_word_select_right",
    "preferences_shortcut_select_home",
    "preferences_shortcut_select_end",
    "preferences_shortcut_select_all",
    "preferences_shortcut_copy",
    "preferences_shortcut_cut",
    "preferences_shortcut_paste",
    "preferences_shortcut_undo",
    "preferences_shortcut_redo",
    "preferences_shortcut_bold_selection",
    "preferences_shortcut_italic_selection",
    "preferences_shortcut_underline_selection",
    "preferences_shortcut_code_selection",
    "preferences_shortcut_heading_1",
    "preferences_shortcut_heading_2",
    "preferences_shortcut_heading_3",
    "preferences_shortcut_heading_4",
    "preferences_shortcut_heading_5",
    "preferences_shortcut_heading_6",
    "preferences_shortcut_remove_heading",
    "preferences_shortcut_indent_block",
    "preferences_shortcut_outdent_block",
    "preferences_shortcut_exit_code_block",
    "preferences_shortcut_save_document",
    "preferences_shortcut_save_document_as",
    "preferences_shortcut_new_window",
    "preferences_shortcut_open_file",
    "preferences_shortcut_quit_application",
    "preferences_shortcut_close_window",
    "preferences_shortcut_dismiss_transient_ui",
    "preferences_shortcut_toggle_view_mode",
    "preferences_shortcut_toggle_workspace",
    "workspace_tab_files",
    "workspace_tab_outline",
    "workspace_no_file_title",
    "workspace_no_file_message",
    "workspace_empty_files",
    "workspace_empty_outline",
    "workspace_scan_failed_title",
    "open_link_title",
    "open_link_open",
    "open_link_cancel",
    "view_mode_source",
    "view_mode_switch_to_source",
    "view_mode_rendered",
    "view_mode_switch_to_rendered",
    "context_menu_insert",
    "context_menu_table",
    "table_axis_align_column_left",
    "table_axis_align_column_center",
    "table_axis_align_column_right",
    "table_axis_move_column_left",
    "table_axis_move_column_right",
    "table_axis_delete_column",
    "table_axis_move_row_up",
    "table_axis_move_row_down",
    "table_axis_delete_row",
    "table_header_row",
    "table_insert_title",
    "table_insert_description",
    "table_insert_body_rows",
    "table_insert_columns",
    "table_insert_cancel",
    "table_insert_confirm",
    "image_placeholder",
    "image_loading_without_alt",
    "image_loading_with_alt_template",
    "code_language_placeholder",
    "status_bar_files",
    "status_bar_mode_source",
    "status_bar_mode_rendered",
    "status_bar_word_count_suffix",
    "preferences_nav_status_bar",
    "preferences_status_bar_enabled",
    "preferences_status_bar_show_word_count",
    "preferences_status_bar_show_cursor_position",
    "preferences_status_bar_show_sidebar_toggle",
    "preferences_status_bar_show_mode_switch",
];

impl I18nStringsDe {
    /// Completes absent entries from the supplied defaults, retaining present strings verbatim.
    fn into_strings(self, defaults: I18nStrings) -> I18nStrings {
        I18nStrings {
            dirty_title_marker: self
                .dirty_title_marker
                .unwrap_or(defaults.dirty_title_marker),
            unsaved_changes_title: self
                .unsaved_changes_title
                .unwrap_or(defaults.unsaved_changes_title),
            unsaved_changes_message: self
                .unsaved_changes_message
                .unwrap_or(defaults.unsaved_changes_message),
            unsaved_changes_save_and_close: self
                .unsaved_changes_save_and_close
                .unwrap_or(defaults.unsaved_changes_save_and_close),
            unsaved_changes_discard_and_close: self
                .unsaved_changes_discard_and_close
                .unwrap_or(defaults.unsaved_changes_discard_and_close),
            unsaved_changes_cancel: self
                .unsaved_changes_cancel
                .unwrap_or(defaults.unsaved_changes_cancel),
            drop_replace_title: self
                .drop_replace_title
                .unwrap_or(defaults.drop_replace_title),
            drop_replace_message: self
                .drop_replace_message
                .unwrap_or(defaults.drop_replace_message),
            drop_replace_save_and_replace: self
                .drop_replace_save_and_replace
                .unwrap_or(defaults.drop_replace_save_and_replace),
            drop_replace_discard_and_replace: self
                .drop_replace_discard_and_replace
                .unwrap_or(defaults.drop_replace_discard_and_replace),
            drop_replace_cancel: self
                .drop_replace_cancel
                .unwrap_or(defaults.drop_replace_cancel),
            drop_no_markdown_file_message: self
                .drop_no_markdown_file_message
                .unwrap_or(defaults.drop_no_markdown_file_message),
            info_dialog_ok: self.info_dialog_ok.unwrap_or(defaults.info_dialog_ok),
            help_check_updates_title: self
                .help_check_updates_title
                .unwrap_or(defaults.help_check_updates_title),
            help_check_updates_message: self
                .help_check_updates_message
                .unwrap_or(defaults.help_check_updates_message),
            update_available_title: self
                .update_available_title
                .unwrap_or(defaults.update_available_title),
            update_available_message_template: self
                .update_available_message_template
                .unwrap_or(defaults.update_available_message_template),
            update_up_to_date_title: self
                .update_up_to_date_title
                .unwrap_or(defaults.update_up_to_date_title),
            update_up_to_date_message_template: self
                .update_up_to_date_message_template
                .unwrap_or(defaults.update_up_to_date_message_template),
            update_failed_title: self
                .update_failed_title
                .unwrap_or(defaults.update_failed_title),
            update_failed_message_template: self
                .update_failed_message_template
                .unwrap_or(defaults.update_failed_message_template),
            update_open_release: self
                .update_open_release
                .unwrap_or(defaults.update_open_release),
            update_later: self.update_later.unwrap_or(defaults.update_later),
            update_downloading: self
                .update_downloading
                .unwrap_or(defaults.update_downloading),
            update_ready_message: self
                .update_ready_message
                .unwrap_or(defaults.update_ready_message),
            update_restart: self.update_restart.unwrap_or(defaults.update_restart),
            update_status_ready: self
                .update_status_ready
                .unwrap_or(defaults.update_status_ready),
            update_save_first_message: self
                .update_save_first_message
                .unwrap_or(defaults.update_save_first_message),
            preferences_nav_settings: self
                .preferences_nav_settings
                .unwrap_or(defaults.preferences_nav_settings),
            preferences_updates_auto_download: self
                .preferences_updates_auto_download
                .unwrap_or(defaults.preferences_updates_auto_download),
            preferences_updates_auto_download_hint: self
                .preferences_updates_auto_download_hint
                .unwrap_or(defaults.preferences_updates_auto_download_hint),
            preferences_file_associations: self
                .preferences_file_associations
                .unwrap_or(defaults.preferences_file_associations),
            preferences_associate_formats: self
                .preferences_associate_formats
                .unwrap_or(defaults.preferences_associate_formats),
            preferences_file_associations_checking: self
                .preferences_file_associations_checking
                .unwrap_or(defaults.preferences_file_associations_checking),
            preferences_file_associations_associated: self
                .preferences_file_associations_associated
                .unwrap_or(defaults.preferences_file_associations_associated),
            preferences_file_associations_pending: self
                .preferences_file_associations_pending
                .unwrap_or(defaults.preferences_file_associations_pending),
            preferences_file_associations_failed: self
                .preferences_file_associations_failed
                .unwrap_or(defaults.preferences_file_associations_failed),
            help_about_title: self.help_about_title.unwrap_or(defaults.help_about_title),
            help_about_message: self
                .help_about_message
                .unwrap_or(defaults.help_about_message),
            help_about_github_label: self
                .help_about_github_label
                .unwrap_or(defaults.help_about_github_label),
            help_about_star_message: self
                .help_about_star_message
                .unwrap_or(defaults.help_about_star_message),
            menu_file: self.menu_file.unwrap_or(defaults.menu_file),
            menu_export: self.menu_export.unwrap_or(defaults.menu_export),
            menu_language: self.menu_language.unwrap_or(defaults.menu_language),
            menu_theme: self.menu_theme.unwrap_or(defaults.menu_theme),
            theme_system: self.theme_system.unwrap_or(defaults.theme_system),
            menu_workspace: self.menu_workspace.unwrap_or(defaults.menu_workspace),
            menu_help: self.menu_help.unwrap_or(defaults.menu_help),
            menu_add_language_config: self
                .menu_add_language_config
                .unwrap_or(defaults.menu_add_language_config),
            menu_add_theme_config: self
                .menu_add_theme_config
                .unwrap_or(defaults.menu_add_theme_config),
            menu_new_window: self.menu_new_window.unwrap_or(defaults.menu_new_window),
            menu_close_window: self.menu_close_window.unwrap_or(defaults.menu_close_window),
            menu_open_file: self.menu_open_file.unwrap_or(defaults.menu_open_file),
            menu_open_recent_file: self
                .menu_open_recent_file
                .unwrap_or(defaults.menu_open_recent_file),
            menu_preferences: self.menu_preferences.unwrap_or(defaults.menu_preferences),
            menu_no_recent_files: self
                .menu_no_recent_files
                .unwrap_or(defaults.menu_no_recent_files),
            menu_save: self.menu_save.unwrap_or(defaults.menu_save),
            menu_save_as: self.menu_save_as.unwrap_or(defaults.menu_save_as),
            menu_quit: self.menu_quit.unwrap_or(defaults.menu_quit),
            menu_export_html: self.menu_export_html.unwrap_or(defaults.menu_export_html),
            menu_export_pdf: self.menu_export_pdf.unwrap_or(defaults.menu_export_pdf),
            menu_check_updates: self
                .menu_check_updates
                .unwrap_or(defaults.menu_check_updates),
            menu_about: self.menu_about.unwrap_or(defaults.menu_about),
            menu_install_cli_tool: self
                .menu_install_cli_tool
                .unwrap_or(defaults.menu_install_cli_tool),
            menu_uninstall_cli_tool: self
                .menu_uninstall_cli_tool
                .unwrap_or(defaults.menu_uninstall_cli_tool),
            menu_toggle_workspace: self
                .menu_toggle_workspace
                .unwrap_or(defaults.menu_toggle_workspace),
            open_markdown_files_prompt: self
                .open_markdown_files_prompt
                .unwrap_or(defaults.open_markdown_files_prompt),
            add_language_config_prompt: self
                .add_language_config_prompt
                .unwrap_or(defaults.add_language_config_prompt),
            add_theme_config_prompt: self
                .add_theme_config_prompt
                .unwrap_or(defaults.add_theme_config_prompt),
            open_failed_title: self.open_failed_title.unwrap_or(defaults.open_failed_title),
            recent_file_missing_title: self
                .recent_file_missing_title
                .unwrap_or(defaults.recent_file_missing_title),
            recent_file_missing_message_template: self
                .recent_file_missing_message_template
                .unwrap_or(defaults.recent_file_missing_message_template),
            save_failed_title: self.save_failed_title.unwrap_or(defaults.save_failed_title),
            export_failed_title: self
                .export_failed_title
                .unwrap_or(defaults.export_failed_title),
            image_paste_failed_title: self
                .image_paste_failed_title
                .unwrap_or(defaults.image_paste_failed_title),
            image_paste_picgo_failed_title: self
                .image_paste_picgo_failed_title
                .unwrap_or(defaults.image_paste_picgo_failed_title),
            image_paste_picgo_fallback_message: self
                .image_paste_picgo_fallback_message
                .unwrap_or(defaults.image_paste_picgo_fallback_message),
            image_paste_picgo_waiting: self
                .image_paste_picgo_waiting
                .unwrap_or(defaults.image_paste_picgo_waiting),
            config_import_failed_title: self
                .config_import_failed_title
                .unwrap_or(defaults.config_import_failed_title),
            preferences_window_title: self
                .preferences_window_title
                .unwrap_or(defaults.preferences_window_title),
            preferences_nav_file: self
                .preferences_nav_file
                .unwrap_or(defaults.preferences_nav_file),
            preferences_nav_theme: self
                .preferences_nav_theme
                .unwrap_or(defaults.preferences_nav_theme),
            preferences_nav_image: self
                .preferences_nav_image
                .unwrap_or(defaults.preferences_nav_image),
            preferences_nav_shortcuts: self
                .preferences_nav_shortcuts
                .unwrap_or(defaults.preferences_nav_shortcuts),
            preferences_startup_option: self
                .preferences_startup_option
                .unwrap_or(defaults.preferences_startup_option),
            preferences_startup_new_file: self
                .preferences_startup_new_file
                .unwrap_or(defaults.preferences_startup_new_file),
            preferences_startup_last_opened_file: self
                .preferences_startup_last_opened_file
                .unwrap_or(defaults.preferences_startup_last_opened_file),
            preferences_local_theme: self
                .preferences_local_theme
                .unwrap_or(defaults.preferences_local_theme),
            preferences_font_body: self
                .preferences_font_body
                .unwrap_or(defaults.preferences_font_body),
            preferences_font_code: self
                .preferences_font_code
                .unwrap_or(defaults.preferences_font_code),
            preferences_font_ui: self
                .preferences_font_ui
                .unwrap_or(defaults.preferences_font_ui),
            preferences_font_help: self
                .preferences_font_help
                .unwrap_or(defaults.preferences_font_help),
            preferences_font_unavailable_template: self
                .preferences_font_unavailable_template
                .unwrap_or(defaults.preferences_font_unavailable_template),
            preferences_font_error_empty: self
                .preferences_font_error_empty
                .unwrap_or(defaults.preferences_font_error_empty),
            preferences_font_error_unterminated_quote: self
                .preferences_font_error_unterminated_quote
                .unwrap_or(defaults.preferences_font_error_unterminated_quote),
            preferences_image_insert_behavior: self
                .preferences_image_insert_behavior
                .unwrap_or(defaults.preferences_image_insert_behavior),
            preferences_image_paste_none: self
                .preferences_image_paste_none
                .unwrap_or(defaults.preferences_image_paste_none),
            preferences_image_paste_copy_to_document_folder: self
                .preferences_image_paste_copy_to_document_folder
                .unwrap_or(defaults.preferences_image_paste_copy_to_document_folder),
            preferences_image_paste_copy_to_assets_folder: self
                .preferences_image_paste_copy_to_assets_folder
                .unwrap_or(defaults.preferences_image_paste_copy_to_assets_folder),
            preferences_image_paste_copy_to_named_assets_folder: self
                .preferences_image_paste_copy_to_named_assets_folder
                .unwrap_or(defaults.preferences_image_paste_copy_to_named_assets_folder),
            preferences_image_paste_picgo: self
                .preferences_image_paste_picgo
                .unwrap_or(defaults.preferences_image_paste_picgo),
            preferences_picgo_server_url: self
                .preferences_picgo_server_url
                .unwrap_or(defaults.preferences_picgo_server_url),
            preferences_picgo_checking: self
                .preferences_picgo_checking
                .unwrap_or(defaults.preferences_picgo_checking),
            preferences_picgo_available: self
                .preferences_picgo_available
                .unwrap_or(defaults.preferences_picgo_available),
            preferences_picgo_legacy: self
                .preferences_picgo_legacy
                .unwrap_or(defaults.preferences_picgo_legacy),
            preferences_picgo_unavailable: self
                .preferences_picgo_unavailable
                .unwrap_or(defaults.preferences_picgo_unavailable),
            preferences_save: self.preferences_save.unwrap_or(defaults.preferences_save),
            preferences_cancel: self
                .preferences_cancel
                .unwrap_or(defaults.preferences_cancel),
            preferences_save_failed_title: self
                .preferences_save_failed_title
                .unwrap_or(defaults.preferences_save_failed_title),
            preferences_shortcuts_group_file: self
                .preferences_shortcuts_group_file
                .unwrap_or(defaults.preferences_shortcuts_group_file),
            preferences_shortcuts_group_edit: self
                .preferences_shortcuts_group_edit
                .unwrap_or(defaults.preferences_shortcuts_group_edit),
            preferences_shortcuts_group_navigation: self
                .preferences_shortcuts_group_navigation
                .unwrap_or(defaults.preferences_shortcuts_group_navigation),
            preferences_shortcuts_group_formatting: self
                .preferences_shortcuts_group_formatting
                .unwrap_or(defaults.preferences_shortcuts_group_formatting),
            preferences_shortcuts_group_block: self
                .preferences_shortcuts_group_block
                .unwrap_or(defaults.preferences_shortcuts_group_block),
            preferences_shortcuts_group_other: self
                .preferences_shortcuts_group_other
                .unwrap_or(defaults.preferences_shortcuts_group_other),
            preferences_shortcut_record: self
                .preferences_shortcut_record
                .unwrap_or(defaults.preferences_shortcut_record),
            preferences_shortcut_reset: self
                .preferences_shortcut_reset
                .unwrap_or(defaults.preferences_shortcut_reset),
            preferences_shortcut_recording: self
                .preferences_shortcut_recording
                .unwrap_or(defaults.preferences_shortcut_recording),
            preferences_shortcut_conflict_template: self
                .preferences_shortcut_conflict_template
                .unwrap_or(defaults.preferences_shortcut_conflict_template),
            preferences_shortcut_invalid_template: self
                .preferences_shortcut_invalid_template
                .unwrap_or(defaults.preferences_shortcut_invalid_template),
            preferences_shortcut_newline: self
                .preferences_shortcut_newline
                .unwrap_or(defaults.preferences_shortcut_newline),
            preferences_shortcut_delete_back: self
                .preferences_shortcut_delete_back
                .unwrap_or(defaults.preferences_shortcut_delete_back),
            preferences_shortcut_delete: self
                .preferences_shortcut_delete
                .unwrap_or(defaults.preferences_shortcut_delete),
            preferences_shortcut_word_delete_back: self
                .preferences_shortcut_word_delete_back
                .unwrap_or(defaults.preferences_shortcut_word_delete_back),
            preferences_shortcut_word_delete_forward: self
                .preferences_shortcut_word_delete_forward
                .unwrap_or(defaults.preferences_shortcut_word_delete_forward),
            preferences_shortcut_focus_prev: self
                .preferences_shortcut_focus_prev
                .unwrap_or(defaults.preferences_shortcut_focus_prev),
            preferences_shortcut_focus_next: self
                .preferences_shortcut_focus_next
                .unwrap_or(defaults.preferences_shortcut_focus_next),
            preferences_shortcut_move_left: self
                .preferences_shortcut_move_left
                .unwrap_or(defaults.preferences_shortcut_move_left),
            preferences_shortcut_move_right: self
                .preferences_shortcut_move_right
                .unwrap_or(defaults.preferences_shortcut_move_right),
            preferences_shortcut_word_move_left: self
                .preferences_shortcut_word_move_left
                .unwrap_or(defaults.preferences_shortcut_word_move_left),
            preferences_shortcut_word_move_right: self
                .preferences_shortcut_word_move_right
                .unwrap_or(defaults.preferences_shortcut_word_move_right),
            preferences_shortcut_home: self
                .preferences_shortcut_home
                .unwrap_or(defaults.preferences_shortcut_home),
            preferences_shortcut_end: self
                .preferences_shortcut_end
                .unwrap_or(defaults.preferences_shortcut_end),
            preferences_shortcut_block_up: self
                .preferences_shortcut_block_up
                .unwrap_or(defaults.preferences_shortcut_block_up),
            preferences_shortcut_block_down: self
                .preferences_shortcut_block_down
                .unwrap_or(defaults.preferences_shortcut_block_down),
            preferences_shortcut_page_up: self
                .preferences_shortcut_page_up
                .unwrap_or(defaults.preferences_shortcut_page_up),
            preferences_shortcut_page_down: self
                .preferences_shortcut_page_down
                .unwrap_or(defaults.preferences_shortcut_page_down),
            preferences_shortcut_jump_to_top: self
                .preferences_shortcut_jump_to_top
                .unwrap_or(defaults.preferences_shortcut_jump_to_top),
            preferences_shortcut_jump_to_bottom: self
                .preferences_shortcut_jump_to_bottom
                .unwrap_or(defaults.preferences_shortcut_jump_to_bottom),
            preferences_shortcut_select_left: self
                .preferences_shortcut_select_left
                .unwrap_or(defaults.preferences_shortcut_select_left),
            preferences_shortcut_select_right: self
                .preferences_shortcut_select_right
                .unwrap_or(defaults.preferences_shortcut_select_right),
            preferences_shortcut_word_select_left: self
                .preferences_shortcut_word_select_left
                .unwrap_or(defaults.preferences_shortcut_word_select_left),
            preferences_shortcut_word_select_right: self
                .preferences_shortcut_word_select_right
                .unwrap_or(defaults.preferences_shortcut_word_select_right),
            preferences_shortcut_select_home: self
                .preferences_shortcut_select_home
                .unwrap_or(defaults.preferences_shortcut_select_home),
            preferences_shortcut_select_end: self
                .preferences_shortcut_select_end
                .unwrap_or(defaults.preferences_shortcut_select_end),
            preferences_shortcut_select_all: self
                .preferences_shortcut_select_all
                .unwrap_or(defaults.preferences_shortcut_select_all),
            preferences_shortcut_copy: self
                .preferences_shortcut_copy
                .unwrap_or(defaults.preferences_shortcut_copy),
            preferences_shortcut_cut: self
                .preferences_shortcut_cut
                .unwrap_or(defaults.preferences_shortcut_cut),
            preferences_shortcut_paste: self
                .preferences_shortcut_paste
                .unwrap_or(defaults.preferences_shortcut_paste),
            preferences_shortcut_undo: self
                .preferences_shortcut_undo
                .unwrap_or(defaults.preferences_shortcut_undo),
            preferences_shortcut_redo: self
                .preferences_shortcut_redo
                .unwrap_or(defaults.preferences_shortcut_redo),
            preferences_shortcut_bold_selection: self
                .preferences_shortcut_bold_selection
                .unwrap_or(defaults.preferences_shortcut_bold_selection),
            preferences_shortcut_italic_selection: self
                .preferences_shortcut_italic_selection
                .unwrap_or(defaults.preferences_shortcut_italic_selection),
            preferences_shortcut_underline_selection: self
                .preferences_shortcut_underline_selection
                .unwrap_or(defaults.preferences_shortcut_underline_selection),
            preferences_shortcut_code_selection: self
                .preferences_shortcut_code_selection
                .unwrap_or(defaults.preferences_shortcut_code_selection),
            preferences_shortcut_heading_1: self
                .preferences_shortcut_heading_1
                .unwrap_or(defaults.preferences_shortcut_heading_1),
            preferences_shortcut_heading_2: self
                .preferences_shortcut_heading_2
                .unwrap_or(defaults.preferences_shortcut_heading_2),
            preferences_shortcut_heading_3: self
                .preferences_shortcut_heading_3
                .unwrap_or(defaults.preferences_shortcut_heading_3),
            preferences_shortcut_heading_4: self
                .preferences_shortcut_heading_4
                .unwrap_or(defaults.preferences_shortcut_heading_4),
            preferences_shortcut_heading_5: self
                .preferences_shortcut_heading_5
                .unwrap_or(defaults.preferences_shortcut_heading_5),
            preferences_shortcut_heading_6: self
                .preferences_shortcut_heading_6
                .unwrap_or(defaults.preferences_shortcut_heading_6),
            preferences_shortcut_remove_heading: self
                .preferences_shortcut_remove_heading
                .unwrap_or(defaults.preferences_shortcut_remove_heading),
            preferences_shortcut_indent_block: self
                .preferences_shortcut_indent_block
                .unwrap_or(defaults.preferences_shortcut_indent_block),
            preferences_shortcut_outdent_block: self
                .preferences_shortcut_outdent_block
                .unwrap_or(defaults.preferences_shortcut_outdent_block),
            preferences_shortcut_exit_code_block: self
                .preferences_shortcut_exit_code_block
                .unwrap_or(defaults.preferences_shortcut_exit_code_block),
            preferences_shortcut_save_document: self
                .preferences_shortcut_save_document
                .unwrap_or(defaults.preferences_shortcut_save_document),
            preferences_shortcut_save_document_as: self
                .preferences_shortcut_save_document_as
                .unwrap_or(defaults.preferences_shortcut_save_document_as),
            preferences_shortcut_new_window: self
                .preferences_shortcut_new_window
                .unwrap_or(defaults.preferences_shortcut_new_window),
            preferences_shortcut_open_file: self
                .preferences_shortcut_open_file
                .unwrap_or(defaults.preferences_shortcut_open_file),
            preferences_shortcut_quit_application: self
                .preferences_shortcut_quit_application
                .unwrap_or(defaults.preferences_shortcut_quit_application),
            preferences_shortcut_close_window: self
                .preferences_shortcut_close_window
                .unwrap_or(defaults.preferences_shortcut_close_window),
            preferences_shortcut_dismiss_transient_ui: self
                .preferences_shortcut_dismiss_transient_ui
                .unwrap_or(defaults.preferences_shortcut_dismiss_transient_ui),
            preferences_shortcut_toggle_view_mode: self
                .preferences_shortcut_toggle_view_mode
                .unwrap_or(defaults.preferences_shortcut_toggle_view_mode),
            preferences_shortcut_toggle_workspace: self
                .preferences_shortcut_toggle_workspace
                .unwrap_or(defaults.preferences_shortcut_toggle_workspace),
            workspace_tab_files: self
                .workspace_tab_files
                .unwrap_or(defaults.workspace_tab_files),
            workspace_tab_outline: self
                .workspace_tab_outline
                .unwrap_or(defaults.workspace_tab_outline),
            workspace_no_file_title: self
                .workspace_no_file_title
                .unwrap_or(defaults.workspace_no_file_title),
            workspace_no_file_message: self
                .workspace_no_file_message
                .unwrap_or(defaults.workspace_no_file_message),
            workspace_empty_files: self
                .workspace_empty_files
                .unwrap_or(defaults.workspace_empty_files),
            workspace_empty_outline: self
                .workspace_empty_outline
                .unwrap_or(defaults.workspace_empty_outline),
            workspace_scan_failed_title: self
                .workspace_scan_failed_title
                .unwrap_or(defaults.workspace_scan_failed_title),
            open_link_title: self.open_link_title.unwrap_or(defaults.open_link_title),
            open_link_open: self.open_link_open.unwrap_or(defaults.open_link_open),
            open_link_cancel: self.open_link_cancel.unwrap_or(defaults.open_link_cancel),
            view_mode_source: self.view_mode_source.unwrap_or(defaults.view_mode_source),
            view_mode_switch_to_source: self
                .view_mode_switch_to_source
                .unwrap_or(defaults.view_mode_switch_to_source),
            view_mode_rendered: self
                .view_mode_rendered
                .unwrap_or(defaults.view_mode_rendered),
            view_mode_switch_to_rendered: self
                .view_mode_switch_to_rendered
                .unwrap_or(defaults.view_mode_switch_to_rendered),
            context_menu_insert: self
                .context_menu_insert
                .unwrap_or(defaults.context_menu_insert),
            context_menu_table: self
                .context_menu_table
                .unwrap_or(defaults.context_menu_table),
            table_axis_align_column_left: self
                .table_axis_align_column_left
                .unwrap_or(defaults.table_axis_align_column_left),
            table_axis_align_column_center: self
                .table_axis_align_column_center
                .unwrap_or(defaults.table_axis_align_column_center),
            table_axis_align_column_right: self
                .table_axis_align_column_right
                .unwrap_or(defaults.table_axis_align_column_right),
            table_axis_move_column_left: self
                .table_axis_move_column_left
                .unwrap_or(defaults.table_axis_move_column_left),
            table_axis_move_column_right: self
                .table_axis_move_column_right
                .unwrap_or(defaults.table_axis_move_column_right),
            table_axis_delete_column: self
                .table_axis_delete_column
                .unwrap_or(defaults.table_axis_delete_column),
            table_axis_move_row_up: self
                .table_axis_move_row_up
                .unwrap_or(defaults.table_axis_move_row_up),
            table_axis_move_row_down: self
                .table_axis_move_row_down
                .unwrap_or(defaults.table_axis_move_row_down),
            table_axis_delete_row: self
                .table_axis_delete_row
                .unwrap_or(defaults.table_axis_delete_row),
            table_header_row: self.table_header_row.unwrap_or(defaults.table_header_row),
            table_insert_title: self
                .table_insert_title
                .unwrap_or(defaults.table_insert_title),
            table_insert_description: self
                .table_insert_description
                .unwrap_or(defaults.table_insert_description),
            table_insert_body_rows: self
                .table_insert_body_rows
                .unwrap_or(defaults.table_insert_body_rows),
            table_insert_columns: self
                .table_insert_columns
                .unwrap_or(defaults.table_insert_columns),
            table_insert_cancel: self
                .table_insert_cancel
                .unwrap_or(defaults.table_insert_cancel),
            table_insert_confirm: self
                .table_insert_confirm
                .unwrap_or(defaults.table_insert_confirm),
            image_placeholder: self.image_placeholder.unwrap_or(defaults.image_placeholder),
            image_loading_without_alt: self
                .image_loading_without_alt
                .unwrap_or(defaults.image_loading_without_alt),
            image_loading_with_alt_template: self
                .image_loading_with_alt_template
                .unwrap_or(defaults.image_loading_with_alt_template),
            code_language_placeholder: self
                .code_language_placeholder
                .unwrap_or(defaults.code_language_placeholder),
            status_bar_files: self.status_bar_files.unwrap_or(defaults.status_bar_files),
            status_bar_mode_source: self
                .status_bar_mode_source
                .unwrap_or(defaults.status_bar_mode_source),
            status_bar_mode_rendered: self
                .status_bar_mode_rendered
                .unwrap_or(defaults.status_bar_mode_rendered),
            status_bar_word_count_suffix: self
                .status_bar_word_count_suffix
                .unwrap_or(defaults.status_bar_word_count_suffix),
            preferences_nav_status_bar: self
                .preferences_nav_status_bar
                .unwrap_or(defaults.preferences_nav_status_bar),
            preferences_status_bar_enabled: self
                .preferences_status_bar_enabled
                .unwrap_or(defaults.preferences_status_bar_enabled),
            preferences_status_bar_show_word_count: self
                .preferences_status_bar_show_word_count
                .unwrap_or(defaults.preferences_status_bar_show_word_count),
            preferences_status_bar_show_cursor_position: self
                .preferences_status_bar_show_cursor_position
                .unwrap_or(defaults.preferences_status_bar_show_cursor_position),
            preferences_status_bar_show_sidebar_toggle: self
                .preferences_status_bar_show_sidebar_toggle
                .unwrap_or(defaults.preferences_status_bar_show_sidebar_toggle),
            preferences_status_bar_show_mode_switch: self
                .preferences_status_bar_show_mode_switch
                .unwrap_or(defaults.preferences_status_bar_show_mode_switch),
        }
    }
}

impl<'de> Deserialize<'de> for I18nStrings {
    /// Deserializes a partial string object using English for absent entries.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = I18nStringsDe::deserialize(deserializer)?;
        Ok(raw.into_strings(I18nStrings::en_us()))
    }
}

impl I18nStrings {
    /// Built-in Simplified Chinese UI strings.
    pub fn zh_cn() -> Self {
        let mut strings = Self {
            dirty_title_marker: "\u{00B7}".into(),
            unsaved_changes_title: "不保存并关闭？".into(),
            unsaved_changes_message: "此文档有未保存的更改。关闭前保存可避免丢失最新编辑。".into(),
            unsaved_changes_save_and_close: "保存并关闭".into(),
            unsaved_changes_discard_and_close: "放弃并关闭".into(),
            unsaved_changes_cancel: "继续编辑".into(),
            drop_replace_title: "替换当前文档？".into(),
            drop_replace_message: "当前文档有未保存的更改。替换前保存可避免丢失最新编辑。".into(),
            drop_replace_save_and_replace: "保存并替换".into(),
            drop_replace_discard_and_replace: "直接替换".into(),
            drop_replace_cancel: "取消".into(),
            drop_no_markdown_file_message:
                "请拖入 Markdown 文件（.md 或 .markdown）以在当前窗口打开。".into(),
            info_dialog_ok: "确定".into(),
            help_check_updates_title: "检查更新".into(),
            help_check_updates_message: "正在检查 Velotype 的最新版本...".into(),
            update_available_title: "发现新版本".into(),
            update_available_message_template:
                "当前版本：{current}\n最新版本：{latest}\n是否前往 GitHub Releases 下载？".into(),
            update_up_to_date_title: "已是最新版本".into(),
            update_up_to_date_message_template: "当前版本：{current}\n远程版本：{latest}".into(),
            update_failed_title: "更新失败".into(),
            update_failed_message_template: "无法完成更新：{error}".into(),
            update_open_release: "前往下载".into(),
            update_later: "稍后".into(),
            help_about_title: "关于 Velotype".into(),
            help_about_message: "作者：所有项目贡献者".into(),
            help_about_github_label: "GitHub".into(),
            help_about_star_message: "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
                .into(),
            menu_file: "文件".into(),
            menu_export: "导出".into(),
            menu_language: "语言".into(),
            menu_theme: "主题".into(),
            theme_system: "自适应系统".into(),
            menu_workspace: "工作区".into(),
            menu_help: "帮助".into(),
            menu_add_language_config: "添加语言配置".into(),
            menu_add_theme_config: "添加主题配置".into(),
            menu_new_window: "新建窗口".into(),
            menu_close_window: "关闭窗口".into(),
            menu_open_file: "打开文件".into(),
            menu_open_recent_file: "打开最近文件".into(),
            menu_preferences: "偏好设置".into(),
            menu_no_recent_files: "无最近文件".into(),
            menu_save: "保存".into(),
            menu_save_as: "另存为".into(),
            menu_quit: "退出".into(),
            menu_export_html: "HTML".into(),
            menu_export_pdf: "PDF".into(),
            menu_check_updates: "检查更新".into(),
            menu_about: "关于".into(),
            menu_install_cli_tool: "安装CLI命令".into(),
            menu_uninstall_cli_tool: "卸载CLI命令".into(),
            menu_toggle_workspace: "切换工作区".into(),
            open_markdown_files_prompt: "打开 Markdown 文件".into(),
            add_language_config_prompt: "选择语言配置文件".into(),
            add_theme_config_prompt: "选择主题配置文件".into(),
            open_failed_title: "打开失败".into(),
            recent_file_missing_title: "最近文件不存在".into(),
            recent_file_missing_message_template: "此最近文件已经不存在，已从记录中移除：\n{path}"
                .into(),
            save_failed_title: "保存失败".into(),
            export_failed_title: "导出失败".into(),
            config_import_failed_title: "配置导入失败".into(),
            preferences_window_title: "偏好设置".into(),
            preferences_nav_file: "文件".into(),
            preferences_nav_theme: "主题".into(),
            preferences_nav_shortcuts: "快捷键".into(),
            preferences_startup_option: "启动选项".into(),
            preferences_startup_new_file: "新 md 文件".into(),
            preferences_startup_last_opened_file: "上一次打开的 md 文件".into(),
            preferences_local_theme: "本地主题".into(),
            preferences_font_body: "正文字体".into(),
            preferences_font_code: "代码字体".into(),
            preferences_font_ui: "界面字体".into(),
            preferences_font_help: "输入以逗号分隔的已安装字体族；支持引号，保存后生效。".into(),
            preferences_font_unavailable_template: "未找到可用字体，已回退至系统默认字体。".into(),
            preferences_font_error_empty: "请至少输入一种字体。".into(),
            preferences_font_error_unterminated_quote: "字体名称中的引号未闭合。".into(),
            preferences_save: "保存".into(),
            preferences_cancel: "取消".into(),
            preferences_save_failed_title: "保存偏好设置失败".into(),
            preferences_shortcuts_group_file: "文件".into(),
            preferences_shortcuts_group_edit: "编辑".into(),
            preferences_shortcuts_group_navigation: "移动与选择".into(),
            preferences_shortcuts_group_formatting: "格式化".into(),
            preferences_shortcuts_group_block: "块操作".into(),
            preferences_shortcuts_group_other: "其他".into(),
            preferences_shortcut_record: "录制".into(),
            preferences_shortcut_reset: "重置".into(),
            preferences_shortcut_recording: "按下快捷键...".into(),
            preferences_shortcut_conflict_template: "该快捷键已被“{command}”使用".into(),
            preferences_shortcut_invalid_template: "无法使用快捷键“{shortcut}”".into(),
            preferences_shortcut_newline: "换行".into(),
            preferences_shortcut_delete_back: "向前删除".into(),
            preferences_shortcut_delete: "向后删除".into(),
            preferences_shortcut_word_delete_back: "向前删除单词".into(),
            preferences_shortcut_word_delete_forward: "向后删除单词".into(),
            preferences_shortcut_focus_prev: "上移".into(),
            preferences_shortcut_focus_next: "下移".into(),
            preferences_shortcut_move_left: "光标左移".into(),
            preferences_shortcut_move_right: "光标右移".into(),
            preferences_shortcut_word_move_left: "按词左移".into(),
            preferences_shortcut_word_move_right: "按词右移".into(),
            preferences_shortcut_home: "行首".into(),
            preferences_shortcut_end: "行尾".into(),
            preferences_shortcut_block_up: "上一块开头".into(),
            preferences_shortcut_block_down: "下一块开头".into(),
            preferences_shortcut_page_up: "上翻一页".into(),
            preferences_shortcut_page_down: "下翻一页".into(),
            preferences_shortcut_jump_to_top: "跳至开头".into(),
            preferences_shortcut_jump_to_bottom: "跳至末尾".into(),
            preferences_shortcut_select_left: "向左选择".into(),
            preferences_shortcut_select_right: "向右选择".into(),
            preferences_shortcut_word_select_left: "向左选择单词".into(),
            preferences_shortcut_word_select_right: "向右选择单词".into(),
            preferences_shortcut_select_home: "选择到行首".into(),
            preferences_shortcut_select_end: "选择到行尾".into(),
            preferences_shortcut_select_all: "全选".into(),
            preferences_shortcut_copy: "复制".into(),
            preferences_shortcut_cut: "剪切".into(),
            preferences_shortcut_paste: "粘贴".into(),
            preferences_shortcut_undo: "撤销".into(),
            preferences_shortcut_redo: "重做".into(),
            preferences_shortcut_bold_selection: "加粗".into(),
            preferences_shortcut_italic_selection: "斜体".into(),
            preferences_shortcut_underline_selection: "下划线".into(),
            preferences_shortcut_code_selection: "行内代码".into(),
            preferences_shortcut_heading_1: "标题 1".into(),
            preferences_shortcut_heading_2: "标题 2".into(),
            preferences_shortcut_heading_3: "标题 3".into(),
            preferences_shortcut_heading_4: "标题 4".into(),
            preferences_shortcut_heading_5: "标题 5".into(),
            preferences_shortcut_heading_6: "标题 6".into(),
            preferences_shortcut_remove_heading: "移除标题".into(),
            preferences_shortcut_indent_block: "缩进块".into(),
            preferences_shortcut_outdent_block: "取消缩进块".into(),
            preferences_shortcut_exit_code_block: "退出代码块".into(),
            preferences_shortcut_save_document: "保存文档".into(),
            preferences_shortcut_save_document_as: "另存为".into(),
            preferences_shortcut_new_window: "新建窗口".into(),
            preferences_shortcut_open_file: "打开文件".into(),
            preferences_shortcut_quit_application: "退出应用".into(),
            preferences_shortcut_close_window: "关闭窗口".into(),
            preferences_shortcut_dismiss_transient_ui: "关闭临时界面".into(),
            preferences_shortcut_toggle_view_mode: "切换视图模式".into(),
            preferences_shortcut_toggle_workspace: "切换工作区".into(),
            workspace_tab_files: "文件".into(),
            workspace_tab_outline: "大纲".into(),
            workspace_no_file_title: "未打开 Markdown 文件".into(),
            workspace_no_file_message: "打开或保存一个 .md 文件后，工作区会使用该文件所在目录。"
                .into(),
            workspace_empty_files: "没有可显示的 Markdown 文件".into(),
            workspace_empty_outline: "当前文档没有标题".into(),
            workspace_scan_failed_title: "无法读取工作区".into(),
            open_link_title: "打开链接？".into(),
            open_link_open: "打开".into(),
            open_link_cancel: "取消".into(),
            view_mode_source: "源码".into(),
            view_mode_switch_to_source: "切换到源码".into(),
            view_mode_rendered: "渲染".into(),
            view_mode_switch_to_rendered: "切换到渲染".into(),
            context_menu_insert: "插入".into(),
            context_menu_table: "表格".into(),
            table_axis_align_column_left: "左对齐此列".into(),
            table_axis_align_column_center: "居中此列".into(),
            table_axis_align_column_right: "右对齐此列".into(),
            table_axis_move_column_left: "向左移动此列".into(),
            table_axis_move_column_right: "向右移动此列".into(),
            table_axis_delete_column: "删除此列".into(),
            table_axis_move_row_up: "向上移动此行".into(),
            table_axis_move_row_down: "向下移动此行".into(),
            table_axis_delete_row: "删除此行".into(),
            table_header_row: "标题行".into(),
            table_insert_title: "插入表格".into(),
            table_insert_description: "创建 1 个表头行，并配置正文行数与列数。".into(),
            table_insert_body_rows: "正文行数".into(),
            table_insert_columns: "列数".into(),
            table_insert_cancel: "取消".into(),
            table_insert_confirm: "插入".into(),
            image_placeholder: "图片".into(),
            image_loading_without_alt: "正在加载图片...".into(),
            image_loading_with_alt_template: "正在加载 {alt}".into(),
            code_language_placeholder: "语言".into(),
            status_bar_files: "侧边栏".into(),
            status_bar_mode_source: "源码".into(),
            status_bar_mode_rendered: "渲染".into(),
            status_bar_word_count_suffix: "字".into(),
            ..Self::en_us()
        };
        strings.image_paste_failed_title = "图片粘贴失败".into();
        strings.update_downloading = "正在下载并验证 Velotype {version}...".into();
        strings.update_ready_message = "Velotype {version} 已下载并验证，可以安装并重启。".into();
        strings.update_restart = "更新并重启".into();
        strings.update_status_ready = "更新至 {version} 并重启".into();
        strings.update_save_first_message =
            "请先保存此窗口的修改，再次点击“更新并重启”。保存后窗口会保持打开。".into();
        strings.preferences_nav_settings = "设置".into();
        strings.preferences_file_associations = "Markdown 文件关联（.md）".into();
        strings.preferences_associate_formats = "关联格式".into();
        strings.preferences_file_associations_checking = "正在检查文件关联...".into();
        strings.preferences_file_associations_associated = "已关联到 Velotype。".into();
        strings.preferences_file_associations_pending =
            "请在系统默认应用设置中确认选择 Velotype。".into();
        strings.preferences_file_associations_failed = "无法检查或更新文件关联：{error}".into();
        strings.preferences_updates_auto_download = "自动下载更新".into();
        strings.preferences_updates_auto_download_hint = "启动时自动检查并下载最新正式版".into();
        strings.image_paste_picgo_failed_title = "PicGo 上传失败".into();
        strings.image_paste_picgo_fallback_message = "已按上次的本地图片策略插入本批图片。".into();
        strings.image_paste_picgo_waiting = "等待图床链接返回...".into();
        strings.preferences_nav_image = "图像".into();
        strings.preferences_nav_status_bar = "状态栏".into();
        strings.preferences_status_bar_enabled = "显示状态栏".into();
        strings.preferences_status_bar_show_word_count = "字数统计".into();
        strings.preferences_status_bar_show_cursor_position = "光标位置".into();
        strings.preferences_status_bar_show_sidebar_toggle = "侧边栏".into();
        strings.preferences_status_bar_show_mode_switch = "模式切换".into();
        strings.preferences_image_insert_behavior = "插入图片时...".into();
        strings.preferences_image_paste_none = "无特殊操作".into();
        strings.preferences_image_paste_copy_to_document_folder = "复制图片到 ./ 文件夹".into();
        strings.preferences_image_paste_copy_to_assets_folder = "复制图片到 ./assets 文件夹".into();
        strings.preferences_image_paste_copy_to_named_assets_folder =
            "复制图片到 ./${filename}.assets 文件夹".into();
        strings.preferences_image_paste_picgo = "使用 PicGo".into();
        strings.preferences_picgo_server_url = "PicGo Server 地址".into();
        strings.preferences_picgo_checking = "正在检查 PicGo Server...".into();
        strings.preferences_picgo_available = "PicGo Server 可用".into();
        strings.preferences_picgo_legacy = "本地服务可连接；粘贴时将验证上传接口".into();
        strings.preferences_picgo_unavailable =
            "PicGo Server 不可用；粘贴时将使用上次的本地策略".into();
        strings
    }

    /// Built-in English UI strings.
    pub fn en_us() -> Self {
        Self {
            dirty_title_marker: "\u{00B7}".into(),
            unsaved_changes_title: "Close without saving?".into(),
            unsaved_changes_message:
                "This document has unsaved changes. Save before closing to avoid losing your latest edits."
                    .into(),
            unsaved_changes_save_and_close: "Save and Close".into(),
            unsaved_changes_discard_and_close: "Discard and Close".into(),
            unsaved_changes_cancel: "Keep Editing".into(),
            drop_replace_title: "Replace current document?".into(),
            drop_replace_message:
                "This document has unsaved changes. Save before replacing it with the dropped file to avoid losing edits."
                    .into(),
            drop_replace_save_and_replace: "Save and Replace".into(),
            drop_replace_discard_and_replace: "Replace Without Saving".into(),
            drop_replace_cancel: "Cancel".into(),
            drop_no_markdown_file_message:
                "Drop a Markdown file (.md or .markdown) to open it in this window.".into(),
            info_dialog_ok: "OK".into(),
            help_check_updates_title: "Check for Updates".into(),
            help_check_updates_message: "Checking the latest Velotype version...".into(),
            update_available_title: "Update Available".into(),
            update_available_message_template:
                "Current version: {current}\nLatest version: {latest}\nOpen GitHub Releases to download it?"
                    .into(),
            update_up_to_date_title: "You're Up to Date".into(),
            update_up_to_date_message_template:
                "Current version: {current}\nRemote version: {latest}".into(),
            update_failed_title: "Update Failed".into(),
            update_failed_message_template: "Unable to complete the update: {error}"
                .into(),
            update_open_release: "Open Releases".into(),
            update_later: "Later".into(),
            update_downloading: "Downloading and verifying Velotype {version}...".into(),
            update_ready_message: "Velotype {version} has been downloaded and verified. Install it and restart now?".into(),
            update_restart: "Update and Restart".into(),
            update_status_ready: "Update to {version} and Restart".into(),
            update_save_first_message: "Save this window's changes, then click Update and Restart again. Saving will keep the window open.".into(),
            preferences_nav_settings: "Settings".into(),
            preferences_file_associations: "Markdown File Association (.md)".into(),
            preferences_associate_formats: "Associate Formats".into(),
            preferences_file_associations_checking: "Checking file associations...".into(),
            preferences_file_associations_associated: "Associated with Velotype.".into(),
            preferences_file_associations_pending: "Confirm Velotype in your system's default-app settings.".into(),
            preferences_file_associations_failed: "Could not check or update file associations: {error}".into(),
            preferences_updates_auto_download: "Automatically Download Updates".into(),
            preferences_updates_auto_download_hint: "Automatically check for and download the latest stable version on startup.".into(),
            help_about_title: "About Velotype".into(),
            help_about_message: "Author: All project contributors".into(),
            help_about_github_label: "GitHub".into(),
            help_about_star_message:
                "If this project helps you, consider giving it a Star⭐. Thank you!".into(),
            menu_file: "File".into(),
            menu_export: "Export".into(),
            menu_language: "Language".into(),
            menu_theme: "Theme".into(),
            theme_system: "System".into(),
            menu_workspace: "Workspace".into(),
            menu_help: "Help".into(),
            menu_add_language_config: "Add Language Config".into(),
            menu_add_theme_config: "Add Theme Config".into(),
            menu_new_window: "New Window".into(),
            menu_close_window: "Close Window".into(),
            menu_open_file: "Open File".into(),
            menu_open_recent_file: "Open Recent File".into(),
            menu_preferences: "Preferences".into(),
            menu_no_recent_files: "No Recent Files".into(),
            menu_save: "Save".into(),
            menu_save_as: "Save As".into(),
            menu_quit: "Quit".into(),
            menu_export_html: "HTML".into(),
            menu_export_pdf: "PDF".into(),
            menu_check_updates: "Check for Updates".into(),
            menu_about: "About".into(),
            menu_install_cli_tool: "Install CLI Command".into(),
            menu_uninstall_cli_tool: "Uninstall CLI Command".into(),
            menu_toggle_workspace: "Toggle Workspace".into(),
            open_markdown_files_prompt: "Open Markdown Files".into(),
            add_language_config_prompt: "Choose Language Config".into(),
            add_theme_config_prompt: "Choose Theme Config".into(),
            open_failed_title: "Open Failed".into(),
            recent_file_missing_title: "Recent File Missing".into(),
            recent_file_missing_message_template:
                "This recent file no longer exists and has been removed:\n{path}".into(),
            save_failed_title: "Save Failed".into(),
            export_failed_title: "Export Failed".into(),
            image_paste_failed_title: "Image Paste Failed".into(),
            image_paste_picgo_failed_title: "PicGo Upload Failed".into(),
            image_paste_picgo_fallback_message: "This paste batch was inserted using the previous local image policy.".into(),
            image_paste_picgo_waiting: "Waiting for image links...".into(),
            config_import_failed_title: "Config Import Failed".into(),
            preferences_window_title: "Preferences".into(),
            preferences_nav_file: "File".into(),
            preferences_nav_theme: "Theme".into(),
            preferences_nav_image: "Image".into(),
            preferences_nav_shortcuts: "Shortcuts".into(),
            preferences_startup_option: "Startup Option".into(),
            preferences_startup_new_file: "New Markdown File".into(),
            preferences_startup_last_opened_file: "Last Opened Markdown File".into(),
            preferences_local_theme: "Local Theme".into(),
            preferences_font_body: "Body font".into(),
            preferences_font_code: "Code font".into(),
            preferences_font_ui: "Interface font".into(),
            preferences_font_help: "Comma-separated installed font families. Quotes are supported; changes apply when saved.".into(),
            preferences_font_unavailable_template: "No selected font is available. Falling back to the system default font.".into(),
            preferences_font_error_empty: "Enter at least one font family.".into(),
            preferences_font_error_unterminated_quote: "A font family has an unterminated quote.".into(),
            preferences_image_insert_behavior: "When inserting images...".into(),
            preferences_image_paste_none: "No special action".into(),
            preferences_image_paste_copy_to_document_folder:
                "Copy image to ./ folder".into(),
            preferences_image_paste_copy_to_assets_folder:
                "Copy image to ./assets folder".into(),
            preferences_image_paste_copy_to_named_assets_folder:
                "Copy image to ./${filename}.assets folder".into(),
            preferences_image_paste_picgo: "Use PicGo".into(),
            preferences_picgo_server_url: "PicGo Server address".into(),
            preferences_picgo_checking: "Checking PicGo Server...".into(),
            preferences_picgo_available: "PicGo Server is available".into(),
            preferences_picgo_legacy: "Local server reachable; upload support will be checked on paste".into(),
            preferences_picgo_unavailable: "PicGo Server unavailable; pastes will use the previous local policy".into(),
            preferences_save: "Save".into(),
            preferences_cancel: "Cancel".into(),
            preferences_save_failed_title: "Save Preferences Failed".into(),
            preferences_shortcuts_group_file: "File".into(),
            preferences_shortcuts_group_edit: "Edit".into(),
            preferences_shortcuts_group_navigation: "Move and Select".into(),
            preferences_shortcuts_group_formatting: "Formatting".into(),
            preferences_shortcuts_group_block: "Block Operations".into(),
            preferences_shortcuts_group_other: "Other".into(),
            preferences_shortcut_record: "Record".into(),
            preferences_shortcut_reset: "Reset".into(),
            preferences_shortcut_recording: "Press shortcut...".into(),
            preferences_shortcut_conflict_template: "This shortcut is already used by {command}"
                .into(),
            preferences_shortcut_invalid_template: "Cannot use shortcut {shortcut}".into(),
            preferences_shortcut_newline: "Newline".into(),
            preferences_shortcut_delete_back: "Delete Backward".into(),
            preferences_shortcut_delete: "Delete Forward".into(),
            preferences_shortcut_word_delete_back: "Word Delete Backward".into(),
            preferences_shortcut_word_delete_forward: "Word Delete Forward".into(),
            preferences_shortcut_focus_prev: "Move Up".into(),
            preferences_shortcut_focus_next: "Move Down".into(),
            preferences_shortcut_move_left: "Move Left".into(),
            preferences_shortcut_move_right: "Move Right".into(),
            preferences_shortcut_word_move_left: "Word Move Left".into(),
            preferences_shortcut_word_move_right: "Word Move Right".into(),
            preferences_shortcut_home: "Line Start".into(),
            preferences_shortcut_end: "Line End".into(),
            preferences_shortcut_block_up: "Block Up".into(),
            preferences_shortcut_block_down: "Block Down".into(),
            preferences_shortcut_page_up: "Page Up".into(),
            preferences_shortcut_page_down: "Page Down".into(),
            preferences_shortcut_jump_to_top: "Jump to Top".into(),
            preferences_shortcut_jump_to_bottom: "Jump to Bottom".into(),
            preferences_shortcut_select_left: "Select Left".into(),
            preferences_shortcut_select_right: "Select Right".into(),
            preferences_shortcut_word_select_left: "Word Select Left".into(),
            preferences_shortcut_word_select_right: "Word Select Right".into(),
            preferences_shortcut_select_home: "Select to Line Start".into(),
            preferences_shortcut_select_end: "Select to Line End".into(),
            preferences_shortcut_select_all: "Select All".into(),
            preferences_shortcut_copy: "Copy".into(),
            preferences_shortcut_cut: "Cut".into(),
            preferences_shortcut_paste: "Paste".into(),
            preferences_shortcut_undo: "Undo".into(),
            preferences_shortcut_redo: "Redo".into(),
            preferences_shortcut_bold_selection: "Bold".into(),
            preferences_shortcut_italic_selection: "Italic".into(),
            preferences_shortcut_underline_selection: "Underline".into(),
            preferences_shortcut_code_selection: "Inline Code".into(),
            preferences_shortcut_heading_1: "Heading 1".into(),
            preferences_shortcut_heading_2: "Heading 2".into(),
            preferences_shortcut_heading_3: "Heading 3".into(),
            preferences_shortcut_heading_4: "Heading 4".into(),
            preferences_shortcut_heading_5: "Heading 5".into(),
            preferences_shortcut_heading_6: "Heading 6".into(),
            preferences_shortcut_remove_heading: "Remove Heading".into(),
            preferences_shortcut_indent_block: "Indent Block".into(),
            preferences_shortcut_outdent_block: "Outdent Block".into(),
            preferences_shortcut_exit_code_block: "Exit Code Block".into(),
            preferences_shortcut_save_document: "Save Document".into(),
            preferences_shortcut_save_document_as: "Save Document As".into(),
            preferences_shortcut_new_window: "New Window".into(),
            preferences_shortcut_open_file: "Open File".into(),
            preferences_shortcut_quit_application: "Quit Application".into(),
            preferences_shortcut_close_window: "Close Window".into(),
            preferences_shortcut_dismiss_transient_ui: "Dismiss Temporary UI".into(),
            preferences_shortcut_toggle_view_mode: "Toggle View Mode".into(),
            preferences_shortcut_toggle_workspace: "Toggle Workspace".into(),
            workspace_tab_files: "Files".into(),
            workspace_tab_outline: "Outline".into(),
            workspace_no_file_title: "No Markdown File Open".into(),
            workspace_no_file_message:
                "Open or save a .md file to use its folder as the workspace.".into(),
            workspace_empty_files: "No Markdown files to show".into(),
            workspace_empty_outline: "This document has no headings".into(),
            workspace_scan_failed_title: "Unable to Read Workspace".into(),
            open_link_title: "Open link?".into(),
            open_link_open: "Open".into(),
            open_link_cancel: "Cancel".into(),
            view_mode_source: "Source".into(),
            view_mode_switch_to_source: "Switch to Source".into(),
            view_mode_rendered: "Rendered".into(),
            view_mode_switch_to_rendered: "Switch to Rendered".into(),
            context_menu_insert: "Insert".into(),
            context_menu_table: "Table".into(),
            table_axis_align_column_left: "Align Column Left".into(),
            table_axis_align_column_center: "Align Column Center".into(),
            table_axis_align_column_right: "Align Column Right".into(),
            table_axis_move_column_left: "Move Column Left".into(),
            table_axis_move_column_right: "Move Column Right".into(),
            table_axis_delete_column: "Delete Column".into(),
            table_axis_move_row_up: "Move Row Up".into(),
            table_axis_move_row_down: "Move Row Down".into(),
            table_axis_delete_row: "Delete Row".into(),
            table_header_row: "Header Row".into(),
            table_insert_title: "Insert Table".into(),
            table_insert_description:
                "Create one header row and configure body rows and columns.".into(),
            table_insert_body_rows: "Body Rows".into(),
            table_insert_columns: "Columns".into(),
            table_insert_cancel: "Cancel".into(),
            table_insert_confirm: "Insert".into(),
            image_placeholder: "Image".into(),
            image_loading_without_alt: "Loading image...".into(),
            image_loading_with_alt_template: "Loading {alt}".into(),
            code_language_placeholder: "language".into(),
            status_bar_files: "Sidebar".into(),
            status_bar_mode_source: "Source".into(),
            status_bar_mode_rendered: "Rendered".into(),
            status_bar_word_count_suffix: "words".into(),
            preferences_nav_status_bar: "Status Bar".into(),
            preferences_status_bar_enabled: "Show Status Bar".into(),
            preferences_status_bar_show_word_count: "Word Count".into(),
            preferences_status_bar_show_cursor_position: "Cursor Position".into(),
            preferences_status_bar_show_sidebar_toggle: "Sidebar Toggle".into(),
            preferences_status_bar_show_mode_switch: "Mode Switch".into(),
        }
    }

    /// Returns a built-in string set for a supported language id.
    pub fn for_language_id(language_id: &str) -> Option<Self> {
        match language_id {
            "zh-CN" => Some(Self::zh_cn()),
            "en-US" => Some(Self::en_us()),
            _ => None,
        }
    }
}

/// Metadata for a selectable UI language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageCatalogEntry {
    /// Identifier accepted by [`I18nManager::set_language_by_id`].
    pub id: String,
    /// Human-readable language label in selection menus.
    pub name: String,
}

/// Persisted identifier for built-in Simplified Chinese.
const BUILTIN_LANGUAGE_ZH_CN_ID: &str = "zh-CN";
/// Native-language display name of the Simplified Chinese pack.
const BUILTIN_LANGUAGE_ZH_CN_NAME: &str = "简体中文";
/// Persisted identifier for built-in English.
const BUILTIN_LANGUAGE_EN_US_ID: &str = "en-US";
/// Display name of the built-in English pack.
const BUILTIN_LANGUAGE_EN_US_NAME: &str = "English";

/// Lists built-in languages in menu order, Chinese before English.
fn builtin_language_catalog() -> Vec<LanguageCatalogEntry> {
    vec![
        LanguageCatalogEntry {
            id: BUILTIN_LANGUAGE_ZH_CN_ID.into(),
            name: BUILTIN_LANGUAGE_ZH_CN_NAME.into(),
        },
        LanguageCatalogEntry {
            id: BUILTIN_LANGUAGE_EN_US_ID.into(),
            name: BUILTIN_LANGUAGE_EN_US_NAME.into(),
        },
    ]
}

/// A JSON language pack with metadata and fallback-completed strings.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct I18nLanguagePack {
    /// Language identifier used for selection and fallback lookup.
    pub id: String,
    /// Display name, defaulting to the built-in name or identifier when parsed directly.
    pub name: String,
    /// Optional pack author metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Optional pack description metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional pack version metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional pack homepage metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Optional pack license metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Complete strings after filling omissions from the ID's built-in set or English.
    pub strings: I18nStrings,
}

/// Partial JSON pack input used before ID-specific string fallback is applied.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct I18nLanguagePackDe {
    /// Required language identifier.
    id: String,
    /// Optional display name; direct parsing can infer one.
    name: Option<String>,
    /// Optional author carried into the resolved pack.
    author: Option<String>,
    /// Optional description carried into the resolved pack.
    description: Option<String>,
    /// Optional version carried into the resolved pack.
    version: Option<String>,
    /// Optional homepage carried into the resolved pack.
    homepage: Option<String>,
    /// Optional license carried into the resolved pack.
    license: Option<String>,
    /// Optional overrides, defaulting to an entirely absent string set.
    #[serde(default)]
    strings: I18nStringsDe,
}

#[allow(dead_code)]
impl I18nLanguagePack {
    /// Parses strict JSON, prunes empty values, and fills missing strings by language ID.
    /// Unlike importing, direct parsing permits built-in IDs and an omitted name.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let mut value: Value = serde_json::from_str(json)?;
        prune_empty_json_values(&mut value);
        Self::from_value(value)
    }

    /// Deserializes pack metadata and resolves partial strings without import validation.
    fn from_value(value: Value) -> anyhow::Result<Self> {
        let raw: I18nLanguagePackDe = serde_json::from_value(value)?;
        Ok(Self::from_partial(raw))
    }

    /// Completes strings from a matching built-in language or English and infers a name.
    fn from_partial(raw: I18nLanguagePackDe) -> Self {
        let fallback = I18nStrings::for_language_id(&raw.id).unwrap_or_else(I18nStrings::en_us);
        let name = raw
            .name
            .unwrap_or_else(|| language_name_for_id(&raw.id).unwrap_or(&raw.id).to_string());
        Self {
            id: raw.id,
            name,
            author: raw.author,
            description: raw.description,
            version: raw.version,
            homepage: raw.homepage,
            license: raw.license,
            strings: raw.strings.into_strings(fallback),
        }
    }
}

#[allow(dead_code)]
/// Returns the display name of an exact built-in language ID.
fn language_name_for_id(language_id: &str) -> Option<&'static str> {
    match language_id {
        BUILTIN_LANGUAGE_ZH_CN_ID => Some(BUILTIN_LANGUAGE_ZH_CN_NAME),
        BUILTIN_LANGUAGE_EN_US_ID => Some(BUILTIN_LANGUAGE_EN_US_NAME),
        _ => None,
    }
}

/// Identifies reserved language IDs that imported packs may not replace.
fn is_builtin_language_id(language_id: &str) -> bool {
    matches!(
        language_id,
        BUILTIN_LANGUAGE_ZH_CN_ID | BUILTIN_LANGUAGE_EN_US_ID
    )
}

/// Accepts nonblank ASCII IDs containing a letter and only alphanumerics or `-_.`.
fn is_valid_custom_language_id(language_id: &str) -> bool {
    !language_id.trim().is_empty()
        && language_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && language_id.chars().any(|ch| ch.is_ascii_alphabetic())
}

/// Uses the first supported preferred locale, mapping `zh` to Chinese and `en` to English.
/// Falls back to English when no locale matches.
pub fn language_id_for_locale_preferences<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    locales
        .into_iter()
        .find_map(|locale| language_id_for_locale(locale.as_ref()))
        .unwrap_or(BUILTIN_LANGUAGE_EN_US_ID)
}

/// Matches a locale's language after removing encoding/modifiers and normalizing separators.
fn language_id_for_locale(locale: &str) -> Option<&'static str> {
    let locale = locale.trim();
    if locale.is_empty() {
        return None;
    }

    let no_encoding = locale
        .split_once('.')
        .map_or(locale, |(locale, _encoding)| locale);
    let no_modifier = no_encoding
        .split_once('@')
        .map_or(no_encoding, |(locale, _modifier)| locale);
    let locale = no_modifier.replace('_', "-");
    let language = locale.split('-').next()?.to_ascii_lowercase();
    if !language.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }

    match language.as_str() {
        "zh" => Some(BUILTIN_LANGUAGE_ZH_CN_ID),
        "en" => Some(BUILTIN_LANGUAGE_EN_US_ID),
        _ => None,
    }
}

/// Global singleton that holds the current UI language strings.
pub struct I18nManager {
    /// Identifier associated with the active strings.
    current_language_id: String,
    /// Shared snapshot used by menus and rendering.
    strings: Arc<I18nStrings>,
    /// Imported packs available for selection.
    custom_languages: Vec<I18nLanguagePack>,
    /// Built-in-first catalog followed by imported language labels.
    language_catalog: Vec<LanguageCatalogEntry>,
}

impl Global for I18nManager {}

impl Default for I18nManager {
    /// Creates an English manager without loading persisted custom packs.
    fn default() -> Self {
        Self::new_with_language_id(BUILTIN_LANGUAGE_EN_US_ID)
    }
}

impl I18nManager {
    /// Installs the configured UI language into GPUI's global state.
    #[allow(dead_code)]
    pub fn init(cx: &mut App) {
        let language_id = crate::config::read_app_preferences()
            .map(|preferences| preferences.default_language_id)
            .unwrap_or_else(|_| BUILTIN_LANGUAGE_EN_US_ID.into());
        Self::init_with_language_id(cx, &language_id);
    }

    /// Loads persisted packs and installs the requested language in GPUI global state.
    /// Unknown IDs leave English selected; pack-loading errors are logged.
    pub fn init_with_language_id(cx: &mut App, language_id: &str) {
        let mut manager = Self::new_with_language_id(BUILTIN_LANGUAGE_EN_US_ID);
        if let Ok(dirs) = VelotypeConfigDirs::from_system()
            && let Err(err) = manager.load_custom_languages_from_dirs(&dirs)
        {
            eprintln!("failed to load custom languages: {err}");
        }
        let _ = manager.set_language_by_id(language_id);
        cx.set_global(manager);
    }

    /// Creates a manager with a known language id, falling back to English.
    pub fn new_with_language_id(language_id: &str) -> Self {
        let current_language_id = if I18nStrings::for_language_id(language_id).is_some() {
            language_id
        } else {
            BUILTIN_LANGUAGE_EN_US_ID
        };
        Self {
            current_language_id: current_language_id.into(),
            strings: Arc::new(
                I18nStrings::for_language_id(current_language_id)
                    .unwrap_or_else(I18nStrings::en_us),
            ),
            custom_languages: Vec::new(),
            language_catalog: builtin_language_catalog(),
        }
    }

    /// Returns the identifier of the currently active UI language.
    pub fn current_language_id(&self) -> &str {
        &self.current_language_id
    }

    /// Returns the strings for the currently active UI language.
    pub fn strings(&self) -> &I18nStrings {
        &self.strings
    }

    /// Returns a shared snapshot of the active strings without copying each string.
    pub fn strings_arc(&self) -> Arc<I18nStrings> {
        self.strings.clone()
    }

    /// Returns all built-in and imported UI languages exposed in the menu.
    pub fn available_languages(&self) -> &[LanguageCatalogEntry] {
        &self.language_catalog
    }

    /// Activates a recognized ID, returning whether the selected ID changed.
    /// Unknown IDs return `false` without replacing strings; reselecting reloads them.
    pub fn set_language_by_id(&mut self, language_id: &str) -> bool {
        let strings = if let Some(strings) = I18nStrings::for_language_id(language_id) {
            strings
        } else if let Some(pack) = self
            .custom_languages
            .iter()
            .find(|pack| pack.id == language_id)
        {
            pack.strings.clone()
        } else {
            return false;
        };
        let changed = self.current_language_id != language_id;
        self.current_language_id = language_id.into();
        self.strings = Arc::new(strings);
        changed
    }

    /// Imports a user language pack, persists a normalized copy, and activates it.
    pub fn import_language_config(&mut self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let dirs = VelotypeConfigDirs::from_system()?;
        self.import_language_config_with_dirs(path, &dirs)
    }

    /// Normalizes, persists, registers, and selects a custom pack in the supplied root.
    fn import_language_config_with_dirs(
        &mut self,
        path: impl AsRef<Path>,
        dirs: &VelotypeConfigDirs,
    ) -> anyhow::Result<String> {
        let raw = read_json_or_jsonc(path.as_ref())?;
        let (pack, normalized) = custom_language_pack_from_value(raw)?;
        let file_name = format!("{}.json", sanitize_config_file_stem(&pack.id));
        let languages_dir = dirs.languages_dir();
        std::fs::create_dir_all(&languages_dir)?;
        std::fs::write(
            languages_dir.join(file_name),
            serde_json::to_string_pretty(&normalized)?,
        )?;
        let imported_id = pack.id.clone();
        self.upsert_custom_language(pack);
        self.set_language_by_id(&imported_id);
        Ok(imported_id)
    }

    /// Loads packs sorted by name/ID, logging and skipping invalid configuration files.
    fn load_custom_languages_from_dirs(&mut self, dirs: &VelotypeConfigDirs) -> anyhow::Result<()> {
        let languages_dir = dirs.languages_dir();
        if !languages_dir.exists() {
            return Ok(());
        }

        let mut loaded = Vec::new();
        for entry in std::fs::read_dir(&languages_dir)? {
            let path = entry?.path();
            if path.is_file() {
                match read_json_or_jsonc(&path).and_then(|value| {
                    custom_language_pack_from_value(value).map(|(pack, _normalized)| pack)
                }) {
                    Ok(pack) => loaded.push(pack),
                    Err(err) => eprintln!(
                        "skipping custom language config '{}': {err}",
                        path.display()
                    ),
                }
            }
        }
        loaded.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        for pack in loaded {
            self.upsert_custom_language(pack);
        }
        Ok(())
    }

    /// Replaces a pack with the same ID or appends it, then rebuilds the catalog.
    fn upsert_custom_language(&mut self, pack: I18nLanguagePack) {
        if let Some(existing) = self
            .custom_languages
            .iter_mut()
            .find(|existing| existing.id == pack.id)
        {
            *existing = pack;
        } else {
            self.custom_languages.push(pack);
        }
        self.rebuild_language_catalog();
    }

    /// Rebuilds built-in-first selection labels from the loaded custom packs.
    fn rebuild_language_catalog(&mut self) {
        let mut catalog = builtin_language_catalog();
        catalog.extend(
            self.custom_languages
                .iter()
                .map(|pack| LanguageCatalogEntry {
                    id: pack.id.clone(),
                    name: pack.name.clone(),
                }),
        );
        self.language_catalog = catalog;
    }
}

/// Validates a custom ID and name, filters known keys, and resolves English fallbacks.
/// Returns the completed pack and its normalized partial JSON for persistence.
fn custom_language_pack_from_value(mut value: Value) -> anyhow::Result<(I18nLanguagePack, Value)> {
    prune_empty_json_values(&mut value);
    let Value::Object(object) = value else {
        bail!("language config must be a JSON object");
    };
    let object = object_without_empty_values(object);
    let id = required_string(&object, "id")?;
    if is_builtin_language_id(&id) {
        bail!("custom language id '{id}' would override a built-in language");
    }
    if !is_valid_custom_language_id(&id) {
        bail!("custom language id '{id}' contains unsupported characters");
    }
    let name = required_string(&object, "name")?;
    let mut normalized_object = Map::new();
    normalized_object.insert("id".into(), Value::String(id.clone()));
    normalized_object.insert("name".into(), Value::String(name));
    for key in ["author", "description", "version", "homepage", "license"] {
        if let Some(value) = object.get(key) {
            normalized_object.insert(key.into(), value.clone());
        }
    }
    if let Some(strings) = object.get("strings").and_then(Value::as_object) {
        let mut normalized_strings = Map::new();
        for key in I18N_STRING_KEYS {
            if let Some(value) = strings.get(*key) {
                normalized_strings.insert((*key).into(), value.clone());
            }
        }
        if !normalized_strings.is_empty() {
            normalized_object.insert("strings".into(), Value::Object(normalized_strings));
        }
    }
    let normalized = Value::Object(normalized_object);
    let pack = I18nLanguagePack::from_value(normalized.clone())
        .with_context(|| format!("failed to parse language config '{id}'"))?;
    Ok((pack, normalized))
}

/// Extracts a required trimmed nonblank string, reporting a field-specific error.
fn required_string(object: &Map<String, Value>, key: &str) -> anyhow::Result<String> {
    let Some(value) = object.get(key) else {
        bail!("missing required field '{key}'");
    };
    let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        bail!("field '{key}' must be a non-empty string");
    };
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::{I18nLanguagePack, I18nManager, I18nStrings, language_id_for_locale_preferences};
    use crate::config::VelotypeConfigDirs;
    use crate::theme::ThemeManager;

    #[test]
    fn built_in_chinese_strings_are_utf8() {
        let strings = I18nStrings::zh_cn();
        assert_eq!(strings.menu_file, "文件");
        assert_eq!(strings.menu_export, "导出");
        assert_eq!(strings.menu_language, "语言");
        assert_eq!(strings.save_failed_title, "保存失败");
        assert_eq!(strings.export_failed_title, "导出失败");
        assert_eq!(strings.view_mode_switch_to_source, "切换到源码");
        assert_eq!(strings.context_menu_insert, "插入");
        assert_eq!(strings.table_insert_title, "插入表格");
        assert_eq!(strings.image_loading_without_alt, "正在加载图片...");
        assert_eq!(strings.preferences_image_paste_picgo, "使用 PicGo");
        assert_eq!(strings.image_paste_picgo_waiting, "等待图床链接返回...");
        assert_eq!(strings.preferences_picgo_available, "PicGo Server 可用");
        assert_eq!(strings.preferences_associate_formats, "关联格式");
        assert_eq!(
            strings.preferences_file_associations,
            "Markdown 文件关联（.md）"
        );
        assert_eq!(
            strings.preferences_file_associations_associated,
            "已关联到 Velotype。"
        );
        assert_eq!(
            strings.help_check_updates_message,
            "正在检查 Velotype 的最新版本..."
        );
        assert_eq!(strings.update_open_release, "前往下载");
        assert_eq!(strings.help_about_github_label, "GitHub");
        assert_eq!(
            strings.help_about_star_message,
            "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
        );
    }

    #[test]
    fn manager_switches_builtin_languages() {
        let mut manager = I18nManager::default();
        assert_eq!(manager.current_language_id(), "en-US");
        assert_eq!(manager.strings().menu_file, "File");
        assert_eq!(manager.strings().menu_export, "Export");
        assert_eq!(manager.strings().preferences_nav_settings, "Settings");
        assert_eq!(
            manager.strings().preferences_associate_formats,
            "Associate Formats"
        );
        assert_eq!(
            manager.strings().preferences_file_associations,
            "Markdown File Association (.md)"
        );
        assert_eq!(
            manager.strings().update_status_ready,
            "Update to {version} and Restart"
        );

        assert!(manager.set_language_by_id("zh-CN"));
        assert_eq!(manager.current_language_id(), "zh-CN");
        assert_eq!(manager.strings().menu_file, "文件");
        assert_eq!(manager.strings().menu_export, "导出");
        assert!(!manager.set_language_by_id("zh-CN"));
        assert!(!manager.set_language_by_id("missing"));
    }

    #[test]
    fn language_catalog_contains_chinese_and_english() {
        let manager = I18nManager::default();
        let ids = manager
            .available_languages()
            .iter()
            .map(|entry| (entry.id.as_str(), entry.name.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![("zh-CN", "简体中文"), ("en-US", "English")]);
    }

    #[test]
    fn manager_can_be_constructed_with_known_language() {
        let manager = I18nManager::new_with_language_id("zh-CN");
        assert_eq!(manager.current_language_id(), "zh-CN");
        assert_eq!(manager.strings().menu_file, "文件");

        let fallback = I18nManager::new_with_language_id("missing");
        assert_eq!(fallback.current_language_id(), "en-US");
        assert_eq!(fallback.strings().menu_file, "File");
    }

    #[test]
    fn theme_switch_does_not_modify_selected_language() {
        let mut theme_manager = ThemeManager::default();
        let mut i18n_manager = I18nManager::new_with_language_id("zh-CN");

        assert!(theme_manager.set_theme_by_id("velotype"));
        assert!(!i18n_manager.set_language_by_id("missing"));

        assert_eq!(theme_manager.current_theme_id(), "velotype");
        assert_eq!(i18n_manager.current_language_id(), "zh-CN");
        assert_eq!(i18n_manager.strings().menu_file, "文件");
    }

    #[test]
    fn locale_preferences_map_to_builtin_languages() {
        assert_eq!(language_id_for_locale_preferences(["zh-CN"]), "zh-CN");
        assert_eq!(language_id_for_locale_preferences(["zh-HK"]), "zh-CN");
        assert_eq!(language_id_for_locale_preferences(["zh-Hant-TW"]), "zh-CN");
        assert_eq!(language_id_for_locale_preferences(["zh_SG.UTF-8"]), "zh-CN");
        assert_eq!(language_id_for_locale_preferences(["en-US"]), "en-US");
        assert_eq!(language_id_for_locale_preferences(["en_GB.UTF-8"]), "en-US");
        assert_eq!(
            language_id_for_locale_preferences(["fr-FR", "zh-CN"]),
            "zh-CN"
        );
        assert_eq!(
            language_id_for_locale_preferences(Vec::<&str>::new()),
            "en-US"
        );
        assert_eq!(language_id_for_locale_preferences(["fr-FR"]), "en-US");
        assert_eq!(language_id_for_locale_preferences(["!!!"]), "en-US");
    }

    #[test]
    fn imports_jsonc_language_pack_and_persists_normalized_json() {
        let root = std::env::temp_dir().join(format!("velotype-i18n-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root should be created");
        let source = root.join("language.jsonc");
        std::fs::write(
            &source,
            r#"{
                // Required metadata.
                "id": "ja-JP",
                "name": "日本語",
                "author": "",
                "strings": {
                    "menu_file": "ファイル",
                    "menu_export": ""
                }
            }"#,
        )
        .expect("language config should be written");

        let dirs = VelotypeConfigDirs::from_root(&root);
        let mut manager = I18nManager::default();
        let imported_id = manager
            .import_language_config_with_dirs(&source, &dirs)
            .expect("language config should import");

        assert_eq!(imported_id, "ja-JP");
        assert_eq!(manager.current_language_id(), "ja-JP");
        assert_eq!(manager.strings().menu_file, "ファイル");
        assert_eq!(manager.strings().menu_export, "Export");
        assert!(
            manager
                .available_languages()
                .iter()
                .any(|entry| entry.id == "ja-JP" && entry.name == "日本語")
        );

        let normalized = std::fs::read_to_string(dirs.languages_dir().join("ja-JP.json"))
            .expect("normalized language config should exist");
        assert!(normalized.contains("\"menu_file\": \"ファイル\""));
        assert!(!normalized.contains("menu_export"));
        assert!(!normalized.contains("author"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn custom_language_cannot_override_builtin_language_id() {
        let root = std::env::temp_dir().join(format!("velotype-i18n-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root should be created");
        let source = root.join("language.json");
        std::fs::write(
            &source,
            r#"{
                "id": "en-US",
                "name": "Override",
                "strings": { "menu_file": "Override" }
            }"#,
        )
        .expect("language config should be written");

        let dirs = VelotypeConfigDirs::from_root(&root);
        let mut manager = I18nManager::default();
        let err = manager
            .import_language_config_with_dirs(&source, &dirs)
            .expect_err("built-in language ids should be rejected");
        assert!(err.to_string().contains("built-in language"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn language_pack_json_falls_back_for_missing_strings() {
        let pack = I18nLanguagePack::from_json(
            r#"{
                "id": "zh-CN",
                "name": "简体中文",
                "strings": {
                    "menu_file": "文件菜单",
                    "unsaved_changes_hint": "legacy hint",
                    "drop_replace_hint": "legacy hint",
                    "unknown_field": "ignored"
                }
            }"#,
        )
        .expect("language pack should load");

        assert_eq!(pack.id, "zh-CN");
        assert_eq!(pack.strings.preferences_image_paste_picgo, "使用 PicGo");
        assert_eq!(
            pack.strings.image_paste_picgo_waiting,
            "等待图床链接返回..."
        );
        assert_eq!(pack.name, "简体中文");
        assert_eq!(pack.strings.menu_file, "文件菜单");
        assert_eq!(pack.strings.menu_export, "导出");
        assert_eq!(pack.strings.info_dialog_ok, "确定");
        assert_eq!(pack.strings.preferences_nav_settings, "设置");
        assert_eq!(pack.strings.preferences_associate_formats, "关联格式");
        assert_eq!(pack.strings.update_status_ready, "更新至 {version} 并重启");
        assert_eq!(pack.strings.update_open_release, "前往下载");
        assert_eq!(pack.strings.help_about_github_label, "GitHub");
        assert_eq!(
            pack.strings.help_about_star_message,
            "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
        );
    }

    #[test]
    fn unknown_language_pack_falls_back_to_english_strings() {
        let pack = I18nLanguagePack::from_json(
            r#"{
                "id": "fr-FR",
                "strings": {
                    "menu_file": "Fichier"
                }
            }"#,
        )
        .expect("language pack should load");

        assert_eq!(pack.id, "fr-FR");
        assert_eq!(pack.name, "fr-FR");
        assert_eq!(pack.strings.menu_file, "Fichier");
        assert_eq!(
            pack.strings.preferences_associate_formats,
            "Associate Formats"
        );
        assert_eq!(pack.strings.menu_export, "Export");
        assert_eq!(pack.strings.info_dialog_ok, "OK");
        assert_eq!(pack.strings.update_open_release, "Open Releases");
        assert_eq!(pack.strings.menu_open_recent_file, "Open Recent File");
        assert_eq!(pack.strings.menu_no_recent_files, "No Recent Files");
        assert_eq!(
            pack.strings.recent_file_missing_title,
            "Recent File Missing"
        );
        assert_eq!(pack.strings.help_about_github_label, "GitHub");
        assert_eq!(
            pack.strings.help_about_star_message,
            "If this project helps you, consider giving it a Star⭐. Thank you!"
        );
    }

    #[test]
    fn file_association_strings_support_custom_overrides_and_fallbacks() {
        let pack = I18nLanguagePack::from_json(
            r#"{
                "id": "custom",
                "strings": {
                    "preferences_associate_formats": "Connect Markdown",
                    "preferences_file_associations_pending": ""
                }
            }"#,
        )
        .unwrap();
        let defaults = I18nStrings::en_us();
        assert_eq!(
            pack.strings.preferences_associate_formats,
            "Connect Markdown"
        );
        assert_eq!(
            pack.strings.preferences_file_associations,
            defaults.preferences_file_associations
        );
        assert_eq!(
            pack.strings.preferences_file_associations_checking,
            defaults.preferences_file_associations_checking
        );
        assert_eq!(
            pack.strings.preferences_file_associations_associated,
            defaults.preferences_file_associations_associated
        );
        assert_eq!(
            pack.strings.preferences_file_associations_pending,
            defaults.preferences_file_associations_pending
        );
        assert_eq!(
            pack.strings.preferences_file_associations_failed,
            defaults.preferences_file_associations_failed
        );
        for key in super::I18N_STRING_KEYS.iter().filter(|key| {
            key.starts_with("preferences_file_associations")
                || **key == "preferences_associate_formats"
        }) {
            assert!(
                include_str!("../../assets/custom-language.example.jsonc")
                    .contains(&format!("\"{key}\""))
            );
        }
    }
}
