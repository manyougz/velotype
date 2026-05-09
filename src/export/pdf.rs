//! PDF generation through ironpress.

use std::path::Path;

use anyhow::Context as _;
use ironpress::HtmlConverter;

use crate::export::html::render_html;
use crate::theme::Theme;

/// Renders themed PDF bytes from Markdown by reusing the HTML exporter.
pub(crate) fn render_pdf(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let html = render_html(markdown, theme, title);
    let mut converter = HtmlConverter::new().sanitize(false);
    if let Some(base_path) = base_path {
        converter = converter.base_path(base_path);
    }

    converter
        .convert(&html)
        .context("failed to convert HTML export to PDF")
}

#[cfg(test)]
mod tests {
    use super::render_pdf;
    use crate::theme::Theme;

    #[test]
    fn renders_pdf_bytes() {
        let pdf = render_pdf("# Title\n\nBody", &Theme::default_theme(), "Doc", None)
            .expect("pdf should render");

        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn renders_themed_pdf_for_rich_markdown() {
        let markdown = concat!(
            "# Title\n\n",
            "Body text with `inline code`.\n\n",
            "```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n",
            "| A | B |\n| - | - |\n| 1 | 2 |\n\n",
            "> [!NOTE]\n> Callout body\n\n",
            "> Quoted text\n"
        );
        let pdf = render_pdf(markdown, &Theme::default_theme(), "Doc", None)
            .expect("rich themed pdf should render");

        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 1000);
    }
}
