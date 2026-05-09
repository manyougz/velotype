//! Rendering for [`Block`] via GPUI's high-level [`Render`] trait.
//!
//! Each block kind produces a distinct visual style: H1 has a bottom border,
//! list items render a marker column (bullet / ordinal), and raw Markdown
//! fallback renders as plain text.

use gpui::*;

use super::element::{BlockTextElement, CodeLanguageInputElement};
use super::{Block, BlockEvent, BlockKind, ImageResolvedSource, ImageRuntime};
use crate::components::{
    Editor, HtmlCssColor, HtmlDocument, HtmlNode, HtmlNodeKind, TableAxisHighlight, TableAxisKind,
    TableAxisMarker, TableColumnLayout, attr_value, resolve_image_source, style_for_node,
};
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::{Theme, ThemeDimensions, ThemeManager};

// Unicode bullet glyphs for nested list depths.
const BULLET_FILLED: &str = "\u{2022}";
const BULLET_HOLLOW: &str = "\u{25E6}";
const BULLET_SQUARE: &str = "\u{25A1}";
const TASK_CHECKMARK: &str = "\u{2713}";

fn bulleted_list_marker(depth: usize) -> &'static str {
    match depth {
        0 => BULLET_FILLED,
        1 => BULLET_HOLLOW,
        _ => BULLET_SQUARE,
    }
}

fn column_axis_gutter_visible(
    preview_marker: Option<TableAxisMarker>,
    selected_marker: Option<TableAxisMarker>,
) -> bool {
    matches!(
        preview_marker,
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            ..
        })
    ) || matches!(
        selected_marker,
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            ..
        })
    )
}

fn fallback_image_label(alt: &str, strings: &I18nStrings) -> SharedString {
    if alt.trim().is_empty() {
        SharedString::from(strings.image_placeholder.clone())
    } else {
        SharedString::from(alt.to_string())
    }
}

fn render_image_placeholder(
    runtime: &ImageRuntime,
    width: Length,
    height: Pixels,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(d.image_radius))
        .border(px(1.0))
        .border_color(c.image_placeholder_border)
        .bg(c.image_placeholder_bg)
        .px(px(d.block_padding_x))
        .text_center()
        .text_size(px(t.text_size))
        .text_color(c.image_placeholder_text)
        .child(fallback_image_label(&runtime.alt, strings))
        .into_any_element()
}

fn render_loading_placeholder(
    runtime: &ImageRuntime,
    width: Length,
    height: Pixels,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(d.image_radius))
        .border(px(1.0))
        .border_color(c.image_placeholder_border)
        .bg(c.image_placeholder_bg)
        .px(px(d.block_padding_x))
        .text_center()
        .text_size(px(t.code_size))
        .text_color(c.image_placeholder_text)
        .child(if runtime.alt.trim().is_empty() {
            SharedString::from(strings.image_loading_without_alt.clone())
        } else {
            SharedString::from(
                strings
                    .image_loading_with_alt_template
                    .replace("{alt}", &runtime.alt),
            )
        })
        .into_any_element()
}

fn wrap_with_quote_guides(content: AnyElement, quote_depth: usize, theme: &Theme) -> AnyElement {
    if quote_depth == 0 {
        return content;
    }

    let c = &theme.colors;
    let d = &theme.dimensions;
    let guide_offset = d.quote_padding_left;
    let total_padding = guide_offset * quote_depth as f32;

    div()
        .w_full()
        .relative()
        .pl(px(total_padding))
        .child(content)
        .children((0..quote_depth).map(|level| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(guide_offset * level as f32))
                .w(px(d.quote_border_width))
                .bg(c.border_quote)
        }))
        .into_any_element()
}

fn callout_accent_and_background(variant: super::CalloutVariant, theme: &Theme) -> (Hsla, Hsla) {
    let c = &theme.colors;
    match variant {
        super::CalloutVariant::Note => (c.callout_note_border, c.callout_note_bg),
        super::CalloutVariant::Tip => (c.callout_tip_border, c.callout_tip_bg),
        super::CalloutVariant::Important => (c.callout_important_border, c.callout_important_bg),
        super::CalloutVariant::Warning => (c.callout_warning_border, c.callout_warning_bg),
        super::CalloutVariant::Caution => (c.callout_caution_border, c.callout_caution_bg),
    }
}

fn visible_quote_guides(block: &Block) -> usize {
    block.visible_quote_depth
}

fn effective_table_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let centered_width = Editor::centered_column_width(viewport_width, d);
    let visible_quote_guides = visible_quote_guides(block);
    let quote_inset = d.quote_padding_left * visible_quote_guides as f32;
    let callout_inset = if block.callout_depth > 0 {
        d.callout_padding_x * 2.0 + d.callout_border_width
    } else {
        0.0
    };

    (centered_width - quote_inset - callout_inset)
        .max((d.table_cell_padding_x * 2.0 + 80.0).max(120.0))
}

fn container_image_width_budget(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let centered_width = Editor::centered_column_width(viewport_width, d);
    let visible_quote_guides = visible_quote_guides(block);
    let quote_inset = d.quote_padding_left * visible_quote_guides as f32;
    let callout_inset = if block.callout_depth > 0 {
        d.callout_padding_x * 2.0 + d.callout_border_width
    } else {
        0.0
    };

    centered_width - quote_inset - callout_inset
}

fn effective_image_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let list_inset = d.nested_block_indent * block.render_depth as f32;
    (container_image_width_budget(block, viewport_width, d) - d.block_padding_x * 2.0 - list_inset)
        .max(160.0)
}

fn effective_list_item_image_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let marker_width = match block.kind() {
        BlockKind::BulletedListItem => d.list_marker_width,
        BlockKind::TaskListItem { .. } => d.list_marker_width.max(d.task_checkbox_size),
        BlockKind::NumberedListItem => d.ordered_list_marker_width,
        _ => 0.0,
    };
    let list_inset = d.nested_block_indent * block.render_depth as f32;

    (container_image_width_budget(block, viewport_width, d)
        - d.block_padding_x * 2.0
        - list_inset
        - marker_width
        - d.list_marker_gap)
        .max(160.0)
}

/// Returns a human-readable list ordinal: numbers at depth 0, lowercase
/// letters at depth 1, and unicode roman numerals at depth 2+.
fn numbered_list_marker(depth: usize, ordinal: usize) -> String {
    match depth {
        0 => format!("{ordinal}."),
        1 => format!("{}.", alphabetic_list_marker(ordinal)),
        _ => format!("{}.", roman_list_marker(ordinal)),
    }
}

/// Expands beyond 26 by wrapping: a...z, a1...z1, a2...z2, ...
fn alphabetic_list_marker(ordinal: usize) -> String {
    const ALPHABET: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";

    let ordinal = ordinal.max(1);
    if ordinal <= ALPHABET.len() {
        return char::from(ALPHABET[ordinal - 1]).to_string();
    }

    let wrapped = ordinal - (ALPHABET.len() + 1);
    let letter = char::from(ALPHABET[wrapped % ALPHABET.len()]);
    let suffix = wrapped + 1;
    format!("{letter}{suffix}")
}

/// Converts an ASCII roman numeral string to its unicode ligature equivalents
/// where possible (e.g. "III" → "Ⅲ", "VIII" → "Ⅷ").
fn roman_list_marker(ordinal: usize) -> String {
    let ascii = ascii_roman_numeral(ordinal.max(1));
    let mut index = 0;
    let mut marker = String::new();

    while index < ascii.len() {
        let remaining = &ascii[index..];
        if let Some((token_len, token)) = roman_unicode_token(remaining) {
            marker.push_str(token);
            index += token_len;
        } else {
            break;
        }
    }

    marker
}

fn ascii_roman_numeral(mut ordinal: usize) -> String {
    const MAP: &[(usize, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut result = String::new();
    for (value, symbol) in MAP {
        while ordinal >= *value {
            result.push_str(symbol);
            ordinal -= *value;
        }
    }
    result
}

fn roman_unicode_token(remaining: &str) -> Option<(usize, &'static str)> {
    const TOKENS: &[(&str, &str)] = &[
        ("XII", "\u{216B}"),
        ("XI", "\u{216A}"),
        ("IX", "\u{2168}"),
        ("VIII", "\u{2167}"),
        ("VII", "\u{2166}"),
        ("VI", "\u{2165}"),
        ("IV", "\u{2163}"),
        ("III", "\u{2162}"),
        ("II", "\u{2161}"),
        ("I", "\u{2160}"),
        ("V", "\u{2164}"),
        ("X", "\u{2169}"),
        ("L", "\u{216C}"),
        ("C", "\u{216D}"),
        ("D", "\u{216E}"),
        ("M", "\u{216F}"),
    ];

    TOKENS.iter().find_map(|(ascii, unicode)| {
        remaining
            .starts_with(ascii)
            .then_some((ascii.len(), *unicode))
    })
}

fn html_children_text(node: &HtmlNode) -> String {
    if node.children.is_empty() {
        return node.raw_source.clone();
    }

    let mut text = String::new();
    for child in &node.children {
        if child.tag_name == "br" {
            text.push('\n');
        } else {
            text.push_str(&html_children_text(child));
        }
    }
    text
}

#[derive(Clone, Copy, Debug)]
struct HtmlComputedStyle {
    color: Hsla,
    font_size: f32,
    root_font_size: f32,
}

#[derive(Clone, Copy, Debug)]
struct HtmlNodeVisualStyle {
    computed: HtmlComputedStyle,
    background: Option<Hsla>,
}

impl HtmlComputedStyle {
    fn root(theme: &Theme) -> Self {
        Self {
            color: theme.colors.text_default,
            font_size: theme.typography.text_size,
            root_font_size: theme.typography.text_size,
        }
    }
}

fn html_css_color_to_hsla(color: HtmlCssColor, current_color: Hsla) -> Hsla {
    match color {
        HtmlCssColor::CurrentColor => current_color,
        HtmlCssColor::Rgba(color) => Hsla::from(Rgba {
            r: color.red as f32 / 255.0,
            g: color.green as f32 / 255.0,
            b: color.blue as f32 / 255.0,
            a: color.alpha.clamp(0.0, 1.0),
        }),
    }
}

fn html_node_visual_style(
    node: &HtmlNode,
    parent: HtmlComputedStyle,
    theme: &Theme,
) -> HtmlNodeVisualStyle {
    let c = &theme.colors;
    let t = &theme.typography;
    let mut computed = parent;
    let mut background = None;

    match node.tag_name.as_str() {
        "a" => computed.color = c.text_link,
        "blockquote" => computed.color = c.text_quote,
        "code" | "kbd" | "pre" => {
            computed.color = c.code_text;
            computed.font_size = t.code_size;
            background = Some(c.code_bg);
        }
        "mark" => background = Some(c.comment_bg),
        "figcaption" => {
            computed.color = c.image_caption_text;
            computed.font_size = t.code_size;
        }
        "small" | "sup" | "sub" => computed.font_size = (computed.font_size * 0.8).max(6.0),
        "th" => background = Some(c.table_header_bg),
        "td" => background = Some(c.table_cell_bg),
        _ => {}
    }

    let inline_style = style_for_node(node);
    if let Some(color) = inline_style.color {
        computed.color = html_css_color_to_hsla(color, computed.color);
    }
    if let Some(font_size) = inline_style.font_size {
        computed.font_size = font_size.resolve(computed.font_size, computed.root_font_size);
    }
    if let Some(color) = inline_style.background_color {
        background = Some(html_css_color_to_hsla(color, computed.color));
    }

    HtmlNodeVisualStyle {
        computed,
        background,
    }
}

impl Block {
    fn on_html_details_toggle_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.html_details_open = !self.html_details_open;
        cx.stop_propagation();
        cx.notify();
    }

    fn render_image_content(
        &self,
        runtime: &ImageRuntime,
        max_width: Length,
        max_height: Pixels,
        placeholder_height: Pixels,
        theme: &Theme,
        strings: &I18nStrings,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let source = runtime.resolved_source.clone();
        let placeholder_theme = theme.clone();
        let loading_theme = theme.clone();
        let placeholder_strings = strings.clone();
        let loading_strings = strings.clone();
        let runtime_for_fallback = runtime.clone();
        let runtime_for_loading = runtime.clone();

        let image = match source {
            ImageResolvedSource::Local(path) => img(path),
            ImageResolvedSource::Remote(uri) => img(uri),
        }
        .max_w(max_width)
        .max_h(max_height)
        .object_fit(ObjectFit::Contain)
        .with_fallback(move || {
            render_image_placeholder(
                &runtime_for_fallback,
                max_width,
                placeholder_height,
                &placeholder_theme,
                &placeholder_strings,
            )
        })
        .with_loading(move || {
            render_loading_placeholder(
                &runtime_for_loading,
                max_width,
                placeholder_height,
                &loading_theme,
                &loading_strings,
            )
        });

        let mut container = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(d.image_caption_gap))
            .child(image);

        if let Some(title) = runtime
            .title
            .as_ref()
            .filter(|title| !title.trim().is_empty())
        {
            container = container.child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(t.code_size))
                    .text_color(c.image_caption_text)
                    .child(SharedString::from(title.clone())),
            );
        }

        container.into_any_element()
    }

    fn render_html_document(
        &self,
        document: &HtmlDocument,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        if !document.is_semantic() {
            return div()
                .w_full()
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .px(px(d.block_padding_x))
                .py(px(d.block_padding_y))
                .text_size(px(t.code_size))
                .text_color(c.text_default)
                .child(SharedString::from(document.raw_source.clone()))
                .into_any_element();
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(d.block_gap * 0.4))
            .children(
                document.nodes.iter().map(|node| {
                    self.render_html_node(node, theme, HtmlComputedStyle::root(theme), cx)
                }),
            )
            .into_any_element()
    }

    fn render_html_node(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        inherited_style: HtmlComputedStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;

        if node.kind == HtmlNodeKind::RawTextBlock {
            return div()
                .w_full()
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .px(px(d.block_padding_x * 0.6))
                .py(px(d.block_padding_y * 0.6))
                .text_size(px(t.code_size))
                .text_color(c.text_default)
                .child(SharedString::from(node.raw_source.clone()))
                .into_any_element();
        }

        if node.tag_name == "#text" {
            return div()
                .min_w(px(0.0))
                .text_size(px(inherited_style.font_size))
                .text_color(inherited_style.color)
                .child(SharedString::from(node.raw_source.clone()))
                .into_any_element();
        }

        let node_style = html_node_visual_style(node, inherited_style, theme);
        match node.tag_name.as_str() {
            "strong" | "b" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::BOLD, cx)
            }
            "em" | "i" | "span" | "abbr" | "dfn" | "time" | "u" | "ins" | "del" | "small"
            | "sup" | "sub" | "a" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::NORMAL, cx)
            }
            "mark" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::NORMAL, cx)
            }
            "code" | "kbd" => {
                let mut element =
                    div()
                        .flex()
                        .rounded(px(4.0))
                        .px(px(4.0))
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "q" => {
                let mut element = div()
                    .flex()
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children([
                        div().child("\u{201C}").into_any_element(),
                        div()
                            .children(node.children.iter().map(|child| {
                                self.render_html_node(child, theme, node_style.computed, cx)
                            }))
                            .into_any_element(),
                        div().child("\u{201D}").into_any_element(),
                    ]);
                if let Some(bg) = node_style.background {
                    element = element.bg(bg).rounded(px(3.0)).px(px(2.0));
                }
                element.into_any_element()
            }
            "br" => div().child("\n").into_any_element(),
            "hr" => div()
                .w_full()
                .h(px(d.separator_thickness))
                .my(px(d.separator_margin_y))
                .bg(c.separator_color)
                .rounded(px(999.0))
                .into_any_element(),
            "blockquote" => {
                let mut element =
                    div()
                        .w_full()
                        .pl(px(d.quote_padding_left))
                        .border_l(px(d.quote_border_width))
                        .border_color(c.border_quote)
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "pre" => {
                let mut element = div()
                    .w_full()
                    .rounded_sm()
                    .px(px(d.code_block_padding_x))
                    .py(px(d.code_block_padding_y))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .child(SharedString::from(html_children_text(node)));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "img" => self.render_html_image(node, theme, node_style, cx),
            "table" => self.render_html_table(node, theme, node_style, cx),
            "thead" | "tbody" | "tfoot" => {
                let mut element =
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "tr" => self.render_html_table_row(node, theme, node_style, cx),
            "th" | "td" => {
                let mut element =
                    div()
                        .min_w(px(0.0))
                        .flex_grow()
                        .border(px(1.0))
                        .border_color(c.table_border)
                        .px(px(d.table_cell_padding_x))
                        .py(px(d.table_cell_padding_y))
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .font_weight(if node.tag_name == "th" {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "details" => self.render_html_details(node, theme, node_style, cx),
            "summary" => {
                let mut element =
                    div()
                        .w_full()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "figure" => {
                let mut element =
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(d.image_caption_gap))
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "figcaption" => {
                let mut element =
                    div()
                        .w_full()
                        .text_center()
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            _ => {
                let mut element =
                    div()
                        .w_full()
                        .text_size(px(node_style.computed.font_size))
                        .text_color(node_style.computed.color)
                        .children(node.children.iter().map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        }));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
        }
    }

    fn render_html_inline_container(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut element = div()
            .flex()
            .min_w(px(0.0))
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .font_weight(weight)
            .children(
                node.children
                    .iter()
                    .map(|child| self.render_html_node(child, theme, node_style.computed, cx)),
            );
        if let Some(bg) = node_style.background {
            element = element.bg(bg).rounded(px(3.0)).px(px(2.0));
        }
        element.into_any_element()
    }

    fn render_html_image(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(src) = attr_value(node, "src").filter(|src| !src.trim().is_empty()) else {
            let mut element = div()
                .text_size(px(node_style.computed.font_size))
                .text_color(node_style.computed.color)
                .child(SharedString::from(node.raw_source.clone()));
            if let Some(bg) = node_style.background {
                element = element.bg(bg);
            }
            return element.into_any_element();
        };
        let runtime = ImageRuntime {
            alt: attr_value(node, "alt").unwrap_or_default().to_string(),
            src: src.to_string(),
            title: attr_value(node, "title").map(str::to_string),
            resolved_source: resolve_image_source(src, None),
        };
        let strings = cx.global::<I18nManager>().strings().clone();
        let content = self.render_image_content(
            &runtime,
            Length::Definite(relative(1.0)),
            px(theme.dimensions.image_root_max_height),
            px(theme.dimensions.image_root_placeholder_height),
            theme,
            &strings,
        );
        if let Some(bg) = node_style.background {
            div().w_full().bg(bg).child(content).into_any_element()
        } else {
            content
        }
    }

    fn render_html_table(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut element = div()
            .w_full()
            .border(px(1.0))
            .border_color(theme.colors.table_border)
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .children(
                node.children
                    .iter()
                    .map(|child| self.render_html_node(child, theme, node_style.computed, cx)),
            );
        if let Some(bg) = node_style.background {
            element = element.bg(bg);
        }
        element.into_any_element()
    }

    fn render_html_table_row(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut element = div()
            .w_full()
            .flex()
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .children(
                node.children
                    .iter()
                    .map(|child| self.render_html_node(child, theme, node_style.computed, cx)),
            );
        if let Some(bg) = node_style.background {
            element = element.bg(bg);
        }
        element.into_any_element()
    }

    fn render_html_details(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_open = attr_value(node, "open").is_some() || self.html_details_open;
        let summary = node
            .children
            .iter()
            .find(|child| child.tag_name == "summary");
        let body = node
            .children
            .iter()
            .filter(|child| child.tag_name != "summary");

        let mut container = div()
            .w_full()
            .rounded_sm()
            .border(px(1.0))
            .border_color(theme.colors.table_border)
            .px(px(theme.dimensions.block_padding_x))
            .py(px(theme.dimensions.block_padding_y))
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .child(
                div()
                    .w_full()
                    .flex()
                    .gap(px(theme.dimensions.list_marker_gap))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(Self::on_html_details_toggle_mouse_down),
                    )
                    .child(if is_open { "\u{25BE}" } else { "\u{25B8}" })
                    .children(summary.into_iter().map(|summary| {
                        self.render_html_node(summary, theme, node_style.computed, cx)
                    })),
            );
        if let Some(bg) = node_style.background {
            container = container.bg(bg);
        }

        if is_open {
            container =
                container.child(
                    div()
                        .w_full()
                        .pt(px(theme.dimensions.block_padding_y))
                        .children(body.map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        })),
                );
        }

        container.into_any_element()
    }

    fn render_shell(
        &self,
        block_id: ElementId,
        source_mode: bool,
        cursor_style: CursorStyle,
        padding_left: f32,
        padding_right: f32,
        dimensions: &ThemeDimensions,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let base = div()
            .id(block_id)
            .key_context("BlockEditor")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_newline))
            .on_action(cx.listener(Self::on_delete_back))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_focus_prev))
            .on_action(cx.listener(Self::on_focus_next))
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .min_w(px(0.0))
            .flex_shrink_0()
            .min_h(px(dimensions.block_min_height))
            .py(px(dimensions.block_padding_y))
            .pl(px(padding_left))
            .pr(px(padding_right))
            .cursor(cursor_style);

        let base = if source_mode {
            base
        } else {
            base.on_action(cx.listener(Self::on_indent_block))
                .on_action(cx.listener(Self::on_outdent_block))
                .on_action(cx.listener(Self::on_bold_selection))
                .on_action(cx.listener(Self::on_italic_selection))
                .on_action(cx.listener(Self::on_underline_selection))
                .on_action(cx.listener(Self::on_code_selection))
                .on_action(cx.listener(Self::on_exit_code_block))
        };

        base
    }
}

impl Focusable for Block {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The render method builds the full element tree for a block:
/// - Common wrapper: key_context, track_focus, action handlers, mouse events.
/// - Kind-specific styling: headings get size/weight/border, list items get
///   a flex row with marker + content, everything else renders as plain text.
/// - The [`BlockTextElement`] handles text layout, selection, and cursor.
impl Render for Block {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let code_language_focused = self.code_language_focus_handle.is_focused(window);
        let input_active = focused || code_language_focused;
        if self.sync_image_focus_state(focused) {
            cx.notify();
        }

        self.sync_inline_projection_for_focus(focused && !self.showing_rendered_image());

        if input_active && self.cursor_blink_task.is_none() {
            self.start_cursor_blink(cx);
        } else if !input_active && self.cursor_blink_task.is_some() {
            self.cursor_blink_task = None;
        }
        if !input_active {
            self.reset_code_language_input_layout();
        }

        let block_id = ElementId::Name(format!("block-{}", self.record.id).into());
        let is_placeholder =
            focused && self.display_text().is_empty() && self.marked_range.is_none();

        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let depth_padding = d.block_padding_x + d.nested_block_indent * self.render_depth as f32;
        let showing_rendered_image = self.showing_rendered_image();

        if self.is_table_cell() {
            let is_header = self
                .table_cell_position()
                .map(|position| position.is_header())
                .unwrap_or(false);
            let highlight = self.table_axis_highlight;
            let base_bg = if is_header {
                c.table_header_bg
            } else {
                c.table_cell_bg
            };
            let bg = match highlight {
                TableAxisHighlight::None => base_bg,
                TableAxisHighlight::Preview => c.table_axis_preview_bg,
                TableAxisHighlight::Selected => c.table_axis_selected_bg,
            };
            let border_color = if focused {
                c.table_cell_active_outline
            } else {
                match highlight {
                    TableAxisHighlight::None => c.table_border,
                    TableAxisHighlight::Preview => c.table_axis_preview_bg,
                    TableAxisHighlight::Selected => c.table_axis_selected_bg,
                }
            };
            let cell_base = self
                .render_shell(
                    block_id,
                    false,
                    if showing_rendered_image {
                        CursorStyle::PointingHand
                    } else {
                        CursorStyle::IBeam
                    },
                    0.0,
                    0.0,
                    d,
                    cx,
                )
                .w_full()
                .h_full()
                .min_h(px(d.table_cell_min_height))
                .px(px(d.table_cell_padding_x))
                .py(px(d.table_cell_padding_y))
                .rounded(px(2.0))
                .border(px(1.0))
                .border_color(border_color)
                .bg(bg)
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height));

            let cell_base = if is_header {
                cell_base.font_weight(FontWeight::MEDIUM)
            } else {
                cell_base
            };

            if showing_rendered_image && let Some(runtime) = self.image_runtime() {
                return cell_base
                    .child(self.render_image_content(
                        runtime,
                        Length::Definite(relative(1.0)),
                        px(d.image_cell_max_height),
                        px(d.image_cell_placeholder_height),
                        &theme,
                        &strings,
                    ))
                    .into_any_element();
            }

            return cell_base
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element();
        }

        // Source-mode rendering: raw text with no formatting.
        if self.is_source_raw_mode() && (focused || self.kind() != BlockKind::HtmlBlock) {
            if focused && self.cursor_blink_task.is_none() {
                self.start_cursor_blink(cx);
            } else if !focused && self.cursor_blink_task.is_some() {
                self.cursor_blink_task = None;
            }
            let source_base = self
                .render_shell(
                    block_id.clone(),
                    true,
                    CursorStyle::IBeam,
                    d.block_padding_x,
                    d.block_padding_x,
                    d,
                    cx,
                )
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height));

            let source_base = if self.kind() == BlockKind::Comment {
                source_base.bg(c.comment_bg).rounded_sm()
            } else if focused {
                source_base.bg(c.source_mode_block_bg).rounded_sm()
            } else {
                source_base
            };

            return source_base
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element();
        }

        let focused_base = self.render_shell(
            block_id.clone(),
            false,
            if showing_rendered_image {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            },
            if self.kind().is_separator() {
                depth_padding + d.separator_inset_x
            } else {
                depth_padding
            },
            if self.kind().is_separator() {
                d.block_padding_x + d.separator_inset_x
            } else {
                d.block_padding_x
            },
            d,
            cx,
        );

        if showing_rendered_image && self.kind() == BlockKind::Paragraph {
            let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
            let max_width = px(effective_image_width(self, viewport_width, d));
            if let Some(runtime) = self.image_runtime() {
                return focused_base
                    .child(self.render_image_content(
                        runtime,
                        max_width.into(),
                        px(d.image_root_max_height),
                        px(d.image_root_placeholder_height),
                        &theme,
                        &strings,
                    ))
                    .into_any_element();
            }
        }

        let content = match self.kind() {
            BlockKind::Separator => focused_base
                .py(px(d.separator_margin_y))
                .child(
                    div()
                        .w_full()
                        .h(px(d.separator_thickness))
                        .bg(c.separator_color)
                        .rounded(px(999.0)),
                )
                .into_any_element(),
            BlockKind::Heading { level: 1 } => focused_base
                .text_size(px(t.h1_size))
                .font_weight(t.h1_weight.to_font_weight())
                .text_color(c.text_h1)
                .pb(px(d.h1_padding_bottom))
                .mb(px(d.h1_margin_bottom))
                .border_b(px(d.h1_border_width))
                .border_color(c.border_h1)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Heading { level: 2 } => focused_base
                .text_size(px(t.h2_size))
                .font_weight(t.h2_weight.to_font_weight())
                .text_color(c.text_h2)
                .pb(px(d.h1_padding_bottom))
                .mb(px(d.h1_margin_bottom))
                .border_b(px(d.h1_border_width))
                .border_color(c.border_h2)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Heading { level: 3 } => focused_base
                .text_size(px(t.h3_size))
                .font_weight(t.h3_weight.to_font_weight())
                .text_color(c.text_h3)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Heading { level: 4 } => focused_base
                .text_size(px(t.h4_size))
                .font_weight(t.h4_weight.to_font_weight())
                .text_color(c.text_h4)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Heading { level: 5 } => focused_base
                .text_size(px(t.h5_size))
                .font_weight(t.h5_weight.to_font_weight())
                .text_color(c.text_h5)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Heading { level: 6 } => focused_base
                .text_size(px(t.h6_size))
                .font_weight(t.h6_weight.to_font_weight())
                .text_color(c.text_h6)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::BulletedListItem => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height))
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(d.list_marker_gap))
                .children([
                    div()
                        .min_w(px(d.list_marker_width))
                        .child(SharedString::new(bulleted_list_marker(self.render_depth))),
                    if showing_rendered_image {
                        let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
                        let max_width =
                            px(effective_list_item_image_width(self, viewport_width, d));
                        if let Some(runtime) = self.image_runtime() {
                            div().flex_grow().child(self.render_image_content(
                                runtime,
                                max_width.into(),
                                px(d.image_root_max_height),
                                px(d.image_root_placeholder_height),
                                &theme,
                                &strings,
                            ))
                        } else {
                            div()
                                .min_w(px(0.0))
                                .flex_grow()
                                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                        }
                    } else {
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .child(BlockTextElement::new(cx.entity(), is_placeholder))
                    },
                ])
                .into_any_element(),
            BlockKind::TaskListItem { checked } => {
                let marker_width = d.list_marker_width.max(d.task_checkbox_size);
                focused_base
                    .text_size(px(t.text_size))
                    .text_color(c.text_default)
                    .line_height(rems(t.text_line_height))
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(d.list_marker_gap))
                    .children([
                        div()
                            .min_w(px(marker_width))
                            .pt(px(d.block_padding_y.max(1.0)))
                            .child(
                                div()
                                    .size(px(d.task_checkbox_size))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(d.task_checkbox_radius))
                                    .border(px(d.task_checkbox_border_width))
                                    .border_color(c.task_checkbox_border)
                                    .bg(if checked {
                                        c.task_checkbox_checked_bg
                                    } else {
                                        c.task_checkbox_bg
                                    })
                                    .text_size(px(d.task_checkbox_check_size))
                                    .text_color(c.task_checkbox_check)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::on_task_checkbox_mouse_down),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::on_task_checkbox_mouse_up),
                                    )
                                    .child(if checked {
                                        SharedString::new(TASK_CHECKMARK)
                                    } else {
                                        SharedString::new("")
                                    }),
                            ),
                        if showing_rendered_image {
                            let viewport_width =
                                f32::from(window.viewport_size().width.max(px(1.0)));
                            let max_width =
                                px(effective_list_item_image_width(self, viewport_width, d));
                            if let Some(runtime) = self.image_runtime() {
                                div().flex_grow().child(self.render_image_content(
                                    runtime,
                                    max_width.into(),
                                    px(d.image_root_max_height),
                                    px(d.image_root_placeholder_height),
                                    &theme,
                                    &strings,
                                ))
                            } else {
                                div()
                                    .min_w(px(0.0))
                                    .flex_grow()
                                    .child(BlockTextElement::new(cx.entity(), is_placeholder))
                            }
                        } else {
                            div()
                                .min_w(px(0.0))
                                .flex_grow()
                                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                        },
                    ])
                    .into_any_element()
            }
            BlockKind::NumberedListItem => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height))
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(d.list_marker_gap))
                .children([
                    div()
                        .min_w(px(d.ordered_list_marker_width))
                        .child(SharedString::from(numbered_list_marker(
                            self.render_depth,
                            self.list_ordinal.unwrap_or(1),
                        ))),
                    if showing_rendered_image {
                        let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
                        let max_width =
                            px(effective_list_item_image_width(self, viewport_width, d));
                        if let Some(runtime) = self.image_runtime() {
                            div().flex_grow().child(self.render_image_content(
                                runtime,
                                max_width.into(),
                                px(d.image_root_max_height),
                                px(d.image_root_placeholder_height),
                                &theme,
                                &strings,
                            ))
                        } else {
                            div()
                                .min_w(px(0.0))
                                .flex_grow()
                                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                        }
                    } else {
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .child(BlockTextElement::new(cx.entity(), is_placeholder))
                    },
                ])
                .into_any_element(),
            BlockKind::Quote => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_quote)
                .line_height(rems(t.text_line_height))
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Callout(variant) => {
                let (accent, _) = callout_accent_and_background(variant, &theme);
                let title_is_empty = self.record.title.visible_text().is_empty();
                let show_static_default_label = title_is_empty && !focused;
                let header_label = SharedString::from(variant.label());
                let header_text = if show_static_default_label {
                    div()
                        .text_size(px(t.text_size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(header_label.clone())
                        .into_any_element()
                } else {
                    div()
                        .min_w(px(0.0))
                        .flex_grow()
                        .text_size(px(t.text_size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(BlockTextElement::with_placeholder(
                            cx.entity(),
                            is_placeholder,
                            header_label,
                            Some(accent),
                        ))
                        .into_any_element()
                };

                focused_base
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(d.callout_header_gap))
                    .child(
                        div()
                            .text_size(px(t.text_size))
                            .font_weight(FontWeight::BOLD)
                            .text_color(accent)
                            .child(variant.icon()),
                    )
                    .child(header_text)
                    .into_any_element()
            }
            BlockKind::FootnoteDefinition => {
                let ordinal = self.footnote_definition_ordinal();
                let badge = ordinal
                    .map(|ordinal| ordinal.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let badge_text_size = px((t.code_size - 1.0).max(10.0));
                let header = focused_base
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(d.list_marker_gap))
                    .text_size(px(t.code_size))
                    .text_color(c.text_quote)
                    .child(
                        div()
                            .px(px(d.footnote_badge_padding_x))
                            .py(px(d.footnote_badge_padding_y))
                            .rounded(px(999.0))
                            .bg(c.footnote_badge_bg)
                            .text_size(badge_text_size)
                            .text_color(c.footnote_badge_text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(badge)),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .text_color(c.text_quote)
                            .child(BlockTextElement::new(cx.entity(), is_placeholder)),
                    );

                if self.footnote_definition_has_backref() {
                    header
                        .child(
                            div()
                                .text_color(c.footnote_backref)
                                .hover(|this| this.text_color(c.text_link))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(Self::on_footnote_backref_mouse_down),
                                )
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(Self::on_footnote_backref_mouse_up),
                                )
                                .child("\u{21A9}"),
                        )
                        .into_any_element()
                } else {
                    header.into_any_element()
                }
            }
            BlockKind::CodeBlock { .. } => {
                let show_language_input = focused || code_language_focused;
                let language_placeholder =
                    SharedString::from(strings.code_language_placeholder.clone());
                let code_panel = focused_base
                    .bg(c.code_bg)
                    .rounded_sm()
                    .pl(px(d.code_block_padding_x))
                    .pr(px(d.code_block_padding_x))
                    .py(px(d.code_block_padding_y))
                    .text_size(px(t.code_size))
                    .text_color(c.code_text)
                    .line_height(rems(t.text_line_height))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .w_full()
                            .child(BlockTextElement::new(cx.entity(), is_placeholder)),
                    );

                if show_language_input {
                    let input_height = d.code_language_input_height
                        + d.code_language_input_padding_y * 2.0
                        + d.code_language_input_border_width * 2.0;
                    div()
                        .w_full()
                        .relative()
                        .pb(px(input_height + d.code_language_input_gap))
                        .child(code_panel)
                        .child(
                            div()
                                .absolute()
                                .right(px(d.code_language_input_gap))
                                .bottom(px(0.0))
                                .occlude()
                                .key_context("BlockEditor")
                                .track_focus(&self.code_language_focus_handle)
                                .on_action(cx.listener(Self::on_code_language_newline))
                                .on_action(cx.listener(Self::on_code_language_dismiss))
                                .on_action(cx.listener(Self::on_code_language_delete_back))
                                .on_action(cx.listener(Self::on_code_language_delete))
                                .on_action(cx.listener(Self::on_code_language_focus_content))
                                .on_action(cx.listener(Self::on_code_language_focus_next))
                                .on_action(cx.listener(Self::on_code_language_move_left))
                                .on_action(cx.listener(Self::on_code_language_move_right))
                                .on_action(cx.listener(Self::on_code_language_home))
                                .on_action(cx.listener(Self::on_code_language_end))
                                .on_action(cx.listener(Self::on_code_language_select_left))
                                .on_action(cx.listener(Self::on_code_language_select_right))
                                .on_action(cx.listener(Self::on_code_language_select_all))
                                .on_action(cx.listener(Self::on_code_language_copy))
                                .on_action(cx.listener(Self::on_code_language_cut))
                                .on_action(cx.listener(Self::on_code_language_paste))
                                .on_action(cx.listener(Self::on_code_language_indent))
                                .on_action(cx.listener(Self::on_code_language_outdent))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(Self::on_code_language_mouse_down),
                                )
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(Self::on_code_language_mouse_up),
                                )
                                .on_mouse_up_out(
                                    MouseButton::Left,
                                    cx.listener(Self::on_code_language_mouse_up),
                                )
                                .on_mouse_move(cx.listener(Self::on_code_language_mouse_move))
                                .w(px(d.code_language_input_width))
                                .px(px(d.code_language_input_padding_x))
                                .py(px(d.code_language_input_padding_y))
                                .rounded(px(d.code_language_input_radius))
                                .border(px(d.code_language_input_border_width))
                                .border_color(c.code_language_input_border)
                                .bg(c.code_language_input_bg)
                                .text_size(px((t.code_size - 1.0).max(10.0)))
                                .text_color(c.code_language_input_text)
                                .cursor(CursorStyle::IBeam)
                                .child(CodeLanguageInputElement::new(
                                    cx.entity(),
                                    language_placeholder,
                                )),
                        )
                        .into_any_element()
                } else {
                    code_panel.into_any_element()
                }
            }
            BlockKind::Table => {
                let Some(runtime) = self.table_runtime.clone() else {
                    return focused_base
                        .text_size(px(t.text_size))
                        .text_color(c.text_default)
                        .line_height(rems(t.text_line_height))
                        .child(BlockTextElement::new(cx.entity(), is_placeholder))
                        .into_any_element();
                };

                let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
                let table_width = effective_table_width(self, viewport_width, d);
                let column_layout = self
                    .record
                    .table
                    .as_ref()
                    .map(|table| TableColumnLayout::measure(table, table_width, window, &theme))
                    .unwrap_or_else(|| TableColumnLayout::equal(runtime.header.len()));
                let preview_marker = self.table_axis_preview;
                let selected_marker = self.table_axis_selection;
                let body_row_count = runtime.rows.len();
                let append_extent = px(d.table_append_button_extent);
                let append_inset = px(d.table_append_button_inset);
                let activation_band = px(d.table_append_activation_band);
                let top_gutter = if column_axis_gutter_visible(preview_marker, selected_marker) {
                    activation_band
                } else {
                    px(0.0)
                };
                let column_append_top = top_gutter + activation_band;
                let column_control_visible = self.table_append_column_hovered;
                let row_control_visible = self.table_append_row_hovered;
                let right_gutter = if column_control_visible {
                    append_extent + append_inset
                } else {
                    px(0.0)
                };
                let bottom_gutter = if row_control_visible {
                    append_extent + append_inset
                } else {
                    px(0.0)
                };
                let weak_table_block = cx.entity().downgrade();

                let header_cells = runtime.header;
                let column_axis_row = (top_gutter > px(0.0)).then(|| {
                    div().w_full().h(top_gutter).flex().gap(px(0.0)).children(
                        header_cells.iter().enumerate().map(|(column, _cell)| {
                            let hover_block = weak_table_block.clone();
                            let select_block = weak_table_block.clone();
                            let menu_block = weak_table_block.clone();
                            let marker = crate::components::TableAxisMarker {
                                kind: TableAxisKind::Column,
                                index: column,
                            };
                            let band_bg = if selected_marker == Some(marker) {
                                c.table_axis_selected_bg
                            } else if preview_marker == Some(marker) {
                                c.table_axis_preview_bg
                            } else {
                                hsla(0.0, 0.0, 0.0, 0.0)
                            };
                            div()
                                .relative()
                                .flex_none()
                                .flex_basis(relative(column_layout.fraction(column)))
                                .w(relative(column_layout.fraction(column)))
                                .h_full()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!(
                                                "table-column-axis-band-{}-{}",
                                                self.record.id, column
                                            )
                                            .into(),
                                        ))
                                        .w_full()
                                        .h_full()
                                        .rounded(px(6.0))
                                        .bg(band_bg)
                                        .cursor_pointer()
                                        .on_hover(move |hovered, _window, cx| {
                                            let _ = hover_block.update(cx, |_block, cx| {
                                                cx.emit(BlockEvent::RequestTableAxisPreview {
                                                    kind: TableAxisKind::Column,
                                                    index: hovered.then_some(column),
                                                });
                                            });
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            move |_event, _window, cx| {
                                                let _ = select_block.update(cx, |_block, cx| {
                                                    cx.stop_propagation();
                                                    cx.emit(BlockEvent::RequestSelectTableAxis {
                                                        kind: TableAxisKind::Column,
                                                        index: column,
                                                    });
                                                });
                                            },
                                        )
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            move |event, _window, cx| {
                                                let _ = menu_block.update(cx, |_block, cx| {
                                                    cx.stop_propagation();
                                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                                        kind: TableAxisKind::Column,
                                                        index: column,
                                                        position: event.position,
                                                    });
                                                });
                                            },
                                        )
                                        .occlude(),
                                )
                        }),
                    )
                });

                let header_row = div().w_full().flex().gap(px(0.0)).children(
                    header_cells.into_iter().enumerate().map(|(column, cell)| {
                        let hover_block = weak_table_block.clone();
                        let select_block = weak_table_block.clone();
                        let menu_block = weak_table_block.clone();
                        div()
                            .relative()
                            .flex_none()
                            .flex_basis(relative(column_layout.fraction(column)))
                            .w(relative(column_layout.fraction(column)))
                            .h_full()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!(
                                            "table-column-axis-activation-{}-{}",
                                            self.record.id, column
                                        )
                                        .into(),
                                    ))
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .h(activation_band)
                                    .cursor_pointer()
                                    .on_hover(move |hovered, _window, cx| {
                                        let _ = hover_block.update(cx, |_block, cx| {
                                            cx.emit(BlockEvent::RequestTableAxisPreview {
                                                kind: TableAxisKind::Column,
                                                index: hovered.then_some(column),
                                            });
                                        });
                                    })
                                    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                        let _ = select_block.update(cx, |_block, cx| {
                                            cx.stop_propagation();
                                            cx.emit(BlockEvent::RequestSelectTableAxis {
                                                kind: TableAxisKind::Column,
                                                index: column,
                                            });
                                        });
                                    })
                                    .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                        let _ = menu_block.update(cx, |_block, cx| {
                                            cx.stop_propagation();
                                            cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                                kind: TableAxisKind::Column,
                                                index: column,
                                                position: event.position,
                                            });
                                        });
                                    })
                                    .occlude(),
                            )
                            .child(cell)
                    }),
                );

                let body_rows =
                    runtime
                        .rows
                        .into_iter()
                        .enumerate()
                        .map(|(body_row_index, row)| {
                            let hover_block = weak_table_block.clone();
                            let select_block = weak_table_block.clone();
                            let menu_block = weak_table_block.clone();
                            let marker = crate::components::TableAxisMarker {
                                kind: TableAxisKind::Row,
                                index: body_row_index,
                            };
                            let band_bg = if selected_marker == Some(marker) {
                                c.table_axis_selected_bg
                            } else if preview_marker == Some(marker) {
                                c.table_axis_preview_bg
                            } else {
                                hsla(0.0, 0.0, 0.0, 0.0)
                            };
                            div()
                                .relative()
                                .w_full()
                                .flex()
                                .gap(px(0.0))
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!(
                                                "table-row-axis-band-{}-{}",
                                                self.record.id, body_row_index
                                            )
                                            .into(),
                                        ))
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(-activation_band)
                                        .w(activation_band)
                                        .rounded(px(6.0))
                                        .bg(band_bg)
                                        .cursor_pointer()
                                        .on_hover(move |hovered, _window, cx| {
                                            let _ = hover_block.update(cx, |_block, cx| {
                                                cx.emit(BlockEvent::RequestTableAxisPreview {
                                                    kind: TableAxisKind::Row,
                                                    index: hovered.then_some(body_row_index),
                                                });
                                            });
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            move |_event, _window, cx| {
                                                let _ = select_block.update(cx, |_block, cx| {
                                                    cx.stop_propagation();
                                                    cx.emit(BlockEvent::RequestSelectTableAxis {
                                                        kind: TableAxisKind::Row,
                                                        index: body_row_index,
                                                    });
                                                });
                                            },
                                        )
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            move |event, _window, cx| {
                                                let _ = menu_block.update(cx, |_block, cx| {
                                                    cx.stop_propagation();
                                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                                        kind: TableAxisKind::Row,
                                                        index: body_row_index,
                                                        position: event.position,
                                                    });
                                                });
                                            },
                                        )
                                        .occlude(),
                                )
                                .children(row.into_iter().enumerate().map(|(column, cell)| {
                                    div()
                                        .flex_none()
                                        .flex_basis(relative(column_layout.fraction(column)))
                                        .w(relative(column_layout.fraction(column)))
                                        .h_full()
                                        .min_w(px(0.0))
                                        .child(cell)
                                }))
                        });

                {
                    let mut rows = Vec::with_capacity(2 + body_row_count);
                    if let Some(column_axis_row) = column_axis_row {
                        rows.push(column_axis_row.into_any_element());
                    }
                    rows.push(header_row.into_any_element());
                    rows.extend(body_rows.map(|row| row.into_any_element()));

                    let column_edge_band = div()
                        .id(ElementId::Name(
                            format!("table-append-column-edge-{}", self.record.id).into(),
                        ))
                        .absolute()
                        .top(column_append_top)
                        .bottom(bottom_gutter)
                        .right(right_gutter)
                        .w(activation_band)
                        .on_hover(cx.listener(Self::on_table_append_column_edge_hover));

                    let row_edge_band = div()
                        .id(ElementId::Name(
                            format!("table-append-row-edge-{}", self.record.id).into(),
                        ))
                        .absolute()
                        .left_0()
                        .right(right_gutter)
                        .bottom(bottom_gutter)
                        .h(activation_band)
                        .on_hover(cx.listener(Self::on_table_append_row_edge_hover));

                    let column_control = {
                        let base = div()
                            .id(ElementId::Name(
                                format!("table-append-column-zone-{}", self.record.id).into(),
                            ))
                            .absolute()
                            .top(column_append_top)
                            .bottom(bottom_gutter)
                            .right_0()
                            .w(right_gutter)
                            .on_hover(cx.listener(Self::on_table_append_column_zone_hover));

                        if column_control_visible {
                            base.child(
                                div()
                                    .id(ElementId::Name(
                                        format!("table-append-column-button-{}", self.record.id)
                                            .into(),
                                    ))
                                    .absolute()
                                    .top(append_inset)
                                    .bottom_0()
                                    .right_0()
                                    .w(append_extent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(999.0))
                                    .bg(c.table_append_button_bg)
                                    .hover(|this| this.bg(c.table_append_button_hover))
                                    .cursor_pointer()
                                    .text_size(px(t.text_size))
                                    .text_color(c.table_append_button_text)
                                    .occlude()
                                    .on_hover(
                                        cx.listener(Self::on_table_append_column_button_hover),
                                    )
                                    .on_click(cx.listener(Self::on_append_table_column))
                                    .child("+"),
                            )
                        } else {
                            base
                        }
                    };

                    let row_control = {
                        let base = div()
                            .id(ElementId::Name(
                                format!("table-append-row-zone-{}", self.record.id).into(),
                            ))
                            .absolute()
                            .left_0()
                            .right(right_gutter)
                            .bottom_0()
                            .h(bottom_gutter)
                            .on_hover(cx.listener(Self::on_table_append_row_zone_hover));

                        if row_control_visible {
                            base.child(
                                div()
                                    .id(ElementId::Name(
                                        format!("table-append-row-button-{}", self.record.id)
                                            .into(),
                                    ))
                                    .absolute()
                                    .left(append_inset)
                                    .right(append_inset)
                                    .bottom_0()
                                    .h(append_extent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(999.0))
                                    .bg(c.table_append_button_bg)
                                    .hover(|this| this.bg(c.table_append_button_hover))
                                    .cursor_pointer()
                                    .text_size(px(t.text_size))
                                    .text_color(c.table_append_button_text)
                                    .occlude()
                                    .on_hover(cx.listener(Self::on_table_append_row_button_hover))
                                    .on_click(cx.listener(Self::on_append_table_row))
                                    .child("+"),
                            )
                        } else {
                            base
                        }
                    };

                    div()
                        .id(block_id)
                        .w_full()
                        .relative()
                        .flex()
                        .flex_col()
                        .pr(right_gutter)
                        .pb(bottom_gutter)
                        .gap(px(0.0))
                        .children(rows)
                        .child(column_edge_band)
                        .child(row_edge_band)
                        .child(column_control)
                        .child(row_control)
                        .into_any_element()
                }
            }
            BlockKind::HtmlBlock => {
                let html = self.record.html.as_ref().cloned().unwrap_or_else(|| {
                    crate::components::parse_html_document(
                        self.record
                            .raw_fallback
                            .as_deref()
                            .unwrap_or_else(|| self.display_text()),
                    )
                });
                focused_base
                    .text_size(px(t.text_size))
                    .text_color(c.text_default)
                    .line_height(rems(t.text_line_height))
                    .child(self.render_html_document(&html, &theme, cx))
                    .into_any_element()
            }
            BlockKind::Paragraph
            | BlockKind::Comment
            | BlockKind::RawMarkdown
            | BlockKind::Heading { .. } => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height))
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
        };

        wrap_with_quote_guides(content, visible_quote_guides(self), &theme)
    }
}

#[cfg(test)]
mod tests {
    use super::{HtmlComputedStyle, column_axis_gutter_visible, html_node_visual_style};
    use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree, parse_html_document};
    use crate::components::{TableAxisKind, TableAxisMarker};
    use crate::i18n::I18nManager;
    use crate::theme::{Theme, ThemeManager};
    use gpui::{Hsla, Rgba, TestAppContext, px};

    #[test]
    fn top_gutter_only_appears_for_column_axis_state() {
        assert!(!column_axis_gutter_visible(None, None));
        assert!(!column_axis_gutter_visible(
            Some(TableAxisMarker {
                kind: TableAxisKind::Row,
                index: 0,
            }),
            None,
        ));
        assert!(column_axis_gutter_visible(
            Some(TableAxisMarker {
                kind: TableAxisKind::Column,
                index: 0,
            }),
            None,
        ));
        assert!(column_axis_gutter_visible(
            None,
            Some(TableAxisMarker {
                kind: TableAxisKind::Column,
                index: 0,
            }),
        ));
    }

    fn assert_color_near(color: Hsla, red: u8, green: u8, blue: u8, alpha: u8) {
        let color = Rgba::from(color);
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as i16;
        assert!((channel(color.r) - red as i16).abs() <= 1);
        assert!((channel(color.g) - green as i16).abs() <= 1);
        assert!((channel(color.b) - blue as i16).abs() <= 1);
        assert!((channel(color.a) - alpha as i16).abs() <= 1);
    }

    #[test]
    fn html_render_style_inherits_color_and_font_size() {
        let theme = Theme::default_theme();
        let doc = parse_html_document(
            "<div style=\"color:blue; font-size:20px\"><span style=\"font-size:120%\">x</span></div>",
        );
        let root = HtmlComputedStyle::root(&theme);
        let parent = html_node_visual_style(&doc.nodes[0], root, &theme);
        let child = html_node_visual_style(&doc.nodes[0].children[0], parent.computed, &theme);

        assert_color_near(parent.computed.color, 0, 0, 255, 255);
        assert_color_near(child.computed.color, 0, 0, 255, 255);
        assert!((child.computed.font_size - 24.0).abs() < 0.01);
    }

    #[test]
    fn html_render_style_overrides_link_and_mark_defaults() {
        let theme = Theme::default_theme();
        let link_doc = parse_html_document("<a style=\"color:red\">x</a>");
        let link_style =
            html_node_visual_style(&link_doc.nodes[0], HtmlComputedStyle::root(&theme), &theme);
        assert_color_near(link_style.computed.color, 255, 0, 0, 255);

        let mark_doc = parse_html_document("<mark style=\"background-color:#123\">x</mark>");
        let mark_style =
            html_node_visual_style(&mark_doc.nodes[0], HtmlComputedStyle::root(&theme), &theme);
        assert_color_near(mark_style.background.unwrap(), 0x11, 0x22, 0x33, 0xff);
    }

    #[test]
    fn html_render_style_does_not_inherit_background_color() {
        let theme = Theme::default_theme();
        let doc =
            parse_html_document("<div style=\"background-color:#112233\"><span>child</span></div>");
        let root = HtmlComputedStyle::root(&theme);
        let parent = html_node_visual_style(&doc.nodes[0], root, &theme);
        let child = html_node_visual_style(&doc.nodes[0].children[0], parent.computed, &theme);

        assert_color_near(parent.background.unwrap(), 0x11, 0x22, 0x33, 0xff);
        assert!(child.background.is_none());
    }

    #[gpui::test]
    async fn code_language_input_docks_to_right_edge(cx: &mut TestAppContext) {
        cx.update(|cx| {
            I18nManager::init(cx);
            ThemeManager::init(cx);
        });
        let (block, cx) = cx.add_window_view(|_window, cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::CodeBlock {
                        language: Some("rust".into()),
                    },
                    InlineTextTree::plain("fn main() {}\n"),
                ),
            )
        });

        cx.update(|window, cx| {
            block.update(cx, |block, _cx| {
                block.focus_handle.focus(window);
            });
            window.draw(cx).clear();
        });
        cx.run_until_parked();

        let (text_bounds, language_bounds) = block.read_with(cx, |block, _cx| {
            (
                block.last_bounds.expect("code text should render"),
                block
                    .code_language_last_bounds
                    .expect("language input should render"),
            )
        });
        assert!(language_bounds.left() > text_bounds.left());
        assert!(language_bounds.top() > text_bounds.bottom());
        let right_gap = f32::from(language_bounds.right() - text_bounds.right());
        assert!(
            right_gap.abs() <= 12.0,
            "expected language input to sit near the code block right edge; right_gap={right_gap}, text_bounds={text_bounds:?}, language_bounds={language_bounds:?}"
        );
        assert!(language_bounds.size.width <= px(156.0));
    }
}
