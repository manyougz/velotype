//! HTML document generation for Markdown export.

use std::fs;
use std::path::Path;

use base64::{Engine as _, engine::general_purpose};
use gpui::{Hsla, Rgba};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

use crate::components::{
    inline_math_font_size, is_mermaid_closing_fence, parse_display_math_source,
    parse_html_image_block, parse_mermaid_fence_source, parse_mermaid_fence_start,
    render_latex_to_svg, render_mermaid_to_svg, sanitize_html_for_export,
};
use crate::fonts::{FontCatalog, FontPreferences, FontSettings};
use crate::net;
use crate::theme::{FontWeightDef, Theme};

/// Builds a full HTML document with embedded CSS derived from the active theme.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_html(markdown: &str, theme: &Theme, title: &str) -> String {
    render_html_with_base_dir(markdown, theme, title, None)
}

/// Builds export HTML and resolves local Markdown image paths relative to the source document.
pub(crate) fn render_html_with_base_dir(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
) -> String {
    render_html_with_base_dir_and_fonts(markdown, theme, title, base_dir, &default_font_settings())
}

pub(crate) fn render_html_with_base_dir_and_fonts(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
    fonts: &FontSettings,
) -> String {
    render_html_document(markdown, theme, title, base_dir, &theme_css(theme, fonts))
}

pub(crate) fn render_chromium_pdf_html_with_base_dir_and_fonts(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
    fonts: &FontSettings,
) -> String {
    render_html_document(
        markdown,
        theme,
        title,
        base_dir,
        &chromium_pdf_theme_css(theme, fonts),
    )
}

fn render_html_document(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
    css: &str,
) -> String {
    let document_lang = if contains_tibetan_text(markdown) || contains_tibetan_text(title) {
        "bo"
    } else {
        "en"
    };
    let body = render_browser_html_body(markdown, theme, base_dir);

    format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n{}\n</style>\n</head>\n<body>\n<main class=\"vlt-document\">\n{}</main>\n</body>\n</html>\n",
        document_lang,
        escape_html(title),
        css,
        body,
    )
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_GFM);
    options
}

fn render_browser_html_body(markdown: &str, theme: &Theme, base_dir: Option<&Path>) -> String {
    let rewritten = rewrite_visible_comment_blocks(markdown);
    let rewritten = rewrite_unsafe_html_blocks(&rewritten, base_dir);
    let rewritten = rewrite_display_math_blocks(&rewritten, theme);
    let rewritten = rewrite_inline_math(&rewritten, theme);
    let rewritten = rewrite_mermaid_blocks(&rewritten);
    let parser = Parser::new_ext(&rewritten, markdown_options())
        .map(|event| rewrite_local_image_event(event, base_dir));
    let mut body = String::new();
    html::push_html(&mut body, parser);
    body
}

fn rewrite_visible_comment_blocks(markdown: &str) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut active_fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        if !is_root_comment_start(line) {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let start = index;
        let mut end = index;
        while end < lines.len() && !lines[end].contains("-->") {
            end += 1;
        }

        if end >= lines.len() {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let raw_comment = lines[start..=end].join("\n");
        rewritten.push(format!(
            "<pre class=\"vlt-comment\">{}</pre>",
            escape_html(&raw_comment)
        ));
        index = end + 1;
    }

    rewritten.join("\n")
}

fn rewrite_inline_math(markdown: &str, theme: &Theme) -> String {
    let mut rewritten = Vec::new();
    let mut active_fence: Option<(char, usize)> = None;
    for line in markdown.split('\n') {
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            continue;
        }

        rewritten.push(rewrite_inline_math_line(line, theme));
    }

    rewritten.join("\n")
}

fn rewrite_inline_math_line(line: &str, theme: &Theme) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < line.len() {
        if line[index..].starts_with('`') {
            let run_len = line[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if let Some(close) = find_backtick_run(line, index + run_len, run_len) {
                output.push_str(&line[index..close + run_len]);
                index = close + run_len;
                continue;
            }
        }

        if let Some((end, body)) = locate_inline_dollar_math_source(line, index)
            .or_else(|| locate_inline_paren_math_source(line, index))
        {
            match render_latex_to_svg(
                &body,
                theme.colors.text_default,
                inline_math_font_size(theme.typography.text_size),
            ) {
                Ok(svg) => {
                    output.push_str(&format!("<span class=\"vlt-inline-math\">{svg}</span>"))
                }
                Err(_) => output.push_str(&escape_html(&line[index..end])),
            }
            index = end;
            continue;
        }

        if let Some((end, body, tag)) = locate_inline_script_source(line, index) {
            output.push_str(&format!("<{tag}>{}</{tag}>", escape_html(&body)));
            index = end;
            continue;
        }

        let Some(ch) = line[index..].chars().next() else {
            break;
        };
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn find_backtick_run(line: &str, mut index: usize, run_len: usize) -> Option<usize> {
    while index < line.len() {
        if line[index..].starts_with(&"`".repeat(run_len)) {
            return Some(index);
        }
        index += line[index..].chars().next()?.len_utf8();
    }
    None
}

fn locate_inline_dollar_math_source(line: &str, index: usize) -> Option<(usize, String)> {
    if !line[index..].starts_with('$')
        || line[index..].starts_with("$$")
        || is_escaped_ascii(line, index)
    {
        return None;
    }
    let mut cursor = index + 1;
    while cursor < line.len() {
        if line[cursor..].starts_with('$')
            && !line[cursor..].starts_with("$$")
            && !is_escaped_ascii(line, cursor)
        {
            let body = &line[index + 1..cursor];
            if valid_inline_math_body(body)
                && !looks_like_export_currency(line, index, cursor, body)
            {
                return Some((cursor + 1, body.to_string()));
            }
            return None;
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }
    None
}

fn locate_inline_script_source(line: &str, index: usize) -> Option<(usize, String, &'static str)> {
    if is_escaped_ascii(line, index) {
        return None;
    }

    if line[index..].starts_with('^') {
        locate_script_close(line, index, '^').map(|(end, body)| (end, body, "sup"))
    } else if is_single_tilde_marker(line, index) {
        locate_script_close(line, index, '~').map(|(end, body)| (end, body, "sub"))
    } else {
        None
    }
}

fn locate_script_close(line: &str, index: usize, marker: char) -> Option<(usize, String)> {
    let prev = previous_char(line, index)?;
    if !prev.is_ascii_alphanumeric() {
        return None;
    }

    let body_start = index + marker.len_utf8();
    let first = line[body_start..].chars().next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }

    let mut cursor = body_start;
    while cursor < line.len() {
        if line[cursor..].starts_with(marker)
            && !is_escaped_ascii(line, cursor)
            && (marker != '~' || is_single_tilde_marker(line, cursor))
        {
            let body = &line[body_start..cursor];
            return body
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric())
                .then(|| (cursor + marker.len_utf8(), body.to_string()));
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }

    None
}

fn previous_char(line: &str, index: usize) -> Option<char> {
    line.get(..index)?.chars().next_back()
}

fn is_single_tilde_marker(line: &str, index: usize) -> bool {
    line[index..].starts_with('~')
        && previous_char(line, index).is_none_or(|ch| ch != '~')
        && line[index + 1..].chars().next().is_none_or(|ch| ch != '~')
}

fn locate_inline_paren_math_source(line: &str, index: usize) -> Option<(usize, String)> {
    if !line[index..].starts_with("\\(") {
        return None;
    }
    let mut cursor = index + 2;
    while cursor + 1 < line.len() {
        if line[cursor..].starts_with("\\)") {
            let body = &line[index + 2..cursor];
            if valid_inline_math_body(body) {
                return Some((cursor + 2, body.to_string()));
            }
            return None;
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }
    None
}

fn valid_inline_math_body(body: &str) -> bool {
    !body.is_empty() && !body.contains(['\n', '\r']) && body.trim() == body && !body.is_empty()
}

fn is_escaped_ascii(line: &str, index: usize) -> bool {
    let mut slash_count = 0usize;
    let mut cursor = index;
    while cursor > 0 && line.as_bytes()[cursor - 1] == b'\\' {
        slash_count += 1;
        cursor -= 1;
    }
    slash_count % 2 == 1
}

fn looks_like_export_currency(line: &str, open: usize, close: usize, body: &str) -> bool {
    let prev_is_digit = open > 0 && line.as_bytes()[open - 1].is_ascii_digit();
    let next_is_digit = close + 1 < line.len() && line.as_bytes()[close + 1].is_ascii_digit();
    (prev_is_digit || next_is_digit)
        || (body
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ',' | '_'))
            && body.chars().any(|ch| ch.is_ascii_digit())
            && body.len() > 1)
}

fn rewrite_unsafe_html_blocks(markdown: &str, base_dir: Option<&Path>) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut active_fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let Some(html_start) = root_html_start(line) else {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        };

        let end = collect_export_html_region(&lines, index, &html_start);
        let raw = lines[index..end].join("\n");
        if let Some(image) = parse_html_image_block(&raw) {
            let src =
                local_image_data_uri(&image.src, base_dir).unwrap_or_else(|| image.src.clone());
            rewritten.push(image.to_sanitized_html_with_src(&src));
        } else {
            rewritten.push(sanitize_html_for_export(&raw));
        }
        index = end;
    }

    rewritten.join("\n")
}

fn rewrite_display_math_blocks(markdown: &str, theme: &Theme) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut active_fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        if !is_root_display_math_start(line) {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let end = collect_display_math_region(&lines, index);
        let raw = lines[index..end].join("\n");
        if let Some(source) = parse_display_math_source(&raw) {
            match render_latex_to_svg(
                &source.body,
                theme.colors.text_default,
                theme.typography.text_size,
            ) {
                Ok(svg) => rewritten.push(format!("<div class=\"vlt-math\">{svg}</div>")),
                Err(_) => rewritten.push(format!(
                    "<pre class=\"vlt-math-error\">{}</pre>",
                    escape_html(&raw)
                )),
            }
        } else {
            rewritten.push(raw);
        }
        index = end;
    }

    rewritten.join("\n")
}

fn rewrite_mermaid_blocks(markdown: &str) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let Some(fence) = parse_mermaid_fence_start(line) else {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        };

        let mut end = index + 1;
        while end < lines.len() && !is_mermaid_closing_fence(lines[end], fence) {
            end += 1;
        }
        if end >= lines.len() {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let raw = lines[index..=end].join("\n");
        if let Some(source) = parse_mermaid_fence_source(&raw) {
            match render_mermaid_to_svg(&source.body) {
                Ok(svg) => {
                    let src = data_uri_for_bytes("image/svg+xml", svg.as_bytes());
                    rewritten.push(format!(
                        "<div class=\"vlt-mermaid\"><img alt=\"Mermaid diagram\" src=\"{src}\"></div>"
                    ));
                }
                Err(_) => rewritten.push(format!(
                    "<pre class=\"vlt-mermaid-error\">{}</pre>",
                    escape_html(&raw)
                )),
            }
        } else {
            rewritten.push(raw);
        }
        index = end + 1;
    }

    rewritten.join("\n")
}

fn rewrite_local_image_event<'a>(event: Event<'a>, base_dir: Option<&Path>) -> Event<'a> {
    match event {
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = local_image_data_uri(dest_url.as_ref(), base_dir)
                .map(CowStr::from)
                .unwrap_or(dest_url);
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        event => event,
    }
}

fn local_image_data_uri(source: &str, base_dir: Option<&Path>) -> Option<String> {
    if source.is_empty()
        || source.starts_with('#')
        || source.starts_with("data:")
        || net::is_remote_image_source(source)
    {
        return None;
    }

    let path = Path::new(source);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    let mime = image_mime_from_path(&resolved)?;
    let bytes = fs::read(&resolved).ok()?;
    Some(data_uri_for_bytes(mime, &bytes))
}

fn image_mime_from_path(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn data_uri_for_bytes(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    )
}

#[derive(Clone, Debug)]
struct ExportHtmlStart {
    name: String,
    self_closing: bool,
    closes_same_line: bool,
}

fn root_html_start(line: &str) -> Option<ExportHtmlStart> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 || trimmed.starts_with("<!--") {
        return None;
    }

    let tagged = trimmed.strip_prefix('<')?;
    if tagged.starts_with('/') || tagged.starts_with('!') || tagged.starts_with('?') {
        return None;
    }
    let name_len = tagged
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .map(char::len_utf8)
        .sum::<usize>();
    if name_len == 0 {
        return None;
    }
    let name = tagged[..name_len].to_ascii_lowercase();
    let suffix = &tagged[name_len..];
    let next = suffix.chars().next()?;
    if !matches!(next, '>' | ' ' | '\t' | '/') {
        return None;
    }
    Some(ExportHtmlStart {
        self_closing: trimmed.ends_with("/>") || is_export_void_html_tag(&name),
        closes_same_line: trimmed.to_ascii_lowercase().contains(&format!("</{name}>")),
        name,
    })
}

fn is_export_void_html_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

fn collect_export_html_region(lines: &[&str], start: usize, html: &ExportHtmlStart) -> usize {
    if html.self_closing || html.closes_same_line {
        return start + 1;
    }

    let close = format!("</{}>", html.name);
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        if line.to_ascii_lowercase().contains(&close) {
            return index + 1;
        }
        if line.trim().is_empty() {
            return index;
        }
        index += 1;
    }

    lines.len()
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }

    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let run_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    (run_len >= 3).then_some((marker, run_len))
}

fn is_closing_fence(line: &str, marker: char, opening_run_len: usize) -> bool {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return false;
    }

    let run_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    run_len >= opening_run_len && trimmed[marker.len_utf8() * run_len..].trim().is_empty()
}

fn is_root_comment_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("<!--") && line.len() - trimmed.len() <= 3
}

fn is_root_display_math_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("$$") && line.len() - trimmed.len() <= 3
}

fn collect_display_math_region(lines: &[&str], start: usize) -> usize {
    let opener = lines[start].trim_start().trim_end();
    if opener != "$$" && opener[2..].contains("$$") {
        return start + 1;
    }

    let mut index = start + 1;
    while index < lines.len() {
        if lines[index].trim() == "$$" {
            return index + 1;
        }
        if lines[index].trim().is_empty() {
            return index;
        }
        index += 1;
    }
    lines.len()
}

fn theme_css(theme: &Theme, fonts: &FontSettings) -> String {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    let color_scheme = if c.editor_background.l >= 0.5 {
        "light"
    } else {
        "dark"
    };
    let pre_overflow = "overflow: auto;";
    let media_overflow = "overflow-x: auto;";
    format!(
        r#":root {{
  color-scheme: {};
  --vlt-bg: {};
  --vlt-text: {};
  --vlt-muted: {};
  --vlt-link: {};
  --vlt-border: {};
  --vlt-code-bg: {};
  --vlt-code-text: {};
  --vlt-comment-bg: {};
  --vlt-table-head-bg: {};
  --vlt-table-cell-bg: {};
  --vlt-quote-border: {};
  --vlt-quote-text: {};
  --vlt-callout-note-bg: {};
  --vlt-callout-note-border: {};
  --vlt-callout-tip-bg: {};
  --vlt-callout-tip-border: {};
  --vlt-callout-important-bg: {};
  --vlt-callout-important-border: {};
  --vlt-callout-warning-bg: {};
  --vlt-callout-warning-border: {};
  --vlt-callout-caution-bg: {};
  --vlt-callout-caution-border: {};
}}

* {{ box-sizing: border-box; }}
html {{ background-color: var(--vlt-bg); color: var(--vlt-text); }}
body {{
  margin: 0;
  background-color: var(--vlt-bg);
  color: var(--vlt-text);
  font-family: {};
  font-size: {}px;
  line-height: {};
}}
{}
p, ul, ol, blockquote, pre, table, hr {{ margin: 0 0 1rem; }}
h1, h2, h3, h4, h5, h6 {{
  margin: 1.6em 0 0.65em;
  line-height: 1.2;
  font-weight: {};
}}
h1 {{ color: {}; font-size: {}px; border-bottom: 1px solid; border-color: {}; padding-bottom: 0.2em; }}
h2 {{ color: {}; font-size: {}px; border-bottom: 1px solid; border-color: {}; padding-bottom: 0.18em; }}
h3 {{ color: {}; font-size: {}px; }}
h4 {{ color: {}; font-size: {}px; }}
h5 {{ color: {}; font-size: {}px; }}
h6 {{ color: {}; font-size: {}px; }}
a {{ color: var(--vlt-link); text-decoration-thickness: 0.08em; text-underline-offset: 0.18em; }}
blockquote {{
  margin-left: 0;
  padding: 0.5rem 0 0.5rem 1rem;
  border-left: 3px solid;
  border-color: var(--vlt-quote-border);
  color: var(--vlt-quote-text);
}}
blockquote.markdown-alert-note,
blockquote.markdown-alert-tip,
blockquote.markdown-alert-important,
blockquote.markdown-alert-warning,
blockquote.markdown-alert-caution {{
  padding: 0.75rem 1rem;
  border-left: 4px solid;
  border-radius: {}px;
}}
blockquote.markdown-alert-note {{ background-color: var(--vlt-callout-note-bg); border-color: var(--vlt-callout-note-border); }}
blockquote.markdown-alert-tip {{ background-color: var(--vlt-callout-tip-bg); border-color: var(--vlt-callout-tip-border); }}
blockquote.markdown-alert-important {{ background-color: var(--vlt-callout-important-bg); border-color: var(--vlt-callout-important-border); }}
blockquote.markdown-alert-warning {{ background-color: var(--vlt-callout-warning-bg); border-color: var(--vlt-callout-warning-border); }}
blockquote.markdown-alert-caution {{ background-color: var(--vlt-callout-caution-bg); border-color: var(--vlt-callout-caution-border); }}
code {{
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
  border-radius: 4px;
  padding: 0.12em 0.32em;
  font-family: {};
  font-size: {}px;
}}
pre {{
  {}
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
  border-radius: {}px;
  padding: 1rem;
}}
pre code {{ padding: 0; background-color: transparent; }}
.vlt-comment {{
  white-space: pre-wrap;
  background-color: var(--vlt-comment-bg);
  color: var(--vlt-text);
}}
.vlt-raw-html {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
.vlt-math {{
  display: flex;
  justify-content: center;
  margin: 1rem 0;
  {}
}}
.vlt-math svg {{
  max-width: 100%;
  height: auto;
}}
.vlt-mermaid {{
  display: flex;
  justify-content: center;
  margin: 1rem 0;
  {}
}}
.vlt-mermaid img {{
  max-width: 100%;
  height: auto;
  display: block;
  margin: 0 auto;
}}
.vlt-inline-math {{
  display: inline-flex;
  align-items: center;
  vertical-align: middle;
  max-width: 100%;
}}
.vlt-inline-math svg {{
  max-height: 1.8em;
  width: auto;
}}
.vlt-math-error {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
.vlt-mermaid-error {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
table {{
  width: 100%;
  border-collapse: collapse;
  display: table;
}}
th, td {{
  border: 1px solid;
  border-color: var(--vlt-border);
  padding: 0.5rem 0.65rem;
  vertical-align: top;
}}
th {{ background-color: var(--vlt-table-head-bg); font-weight: 600; }}
td {{ background-color: var(--vlt-table-cell-bg); }}
img {{ max-width: 100%; height: auto; display: block; margin: 1rem auto; }}
hr {{ border: 0; border-top: 1px solid; border-color: var(--vlt-border); }}
.footnote-definition {{
  color: var(--vlt-muted);
  font-size: 0.92em;
}}
"#,
        color_scheme,
        css_color(c.editor_background),
        css_color(c.text_default),
        css_color(c.dialog_muted),
        css_color(c.text_link),
        css_color(c.table_border),
        css_color(c.code_bg),
        css_color(c.code_text),
        css_color(c.comment_bg),
        css_color(c.table_header_bg),
        css_color(c.table_cell_bg),
        css_color(c.border_quote),
        css_color(c.text_quote),
        css_color(c.callout_note_bg),
        css_color(c.callout_note_border),
        css_color(c.callout_tip_bg),
        css_color(c.callout_tip_border),
        css_color(c.callout_important_bg),
        css_color(c.callout_important_border),
        css_color(c.callout_warning_bg),
        css_color(c.callout_warning_border),
        css_color(c.callout_caution_bg),
        css_color(c.callout_caution_border),
        fonts.body.css_font_family(),
        t.text_size,
        t.text_line_height,
        document_layout_css(),
        css_font_weight(&t.h1_weight),
        css_color(c.text_h1),
        t.h1_size,
        css_color(c.border_h1),
        css_color(c.text_h2),
        t.h2_size,
        css_color(c.border_h2),
        css_color(c.text_h3),
        t.h3_size,
        css_color(c.text_h4),
        t.h4_size,
        css_color(c.text_h5),
        t.h5_size,
        css_color(c.text_h6),
        t.h6_size,
        d.callout_radius,
        fonts.code.css_font_family(),
        t.code_size,
        pre_overflow,
        d.code_bg_radius,
        media_overflow,
        media_overflow
    )
}

fn chromium_pdf_theme_css(theme: &Theme, fonts: &FontSettings) -> String {
    let mut css = theme_css(theme, fonts);
    css = css.replace(
        document_layout_css(),
        ".vlt-document {\n  width: auto;\n  max-width: none;\n  margin: 0;\n  padding: 0;\n}",
    );
    css.push_str(
        r#"

@page {
  size: A4;
  margin: 15mm;
}

@media print {
  html,
  body {
    background-color: var(--vlt-bg);
    border: 0;
    outline: 0;
    box-shadow: none;
    print-color-adjust: exact;
    -webkit-print-color-adjust: exact;
  }

  .vlt-document {
    width: auto;
    max-width: none;
    margin: 0;
    padding: 0;
    border: 0;
    outline: 0;
    box-shadow: none;
  }

  pre,
  code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  img,
  svg {
    max-width: 100%;
    height: auto;
    break-inside: avoid;
  }

  table,
  blockquote,
  pre,
  .vlt-math,
  .vlt-mermaid {
    break-inside: avoid;
  }
}
"#,
    );
    css
}

fn default_font_settings() -> FontSettings {
    FontSettings::resolve(
        FontPreferences::default(),
        &FontCatalog::from_names(vec![".SystemUIFont".into()]),
        std::env::consts::OS,
    )
    .expect("default export font stacks are valid")
}

fn document_layout_css() -> &'static str {
    ".vlt-document {\n  width: min(100% - 48px, 920px);\n  margin: 0 auto;\n  padding: 48px 0 72px;\n}"
}

fn contains_tibetan_text(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{0f00}'..='\u{0fff}').contains(&ch))
}

fn css_color(color: Hsla) -> String {
    let color = Rgba::from(color);
    format!(
        "rgba({},{},{},{:.3})",
        css_color_channel(color.r),
        css_color_channel(color.g),
        css_color_channel(color.b),
        color.a.clamp(0.0, 1.0)
    )
}

fn css_color_channel(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn css_font_weight(weight: &FontWeightDef) -> u16 {
    match weight {
        FontWeightDef::Thin => 100,
        FontWeightDef::Light => 300,
        FontWeightDef::Normal => 400,
        FontWeightDef::Medium => 500,
        FontWeightDef::Semibold => 600,
        FontWeightDef::Bold => 700,
        FontWeightDef::Extrabold => 800,
        FontWeightDef::Black => 900,
    }
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        contains_tibetan_text, render_chromium_pdf_html_with_base_dir_and_fonts, render_html,
        render_html_with_base_dir,
    };
    use crate::fonts::{FontCatalog, FontPreferences, FontSettings, SYSTEM_UI_FONT};
    use crate::theme::Theme;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn renders_complete_html_document_with_theme_css() {
        let html = render_html("# Title\n\ntext", &Theme::default_theme(), "Doc");

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("<title>Doc</title>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("--vlt-bg:"));
        assert!(html.contains("<main class=\"vlt-document\">"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>text</p>"));
    }

    #[test]
    fn exports_distinct_safely_serialized_body_and_code_font_stacks() {
        let fonts = FontSettings::resolve(
            FontPreferences {
                body_stack: "'Body Family', sans-serif".into(),
                code_stack: "'Code \\\"Family', monospace".into(),
                ui_stack: ".SystemUIFont".into(),
            },
            &FontCatalog::from_names(Vec::new()),
            "windows",
        )
        .unwrap();
        let html = super::render_html_with_base_dir_and_fonts(
            "text `code`",
            &Theme::default_theme(),
            "Doc",
            None,
            &fonts,
        );
        assert!(html.contains("font-family: \"Body Family\", sans-serif"));
        assert!(html.contains("font-family: \"Code \\\"Family\", monospace"));
        assert!(!html.contains("Code \"Family"));
    }
    #[test]
    fn detects_tibetan_text_for_document_language() {
        assert!(contains_tibetan_text("\u{0f56}\u{0f7c}\u{0f51}"));
        assert!(!contains_tibetan_text("Chinese text"));
    }

    #[test]
    fn exports_tibetan_with_language_and_font_fallbacks() {
        let markdown = concat!(
            "\u{0f56}\u{0f7c}\u{0f51}\u{0f0b}\u{0f61}\u{0f72}\u{0f42}",
            " ",
            "\u{0f56}\u{0f7c}\u{0f51}\u{0f0b}\u{0f61}\u{0f72}\u{0f42} "
        );
        let html = render_html(markdown, &Theme::default_theme(), "Doc");

        assert!(html.contains("<html lang=\"bo\">"));
        assert!(html.contains("\u{0f56}\u{0f7c}\u{0f51}"));
        assert!(html.contains("\u{0f61}\u{0f72}\u{0f42}"));
        assert!(html.contains("\"Noto Serif Tibetan\""));
        assert!(html.contains("\"Microsoft Himalaya\""));
    }
    #[test]
    fn emits_pdf_compatible_theme_css() {
        let html = render_html("# Title\n\ntext", &Theme::default_theme(), "Doc");

        assert!(!html.contains("hsla("));
        assert!(html.contains("color-scheme: dark;"));
        assert!(html.contains("--vlt-bg: rgba(25,25,25,1.000);"));
        assert!(html.contains("html { background-color: var(--vlt-bg); color: var(--vlt-text); }"));
        assert!(html.contains("background-color: var(--vlt-code-bg);"));
        assert!(html.contains("border: 1px solid;\n  border-color: var(--vlt-border);"));
        assert!(html.contains(
            "blockquote.markdown-alert-note { background-color: var(--vlt-callout-note-bg); border-color: var(--vlt-callout-note-border); }"
        ));
        assert!(!html.contains("background: var("));
        assert!(!html.contains("border-left-color:"));
    }

    #[test]
    fn light_theme_exports_light_color_scheme() {
        let html = render_html("# Title\n\ntext", &Theme::light_theme(), "Doc");

        assert!(html.contains("color-scheme: light;"));
        assert!(html.contains("--vlt-bg: rgba(247,248,251,1.000);"));
        assert!(!html.contains("color-scheme: dark;"));
    }

    #[test]
    fn chromium_pdf_light_theme_clears_print_container_frames() {
        let fonts = FontSettings::resolve(
            FontPreferences::default(),
            &FontCatalog::from_names(vec![SYSTEM_UI_FONT.into()]),
            std::env::consts::OS,
        )
        .expect("default font stacks are valid");
        let html = render_chromium_pdf_html_with_base_dir_and_fonts(
            "# Title\n\ntext",
            &Theme::light_theme(),
            "Doc",
            None,
            &fonts,
        );

        assert!(html.contains("color-scheme: light;"));
        assert!(html.contains("background-color: var(--vlt-bg);"));
        assert!(html.contains("border: 0;"));
        assert!(html.contains("outline: 0;"));
        assert!(html.contains("box-shadow: none;"));
        assert!(!html.contains("color-scheme: dark;"));
    }

    #[test]
    fn enables_extended_markdown_features() {
        let markdown = "> [!NOTE]\n> body\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n\n~~old~~\n\nhello[^a]\n\n[^a]: footnote";
        let html = render_html(markdown, &Theme::default_theme(), "Doc");

        assert!(html.contains("markdown-alert-note"));
        assert!(html.contains("<table>"));
        assert!(html.contains("checked"));
        assert!(html.contains("<del>old</del>"));
        assert!(html.contains("footnote"));
    }

    #[test]
    fn renders_velotype_comment_blocks_as_visible_escaped_text() {
        let markdown = "<!--\n<strong>not html</strong>\n-->";
        let html = render_html(markdown, &Theme::default_theme(), "Doc");

        assert!(html.contains("class=\"vlt-comment\""));
        assert!(html.contains("&lt;!--"));
        assert!(html.contains("&lt;strong&gt;not html&lt;/strong&gt;"));
        assert!(!html.contains("<!--\n<strong>not html</strong>\n-->"));
    }

    #[test]
    fn does_not_rewrite_comment_markers_inside_fenced_code() {
        let markdown = "```\n<!--\nnot a comment block\n-->\n```";
        let html = render_html(markdown, &Theme::default_theme(), "Doc");

        assert!(!html.contains("class=\"vlt-comment\""));
        assert!(html.contains("&lt;!--"));
        assert!(html.contains("not a comment block"));
    }

    #[test]
    fn escapes_risky_raw_html_blocks_for_export() {
        let html = render_html("<script>alert(1)</script>", &Theme::default_theme(), "Doc");

        assert!(html.contains("class=\"vlt-raw-html\""));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn escapes_risky_child_inside_safe_html_for_export() {
        let html = render_html(
            "<div>safe<script>alert(1)</script>tail</div>",
            &Theme::default_theme(),
            "Doc",
        );

        assert!(html.contains("<div>safe"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("tail</div>"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn sanitizes_safe_html_style_attributes_for_export() {
        let html = render_html(
            "<span style=\"color:blue; background-image:url(javascript:bad); background-color:#ff0; font-size:120%\">x</span>",
            &Theme::default_theme(),
            "Doc",
        );

        assert!(html.contains(
            "style=\"color: rgba(0,0,255,1.000); background-color: rgba(255,255,0,1.000); font-size: 120%;\""
        ));
        assert!(!html.contains("background-image"));
    }

    #[test]
    fn escapes_title_and_markdown_body_html() {
        let html = render_html("# A & B", &Theme::default_theme(), "A & <B>");

        assert!(html.contains("<title>A &amp; &lt;B&gt;</title>"));
        assert!(html.contains("<h1>A &amp; B</h1>"));
    }

    #[test]
    fn exports_display_math_as_svg() {
        let html = render_html("$$\n\\frac{1}{2}\n$$", &Theme::default_theme(), "Doc");

        assert!(html.contains("class=\"vlt-math\""));
        assert!(html.contains("<svg"));
        assert!(!html.contains("$$\n\\frac{1}{2}\n$$"));
    }

    #[test]
    fn exports_mermaid_block_as_svg() {
        let html = render_html(
            "```mermaid\nflowchart LR\nA --> B\n```",
            &Theme::default_theme(),
            "Doc",
        );

        assert!(html.contains("class=\"vlt-mermaid\""));
        assert!(html.contains("<img alt=\"Mermaid diagram\""));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(!html.contains("```mermaid\nflowchart LR\nA --&gt; B\n```"));
    }

    #[test]
    fn exports_local_image_as_data_uri_when_base_dir_is_available() {
        let root = std::env::temp_dir().join(format!("velotype-html-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp export dir");
        fs::write(
            root.join("diagram.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"></svg>",
        )
        .expect("write local image");

        let html = render_html_with_base_dir(
            "![diagram](diagram.svg)",
            &Theme::default_theme(),
            "Doc",
            Some(&root),
        );
        let _ = fs::remove_dir_all(&root);

        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(!html.contains("src=\"diagram.svg\""));
    }

    #[test]
    fn exports_standalone_html_image_with_sanitized_zoom() {
        let root = std::env::temp_dir().join(format!("velotype-html-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp export dir");
        fs::write(root.join("diagram.png"), [137, 80, 78, 71]).expect("write local image");

        let html = render_html_with_base_dir(
            "<img src=\"diagram.png\" alt=\"diagram\" style=\"color:red; zoom:80%; width:10px\" />",
            &Theme::default_theme(),
            "Doc",
            Some(&root),
        );
        let _ = fs::remove_dir_all(&root);

        assert!(html.contains("<img src=\"data:image/png;base64,"));
        assert!(html.contains("alt=\"diagram\""));
        assert!(html.contains("style=\"zoom: 80%;\""));
        assert!(!html.contains("color:red"));
        assert!(!html.contains("width:10px"));
    }

    #[test]
    fn export_keeps_missing_local_image_path() {
        let root = std::env::temp_dir().join(format!("velotype-html-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp export dir");

        let html = render_html_with_base_dir(
            "![diagram](missing.png)",
            &Theme::default_theme(),
            "Doc",
            Some(&root),
        );
        let _ = fs::remove_dir_all(&root);

        assert!(html.contains("src=\"missing.png\""));
        assert!(!html.contains("data:image/png;base64,"));
    }

    #[test]
    fn exports_inline_math_as_svg() {
        let html = render_html("before $x^2$ after", &Theme::default_theme(), "Doc");

        assert!(html.contains("class=\"vlt-inline-math\""));
        assert!(html.contains("<svg"));
        assert!(!html.contains("$x^2$"));
        assert!(html.contains("before"));
        assert!(html.contains("after"));
    }

    #[test]
    fn export_inline_math_ignores_code_and_escaped_delimiters() {
        let html = render_html("`$x$` and \\$y$", &Theme::default_theme(), "Doc");

        assert!(!html.contains("class=\"vlt-inline-math\""));
        assert!(html.contains("$x$"));
        assert!(html.contains("$y$"));
    }

    #[test]
    fn exports_superscript_and_subscript_as_html_tags() {
        let html = render_html("x^2^ and H~2~O", &Theme::default_theme(), "Doc");

        assert!(html.contains("x<sup>2</sup>"));
        assert!(html.contains("H<sub>2</sub>O"));
    }

    #[test]
    fn export_script_rewrite_ignores_code_escaped_and_strikethrough() {
        let html = render_html(
            "`x^2^ H~2~O` \\^2^ \\~2~ ~~old~~",
            &Theme::default_theme(),
            "Doc",
        );

        assert!(!html.contains("<sup>2</sup>"));
        assert!(!html.contains("<sub>2</sub>"));
        assert!(html.contains("<code>x^2^ H~2~O</code>"));
        assert!(html.contains("^2^"));
        assert!(html.contains("~2~"));
        assert!(html.contains("<del>old</del>"));
    }

    #[test]
    fn invalid_display_math_exports_escaped_raw_markdown() {
        let html = render_html("$$\n\\frac{a}\n$$", &Theme::default_theme(), "Doc");

        assert!(html.contains("class=\"vlt-math-error\""));
        assert!(html.contains("$$\n\\frac{a}\n$$"));
        assert!(!html.contains("class=\"vlt-math\"><svg"));
    }

    #[test]
    fn invalid_mermaid_exports_escaped_raw_markdown() {
        let html = render_html(
            "```mermaid\nnot a real mermaid diagram ::::\n```",
            &Theme::default_theme(),
            "Doc",
        );

        assert!(html.contains("class=\"vlt-mermaid-error\""));
        assert!(html.contains("not a real mermaid diagram ::::"));
        assert!(!html.contains("data:image/svg+xml;base64,"));
    }
}
