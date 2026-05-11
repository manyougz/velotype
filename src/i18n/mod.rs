//! Localised UI strings and runtime language selection.
//!
//! This module owns language packs, system-locale matching, and the global
//! manager used by menus and editor UI. Visual styling remains in `theme`.

use std::path::Path;

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
    /// Title of the placeholder update-check dialog.
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
    /// Top-level Help menu label.
    pub menu_help: String,
    /// Language menu item for importing a custom language pack.
    pub menu_add_language_config: String,
    /// Theme menu item for importing a custom theme pack.
    pub menu_add_theme_config: String,
    /// File menu item for opening a new window.
    pub menu_new_window: String,
    /// File menu item for opening Markdown files.
    pub menu_open_file: String,
    /// File menu item for opening a recent file submenu.
    pub menu_open_recent_file: String,
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
    /// Title of the custom configuration import failure prompt.
    pub config_import_failed_title: String,
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
}

/// Partial string set used by JSON language packs.
#[derive(Debug, Default, Deserialize)]
struct I18nStringsDe {
    dirty_title_marker: Option<String>,
    unsaved_changes_title: Option<String>,
    unsaved_changes_message: Option<String>,
    unsaved_changes_save_and_close: Option<String>,
    unsaved_changes_discard_and_close: Option<String>,
    unsaved_changes_cancel: Option<String>,
    drop_replace_title: Option<String>,
    drop_replace_message: Option<String>,
    drop_replace_save_and_replace: Option<String>,
    drop_replace_discard_and_replace: Option<String>,
    drop_replace_cancel: Option<String>,
    drop_no_markdown_file_message: Option<String>,
    info_dialog_ok: Option<String>,
    help_check_updates_title: Option<String>,
    help_check_updates_message: Option<String>,
    update_available_title: Option<String>,
    update_available_message_template: Option<String>,
    update_up_to_date_title: Option<String>,
    update_up_to_date_message_template: Option<String>,
    update_failed_title: Option<String>,
    update_failed_message_template: Option<String>,
    update_open_release: Option<String>,
    update_later: Option<String>,
    help_about_title: Option<String>,
    help_about_message: Option<String>,
    help_about_github_label: Option<String>,
    help_about_star_message: Option<String>,
    menu_file: Option<String>,
    menu_export: Option<String>,
    menu_language: Option<String>,
    menu_theme: Option<String>,
    menu_help: Option<String>,
    menu_add_language_config: Option<String>,
    menu_add_theme_config: Option<String>,
    menu_new_window: Option<String>,
    menu_open_file: Option<String>,
    menu_open_recent_file: Option<String>,
    menu_no_recent_files: Option<String>,
    menu_save: Option<String>,
    menu_save_as: Option<String>,
    menu_quit: Option<String>,
    menu_export_html: Option<String>,
    menu_export_pdf: Option<String>,
    menu_check_updates: Option<String>,
    menu_about: Option<String>,
    open_markdown_files_prompt: Option<String>,
    add_language_config_prompt: Option<String>,
    add_theme_config_prompt: Option<String>,
    open_failed_title: Option<String>,
    recent_file_missing_title: Option<String>,
    recent_file_missing_message_template: Option<String>,
    save_failed_title: Option<String>,
    export_failed_title: Option<String>,
    config_import_failed_title: Option<String>,
    open_link_title: Option<String>,
    open_link_open: Option<String>,
    open_link_cancel: Option<String>,
    view_mode_source: Option<String>,
    view_mode_switch_to_source: Option<String>,
    view_mode_rendered: Option<String>,
    view_mode_switch_to_rendered: Option<String>,
    context_menu_insert: Option<String>,
    context_menu_table: Option<String>,
    table_axis_align_column_left: Option<String>,
    table_axis_align_column_center: Option<String>,
    table_axis_align_column_right: Option<String>,
    table_axis_move_column_left: Option<String>,
    table_axis_move_column_right: Option<String>,
    table_axis_delete_column: Option<String>,
    table_axis_move_row_up: Option<String>,
    table_axis_move_row_down: Option<String>,
    table_axis_delete_row: Option<String>,
    table_insert_title: Option<String>,
    table_insert_description: Option<String>,
    table_insert_body_rows: Option<String>,
    table_insert_columns: Option<String>,
    table_insert_cancel: Option<String>,
    table_insert_confirm: Option<String>,
    image_placeholder: Option<String>,
    image_loading_without_alt: Option<String>,
    image_loading_with_alt_template: Option<String>,
    code_language_placeholder: Option<String>,
}

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
    "help_about_title",
    "help_about_message",
    "help_about_github_label",
    "help_about_star_message",
    "menu_file",
    "menu_export",
    "menu_language",
    "menu_theme",
    "menu_help",
    "menu_add_language_config",
    "menu_add_theme_config",
    "menu_new_window",
    "menu_open_file",
    "menu_open_recent_file",
    "menu_no_recent_files",
    "menu_save",
    "menu_save_as",
    "menu_quit",
    "menu_export_html",
    "menu_export_pdf",
    "menu_check_updates",
    "menu_about",
    "open_markdown_files_prompt",
    "add_language_config_prompt",
    "add_theme_config_prompt",
    "open_failed_title",
    "recent_file_missing_title",
    "recent_file_missing_message_template",
    "save_failed_title",
    "export_failed_title",
    "config_import_failed_title",
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
];

impl I18nStringsDe {
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
            menu_help: self.menu_help.unwrap_or(defaults.menu_help),
            menu_add_language_config: self
                .menu_add_language_config
                .unwrap_or(defaults.menu_add_language_config),
            menu_add_theme_config: self
                .menu_add_theme_config
                .unwrap_or(defaults.menu_add_theme_config),
            menu_new_window: self.menu_new_window.unwrap_or(defaults.menu_new_window),
            menu_open_file: self.menu_open_file.unwrap_or(defaults.menu_open_file),
            menu_open_recent_file: self
                .menu_open_recent_file
                .unwrap_or(defaults.menu_open_recent_file),
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
            config_import_failed_title: self
                .config_import_failed_title
                .unwrap_or(defaults.config_import_failed_title),
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
        }
    }
}

impl<'de> Deserialize<'de> for I18nStrings {
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
        Self {
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
            update_failed_title: "检查更新失败".into(),
            update_failed_message_template: "无法完成在线更新检查：{error}".into(),
            update_open_release: "前往下载".into(),
            update_later: "稍后".into(),
            help_about_title: "关于 Velotype".into(),
            help_about_message: "作者：manyougz".into(),
            help_about_github_label: "GitHub".into(),
            help_about_star_message: "如果本项目对您有帮助，那不妨给本项目一颗 Star⭐，十分感谢！"
                .into(),
            menu_file: "文件".into(),
            menu_export: "导出".into(),
            menu_language: "语言".into(),
            menu_theme: "主题".into(),
            menu_help: "帮助".into(),
            menu_add_language_config: "添加语言配置".into(),
            menu_add_theme_config: "添加主题配置".into(),
            menu_new_window: "新建窗口".into(),
            menu_open_file: "打开文件".into(),
            menu_open_recent_file: "打开最近文件".into(),
            menu_no_recent_files: "无最近文件".into(),
            menu_save: "保存".into(),
            menu_save_as: "另存为".into(),
            menu_quit: "退出".into(),
            menu_export_html: "HTML".into(),
            menu_export_pdf: "PDF".into(),
            menu_check_updates: "检查更新".into(),
            menu_about: "关于".into(),
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
        }
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
            update_failed_title: "Update Check Failed".into(),
            update_failed_message_template: "Unable to complete the online update check: {error}"
                .into(),
            update_open_release: "Open Releases".into(),
            update_later: "Later".into(),
            help_about_title: "About Velotype".into(),
            help_about_message: "Author: manyougz".into(),
            help_about_github_label: "GitHub".into(),
            help_about_star_message:
                "If this project helps you, consider giving it a Star⭐. Thank you!".into(),
            menu_file: "File".into(),
            menu_export: "Export".into(),
            menu_language: "Language".into(),
            menu_theme: "Theme".into(),
            menu_help: "Help".into(),
            menu_add_language_config: "Add Language Config".into(),
            menu_add_theme_config: "Add Theme Config".into(),
            menu_new_window: "New Window".into(),
            menu_open_file: "Open File".into(),
            menu_open_recent_file: "Open Recent File".into(),
            menu_no_recent_files: "No Recent Files".into(),
            menu_save: "Save".into(),
            menu_save_as: "Save As".into(),
            menu_quit: "Quit".into(),
            menu_export_html: "HTML".into(),
            menu_export_pdf: "PDF".into(),
            menu_check_updates: "Check for Updates".into(),
            menu_about: "About".into(),
            open_markdown_files_prompt: "Open Markdown Files".into(),
            add_language_config_prompt: "Choose Language Config".into(),
            add_theme_config_prompt: "Choose Theme Config".into(),
            open_failed_title: "Open Failed".into(),
            recent_file_missing_title: "Recent File Missing".into(),
            recent_file_missing_message_template:
                "This recent file no longer exists and has been removed:\n{path}".into(),
            save_failed_title: "Save Failed".into(),
            export_failed_title: "Export Failed".into(),
            config_import_failed_title: "Config Import Failed".into(),
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
    pub id: String,
    pub name: String,
}

const BUILTIN_LANGUAGE_ZH_CN_ID: &str = "zh-CN";
const BUILTIN_LANGUAGE_ZH_CN_NAME: &str = "简体中文";
const BUILTIN_LANGUAGE_EN_US_ID: &str = "en-US";
const BUILTIN_LANGUAGE_EN_US_NAME: &str = "English";

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
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub strings: I18nStrings,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct I18nLanguagePackDe {
    id: String,
    name: Option<String>,
    author: Option<String>,
    description: Option<String>,
    version: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
    #[serde(default)]
    strings: I18nStringsDe,
}

#[allow(dead_code)]
impl I18nLanguagePack {
    /// Parses a language pack from JSON text.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let mut value: Value = serde_json::from_str(json)?;
        prune_empty_json_values(&mut value);
        Self::from_value(value)
    }

    fn from_value(value: Value) -> anyhow::Result<Self> {
        let raw: I18nLanguagePackDe = serde_json::from_value(value)?;
        Ok(Self::from_partial(raw))
    }

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
fn language_name_for_id(language_id: &str) -> Option<&'static str> {
    match language_id {
        BUILTIN_LANGUAGE_ZH_CN_ID => Some(BUILTIN_LANGUAGE_ZH_CN_NAME),
        BUILTIN_LANGUAGE_EN_US_ID => Some(BUILTIN_LANGUAGE_EN_US_NAME),
        _ => None,
    }
}

fn is_builtin_language_id(language_id: &str) -> bool {
    matches!(
        language_id,
        BUILTIN_LANGUAGE_ZH_CN_ID | BUILTIN_LANGUAGE_EN_US_ID
    )
}

fn is_valid_custom_language_id(language_id: &str) -> bool {
    !language_id.trim().is_empty()
        && language_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && language_id.chars().any(|ch| ch.is_ascii_alphabetic())
}

/// Selects a built-in language id from preferred system locales.
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
    current_language_id: String,
    strings: I18nStrings,
    custom_languages: Vec<I18nLanguagePack>,
    language_catalog: Vec<LanguageCatalogEntry>,
}

impl Global for I18nManager {}

impl Default for I18nManager {
    fn default() -> Self {
        Self::new_with_language_id(BUILTIN_LANGUAGE_EN_US_ID)
    }
}

impl I18nManager {
    /// Installs the detected UI language into GPUI's global state.
    pub fn init(cx: &mut App) {
        let language_id = language_id_for_locale_preferences(sys_locale::get_locales());
        let mut manager = Self::new_with_language_id(language_id);
        if let Ok(dirs) = VelotypeConfigDirs::from_system() {
            if let Err(err) = manager.load_custom_languages_from_dirs(&dirs) {
                eprintln!("failed to load custom languages: {err}");
            }
        }
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
            strings: I18nStrings::for_language_id(current_language_id)
                .unwrap_or_else(I18nStrings::en_us),
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

    /// Returns all built-in and imported UI languages exposed in the menu.
    pub fn available_languages(&self) -> &[LanguageCatalogEntry] {
        &self.language_catalog
    }

    /// Activates a UI language by identifier.
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
        self.strings = strings;
        changed
    }

    /// Imports a user language pack, persists a normalized copy, and activates it.
    pub fn import_language_config(&mut self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let dirs = VelotypeConfigDirs::from_system()?;
        self.import_language_config_with_dirs(path, &dirs)
    }

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
        assert_eq!(pack.name, "简体中文");
        assert_eq!(pack.strings.menu_file, "文件菜单");
        assert_eq!(pack.strings.menu_export, "导出");
        assert_eq!(pack.strings.info_dialog_ok, "确定");
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
}
