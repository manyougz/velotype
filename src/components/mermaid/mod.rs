//! Mermaid fenced-block parsing and SVG rendering helpers.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use anyhow::{Context as _, anyhow};
use directories::ProjectDirs;

const SIMPLE_MERMAID_LINE_LIMIT: usize = 8;
const MERMAID_COMPLEX_TARGET_WIDTH_RATIO: f32 = 0.9;
const MERMAID_MAX_VIEWPORT_WIDTH_RATIO: f32 = 1.15;
const MERMAID_SCALE_PER_EXTRA_LINE: f32 = 0.035;
const MERMAID_MAX_SCALE: f32 = 1.75;

/// Opening fence metadata for a Mermaid fenced code block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MermaidFence {
    /// Fence marker, either backtick or tilde.
    pub(crate) marker: char,
    /// Opening fence run length.
    pub(crate) len: usize,
}

/// Parsed Mermaid source preserved from Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MermaidSource {
    /// Full Markdown source, including the opening and closing fences.
    pub(crate) raw: String,
    /// Mermaid diagram source between the fences.
    pub(crate) body: String,
    /// The full info string after the opening fence.
    pub(crate) info: String,
}

/// Result of rendering a Mermaid diagram into an SVG cache file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MermaidSvgRender {
    /// Path to the SVG file consumed by GPUI's image element.
    pub(crate) path: PathBuf,
    /// SVG document content, used by export paths.
    pub(crate) svg: String,
    /// Concrete display width encoded into the cached SVG.
    pub(crate) display_width: f32,
    /// Concrete display height encoded into the cached SVG.
    pub(crate) display_height: f32,
    /// Scale applied to the renderer's intrinsic SVG size for editor display.
    pub(crate) display_scale: f32,
}

/// Concrete dimensions encoded into a display SVG.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MermaidSvgSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Returns true when a fenced code info string declares Mermaid content.
pub(crate) fn is_mermaid_info_string(info: Option<&str>) -> bool {
    info.and_then(|info| info.split_whitespace().next())
        .is_some_and(|first| {
            first.eq_ignore_ascii_case("mermaid") || first.eq_ignore_ascii_case("mmd")
        })
}

/// Parse a line as a Mermaid opening fence.
pub(crate) fn parse_mermaid_fence_start(line: &str) -> Option<MermaidFence> {
    let trimmed = strip_fence_indent(line)?.trim_end();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if len < 3 {
        return None;
    }

    let info = trimmed[marker.len_utf8() * len..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }

    is_mermaid_info_string((!info.is_empty()).then_some(info))
        .then_some(MermaidFence { marker, len })
}

/// Returns true when `line` closes the given Mermaid fence.
pub(crate) fn is_mermaid_closing_fence(line: &str, fence: MermaidFence) -> bool {
    let Some(trimmed) = strip_fence_indent(line).map(str::trim_end) else {
        return false;
    };
    if !trimmed.starts_with(fence.marker) {
        return false;
    }

    let len = trimmed.chars().take_while(|ch| *ch == fence.marker).count();
    len >= fence.len && trimmed[fence.marker.len_utf8() * len..].trim().is_empty()
}

/// Parse raw fenced Markdown into the Mermaid diagram source it contains.
pub(crate) fn parse_mermaid_fence_source(raw: &str) -> Option<MermaidSource> {
    let raw = raw.trim_matches('\n').to_string();
    let lines = raw.split('\n').collect::<Vec<_>>();
    if lines.len() < 2 {
        return None;
    }

    let opening = strip_fence_indent(lines[0])?.trim_end();
    let fence = parse_mermaid_fence_start(opening)?;
    let info = opening[fence.marker.len_utf8() * fence.len..]
        .trim()
        .to_string();
    if !is_mermaid_closing_fence(lines.last()?, fence) {
        return None;
    }

    let body = lines[1..lines.len() - 1].join("\n");
    Some(MermaidSource { raw, body, info })
}

/// Render Mermaid source into a cached SVG sized for editor display.
pub(crate) fn render_mermaid_svg_for_display(
    source: &MermaidSource,
    available_width: f32,
    viewport_width: f32,
) -> anyhow::Result<MermaidSvgRender> {
    let svg = render_mermaid_to_svg(&source.body)?;
    let intrinsic = mermaid_svg_intrinsic_size(&svg)?;
    let scale = mermaid_display_scale(
        &source.body,
        intrinsic.width,
        intrinsic.height,
        available_width,
        viewport_width,
    );
    let (svg, size) = scale_mermaid_svg_for_display(&svg, scale)?;
    let key = mermaid_display_cache_key(&source.body, scale);
    let path = mermaid_cache_dir()?.join(format!("{key}.svg"));
    if !path.exists() {
        fs::write(&path, &svg).with_context(|| {
            format!(
                "failed to write Mermaid display SVG cache '{}'",
                path.display()
            )
        })?;
    }
    Ok(MermaidSvgRender {
        path,
        svg,
        display_width: size.width,
        display_height: size.height,
        display_scale: scale,
    })
}

/// Render a Mermaid diagram body into SVG text.
pub(crate) fn render_mermaid_to_svg(source: &str) -> anyhow::Result<String> {
    if !looks_like_supported_mermaid_source(source) {
        return Err(anyhow::anyhow!("unsupported Mermaid diagram"));
    }
    let svg = mermaid_rs_renderer::render(source).map_err(|err| anyhow::anyhow!("{err}"))?;
    if svg.contains("class=\"error-text\"") || svg.contains("Syntax error in text") {
        return Err(anyhow::anyhow!("Mermaid syntax error"));
    }
    Ok(svg)
}

/// Stable cache key for Mermaid content.
pub(crate) fn mermaid_cache_key(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Stable cache key for editor display SVG content and scale.
pub(crate) fn mermaid_display_cache_key(source: &str, scale: f32) -> String {
    let mut hasher = DefaultHasher::new();
    mermaid_cache_key(source).hash(&mut hasher);
    scale.max(0.1).to_bits().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Counts diagram lines that materially contribute to rendered complexity.
pub(crate) fn semantic_mermaid_line_count(source: &str) -> usize {
    let mut in_frontmatter = false;
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            if trimmed == "---" {
                in_frontmatter = !in_frontmatter;
                return false;
            }
            !(in_frontmatter || trimmed.starts_with("%%"))
        })
        .count()
}

/// Display scale used by the editor for rendered Mermaid diagrams.
pub(crate) fn mermaid_display_scale(
    source: &str,
    intrinsic_width: f32,
    intrinsic_height: f32,
    available_width: f32,
    viewport_width: f32,
) -> f32 {
    let line_count = semantic_mermaid_line_count(source);
    if line_count <= SIMPLE_MERMAID_LINE_LIMIT {
        return 1.0;
    }

    let intrinsic_width = intrinsic_width.max(1.0);
    let intrinsic_height = intrinsic_height.max(1.0);
    let available_width = available_width.max(1.0);
    let viewport_width = viewport_width.max(available_width);
    let extra_lines = line_count.saturating_sub(SIMPLE_MERMAID_LINE_LIMIT) as f32;

    let complexity_scale = (1.0 + extra_lines * MERMAID_SCALE_PER_EXTRA_LINE)
        .max(1.0)
        .min(MERMAID_MAX_SCALE);
    let target_column_width = available_width * MERMAID_COMPLEX_TARGET_WIDTH_RATIO;
    let column_fit_scale = if intrinsic_width < target_column_width {
        target_column_width / intrinsic_width
    } else {
        1.0
    };
    let viewport_limit_scale =
        (viewport_width * MERMAID_MAX_VIEWPORT_WIDTH_RATIO / intrinsic_width).max(1.0);
    let height_sanity_scale =
        (viewport_width * MERMAID_MAX_VIEWPORT_WIDTH_RATIO / intrinsic_height).max(1.0);

    complexity_scale
        .max(column_fit_scale)
        .min(viewport_limit_scale)
        .min(height_sanity_scale)
        .min(MERMAID_MAX_SCALE)
        .max(1.0)
}

fn strip_fence_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

/// Rewrites the root SVG element so GPUI sees the intended intrinsic size.
pub(crate) fn scale_mermaid_svg_for_display(
    svg: &str,
    scale: f32,
) -> anyhow::Result<(String, MermaidSvgSize)> {
    let scale = scale.max(0.1);
    let (start, end) = svg_root_tag_range(svg)?;
    let root_tag = &svg[start..end];
    let base_size = svg_root_size(root_tag)?;
    let size = MermaidSvgSize {
        width: (base_size.width * scale).max(1.0),
        height: (base_size.height * scale).max(1.0),
    };
    let rewritten_root = rewrite_svg_root_tag(root_tag, size)?;
    let mut rewritten = String::with_capacity(svg.len() + 48);
    rewritten.push_str(&svg[..start]);
    rewritten.push_str(&rewritten_root);
    rewritten.push_str(&svg[end..]);
    Ok((rewritten, size))
}

fn mermaid_svg_intrinsic_size(svg: &str) -> anyhow::Result<MermaidSvgSize> {
    let (start, end) = svg_root_tag_range(svg)?;
    svg_root_size(&svg[start..end])
}

fn svg_root_tag_range(svg: &str) -> anyhow::Result<(usize, usize)> {
    let start = svg
        .find("<svg")
        .ok_or_else(|| anyhow!("Mermaid renderer output did not contain an SVG root"))?;
    let bytes = svg.as_bytes();
    let mut quote = None;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Ok((start, index + 1));
        }
        index += 1;
    }
    Err(anyhow!(
        "Mermaid renderer output had an unterminated SVG root tag"
    ))
}

fn svg_root_size(root_tag: &str) -> anyhow::Result<MermaidSvgSize> {
    if let Some(view_box) = svg_root_attr(root_tag, "viewBox") {
        if let Some(size) = parse_view_box_size(&view_box) {
            return Ok(size);
        }
    }

    let width = svg_root_attr(root_tag, "width")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable width"))?;
    let height = svg_root_attr(root_tag, "height")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable height"))?;
    Ok(MermaidSvgSize { width, height })
}

fn parse_view_box_size(view_box: &str) -> Option<MermaidSvgSize> {
    let values = view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 4 && values[2].is_finite() && values[3].is_finite()).then_some(
        MermaidSvgSize {
            width: values[2].max(1.0),
            height: values[3].max(1.0),
        },
    )
}

fn parse_svg_length(value: &str) -> Option<f32> {
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E'))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()?;
    let parsed = value[..end].parse::<f32>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

fn svg_root_attr(root_tag: &str, attr_name: &str) -> Option<String> {
    svg_root_attrs(root_tag)
        .into_iter()
        .find(|attr| attr.name.eq_ignore_ascii_case(attr_name))
        .and_then(|attr| attr.value)
}

fn rewrite_svg_root_tag(root_tag: &str, size: MermaidSvgSize) -> anyhow::Result<String> {
    let attrs = svg_root_attrs(root_tag)
        .into_iter()
        .filter(|attr| {
            !["width", "height", "style"]
                .iter()
                .any(|name| attr.name.eq_ignore_ascii_case(name))
        })
        .map(|attr| attr.raw)
        .collect::<Vec<_>>();

    let mut rewritten = String::from("<svg");
    for attr in attrs {
        rewritten.push(' ');
        rewritten.push_str(attr.trim());
    }
    rewritten.push_str(&format!(
        " width=\"{:.3}\" height=\"{:.3}\">",
        size.width, size.height
    ));
    Ok(rewritten)
}

#[derive(Debug)]
struct SvgRootAttr {
    name: String,
    value: Option<String>,
    raw: String,
}

fn svg_root_attrs(root_tag: &str) -> Vec<SvgRootAttr> {
    let Some(mut index) = root_tag.find("<svg").map(|index| index + "<svg".len()) else {
        return Vec::new();
    };
    let end = root_tag.rfind('>').unwrap_or(root_tag.len());
    let bytes = root_tag.as_bytes();
    let mut attrs = Vec::new();

    while index < end {
        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= end || bytes[index] == b'/' {
            break;
        }

        let attr_start = index;
        while index < end
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
            && bytes[index] != b'/'
        {
            index += 1;
        }
        let name = root_tag[attr_start..index].to_string();
        if name.is_empty() {
            break;
        }

        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        let mut value = None;
        if index < end && bytes[index] == b'=' {
            index += 1;
            while index < end && bytes[index].is_ascii_whitespace() {
                index += 1;
            }

            if index < end && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < end && bytes[index] != quote {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
                if index < end {
                    index += 1;
                }
            } else {
                let value_start = index;
                while index < end && !bytes[index].is_ascii_whitespace() && bytes[index] != b'/' {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
            }
        }

        let raw = root_tag[attr_start..index].trim().to_string();
        attrs.push(SvgRootAttr { name, value, raw });
    }

    attrs
}

fn mermaid_cache_dir() -> anyhow::Result<PathBuf> {
    let root = ProjectDirs::from("com", "manyougz", "Velotype")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("Velotype"));
    let dir = root.join("mermaid-svg");
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create Mermaid SVG cache '{}'", dir.display()))?;
    Ok(dir)
}

fn looks_like_supported_mermaid_source(source: &str) -> bool {
    let mut in_frontmatter = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter || trimmed.starts_with("%%") {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        return [
            "sequencediagram",
            "classdiagram",
            "statediagram",
            "erdiagram",
            "pie",
            "mindmap",
            "journey",
            "timeline",
            "gantt",
            "requirementdiagram",
            "gitgraph",
            "c4",
            "sankey",
            "quadrantchart",
            "zenuml",
            "block",
            "packet",
            "kanban",
            "architecture",
            "radar",
            "treemap",
            "xychart",
            "flowchart",
            "graph",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mermaid_info_string() {
        assert!(is_mermaid_info_string(Some("mermaid")));
        assert!(is_mermaid_info_string(Some("MMD title")));
        assert!(!is_mermaid_info_string(Some("rust")));
        assert!(!is_mermaid_info_string(None));
    }

    #[test]
    fn parses_backtick_mermaid_fence() {
        let parsed = parse_mermaid_fence_source("```mermaid\nflowchart LR\nA --> B\n```")
            .expect("mermaid fence");
        assert_eq!(parsed.info, "mermaid");
        assert_eq!(parsed.body, "flowchart LR\nA --> B");
    }

    #[test]
    fn parses_tilde_mmd_fence() {
        let parsed = parse_mermaid_fence_source("~~~MMD\nflowchart LR\nA --> B\n~~~")
            .expect("mermaid fence");
        assert_eq!(parsed.info, "MMD");
        assert_eq!(parsed.body, "flowchart LR\nA --> B");
    }

    #[test]
    fn rejects_unclosed_mermaid_fence() {
        assert!(parse_mermaid_fence_source("```mermaid\nflowchart LR").is_none());
    }

    #[test]
    fn cache_key_changes_with_source() {
        assert_ne!(
            mermaid_cache_key("flowchart LR\nA --> B"),
            mermaid_cache_key("flowchart LR\nA --> C")
        );
    }

    #[test]
    fn semantic_line_count_ignores_comments_blank_lines_and_frontmatter() {
        let source = "---\ntitle: Demo\n---\nflowchart LR\n%% comment\n\nA --> B\nB --> C";
        assert_eq!(semantic_mermaid_line_count(source), 3);
    }

    #[test]
    fn display_scale_uses_intrinsic_size_and_caps_growth() {
        let simple = "flowchart LR\nA --> B\nB --> C";
        assert_eq!(
            mermaid_display_scale(simple, 240.0, 120.0, 720.0, 960.0),
            1.0
        );

        let complex = std::iter::once("flowchart LR".to_string())
            .chain((0..20).map(|index| format!("A{index} --> A{}", index + 1)))
            .collect::<Vec<_>>()
            .join("\n");
        let scale = mermaid_display_scale(&complex, 260.0, 140.0, 720.0, 960.0);
        assert!(scale > 1.0);
        assert!(scale <= MERMAID_MAX_SCALE);
        assert!(260.0 * scale <= 960.0 * MERMAID_MAX_VIEWPORT_WIDTH_RATIO);
    }

    #[test]
    fn display_scale_does_not_overgrow_already_wide_diagrams() {
        let complex = std::iter::once("flowchart LR".to_string())
            .chain((0..30).map(|index| format!("A{index} --> A{}", index + 1)))
            .collect::<Vec<_>>()
            .join("\n");
        let scale = mermaid_display_scale(&complex, 1400.0, 400.0, 720.0, 960.0);

        assert_eq!(scale, 1.0);
    }

    #[test]
    fn display_cache_key_changes_with_scale() {
        let source = "flowchart LR\nA --> B";
        assert_ne!(
            mermaid_display_cache_key(source, 1.0),
            mermaid_display_cache_key(source, 2.0)
        );
    }

    #[test]
    fn display_svg_scaling_rewrites_root_dimensions() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50"><rect width="100" height="50"/></svg>"#;
        let (scaled, size) = scale_mermaid_svg_for_display(svg, 2.0).expect("scaled svg");

        assert_eq!(
            size,
            MermaidSvgSize {
                width: 200.0,
                height: 100.0
            }
        );
        assert!(scaled.contains(r#"width="200.000""#));
        assert!(scaled.contains(r#"height="100.000""#));
        assert!(scaled.contains(r#"viewBox="0 0 100 50""#));
    }

    #[test]
    fn display_svg_scaling_removes_responsive_root_attrs() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" style="max-width: 240px; aspect-ratio: 2;" viewBox="0 0 120 60"><text>x</text></svg>"#;
        let (scaled, size) = scale_mermaid_svg_for_display(svg, 1.5).expect("scaled svg");

        assert_eq!(
            size,
            MermaidSvgSize {
                width: 180.0,
                height: 90.0
            }
        );
        let root = &scaled[..scaled.find('>').unwrap()];
        assert!(root.contains(r#"width="180.000""#));
        assert!(root.contains(r#"height="90.000""#));
        assert!(!root.contains("100%"));
        assert!(!root.contains("max-width"));
        assert!(!root.contains("style="));
    }

    #[test]
    fn renders_basic_flowchart_svg() {
        let svg = render_mermaid_to_svg("flowchart LR\nA --> B").expect("svg");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn display_render_uses_scaled_intrinsic_size() {
        let source =
            parse_mermaid_fence_source("```mermaid\nflowchart LR\nA --> B\n```").expect("source");
        let rendered = render_mermaid_svg_for_display(&source, 720.0, 960.0).expect("display svg");

        assert!(rendered.display_width > 1.0);
        assert!(rendered.display_height > 1.0);
        assert!(rendered.display_scale >= 1.0);
        assert!(
            rendered
                .svg
                .contains(&format!("width=\"{:.3}\"", rendered.display_width))
        );
        assert!(
            rendered
                .svg
                .contains(&format!("height=\"{:.3}\"", rendered.display_height))
        );
        assert!(rendered.path.exists());
    }

    #[test]
    fn invalid_mermaid_returns_error() {
        assert!(render_mermaid_to_svg("not a real mermaid diagram ::::").is_err());
    }
}
