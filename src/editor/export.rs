//! Editor-facing export flow and file writing.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gpui::*;

use super::Editor;
use crate::export::{self as document_export, ExportFormat};
use crate::i18n::I18nManager;
use crate::theme::{Theme, ThemeManager};

impl Editor {
    fn export_dialog_defaults(&self, format: ExportFormat) -> (PathBuf, String) {
        let extension = format.extension();
        if let Some(path) = self.file_path.as_ref() {
            let directory = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("untitled");
            return (directory, format!("{stem}.{extension}"));
        }

        (
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            format!("untitled.{extension}"),
        )
    }

    fn export_title(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn render_export_bytes(
        format: ExportFormat,
        markdown: &str,
        theme: &Theme,
        title: &str,
        output_path: &Path,
    ) -> anyhow::Result<Vec<u8>> {
        match format {
            ExportFormat::Html => {
                Ok(document_export::render_html(markdown, theme, title).into_bytes())
            }
            ExportFormat::Pdf => {
                document_export::render_pdf(markdown, theme, title, output_path.parent())
            }
        }
    }

    fn write_export_bytes(
        format: ExportFormat,
        markdown: &str,
        theme: &Theme,
        title: &str,
        path: &Path,
    ) -> anyhow::Result<()> {
        let bytes = Self::render_export_bytes(format, markdown, theme, title, path)?;
        std::fs::write(path, bytes).with_context(|| format!("failed to write '{}'", path.display()))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn export_document_to_path(
        &self,
        format: ExportFormat,
        path: &Path,
        cx: &App,
    ) -> anyhow::Result<()> {
        let markdown = self.serialized_document_text(cx);
        let theme = cx.global::<ThemeManager>().current().clone();
        let title = self.export_title();
        Self::write_export_bytes(format, &markdown, &theme, &title, path)
    }

    pub(crate) fn export_document_via_prompt(
        &mut self,
        format: ExportFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let markdown = self.serialized_document_text(cx);
        let theme = cx.global::<ThemeManager>().current().clone();
        let title = self.export_title();
        let (default_dir, suggested_name) = self.export_dialog_defaults(format);
        let prompt = cx.prompt_for_new_path(&default_dir, Some(&suggested_name));
        let window_handle = window.window_handle();

        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(err)) => {
                    let detail = err.to_string();
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            show_export_error(window, cx, &detail);
                        },
                    );
                    return;
                }
            };

            if path.extension().is_none() {
                path.set_extension(format.extension());
            }

            if let Err(err) = Self::write_export_bytes(format, &markdown, &theme, &title, &path) {
                let detail = err.to_string();
                let _ = cx.update_window(
                    window_handle,
                    move |_view: AnyView, window: &mut Window, cx: &mut App| {
                        show_export_error(window, cx, &detail);
                    },
                );
            }
        })
        .detach();
    }
}

fn show_export_error(window: &mut Window, cx: &mut App, detail: &str) {
    let strings = cx.global::<I18nManager>().strings().clone();
    let buttons = [strings.info_dialog_ok.as_str()];
    let _ = window.prompt(
        PromptLevel::Critical,
        &strings.export_failed_title,
        Some(detail),
        &buttons,
        cx,
    );
}
