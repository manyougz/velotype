//! PDF generation through a local Chromium-compatible browser.
//!
//! The browser HTML export is the source of truth for visual PDF fidelity. This
//! module writes that HTML to a temporary file, opens it in headless Chromium,
//! and asks DevTools to print the page to PDF.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use futures::StreamExt;
use uuid::Uuid;

use crate::export::html::render_chromium_pdf_html_with_base_dir_and_fonts;
use crate::fonts::FontSettings;
use crate::theme::Theme;

const CHROMIUM_VIEWPORT_WIDTH: u32 = 1280;
const CHROMIUM_VIEWPORT_HEIGHT: u32 = 1600;
const PDF_TIMEOUT: Duration = Duration::from_secs(45);

pub(crate) fn render_pdf_with_fonts(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
    fonts: &FontSettings,
) -> anyhow::Result<Vec<u8>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("velotype-pdf-export")
        .build()
        .context("failed to create PDF export runtime")?;
    runtime.block_on(async move {
        tokio::time::timeout(
            PDF_TIMEOUT,
            render_pdf_async_with_fonts(markdown, theme, title, base_path, fonts),
        )
        .await
        .map_err(|_| anyhow!("PDF export timed out while waiting for Chromium"))?
    })
}

async fn render_pdf_async_with_fonts(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
    fonts: &FontSettings,
) -> anyhow::Result<Vec<u8>> {
    let html =
        render_chromium_pdf_html_with_base_dir_and_fonts(markdown, theme, title, base_path, fonts);
    let temp = PdfTempFiles::create(&html)?;
    let result = render_pdf_from_html_file_async(temp.html_path.clone()).await;
    temp.cleanup();
    result
}

async fn render_pdf_from_html_file_async(html_path: PathBuf) -> anyhow::Result<Vec<u8>> {
    let user_data_dir = unique_temp_path("velotype-chromium-profile");
    fs::create_dir_all(&user_data_dir)
        .with_context(|| format!("failed to create '{}'", user_data_dir.display()))?;

    let config = BrowserConfig::builder()
        .new_headless_mode()
        .window_size(CHROMIUM_VIEWPORT_WIDTH, CHROMIUM_VIEWPORT_HEIGHT)
        .user_data_dir(user_data_dir.clone())
        .build()
        .map_err(|err| anyhow!("failed to build Chromium browser config: {err}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|err| {
        anyhow!(
            "failed to launch Chromium for PDF export: {err}. Install Chrome, Chromium, or Edge, or set the CHROME environment variable to the browser executable path"
        )
    })?;

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    let result = async {
        let file_url = file_url_from_path(&html_path)?;
        let page = browser
            .new_page(file_url.as_str())
            .await
            .context("failed to open export HTML in Chromium")?;
        page.wait_for_navigation()
            .await
            .context("Chromium did not finish loading export HTML")?;

        let params = chromium_pdf_params();
        page.pdf(params)
            .await
            .context("Chromium failed to print export HTML to PDF")
    }
    .await;

    let _ = browser.close().await;
    handler_task.abort();
    let _ = fs::remove_dir_all(&user_data_dir);

    result
}

fn chromium_pdf_params() -> PrintToPdfParams {
    let mut params = PrintToPdfParams::default();
    params.print_background = Some(true);
    params.prefer_css_page_size = Some(true);
    params.paper_width = Some(8.27);
    params.paper_height = Some(11.69);
    params.margin_top = Some(0.0);
    params.margin_bottom = Some(0.0);
    params.margin_left = Some(0.0);
    params.margin_right = Some(0.0);
    params
}

fn file_url_from_path(path: &Path) -> anyhow::Result<url::Url> {
    url::Url::from_file_path(path)
        .map_err(|_| anyhow!("failed to convert '{}' to a file URL", path.display()))
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

struct PdfTempFiles {
    html_path: PathBuf,
}

impl PdfTempFiles {
    fn create(html: &str) -> anyhow::Result<Self> {
        let html_path = unique_temp_path("velotype-export").with_extension("html");
        fs::write(&html_path, html)
            .with_context(|| format!("failed to write temporary HTML '{}'", html_path.display()))?;
        Ok(Self { html_path })
    }

    fn cleanup(&self) {
        let _ = fs::remove_file(&self.html_path);
    }
}

impl Drop for PdfTempFiles {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::{chromium_pdf_params, file_url_from_path, render_pdf_with_fonts};
    use crate::export::html::render_chromium_pdf_html_with_base_dir_and_fonts;
    use crate::fonts::{FontCatalog, FontPreferences, FontSettings, SYSTEM_UI_FONT};
    use crate::theme::Theme;

    #[test]
    fn chromium_pdf_html_uses_print_layout_and_preserves_resources() {
        let fonts = FontSettings::resolve(
            FontPreferences::default(),
            &FontCatalog::from_names(vec![SYSTEM_UI_FONT.into()]),
            std::env::consts::OS,
        )
        .expect("default font stacks are valid");
        let html = render_chromium_pdf_html_with_base_dir_and_fonts(
            "# Title\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n$$\nx^2\n$$",
            &Theme::default_theme(),
            "Doc",
            None,
            &fonts,
        );

        assert!(html.contains("@page"));
        assert!(html.contains("size: A4"));
        assert!(html.contains("margin: 15mm"));
        assert!(html.contains("class=\"vlt-document\""));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(html.contains("<svg"));
        assert!(!html.contains("width: min(100% - 48px, 920px);"));
    }

    #[test]
    fn chromium_pdf_params_use_page_css_and_backgrounds() {
        let params = chromium_pdf_params();

        assert_eq!(params.print_background, Some(true));
        assert_eq!(params.prefer_css_page_size, Some(true));
        assert_eq!(params.margin_top, Some(0.0));
        assert_eq!(params.margin_bottom, Some(0.0));
        assert_eq!(params.margin_left, Some(0.0));
        assert_eq!(params.margin_right, Some(0.0));
    }

    #[test]
    fn file_url_from_path_supports_local_paths() {
        let path = std::env::temp_dir().join("velotype pdf test.html");
        let url = file_url_from_path(&path).expect("file url");

        assert_eq!(url.scheme(), "file");
        assert!(url.as_str().contains("velotype%20pdf%20test.html"));
    }

    #[test]
    fn render_pdf_reports_actionable_error_without_chromium() {
        let fonts = FontSettings::resolve(
            FontPreferences::default(),
            &FontCatalog::from_names(vec![SYSTEM_UI_FONT.into()]),
            std::env::consts::OS,
        )
        .expect("default font stacks are valid");
        match render_pdf_with_fonts(
            "# Title\n\nBody",
            &Theme::default_theme(),
            "Doc",
            None,
            &fonts,
        ) {
            Ok(pdf) => assert!(pdf.starts_with(b"%PDF")),
            Err(err) => {
                let message = err.to_string();
                assert!(
                    message.contains("Chromium")
                        || message.contains("Chrome")
                        || message.contains("CHROME"),
                    "unexpected PDF export error: {message}"
                );
            }
        }
    }
}
