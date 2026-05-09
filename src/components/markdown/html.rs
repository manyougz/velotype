//! Native-safe HTML classification for Markdown raw HTML blocks.
//!
//! The parser keeps the original source as the serialization truth and builds
//! a conservative semantic tree only for tags that can be rendered safely in
//! GPUI. Anything risky, unknown, malformed, or ambiguous becomes raw text.

use std::ops::Range;

use cssparser::color::{parse_hash_color, parse_named_color};

#[cfg(feature = "html-native")]
use tree_sitter::Parser;

/// Safety classification for an HTML fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlSafetyClass {
    /// The fragment has at least one safe semantic node.
    Semantic,
    /// The entire fragment must be shown and stored as plain raw text.
    RawTextBlock,
}

/// Broad rendering category of a parsed HTML node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HtmlNodeKind {
    /// Safe inline tag or text that can be represented with text runs.
    InlineSemantic,
    /// Safe block tag that maps to a native block-like GPUI element.
    BlockSemantic,
    /// Opaque raw source that must not be interpreted as HTML.
    RawTextBlock,
}

/// One source attribute from an HTML tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlAttr {
    /// Lowercase attribute name used for safety checks.
    pub(crate) name: String,
    /// Parsed attribute value without surrounding quotes.
    pub(crate) value: Option<String>,
    /// Exact attribute source text.
    pub(crate) raw_source: String,
}

/// Parsed CSS color value from a safe inline `style` attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HtmlCssColor {
    /// The CSS `currentColor` keyword.
    CurrentColor,
    /// An sRGB color with alpha.
    Rgba(HtmlCssRgba),
}

/// RGBA channels normalized enough for both GPUI rendering and export CSS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HtmlCssRgba {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
    pub(crate) alpha: f32,
}

/// Parsed CSS font-size value from a safe inline `style` attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HtmlCssFontSize {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    Keyword(HtmlCssFontSizeKeyword),
}

/// CSS absolute and relative font-size keywords supported by rendered HTML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlCssFontSizeKeyword {
    XxSmall,
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
    XxLarge,
    Smaller,
    Larger,
}

/// Whitelisted visual CSS parsed from a safe HTML `style` attribute.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HtmlInlineStyle {
    pub(crate) color: Option<HtmlCssColor>,
    pub(crate) background_color: Option<HtmlCssColor>,
    pub(crate) font_size: Option<HtmlCssFontSize>,
}

impl Eq for HtmlInlineStyle {}

/// A classified HTML node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlNode {
    /// Rendering category selected by the safety policy.
    pub(crate) kind: HtmlNodeKind,
    /// Lowercase tag name, or `#text` for text nodes.
    pub(crate) tag_name: String,
    /// Safe attributes retained as semantic data.
    pub(crate) attrs: Vec<HtmlAttr>,
    /// Classified child nodes. Empty for raw text nodes.
    pub(crate) children: Vec<HtmlNode>,
    /// Exact source text covered by this node.
    pub(crate) raw_source: String,
    /// Byte range in the original HTML fragment.
    pub(crate) source_range: Range<usize>,
}

/// Classified HTML fragment plus its preserved source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlDocument {
    /// Exact source string used for serialization and raw editing.
    pub(crate) raw_source: String,
    /// Root-level classified nodes.
    pub(crate) nodes: Vec<HtmlNode>,
    /// Overall fragment safety.
    pub(crate) safety: HtmlSafetyClass,
}

impl HtmlDocument {
    pub(crate) fn raw(raw_source: impl Into<String>) -> Self {
        let raw_source = raw_source.into();
        Self {
            nodes: vec![raw_node(&raw_source, 0..raw_source.len())],
            safety: HtmlSafetyClass::RawTextBlock,
            raw_source,
        }
    }

    pub(crate) fn is_semantic(&self) -> bool {
        self.safety == HtmlSafetyClass::Semantic
    }
}

impl HtmlCssColor {
    pub(crate) fn to_css(self) -> String {
        match self {
            Self::CurrentColor => "currentColor".to_string(),
            Self::Rgba(color) => format!(
                "rgba({},{},{},{:.3})",
                color.red,
                color.green,
                color.blue,
                color.alpha.clamp(0.0, 1.0)
            ),
        }
    }
}

impl HtmlCssFontSize {
    pub(crate) fn resolve(self, parent_px: f32, root_px: f32) -> f32 {
        let resolved = match self {
            Self::Px(value) => value,
            Self::Em(value) => parent_px * value,
            Self::Rem(value) => root_px * value,
            Self::Percent(value) => parent_px * value / 100.0,
            Self::Keyword(keyword) => match keyword {
                HtmlCssFontSizeKeyword::XxSmall => root_px * 0.6,
                HtmlCssFontSizeKeyword::XSmall => root_px * 0.75,
                HtmlCssFontSizeKeyword::Small => root_px * 0.875,
                HtmlCssFontSizeKeyword::Medium => root_px,
                HtmlCssFontSizeKeyword::Large => root_px * 1.125,
                HtmlCssFontSizeKeyword::XLarge => root_px * 1.5,
                HtmlCssFontSizeKeyword::XxLarge => root_px * 2.0,
                HtmlCssFontSizeKeyword::Smaller => parent_px * 0.833,
                HtmlCssFontSizeKeyword::Larger => parent_px * 1.2,
            },
        };

        if resolved.is_finite() {
            resolved.clamp(6.0, 96.0)
        } else {
            parent_px
        }
    }

    pub(crate) fn to_css(self) -> String {
        match self {
            Self::Px(value) => format!("{}px", css_number(value)),
            Self::Em(value) => format!("{}em", css_number(value)),
            Self::Rem(value) => format!("{}rem", css_number(value)),
            Self::Percent(value) => format!("{}%", css_number(value)),
            Self::Keyword(keyword) => match keyword {
                HtmlCssFontSizeKeyword::XxSmall => "xx-small",
                HtmlCssFontSizeKeyword::XSmall => "x-small",
                HtmlCssFontSizeKeyword::Small => "small",
                HtmlCssFontSizeKeyword::Medium => "medium",
                HtmlCssFontSizeKeyword::Large => "large",
                HtmlCssFontSizeKeyword::XLarge => "x-large",
                HtmlCssFontSizeKeyword::XxLarge => "xx-large",
                HtmlCssFontSizeKeyword::Smaller => "smaller",
                HtmlCssFontSizeKeyword::Larger => "larger",
            }
            .to_string(),
        }
    }
}

impl HtmlInlineStyle {
    pub(crate) fn is_empty(&self) -> bool {
        self.color.is_none() && self.background_color.is_none() && self.font_size.is_none()
    }

    pub(crate) fn to_css(self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut declarations = Vec::new();
        if let Some(color) = self.color {
            declarations.push(format!("color: {}", color.to_css()));
        }
        if let Some(color) = self.background_color {
            declarations.push(format!("background-color: {}", color.to_css()));
        }
        if let Some(font_size) = self.font_size {
            declarations.push(format!("font-size: {}", font_size.to_css()));
        }
        Some(format!("{};", declarations.join("; ")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TagKind {
    Open,
    Close,
    CommentLike,
}

#[derive(Clone, Debug)]
struct TagToken {
    kind: TagKind,
    name: String,
    attrs: Vec<HtmlAttr>,
    self_closing: bool,
    source_range: Range<usize>,
}

/// Parses and classifies a raw HTML fragment. The returned document always
/// preserves `raw_source` exactly, even when semantic parsing succeeds.
pub(crate) fn parse_html_document(raw_source: &str) -> HtmlDocument {
    if raw_source.trim().is_empty() {
        return HtmlDocument::raw(raw_source);
    }

    if tree_sitter_reports_error(raw_source) {
        return HtmlDocument::raw(raw_source);
    }

    let (nodes, index, ok) = parse_nodes(raw_source, 0, None);
    if !ok || index < raw_source.len() || nodes.is_empty() {
        return HtmlDocument::raw(raw_source);
    }

    if nodes
        .iter()
        .all(|node| matches!(node.kind, HtmlNodeKind::RawTextBlock))
    {
        return HtmlDocument::raw(raw_source);
    }

    HtmlDocument {
        raw_source: raw_source.to_string(),
        nodes,
        safety: HtmlSafetyClass::Semantic,
    }
}

/// Rewrites an HTML fragment for document export: safe semantic nodes keep
/// their HTML shape, while raw text nodes are escaped so browsers cannot
/// execute or interpret them.
pub(crate) fn sanitize_html_for_export(raw_source: &str) -> String {
    let document = parse_html_document(raw_source);
    if !document.is_semantic() {
        return format!(
            "<pre class=\"vlt-raw-html\">{}</pre>",
            escape_html(raw_source)
        );
    }

    document
        .nodes
        .iter()
        .map(sanitize_node_for_export)
        .collect::<String>()
}

/// Parses the safe visual subset of a semantic node's `style` attribute.
pub(crate) fn style_for_node(node: &HtmlNode) -> HtmlInlineStyle {
    if node.kind == HtmlNodeKind::RawTextBlock {
        return HtmlInlineStyle::default();
    }

    let Some(style) = attr_value(node, "style") else {
        return HtmlInlineStyle::default();
    };

    parse_inline_style(style)
}

fn sanitize_node_for_export(node: &HtmlNode) -> String {
    if node.kind == HtmlNodeKind::RawTextBlock {
        return format!(
            "<span class=\"vlt-raw-html\">{}</span>",
            escape_html(&node.raw_source)
        );
    }

    if node.tag_name == "#text" {
        return node.raw_source.clone();
    }

    if is_void_tag(&node.tag_name) {
        return sanitized_open_tag(node);
    }

    let Some(_open_end) = node.raw_source.find('>').map(|index| index + 1) else {
        return escape_html(&node.raw_source);
    };
    let close_start =
        find_closing_tag_start(&node.raw_source, &node.tag_name).unwrap_or(node.raw_source.len());
    let close = &node.raw_source[close_start..];
    let children = node
        .children
        .iter()
        .map(sanitize_node_for_export)
        .collect::<String>();
    format!("{}{children}{close}", sanitized_open_tag(node))
}

fn sanitized_open_tag(node: &HtmlNode) -> String {
    let mut open = format!("<{}", node.tag_name);
    for attr in &node.attrs {
        if attr.name == "style" {
            continue;
        }
        open.push(' ');
        open.push_str(&attr.raw_source);
    }
    if let Some(style) = style_for_node(node).to_css() {
        open.push_str(" style=\"");
        open.push_str(&escape_html_attr(&style));
        open.push('"');
    }
    open.push('>');
    open
}

fn find_closing_tag_start(raw_source: &str, tag_name: &str) -> Option<usize> {
    let needle = format!("</{tag_name}");
    raw_source.to_ascii_lowercase().rfind(&needle)
}

fn escape_html_attr(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
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

fn parse_nodes(
    raw: &str,
    mut index: usize,
    closing_tag: Option<&str>,
) -> (Vec<HtmlNode>, usize, bool) {
    let mut nodes = Vec::new();
    while index < raw.len() {
        let Some(tag_start_relative) = raw[index..].find('<') else {
            if closing_tag.is_some() {
                push_text_node(raw, index..raw.len(), &mut nodes);
            } else {
                push_text_node(raw, index..raw.len(), &mut nodes);
            }
            return (nodes, raw.len(), closing_tag.is_none());
        };

        let tag_start = index + tag_start_relative;
        if tag_start > index {
            push_text_node(raw, index..tag_start, &mut nodes);
        }

        let Some(token) = parse_tag_token(raw, tag_start) else {
            push_text_node(raw, tag_start..tag_start + 1, &mut nodes);
            index = tag_start + 1;
            continue;
        };

        match token.kind {
            TagKind::Close => {
                if closing_tag == Some(token.name.as_str()) {
                    return (nodes, token.source_range.end, true);
                }
                nodes.push(raw_node(raw, token.source_range.clone()));
                index = token.source_range.end;
            }
            TagKind::CommentLike => {
                nodes.push(raw_node(raw, token.source_range.clone()));
                index = token.source_range.end;
            }
            TagKind::Open => {
                let class = classify_open_tag(&token);
                if class == HtmlSafetyClass::RawTextBlock {
                    let raw_end = raw_region_end(raw, &token).unwrap_or(token.source_range.end);
                    nodes.push(raw_node(raw, token.source_range.start..raw_end));
                    index = raw_end;
                    continue;
                }

                if token.self_closing || is_void_tag(&token.name) {
                    nodes.push(semantic_node(raw, token, Vec::new()));
                    index = nodes
                        .last()
                        .map(|node| node.source_range.end)
                        .unwrap_or(index);
                    continue;
                }

                let (children, child_end, closed) =
                    parse_nodes(raw, token.source_range.end, Some(&token.name));
                if !closed {
                    nodes.push(raw_node(raw, token.source_range.start..raw.len()));
                    return (nodes, raw.len(), closing_tag.is_none());
                }

                let mut node = semantic_node(raw, token, children);
                node.source_range.end = child_end;
                node.raw_source = raw[node.source_range.clone()].to_string();
                nodes.push(node);
                index = child_end;
            }
        }
    }

    (nodes, index, closing_tag.is_none())
}

fn parse_tag_token(raw: &str, start: usize) -> Option<TagToken> {
    let rest = raw.get(start..)?;
    if !rest.starts_with('<') {
        return None;
    }

    if rest.starts_with("<!--") {
        let end = rest.find("-->").map(|offset| start + offset + 3)?;
        return Some(TagToken {
            kind: TagKind::CommentLike,
            name: "#comment".into(),
            attrs: Vec::new(),
            self_closing: true,
            source_range: start..end,
        });
    }

    if rest.starts_with("<!") || rest.starts_with("<?") {
        let end = rest.find('>').map(|offset| start + offset + 1)?;
        return Some(TagToken {
            kind: TagKind::CommentLike,
            name: "#raw".into(),
            attrs: Vec::new(),
            self_closing: true,
            source_range: start..end,
        });
    }

    let bytes = raw.as_bytes();
    let mut index = start + 1;
    let closing = bytes.get(index) == Some(&b'/');
    if closing {
        index += 1;
    }

    let name_start = index;
    while index < raw.len() {
        let ch = raw[index..].chars().next()?;
        if ch.is_ascii_alphanumeric() || ch == '-' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    if index == name_start {
        return None;
    }

    let name = raw[name_start..index].to_ascii_lowercase();
    let attrs_start = index;
    let mut quote: Option<char> = None;
    while index < raw.len() {
        let ch = raw[index..].chars().next()?;
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            index += ch.len_utf8();
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch.len_utf8();
            continue;
        }

        if ch == '>' {
            let source_range = start..index + 1;
            let attrs_source = &raw[attrs_start..index];
            let self_closing = attrs_source.trim_end().ends_with('/');
            return Some(TagToken {
                kind: if closing {
                    TagKind::Close
                } else {
                    TagKind::Open
                },
                name,
                attrs: if closing {
                    Vec::new()
                } else {
                    parse_html_attrs(attrs_source)
                },
                self_closing,
                source_range,
            });
        }

        index += ch.len_utf8();
    }

    None
}

pub(crate) fn parse_html_attrs(source: &str) -> Vec<HtmlAttr> {
    let mut attrs = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        while index < source.len()
            && source[index..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '/')
        {
            index += source[index..].chars().next().unwrap().len_utf8();
        }
        if index >= source.len() {
            break;
        }

        let start = index;
        while index < source.len() {
            let ch = source[index..].chars().next().unwrap();
            if ch.is_whitespace() || ch == '=' || ch == '/' {
                break;
            }
            index += ch.len_utf8();
        }
        let name_end = index;
        if name_end == start {
            index += source[index..].chars().next().unwrap().len_utf8();
            continue;
        }

        while index < source.len()
            && source[index..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
        {
            index += source[index..].chars().next().unwrap().len_utf8();
        }

        let mut value = None;
        if source[index..].starts_with('=') {
            index += 1;
            while index < source.len()
                && source[index..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_whitespace())
            {
                index += source[index..].chars().next().unwrap().len_utf8();
            }

            if index < source.len() {
                let ch = source[index..].chars().next().unwrap();
                if ch == '"' || ch == '\'' {
                    index += ch.len_utf8();
                    let value_start = index;
                    while index < source.len() && !source[index..].starts_with(ch) {
                        index += source[index..].chars().next().unwrap().len_utf8();
                    }
                    value = Some(source[value_start..index].to_string());
                    if index < source.len() {
                        index += ch.len_utf8();
                    }
                } else {
                    let value_start = index;
                    while index < source.len() {
                        let ch = source[index..].chars().next().unwrap();
                        if ch.is_whitespace() || ch == '/' {
                            break;
                        }
                        index += ch.len_utf8();
                    }
                    value = Some(source[value_start..index].to_string());
                }
            }
        }

        attrs.push(HtmlAttr {
            name: source[start..name_end].to_ascii_lowercase(),
            value,
            raw_source: source[start..index].to_string(),
        });
    }

    attrs
}

fn classify_open_tag(token: &TagToken) -> HtmlSafetyClass {
    if !is_safe_tag(&token.name) || has_dangerous_attrs(&token.attrs) {
        HtmlSafetyClass::RawTextBlock
    } else {
        HtmlSafetyClass::Semantic
    }
}

fn semantic_node(raw: &str, token: TagToken, children: Vec<HtmlNode>) -> HtmlNode {
    HtmlNode {
        kind: if is_inline_tag(&token.name) {
            HtmlNodeKind::InlineSemantic
        } else {
            HtmlNodeKind::BlockSemantic
        },
        tag_name: token.name,
        attrs: token.attrs,
        children,
        raw_source: raw[token.source_range.clone()].to_string(),
        source_range: token.source_range,
    }
}

fn push_text_node(raw: &str, range: Range<usize>, nodes: &mut Vec<HtmlNode>) {
    if range.is_empty() {
        return;
    }
    nodes.push(HtmlNode {
        kind: HtmlNodeKind::InlineSemantic,
        tag_name: "#text".into(),
        attrs: Vec::new(),
        children: Vec::new(),
        raw_source: raw[range.clone()].to_string(),
        source_range: range,
    });
}

fn raw_node(raw: &str, range: Range<usize>) -> HtmlNode {
    HtmlNode {
        kind: HtmlNodeKind::RawTextBlock,
        tag_name: "#raw".into(),
        attrs: Vec::new(),
        children: Vec::new(),
        raw_source: raw[range.clone()].to_string(),
        source_range: range,
    }
}

fn raw_region_end(raw: &str, token: &TagToken) -> Option<usize> {
    if token.self_closing || is_void_tag(&token.name) {
        return Some(token.source_range.end);
    }

    let close = format!("</{}>", token.name);
    let close_upper = close.to_ascii_uppercase();
    let rest = &raw[token.source_range.end..];
    let lower = rest.to_ascii_lowercase();
    let upper = rest.to_ascii_uppercase();
    lower
        .find(&close)
        .or_else(|| upper.find(&close_upper))
        .map(|offset| token.source_range.end + offset + close.len())
        .or(Some(raw.len()))
}

pub(crate) fn has_dangerous_attrs(attrs: &[HtmlAttr]) -> bool {
    attrs.iter().any(|attr| {
        attr.name.starts_with("on")
            || attr.value.as_deref().is_some_and(|value| {
                let normalized = value
                    .chars()
                    .filter(|ch| !ch.is_whitespace() && *ch != '\0')
                    .collect::<String>()
                    .to_ascii_lowercase();
                matches!(
                    attr.name.as_str(),
                    "href" | "src" | "action" | "formaction" | "xlink:href"
                ) && normalized.starts_with("javascript:")
            })
    })
}

pub(crate) fn attr_value<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|attr| attr.name == name)
        .and_then(|attr| attr.value.as_deref())
}

pub(crate) fn parse_inline_style(style: &str) -> HtmlInlineStyle {
    let mut parsed = HtmlInlineStyle::default();
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim();
        match property.as_str() {
            "color" => {
                if let Some(color) = parse_css_color(value) {
                    parsed.color = Some(color);
                }
            }
            "background-color" => {
                if let Some(color) = parse_css_color(value) {
                    parsed.background_color = Some(color);
                }
            }
            "font-size" => {
                if let Some(size) = parse_css_font_size(value) {
                    parsed.font_size = Some(size);
                }
            }
            _ => {}
        }
    }
    parsed
}

fn parse_css_color(value: &str) -> Option<HtmlCssColor> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentcolor") {
        return Some(HtmlCssColor::CurrentColor);
    }
    if value.eq_ignore_ascii_case("transparent") {
        return Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0.0,
        }));
    }
    if let Some(hex) = value.strip_prefix('#') {
        if let Ok((red, green, blue, alpha)) = parse_hash_color(hex.as_bytes()) {
            return Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red,
                green,
                blue,
                alpha,
            }));
        }
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
    {
        if let Ok((red, green, blue)) = parse_named_color(value) {
            return Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red,
                green,
                blue,
                alpha: 1.0,
            }));
        }
    }
    parse_rgb_color(value).or_else(|| parse_hsl_color(value))
}

fn parse_rgb_color(value: &str) -> Option<HtmlCssColor> {
    let args = css_function_args(value, &["rgb", "rgba"])?;
    let parts = css_function_parts(args);
    if parts.len() < 3 {
        return None;
    }

    let red = parse_rgb_component(&parts[0])?;
    let green = parse_rgb_component(&parts[1])?;
    let blue = parse_rgb_component(&parts[2])?;
    let alpha = parts
        .get(3)
        .and_then(|part| parse_alpha_component(part))
        .unwrap_or(1.0);
    Some(HtmlCssColor::Rgba(HtmlCssRgba {
        red,
        green,
        blue,
        alpha,
    }))
}

fn parse_hsl_color(value: &str) -> Option<HtmlCssColor> {
    let args = css_function_args(value, &["hsl", "hsla"])?;
    let parts = css_function_parts(args);
    if parts.len() < 3 {
        return None;
    }

    let hue = parse_hue(&parts[0])?;
    let saturation = parse_percent_component(&parts[1])?;
    let lightness = parse_percent_component(&parts[2])?;
    let alpha = parts
        .get(3)
        .and_then(|part| parse_alpha_component(part))
        .unwrap_or(1.0);
    let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
    Some(HtmlCssColor::Rgba(HtmlCssRgba {
        red,
        green,
        blue,
        alpha,
    }))
}

fn css_function_args<'a>(value: &'a str, names: &[&str]) -> Option<&'a str> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    if close <= open || !value[close + 1..].trim().is_empty() {
        return None;
    }
    let name = value[..open].trim();
    names
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
        .then_some(&value[open + 1..close])
}

fn css_function_parts(args: &str) -> Vec<String> {
    if args.contains(',') {
        return args
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
    }

    let normalized = args.replace('/', " / ");
    normalized
        .split_whitespace()
        .filter(|token| *token != "/")
        .map(str::to_string)
        .collect()
}

fn parse_rgb_component(value: &str) -> Option<u8> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        let value = parse_css_number(percent)?;
        return Some((value.clamp(0.0, 100.0) * 255.0 / 100.0).round() as u8);
    }

    let value = parse_css_number(value)?;
    Some(value.clamp(0.0, 255.0).round() as u8)
}

fn parse_percent_component(value: &str) -> Option<f32> {
    let value = value.trim().strip_suffix('%')?;
    Some((parse_css_number(value)? / 100.0).clamp(0.0, 1.0))
}

fn parse_alpha_component(value: &str) -> Option<f32> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        return Some((parse_css_number(percent)? / 100.0).clamp(0.0, 1.0));
    }
    Some(parse_css_number(value)?.clamp(0.0, 1.0))
}

fn parse_hue(value: &str) -> Option<f32> {
    let trimmed = value.trim().to_ascii_lowercase();
    if let Some(value) = trimmed.strip_suffix("deg") {
        return Some(parse_css_number(value)?);
    }
    if let Some(value) = trimmed.strip_suffix("turn") {
        return Some(parse_css_number(value)? * 360.0);
    }
    if let Some(value) = trimmed.strip_suffix("rad") {
        return Some(parse_css_number(value)? * 180.0 / std::f32::consts::PI);
    }
    Some(parse_css_number(&trimmed)?)
}

fn hsl_to_rgb(hue_degrees: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
    let hue = hue_degrees.rem_euclid(360.0) / 60.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue.floor() as i32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    (
        ((red + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((green + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((blue + m).clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn parse_css_font_size(value: &str) -> Option<HtmlCssFontSize> {
    let trimmed = value.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "xx-small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XxSmall)),
        "x-small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XSmall)),
        "small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Small)),
        "medium" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Medium)),
        "large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Large)),
        "x-large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XLarge)),
        "xx-large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XxLarge)),
        "smaller" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Smaller)),
        "larger" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Larger)),
        _ => {}
    }

    if let Some(value) = trimmed.strip_suffix("rem") {
        return Some(HtmlCssFontSize::Rem(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix("em") {
        return Some(HtmlCssFontSize::Em(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix("px") {
        return Some(HtmlCssFontSize::Px(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix('%') {
        return Some(HtmlCssFontSize::Percent(parse_non_negative_css_number(
            value,
        )?));
    }
    None
}

fn parse_non_negative_css_number(value: &str) -> Option<f32> {
    let value = parse_css_number(value)?;
    (value >= 0.0).then_some(value)
}

fn parse_css_number(value: &str) -> Option<f32> {
    let value = value.trim().parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

fn css_number(value: f32) -> String {
    let mut formatted = format!("{:.3}", value);
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn is_safe_tag(name: &str) -> bool {
    is_inline_tag(name) || is_block_tag(name)
}

pub(crate) fn is_inline_tag(name: &str) -> bool {
    matches!(
        name,
        "a" | "strong"
            | "em"
            | "b"
            | "i"
            | "u"
            | "mark"
            | "del"
            | "ins"
            | "code"
            | "kbd"
            | "sup"
            | "sub"
            | "small"
            | "abbr"
            | "dfn"
            | "time"
            | "q"
            | "span"
    )
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "div"
            | "p"
            | "blockquote"
            | "hr"
            | "br"
            | "details"
            | "summary"
            | "figure"
            | "figcaption"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "th"
            | "td"
            | "img"
            | "pre"
    )
}

fn is_void_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

#[cfg(feature = "html-native")]
fn tree_sitter_reports_error(raw_source: &str) -> bool {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return true;
    }
    parser
        .parse(raw_source, None)
        .is_none_or(|tree| tree.root_node().has_error())
}

#[cfg(not(feature = "html-native"))]
fn tree_sitter_reports_error(_: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_inline_html_classifies_as_semantic() {
        let doc = parse_html_document("<span style='color:blue;'>Blue</span>");
        assert!(doc.is_semantic());
        assert_eq!(doc.nodes[0].tag_name, "span");
        assert_eq!(doc.raw_source, "<span style='color:blue;'>Blue</span>");
    }

    #[test]
    fn risky_tag_classifies_as_raw_text() {
        let doc = parse_html_document("<script>alert(1)</script>");
        assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
        assert_eq!(doc.nodes[0].raw_source, "<script>alert(1)</script>");
    }

    #[test]
    fn dangerous_attribute_classifies_as_raw_text() {
        let doc = parse_html_document("<a href=\"javascript:alert(1)\">bad</a>");
        assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
    }

    #[test]
    fn risky_child_is_local_raw_inside_safe_parent() {
        let doc = parse_html_document("<div>safe<script>alert(1)</script>tail</div>");
        assert!(doc.is_semantic());
        let div = &doc.nodes[0];
        assert!(
            div.children
                .iter()
                .any(|child| child.kind == HtmlNodeKind::RawTextBlock)
        );
    }

    #[test]
    fn malformed_html_falls_back_to_raw_text() {
        let doc = parse_html_document("<details><summary>x</details>");
        assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
    }

    #[test]
    fn parses_whitelisted_style_color_background_and_font_size() {
        let doc = parse_html_document(
            "<span style=\"color:blue; background-color:#fff8; font-size:20px\">x</span>",
        );
        let style = style_for_node(&doc.nodes[0]);

        assert_eq!(
            style.color,
            Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 1.0,
            }))
        );
        assert_eq!(
            style.background_color,
            Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 0.53333336,
            }))
        );
        assert_eq!(style.font_size, Some(HtmlCssFontSize::Px(20.0)));
    }

    #[test]
    fn parses_rgb_hsl_currentcolor_and_font_size_units() {
        let doc = parse_html_document(
            "<span style=\"color:rgba(255, 0, 0, .5); background-color:hsl(120 100% 50% / 25%); font-size:1.25em\">x</span>",
        );
        let style = style_for_node(&doc.nodes[0]);
        assert_eq!(
            style.color,
            Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 0.5,
            }))
        );
        assert_eq!(
            style.background_color,
            Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 0.25,
            }))
        );
        assert_eq!(style.font_size, Some(HtmlCssFontSize::Em(1.25)));

        let doc = parse_html_document(
            "<span style=\"color:currentColor; font-size:120%; background-color:transparent\">x</span>",
        );
        let style = style_for_node(&doc.nodes[0]);
        assert_eq!(style.color, Some(HtmlCssColor::CurrentColor));
        assert_eq!(style.font_size, Some(HtmlCssFontSize::Percent(120.0)));
        assert_eq!(
            style.background_color,
            Some(HtmlCssColor::Rgba(HtmlCssRgba {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0.0,
            }))
        );

        let doc = parse_html_document("<span style=\"font-size:large\">x</span>");
        assert_eq!(
            style_for_node(&doc.nodes[0]).font_size,
            Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Large))
        );
    }

    #[test]
    fn ignores_unrecognized_or_invalid_style_declarations() {
        let doc = parse_html_document(
            "<span style=\"background-image:url(javascript:bad); color:not-a-real-color; font-size:-1px\">x</span>",
        );
        let style = style_for_node(&doc.nodes[0]);
        assert_eq!(style, HtmlInlineStyle::default());
        assert!(doc.is_semantic());
    }

    #[test]
    fn export_sanitizes_style_to_whitelisted_declarations() {
        let html = sanitize_html_for_export(
            "<span style=\"color:blue; background-image:url(javascript:bad); background-color:rgb(255 255 0); font-size:120%\">x</span>",
        );

        assert!(html.contains(
            "style=\"color: rgba(0,0,255,1.000); background-color: rgba(255,255,0,1.000); font-size: 120%;\""
        ));
        assert!(!html.contains("background-image"));
    }

    #[test]
    fn export_escapes_risky_html_even_when_style_is_present() {
        let html = sanitize_html_for_export("<script style=\"color:blue\">alert(1)</script>");

        assert!(
            html.contains("&lt;script style=&quot;color:blue&quot;&gt;alert(1)&lt;/script&gt;")
        );
        assert!(!html.contains("<script"));
    }
}
