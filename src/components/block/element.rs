use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::*;

use super::{Block, InlineFootnoteHit, InlineLinkHit, code_highlight_color};
use crate::components::HtmlCssColor;
use crate::theme::{ThemeColors, ThemeManager};

fn build_text_runs(
    input: &Block,
    display_text: &SharedString,
    base_run: &TextRun,
    underline_thickness: Pixels,
    link_color: Hsla,
    code_bg: Hsla,
    show_inline_code_backgrounds: bool,
) -> Vec<TextRun> {
    let mut boundaries = vec![0, display_text.len()];
    for span in input.inline_spans() {
        boundaries.push(span.range.start);
        boundaries.push(span.range.end);
    }
    if let Some(marked_range) = input.marked_range.as_ref() {
        boundaries.push(marked_range.start);
        boundaries.push(marked_range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let marked_range = input.marked_range.clone();
    let mut runs = Vec::new();
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start >= end {
            continue;
        }

        let inline_style = input.inline_style_at(start);
        let html_style = input.inline_html_style_at(start);
        let is_link = input.inline_link_at(start).is_some();
        let is_footnote = input.inline_footnote_hit_at(start).is_some();
        let is_marked = marked_range
            .as_ref()
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);

        let mut font = base_run.font.clone();
        if inline_style.bold && font.weight < FontWeight::BOLD {
            font.weight = FontWeight::BOLD;
        }
        if inline_style.italic {
            font.style = FontStyle::Italic;
        }

        let mut run_color = if is_link || is_footnote {
            link_color
        } else {
            base_run.color
        };
        if let Some(style) = html_style
            && let Some(color) = style.color
        {
            run_color = html_css_color_to_hsla(color, run_color);
        }
        let underline = (inline_style.underline || is_marked || is_link || is_footnote).then_some(
            UnderlineStyle {
                color: Some(run_color),
                thickness: underline_thickness,
                wavy: false,
            },
        );
        let strikethrough = inline_style.strikethrough.then_some(StrikethroughStyle {
            color: Some(run_color),
            thickness: underline_thickness,
        });

        let mut background_color = if show_inline_code_backgrounds && inline_style.code {
            Some(code_bg)
        } else {
            base_run.background_color
        };
        if let Some(style) = html_style
            && let Some(color) = style.background_color
        {
            background_color = Some(html_css_color_to_hsla(color, run_color));
        }

        runs.push(TextRun {
            len: end - start,
            font,
            color: run_color,
            background_color,
            underline,
            strikethrough,
        });
    }

    if runs.is_empty() {
        vec![base_run.clone()]
    } else {
        runs
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

fn build_code_text_runs(
    input: &Block,
    display_text: &SharedString,
    base_run: &TextRun,
    underline_thickness: Pixels,
    colors: &ThemeColors,
) -> Vec<TextRun> {
    let mut boundaries = vec![0, display_text.len()];
    if let Some(highlight_result) = input.code_highlight_result() {
        for span in &highlight_result.spans {
            boundaries.push(span.range.start);
            boundaries.push(span.range.end);
        }
    }
    if let Some(marked_range) = input.marked_range.as_ref() {
        boundaries.push(marked_range.start);
        boundaries.push(marked_range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let marked_range = input.marked_range.clone();
    let mut runs = Vec::new();
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start >= end {
            continue;
        }

        let is_marked = marked_range
            .as_ref()
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);
        let run_color = input
            .code_highlight_result()
            .and_then(|result| result.class_at(start))
            .map(|class| code_highlight_color(colors, class))
            .unwrap_or(base_run.color);

        runs.push(TextRun {
            len: end - start,
            font: base_run.font.clone(),
            color: run_color,
            background_color: base_run.background_color,
            underline: is_marked.then_some(UnderlineStyle {
                color: Some(run_color),
                thickness: underline_thickness,
                wavy: false,
            }),
            strikethrough: None,
        });
    }

    if runs.is_empty() {
        vec![base_run.clone()]
    } else {
        runs
    }
}

/// Compute byte ranges of each hard-line (`\n`-separated) segment in the
/// visible text.  Index `i` in the returned Vec corresponds to the `i`-th
/// WrappedLine produced by `shape_text`.
pub(super) fn hard_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (idx, _) in text.match_indices('\n') {
        ranges.push(start..idx);
        start = idx + 1;
    }
    ranges.push(start..text.len());
    ranges
}

/// Map a flat visible-text offset to `(line_index, offset_within_line)`.
pub(super) fn line_index_for_offset(ranges: &[Range<usize>], offset: usize) -> (usize, usize) {
    let clamped = offset.min(ranges.last().map(|r| r.end).unwrap_or(0));
    for (i, range) in ranges.iter().enumerate() {
        if clamped <= range.end {
            return (i, clamped.saturating_sub(range.start));
        }
    }
    let last = ranges.len() - 1;
    (last, ranges[last].len())
}

pub(crate) fn aligned_line_left(
    line: &WrappedLine,
    bounds: Bounds<Pixels>,
    align: TextAlign,
) -> Pixels {
    let slack = (bounds.size.width - line.width()).max(px(0.0));
    match align {
        TextAlign::Left => bounds.left(),
        TextAlign::Center => bounds.left() + slack / 2.0,
        TextAlign::Right => bounds.left() + slack,
    }
}

pub(super) fn wrapped_line_height(line: &WrappedLine, line_height: Pixels) -> Pixels {
    line.size(line_height).height
}

pub(super) fn wrapped_line_top(
    lines: &[WrappedLine],
    line_height: Pixels,
    line_idx: usize,
) -> Pixels {
    lines.iter().take(line_idx).fold(px(0.0), |height, line| {
        height + wrapped_line_height(line, line_height)
    })
}

pub(super) fn wrapped_line_for_y(
    lines: &[WrappedLine],
    line_height: Pixels,
    relative_y: Pixels,
) -> Option<(usize, Pixels)> {
    if lines.is_empty() {
        return None;
    }

    let mut top = px(0.0);
    for (line_idx, line) in lines.iter().enumerate() {
        let height = wrapped_line_height(line, line_height);
        if relative_y < top + height || line_idx + 1 == lines.len() {
            return Some((line_idx, (relative_y - top).max(px(0.0))));
        }
        top += height;
    }

    Some((lines.len() - 1, px(0.0)))
}

fn wrap_boundary_offset(line: &WrappedLine, wrap_idx: usize) -> Option<usize> {
    let boundary = line.wrap_boundaries().get(wrap_idx)?;
    let run = line.unwrapped_layout.runs.get(boundary.run_ix)?;
    let glyph = run.glyphs.get(boundary.glyph_ix)?;
    Some(glyph.index)
}

fn wrapped_row_offsets(line: &WrappedLine) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(line.wrap_boundaries().len() + 2);
    offsets.push(0);
    for wrap_idx in 0..line.wrap_boundaries().len() {
        if let Some(offset) = wrap_boundary_offset(line, wrap_idx) {
            offsets.push(offset.min(line.len()));
        }
    }
    offsets.push(line.len());
    offsets.dedup();
    offsets
}

fn wrapped_row_origin_x(
    line: &WrappedLine,
    bounds: Bounds<Pixels>,
    align: TextAlign,
    row_start: usize,
    row_end: usize,
) -> Pixels {
    let row_width =
        line.unwrapped_layout.x_for_index(row_end) - line.unwrapped_layout.x_for_index(row_start);
    let align_width = line.width();
    let slack = (align_width - row_width).max(px(0.0));
    let line_left = aligned_line_left(line, bounds, align);
    match align {
        TextAlign::Left => line_left,
        TextAlign::Center => line_left + slack / 2.0,
        TextAlign::Right => line_left + slack,
    }
}

pub(super) fn position_for_offset(
    line: &WrappedLine,
    offset: usize,
    line_height: Pixels,
    prefer_next_wrap_start: bool,
) -> Option<Point<Pixels>> {
    let offsets = wrapped_row_offsets(line);
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let is_start_of_wrapped_row = prefer_next_wrap_start && row_idx > 0 && offset == row_start;
        if is_start_of_wrapped_row || (offset >= row_start && offset < row_end) {
            let row_start_x = line.unwrapped_layout.x_for_index(row_start);
            let x = line.unwrapped_layout.x_for_index(offset) - row_start_x;
            return Some(point(x, line_height * row_idx as f32));
        }
    }

    line.position_for_index(offset, line_height)
}

pub(super) fn cursor_bounds_for_offset(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    offset: usize,
    align: TextAlign,
    cursor_width: Pixels,
) -> Option<Bounds<Pixels>> {
    let ranges = hard_line_ranges(text);
    let (line_idx, offset_in_line) = line_index_for_offset(&ranges, offset);
    let layout = lines.get(line_idx)?;
    let origin_x = aligned_line_left(layout, bounds, align);
    let cursor_pos = position_for_offset(layout, offset_in_line, line_height, true)?;
    let y_offset = bounds.top() + wrapped_line_top(lines, line_height, line_idx);
    Some(Bounds::new(
        point(origin_x + cursor_pos.x, y_offset + cursor_pos.y),
        size(cursor_width, line_height),
    ))
}

pub(super) fn range_bounds(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
    align: TextAlign,
) -> Option<Bounds<Pixels>> {
    let segments = range_segment_bounds(lines, bounds, line_height, text, range.clone(), align);
    if segments.is_empty() {
        return cursor_bounds_for_offset(
            lines,
            bounds,
            line_height,
            text,
            range.start,
            align,
            px(1.0),
        );
    }

    let mut union = segments[0];
    for segment in segments.iter().skip(1) {
        union = Bounds::from_corners(
            point(
                union.left().min(segment.left()),
                union.top().min(segment.top()),
            ),
            point(
                union.right().max(segment.right()),
                union.bottom().max(segment.bottom()),
            ),
        );
    }
    Some(union)
}

fn range_segment_bounds_for_hard_line(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    line_idx: usize,
    start_offset: usize,
    end_offset: usize,
    align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    let line_top = bounds.top() + wrapped_line_top(lines, line_height, line_idx);
    let offsets = wrapped_row_offsets(line);
    let mut segments = Vec::new();

    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let seg_start = start_offset.max(row_start).min(row_end);
        let seg_end = end_offset.min(row_end).max(row_start);
        if seg_start >= seg_end {
            continue;
        }

        let row_start_x = line.unwrapped_layout.x_for_index(row_start);
        let start_x = line.unwrapped_layout.x_for_index(seg_start) - row_start_x;
        let end_x = line.unwrapped_layout.x_for_index(seg_end) - row_start_x;
        let origin_x = wrapped_row_origin_x(line, bounds, align, row_start, row_end);
        let y = line_top + line_height * row_idx as f32;
        segments.push(Bounds::from_corners(
            point(origin_x + start_x, y),
            point(origin_x + end_x, y + line_height),
        ));
    }

    segments
}

pub(super) fn range_segment_bounds(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
    align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }

    let ranges = hard_line_ranges(text);
    let (start_line, start_offset) = line_index_for_offset(&ranges, range.start);
    let (end_line, end_offset) = line_index_for_offset(&ranges, range.end);
    let mut segments = Vec::new();

    for line_idx in start_line..=end_line {
        let hard_range = &ranges[line_idx];
        let line_start = if line_idx == start_line {
            start_offset
        } else {
            0
        };
        let line_end = if line_idx == end_line {
            end_offset
        } else {
            hard_range.len()
        };
        segments.extend(range_segment_bounds_for_hard_line(
            lines,
            bounds,
            line_height,
            line_idx,
            line_start,
            line_end,
            align,
        ));
    }

    segments
}

fn point_inside_bounds(bounds: Bounds<Pixels>, position: Point<Pixels>) -> bool {
    position.x >= bounds.left()
        && position.x < bounds.right()
        && position.y >= bounds.top()
        && position.y < bounds.bottom()
}

pub(crate) fn link_at_position<'a>(
    input: &'a Block,
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<&'a InlineLinkHit> {
    if input.is_source_raw_mode()
        || input.display_text().is_empty()
        || lines.is_empty()
        || position.y < bounds.top()
        || position.y >= bounds.bottom()
    {
        return None;
    }

    let text = input.display_text();
    let align = input.text_align();

    for span in input.inline_spans() {
        let Some(link) = span.link.as_ref() else {
            continue;
        };
        if span.range.is_empty() {
            continue;
        }

        for link_bounds in
            range_segment_bounds(lines, bounds, line_height, text, span.range.clone(), align)
        {
            if point_inside_bounds(link_bounds, position) {
                return Some(link);
            }
        }
    }

    None
}

pub(crate) fn footnote_at_position<'a>(
    input: &'a Block,
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<&'a InlineFootnoteHit> {
    if input.is_source_raw_mode()
        || input.display_text().is_empty()
        || lines.is_empty()
        || position.y < bounds.top()
        || position.y >= bounds.bottom()
    {
        return None;
    }

    let text = input.display_text();
    let align = input.text_align();

    for span in input.inline_spans() {
        let Some(footnote) = span.footnote.as_ref() else {
            continue;
        };
        if span.range.is_empty() {
            continue;
        }

        for footnote_bounds in
            range_segment_bounds(lines, bounds, line_height, text, span.range.clone(), align)
        {
            if point_inside_bounds(footnote_bounds, position) {
                return Some(footnote);
            }
        }
    }

    None
}

/// Custom low-level [`Element`] that renders a block's inline-formatted
/// text with selection highlights and a blinking cursor.
///
/// Supports multi-line text (used by code blocks) via hard `\n` breaks.
/// Each `\n` produces a separate `WrappedLine` from the text shaper.
pub struct BlockTextElement {
    input: Entity<Block>,
    is_placeholder: bool,
    placeholder_text: Option<SharedString>,
    placeholder_color: Option<Hsla>,
}

/// Single-line text element used to edit a fenced code block's info string.
pub struct CodeLanguageInputElement {
    input: Entity<Block>,
    placeholder: SharedString,
}

impl CodeLanguageInputElement {
    pub fn new(input: Entity<Block>, placeholder: SharedString) -> Self {
        Self { input, placeholder }
    }
}

pub struct CodeLanguageInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    hitbox: Option<Hitbox>,
}

impl IntoElement for CodeLanguageInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CodeLanguageInputElement {
    type RequestLayoutState = ();
    type PrepaintState = CodeLanguageInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let theme = cx.global::<ThemeManager>().current().clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(theme.dimensions.code_language_input_height)
            .max(window.line_height())
            .into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = cx.global::<ThemeManager>().current().clone();
        let input = self.input.read(cx);
        let content = input.code_language_text().to_string();
        let is_placeholder = content.is_empty();
        let display_text: SharedString = if is_placeholder {
            self.placeholder.clone()
        } else {
            content.into()
        };
        let focused = input.code_language_focus_handle.is_focused(window);
        let style = window.text_style();
        let run_color = if is_placeholder {
            theme.colors.code_language_input_placeholder
        } else {
            theme.colors.code_language_input_text
        };
        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: run_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = input
            .code_language_marked_range
            .as_ref()
            .filter(|_| !is_placeholder)
        {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..base_run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run_color),
                        thickness: px(theme.dimensions.underline_thickness),
                        wavy: false,
                    }),
                    ..base_run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..base_run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![base_run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let line_height = bounds.size.height;
        let selection = if focused && !input.code_language_selected_range.is_empty() {
            let start = line.x_for_index(input.code_language_selected_range.start);
            let end = line.x_for_index(input.code_language_selected_range.end);
            Some(fill(
                Bounds::from_corners(
                    point(bounds.left() + start, bounds.top()),
                    point(bounds.left() + end, bounds.bottom()),
                ),
                theme.colors.selection,
            ))
        } else {
            None
        };
        let cursor = if focused && input.code_language_selected_range.is_empty() {
            let cursor_x = line.x_for_index(input.code_language_cursor_offset());
            let mut cursor_color = theme.colors.cursor;
            cursor_color.a *= input.cursor_opacity();
            Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_x, bounds.top()),
                    size(px(theme.dimensions.cursor_width), line_height),
                ),
                cursor_color,
            ))
        } else {
            None
        };
        let hitbox = Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));

        CodeLanguageInputPrepaintState {
            line: Some(line),
            cursor,
            selection,
            hitbox,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(hitbox) = prepaint.hitbox.as_ref()
            && hitbox.is_hovered(window)
        {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }

        let focus_handle = self.input.read(cx).code_language_focus_handle.clone();
        if focus_handle.is_focused(window) {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().expect("line should be shaped");
        line.paint(bounds.origin, bounds.size.height, window, cx)
            .ok();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.code_language_last_layout = Some(line);
            input.code_language_last_bounds = Some(bounds);
        });
    }
}

impl BlockTextElement {
    pub fn new(input: Entity<Block>, is_placeholder: bool) -> Self {
        Self {
            input,
            is_placeholder,
            placeholder_text: None,
            placeholder_color: None,
        }
    }

    pub fn with_placeholder(
        input: Entity<Block>,
        is_placeholder: bool,
        placeholder_text: SharedString,
        placeholder_color: Option<Hsla>,
    ) -> Self {
        Self {
            input,
            is_placeholder,
            placeholder_text: Some(placeholder_text),
            placeholder_color,
        }
    }
}

/// Prepared text layout and paint geometry for one `BlockTextElement` frame.
pub struct PrepaintState {
    lines: Vec<WrappedLine>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
    code_backgrounds: Vec<PaintQuad>,
    line_height: Pixels,
    hitbox: Hitbox,
}

impl IntoElement for BlockTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BlockTextElement {
    type RequestLayoutState = Rc<RefCell<Option<Vec<WrappedLine>>>>;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let theme = cx.global::<ThemeManager>().current().clone();
        let input = self.input.read(cx);
        let content = input.display_text().to_string();
        let is_placeholder = self.is_placeholder;
        let show_inline_code_backgrounds = !input.is_source_raw_mode();
        let style = window.text_style();

        let (display_text, text_color): (SharedString, Hsla) = if is_placeholder {
            (
                self.placeholder_text
                    .clone()
                    .unwrap_or_else(|| theme.placeholders.empty_editing.clone().into()),
                self.placeholder_color
                    .unwrap_or(theme.colors.text_placeholder),
            )
        } else {
            (content.into(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs: Vec<TextRun> = if !is_placeholder {
            if input.kind().is_code_block() {
                build_code_text_runs(
                    &input,
                    &display_text,
                    &run,
                    px(theme.dimensions.underline_thickness),
                    &theme.colors,
                )
            } else {
                build_text_runs(
                    &input,
                    &display_text,
                    &run,
                    px(theme.dimensions.underline_thickness),
                    theme.colors.text_link,
                    theme.colors.code_bg,
                    show_inline_code_backgrounds,
                )
            }
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let shared_lines = Rc::new(RefCell::new(None));
        let shared_lines_clone = shared_lines.clone();

        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        layout_style.min_size.width = px(0.0).into();
        layout_style.max_size.width = relative(1.).into();

        let layout_id = window.request_measured_layout(
            layout_style,
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = known_dimensions.width.or(match available_space.width {
                    AvailableSpace::Definite(x) => Some(x),
                    AvailableSpace::MinContent => Some(px(1.0)),
                    AvailableSpace::MaxContent => Some(window.viewport_size().width.max(px(1.0))),
                });

                match window.text_system().shape_text(
                    display_text.clone(),
                    font_size,
                    &runs,
                    wrap_width,
                    None,
                ) {
                    Ok(lines) => {
                        let mut total_size: Size<Pixels> = Size::default();
                        for line in &lines {
                            let ls = line.size(line_height);
                            total_size.height += ls.height;
                            total_size.width = total_size.width.max(ls.width);
                        }
                        *shared_lines_clone.borrow_mut() = Some(lines.into_vec());
                        total_size
                    }
                    Err(_) => Size::default(),
                }
            },
        );

        (layout_id, shared_lines)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = cx.global::<ThemeManager>().current().clone();
        let input = self.input.read(cx);
        let editor_selection_range = input
            .editor_selection_range
            .as_ref()
            .filter(|range| !range.is_empty())
            .cloned();
        let selected_range = editor_selection_range
            .clone()
            .unwrap_or_else(|| input.selected_range.clone());
        let cursor = input.cursor_offset();
        let line_height = window.line_height();
        let focused = input.focus_handle.is_focused(window);
        let show_inline_code_backgrounds = !input.is_source_raw_mode();

        let lines = request_layout.borrow_mut().take().unwrap_or_default();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

        let cursor_opacity = input.cursor_opacity();
        let cursor_color = {
            let mut c = theme.colors.cursor;
            c.a *= cursor_opacity;
            c
        };
        let cursor_width = theme.dimensions.cursor_width;
        let selection_color = theme.colors.selection;
        let text_align = input.text_align();

        let (selection_quads, cursor_quad) =
            if (focused || editor_selection_range.is_some()) && !lines.is_empty() {
                if self.is_placeholder {
                    // Placeholder: cursor after the placeholder text
                    let layout = &lines[0];
                    let origin_x = aligned_line_left(layout, bounds, text_align);
                    let cursor_pos = layout
                        .position_for_index(0, line_height)
                        .unwrap_or_default();
                    (
                        vec![],
                        Some(fill(
                            Bounds::new(
                                point(origin_x + cursor_pos.x, bounds.top() + cursor_pos.y),
                                size(px(cursor_width), line_height),
                            ),
                            cursor_color,
                        )),
                    )
                } else if selected_range.is_empty() {
                    // No selection: just draw the cursor
                    let text = input.display_text();
                    (
                        vec![],
                        cursor_bounds_for_offset(
                            &lines,
                            bounds,
                            line_height,
                            text,
                            cursor,
                            text_align,
                            px(cursor_width),
                        )
                        .map(|bounds| fill(bounds, cursor_color)),
                    )
                } else {
                    let text = input.display_text();
                    let quads = range_segment_bounds(
                        &lines,
                        bounds,
                        line_height,
                        text,
                        selected_range,
                        text_align,
                    )
                    .into_iter()
                    .map(|bounds| fill(bounds, selection_color))
                    .collect();
                    (quads, None)
                }
            } else {
                (vec![], None)
            };

        // Compute code-span background quads with rounded corners and padding.
        let mut code_quads = Vec::new();
        if show_inline_code_backgrounds && !self.is_placeholder {
            let text = input.display_text();
            let code_color = theme.colors.code_bg;
            let pad_x = px(theme.dimensions.code_bg_pad_x);
            let pad_y = px(theme.dimensions.code_bg_pad_y);
            let radius = px(theme.dimensions.code_bg_radius);
            for span in input.inline_spans() {
                if !span.style.code || span.range.is_empty() {
                    continue;
                }
                for segment in range_segment_bounds(
                    &lines,
                    bounds,
                    line_height,
                    text,
                    span.range.clone(),
                    text_align,
                ) {
                    let quad_bounds = Bounds::from_corners(
                        point(segment.left() - pad_x, segment.top() - pad_y),
                        point(segment.right() + pad_x, segment.bottom() + pad_y),
                    );
                    code_quads.push({
                        let mut q = fill(quad_bounds, code_color);
                        q.corner_radii = Corners::all(radius);
                        q
                    });
                }
            }
        }

        PrepaintState {
            lines,
            cursor: cursor_quad,
            selection: selection_quads,
            code_backgrounds: code_quads,
            line_height,
            hitbox,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, hovered_link) = {
            let input = self.input.read(cx);
            let hovered_link = !self.is_placeholder
                && !input.is_source_raw_mode()
                && prepaint.hitbox.is_hovered(window)
                && link_at_position(
                    &input,
                    &prepaint.lines,
                    bounds,
                    prepaint.line_height,
                    window.mouse_position(),
                )
                .is_some();
            (input.focus_handle.clone(), hovered_link)
        };

        if hovered_link {
            window.set_cursor_style(CursorStyle::PointingHand, &prepaint.hitbox);
        }

        if focus_handle.is_focused(window) {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        // Paint code backgrounds behind text.
        for code_bg in prepaint.code_backgrounds.drain(..) {
            window.paint_quad(code_bg);
        }

        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }

        let line_height = prepaint.line_height;
        let lines = std::mem::take(&mut prepaint.lines);
        let text_align = self.input.read(cx).text_align();
        let mut y_offset = Pixels::default();
        for line in &lines {
            let origin_x = aligned_line_left(line, bounds, text_align);
            line.paint(
                point(origin_x, bounds.origin.y + y_offset),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
            y_offset += wrapped_line_height(line, line_height);
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(lines);
            input.last_bounds = Some(bounds);
            input.last_line_height = line_height;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::link_at_position;
    use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree, TableCellPosition};
    use gpui::{
        AppContext, Bounds, Hsla, SharedString, TestAppContext, TextAlign, TextRun,
        VisualTestContext, font, point, px, rgba, size,
    };

    fn shaped_lines(
        text: &str,
        width: gpui::Pixels,
        cx: &mut VisualTestContext,
    ) -> Vec<gpui::WrappedLine> {
        cx.update(|window, _app| {
            window
                .text_system()
                .shape_text(
                    text.to_string().into(),
                    px(16.0),
                    &[TextRun {
                        len: text.len(),
                        font: font(".SystemUIFont"),
                        color: Hsla::from(rgba(0xffffffff)),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    Some(width),
                    None,
                )
                .expect("text should shape")
                .into_vec()
        })
    }

    #[gpui::test]
    async fn link_hit_matches_only_rendered_link_text(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown("[link](https://example.com)"),
                ),
            )
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(320.0), cx);
        let (hit, miss_right) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(320.0), px(20.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let layout = &lines[0];
            let start = layout
                .position_for_index(span.range.start, px(20.0))
                .expect("start position");
            let end = layout
                .position_for_index(span.range.end, px(20.0))
                .expect("end position");
            let hit = point((start.x + end.x) / 2.0, px(10.0));
            let miss_right = point(end.x + px(24.0), px(10.0));
            (
                link_at_position(block, &lines, bounds, px(20.0), hit)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), miss_right)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(hit, Some("https://example.com".to_string()));
        assert_eq!(miss_right, None);
    }

    #[gpui::test]
    async fn link_hit_respects_center_alignment(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown("[link](https://example.com)"),
                ),
            );
            block.set_table_cell_mode(
                TableCellPosition { row: 0, column: 0 },
                crate::components::TableColumnAlignment::Center,
            );
            block
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(240.0), cx);
        let (miss_left, hit_center) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(240.0), px(20.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let layout = &lines[0];
            let origin_x = super::aligned_line_left(layout, bounds, block.text_align());
            let start = layout
                .position_for_index(span.range.start, px(20.0))
                .expect("start position");
            let end = layout
                .position_for_index(span.range.end, px(20.0))
                .expect("end position");
            let miss_left = point(origin_x - px(12.0), px(10.0));
            let hit_center = point(origin_x + (start.x + end.x) / 2.0, px(10.0));
            (
                link_at_position(block, &lines, bounds, px(20.0), miss_left)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), hit_center)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(miss_left, None);
        assert_eq!(hit_center, Some("https://example.com".to_string()));
    }

    #[gpui::test]
    async fn text_runs_apply_inline_html_color_and_background(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown(
                        "before <span style='color:blue;background-color:#ff0'>marked</span>",
                    ),
                ),
            )
        });

        block.read_with(cx, |block, _cx| {
            let display_text: SharedString = block.display_text().to_string().into();
            let base_run = TextRun {
                len: display_text.len(),
                font: font(".SystemUIFont"),
                color: Hsla::from(rgba(0xffffffff)),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = super::build_text_runs(
                block,
                &display_text,
                &base_run,
                px(1.0),
                Hsla::from(rgba(0x0066ccff)),
                Hsla::from(rgba(0x111111ff)),
                true,
            );
            let marked_run = runs.last().expect("styled text should create a final run");

            assert_eq!(block.display_text(), "before marked");
            assert_eq!(marked_run.len, "marked".len());
            assert_eq!(marked_run.color, Hsla::from(rgba(0x0000ffff)));
            assert_eq!(
                marked_run.background_color,
                Some(Hsla::from(rgba(0xffff00ff)))
            );
        });
    }

    #[gpui::test]
    async fn soft_wrapped_range_segments_stay_within_wrap_width(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let lines = shaped_lines(text, px(80.0), cx);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "test text should soft-wrap"
        );

        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(80.0), px(120.0)));
        let segments = super::range_segment_bounds(
            &lines,
            bounds,
            px(20.0),
            text,
            0..text.len(),
            TextAlign::Left,
        );

        assert!(segments.len() > 1);
        for segment in segments {
            assert!(segment.left() >= bounds.left());
            assert!(segment.right() <= bounds.right() + px(0.5));
        }
    }

    #[gpui::test]
    async fn wrapped_link_hit_matches_only_visible_segments(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let label = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown(&format!("[{label}](https://example.com)")),
                ),
            )
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(80.0), cx);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "link text should soft-wrap"
        );

        let (hit, miss_right) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(80.0), px(120.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let segments = super::range_segment_bounds(
                &lines,
                bounds,
                px(20.0),
                &display_text,
                span.range.clone(),
                block.text_align(),
            );
            assert!(segments.len() > 1);
            let second_segment = segments[1];
            let hit = point(
                (second_segment.left() + second_segment.right()) / 2.0,
                (second_segment.top() + second_segment.bottom()) / 2.0,
            );
            let miss_right = point(second_segment.right() + px(24.0), hit.y);
            (
                link_at_position(block, &lines, bounds, px(20.0), hit)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), miss_right)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(hit, Some("https://example.com".to_string()));
        assert_eq!(miss_right, None);
    }

    #[gpui::test]
    async fn wrapped_hard_line_top_accumulates_soft_wrap_height(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz\nnext";
        let lines = shaped_lines(text, px(80.0), cx);
        assert_eq!(lines.len(), 2);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "first hard line should soft-wrap"
        );

        let first_height = lines[0].size(px(20.0)).height;
        assert!(first_height > px(20.0));
        assert_eq!(super::wrapped_line_top(&lines, px(20.0), 1), first_height);
    }
}
