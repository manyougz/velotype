//! Inline projection engine for editable Markdown delimiters.

use std::ops::Range;

use crate::components::InlineFootnoteReference;
use crate::components::markdown::inline::{
    InlineFragment, InlineLink, InlineRenderCache, InlineStyle, InlineTextTree, StyleFlag,
};

use super::CollapsedCaretAffinity;

/// One displayed segment in an expanded inline projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExpandedInlineSegment {
    pub(super) display_range: Range<usize>,
    pub(super) clean_range: Range<usize>,
    pub(super) fragment_index: usize,
    pub(super) link_group: Option<usize>,
    pub(super) kind: ExpandedInlineSegmentKind,
}

/// Inline construct whose Markdown delimiters can be projected for editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpandedInlineKind {
    /// Link label and target syntax.
    Link,
    /// Strikethrough delimiters.
    Strikethrough,
    /// Code span backtick delimiters.
    Code,
}

impl ExpandedInlineKind {
    fn applies_to(self, style: InlineStyle) -> bool {
        match self {
            Self::Link => false,
            Self::Strikethrough => style.strikethrough,
            Self::Code => style.code,
        }
    }

    fn open_marker(self) -> &'static str {
        match self {
            Self::Link => "[",
            Self::Strikethrough => "~~",
            Self::Code => "`",
        }
    }

    fn close_marker(self) -> &'static str {
        match self {
            Self::Link => ")",
            _ => self.open_marker(),
        }
    }

    pub(super) fn style_flag(self) -> Option<StyleFlag> {
        match self {
            Self::Link => None,
            Self::Strikethrough => Some(StyleFlag::Strikethrough),
            Self::Code => Some(StyleFlag::Code),
        }
    }

    fn projection_rank(self) -> u8 {
        match self {
            Self::Link => 0,
            Self::Strikethrough => 1,
            Self::Code => 2,
        }
    }
}

/// Display role of one projected inline segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpandedInlineSegmentKind {
    /// Text with no projected inline syntax.
    PlainText,
    /// Text carrying projected style.
    StyledText,
    /// Opening delimiter such as `[` or backticks.
    OpeningDelimiter(ExpandedInlineKind),
    /// Middle delimiter such as `](` for links.
    MiddleDelimiter(ExpandedInlineKind),
    /// Editable link target text.
    LinkTargetText,
    /// Editable footnote id text.
    FootnoteIdText,
    /// Closing delimiter such as `)` or backticks.
    ClosingDelimiter(ExpandedInlineKind),
}

/// One projected link run spanning one or more inline fragments.
#[derive(Clone, Debug)]
pub(super) struct ExpandedLinkRun {
    pub(super) link: InlineLink,
    pub(super) start_fragment_index: usize,
    pub(super) end_fragment_index: usize,
    pub(super) clean_range: Range<usize>,
    pub(super) display_range: Range<usize>,
    pub(super) target_display_range: Range<usize>,
}

/// One projected footnote reference run.
#[derive(Clone, Debug)]
pub(super) struct ExpandedFootnoteRun {
    pub(super) footnote: InlineFootnoteReference,
    pub(super) clean_range: Range<usize>,
    pub(super) display_range: Range<usize>,
}

/// Selection snapshot translated into an expanded link display range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectedLinkSelectionSnapshot {
    pub(super) clean_range: Range<usize>,
    pub(super) display_relative_range: Range<usize>,
    pub(super) selection_reversed: bool,
}

/// Render cache and offset maps for an expanded inline projection.
#[derive(Clone, Debug)]
pub(crate) struct ExpandedInlineProjection {
    pub(super) cache: InlineRenderCache,
    pub(super) segments: Vec<ExpandedInlineSegment>,
    pub(super) clean_to_display_cursor: Vec<usize>,
    pub(super) display_to_clean: Vec<usize>,
    pub(super) link_runs: Vec<ExpandedLinkRun>,
    pub(super) footnote_runs: Vec<ExpandedFootnoteRun>,
}

#[cfg(test)]
pub(super) fn expanded_display_offset_for_clean(
    fragments: &[InlineFragment],
    clean: usize,
) -> usize {
    let mut display = 0usize;
    let mut clean_cursor = 0usize;
    for fragment in fragments {
        let clean_len = fragment.text.len();
        let clean_end = clean_cursor + clean_len;
        if clean <= clean_end {
            let off = clean.saturating_sub(clean_cursor);
            if fragment.style.code && clean_len > 0 {
                if off == clean_len {
                    return display + clean_len + 2;
                }
                return display + 1 + off;
            }
            return display + off;
        }
        clean_cursor = clean_end;
        display += if fragment.style.code && clean_len > 0 {
            clean_len + 2
        } else {
            clean_len
        };
    }
    display
}

#[cfg(test)]
pub(super) fn expanded_display_cursor_offset_for_clean(
    fragments: &[InlineFragment],
    clean: usize,
) -> usize {
    let mut display = 0usize;
    let mut clean_cursor = 0usize;
    for fragment in fragments {
        let clean_len = fragment.text.len();
        let clean_end = clean_cursor + clean_len;
        if clean <= clean_end {
            let off = clean.saturating_sub(clean_cursor);
            if fragment.style.code && clean_len > 0 {
                if off == 0 {
                    return display + 1;
                }
                if off >= clean_len {
                    return display + clean_len + 1;
                }
                return display + 1 + off;
            }
            return display + off;
        }
        clean_cursor = clean_end;
        display += if fragment.style.code && clean_len > 0 {
            clean_len + 2
        } else {
            clean_len
        };
    }
    display
}

impl ExpandedInlineProjection {
    pub(super) fn build(
        fragments: &[InlineFragment],
        clean_selected: Range<usize>,
        clean_marked: Option<Range<usize>>,
    ) -> Option<Self> {
        let clean_len = fragments
            .iter()
            .map(|fragment| fragment.text.len())
            .sum::<usize>();
        let mut projected_fragments = Vec::new();
        let mut segments = Vec::new();
        let mut clean_to_display_cursor = vec![0; clean_len + 1];
        let mut display_to_clean = vec![0];
        let mut link_runs = Vec::new();
        let mut footnote_runs = Vec::new();
        let mut clean_cursor = 0usize;
        let mut display_cursor = 0usize;
        let mut any_expanded = false;
        let mut fragment_index = 0usize;

        while fragment_index < fragments.len() {
            let fragment = &fragments[fragment_index];
            let fragment_len = fragment.text.len();
            if fragment_len == 0 {
                fragment_index += 1;
                continue;
            }

            if let Some(footnote) = fragment.footnote.as_ref() {
                let clean_range = clean_cursor..clean_cursor + fragment_len;
                let expand_footnote = Self::fragment_is_touched(
                    clean_range.clone(),
                    &clean_selected,
                    clean_marked.as_ref(),
                );
                let run_display_start = display_cursor;
                if expand_footnote {
                    any_expanded = true;
                    let open_marker = "[^".to_string();
                    let open_len = open_marker.len();
                    projected_fragments.push(InlineFragment {
                        text: open_marker,
                        style: InlineStyle::default(),
                        html_style: None,
                        link: None,
                        footnote: None,
                        math: None,
                    });
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + open_len,
                        clean_range: clean_range.start..clean_range.start,
                        fragment_index,
                        link_group: None,
                        kind: ExpandedInlineSegmentKind::OpeningDelimiter(ExpandedInlineKind::Link),
                    });
                    for _ in 0..open_len {
                        display_to_clean.push(clean_range.start);
                    }
                    display_cursor += open_len;

                    let id_text = footnote.id.clone();
                    let id_len = id_text.len();
                    projected_fragments.push(InlineFragment {
                        text: id_text,
                        style: fragment.style,
                        html_style: fragment.html_style,
                        link: None,
                        footnote: Some(footnote.clone()),
                        math: None,
                    });
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + id_len,
                        clean_range: clean_range.clone(),
                        fragment_index,
                        link_group: None,
                        kind: ExpandedInlineSegmentKind::FootnoteIdText,
                    });
                    for offset in 0..=fragment_len {
                        let mapped = if fragment_len == 0 {
                            0
                        } else {
                            (id_len * offset) / fragment_len
                        };
                        clean_to_display_cursor[clean_range.start + offset] =
                            display_cursor + mapped;
                    }
                    for offset in 1..=id_len {
                        let mapped = if id_len == 0 {
                            0
                        } else {
                            (fragment_len * offset) / id_len
                        };
                        display_to_clean.push(clean_range.start + mapped);
                    }
                    display_cursor += id_len;
                    let close_marker = "]".to_string();
                    let close_len = close_marker.len();
                    projected_fragments.push(InlineFragment {
                        text: close_marker,
                        style: InlineStyle::default(),
                        html_style: None,
                        link: None,
                        footnote: None,
                        math: None,
                    });
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + close_len,
                        clean_range: clean_range.end..clean_range.end,
                        fragment_index,
                        link_group: None,
                        kind: ExpandedInlineSegmentKind::ClosingDelimiter(ExpandedInlineKind::Link),
                    });
                    for _ in 0..close_len {
                        display_to_clean.push(clean_range.end);
                    }
                    display_cursor += close_len;

                    footnote_runs.push(ExpandedFootnoteRun {
                        footnote: footnote.clone(),
                        clean_range: clean_range.clone(),
                        display_range: run_display_start..display_cursor,
                    });
                } else {
                    projected_fragments.push(fragment.clone());
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + fragment_len,
                        clean_range: clean_range.clone(),
                        fragment_index,
                        link_group: None,
                        kind: ExpandedInlineSegmentKind::PlainText,
                    });
                    for offset in 0..=fragment_len {
                        clean_to_display_cursor[clean_range.start + offset] =
                            display_cursor + offset;
                    }
                    for offset in 1..=fragment_len {
                        display_to_clean.push(clean_range.start + offset);
                    }
                    display_cursor += fragment_len;
                }

                clean_cursor = clean_range.end;
                fragment_index += 1;
                continue;
            }

            if let Some(link) = fragment.link.as_ref() {
                let run_start = fragment_index;
                let run_clean_start = clean_cursor;
                let mut run_end = fragment_index;
                let mut run_clean_end = clean_cursor;
                while run_end < fragments.len() {
                    let run_fragment = &fragments[run_end];
                    if run_fragment.link.as_ref() != Some(link) {
                        break;
                    }
                    run_clean_end += run_fragment.text.len();
                    run_end += 1;
                }

                let run_clean_range = run_clean_start..run_clean_end;
                let expand_link = Self::fragment_is_touched(
                    run_clean_range.clone(),
                    &clean_selected,
                    clean_marked.as_ref(),
                );
                let link_group = expand_link.then_some(link_runs.len());
                let run_display_start = display_cursor;
                if expand_link {
                    any_expanded = true;
                    let open_marker = link.open_marker().to_string();
                    let open_len = open_marker.len();
                    projected_fragments.push(InlineFragment {
                        text: open_marker,
                        style: InlineStyle::default(),
                        html_style: None,
                        link: None,
                        footnote: None,
                        math: None,
                    });
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + open_len,
                        clean_range: run_clean_start..run_clean_start,
                        fragment_index: run_start,
                        link_group,
                        kind: ExpandedInlineSegmentKind::OpeningDelimiter(ExpandedInlineKind::Link),
                    });
                    for _ in 0..open_len {
                        display_to_clean.push(run_clean_start);
                    }
                    display_cursor += open_len;
                }

                let mut local_clean_cursor = run_clean_start;
                for current_index in run_start..run_end {
                    let current_fragment = &fragments[current_index];
                    let current_len = current_fragment.text.len();
                    let current_clean_range = local_clean_cursor..local_clean_cursor + current_len;
                    projected_fragments.push(current_fragment.clone());
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + current_len,
                        clean_range: current_clean_range.clone(),
                        fragment_index: current_index,
                        link_group,
                        kind: if expand_link {
                            ExpandedInlineSegmentKind::StyledText
                        } else {
                            ExpandedInlineSegmentKind::PlainText
                        },
                    });
                    for offset in 0..=current_len {
                        clean_to_display_cursor[current_clean_range.start + offset] =
                            display_cursor + offset;
                    }
                    for offset in 1..=current_len {
                        display_to_clean.push(current_clean_range.start + offset);
                    }
                    display_cursor += current_len;
                    local_clean_cursor = current_clean_range.end;
                }
                if expand_link {
                    if let Some(middle_marker) = link.middle_marker() {
                        let middle_len = middle_marker.len();
                        projected_fragments.push(InlineFragment {
                            text: middle_marker.to_string(),
                            style: InlineStyle::default(),
                            html_style: None,
                            link: None,
                            footnote: None,
                            math: None,
                        });
                        segments.push(ExpandedInlineSegment {
                            display_range: display_cursor..display_cursor + middle_len,
                            clean_range: run_clean_end..run_clean_end,
                            fragment_index: run_start,
                            link_group,
                            kind: ExpandedInlineSegmentKind::MiddleDelimiter(
                                ExpandedInlineKind::Link,
                            ),
                        });
                        for _ in 0..middle_len {
                            display_to_clean.push(run_clean_end);
                        }
                        display_cursor += middle_len;
                    }

                    let target_display_start = display_cursor;
                    if let Some(link_target) = link.editable_text() {
                        let target_len = link_target.len();
                        if target_len > 0 {
                            projected_fragments.push(InlineFragment {
                                text: link_target,
                                style: InlineStyle::default(),
                                html_style: None,
                                link: Some(link.clone()),
                                footnote: None,
                                math: None,
                            });
                            segments.push(ExpandedInlineSegment {
                                display_range: display_cursor..display_cursor + target_len,
                                clean_range: run_clean_end..run_clean_end,
                                fragment_index: run_start,
                                link_group,
                                kind: ExpandedInlineSegmentKind::LinkTargetText,
                            });
                            for _ in 0..target_len {
                                display_to_clean.push(run_clean_end);
                            }
                            display_cursor += target_len;
                        }
                    }
                    let target_display_end = display_cursor;

                    let close_marker = link.close_marker().to_string();
                    let close_len = close_marker.len();
                    projected_fragments.push(InlineFragment {
                        text: close_marker,
                        style: InlineStyle::default(),
                        html_style: None,
                        link: None,
                        footnote: None,
                        math: None,
                    });
                    segments.push(ExpandedInlineSegment {
                        display_range: display_cursor..display_cursor + close_len,
                        clean_range: run_clean_end..run_clean_end,
                        fragment_index: run_start,
                        link_group,
                        kind: ExpandedInlineSegmentKind::ClosingDelimiter(ExpandedInlineKind::Link),
                    });
                    for _ in 0..close_len {
                        display_to_clean.push(run_clean_end);
                    }
                    display_cursor += close_len;

                    link_runs.push(ExpandedLinkRun {
                        link: link.clone(),
                        start_fragment_index: run_start,
                        end_fragment_index: run_end,
                        clean_range: run_clean_range.clone(),
                        display_range: run_display_start..display_cursor,
                        target_display_range: target_display_start..target_display_end,
                    });
                }

                clean_cursor = run_clean_end;
                fragment_index = run_end;
                continue;
            }

            let clean_range = clean_cursor..clean_cursor + fragment_len;
            let expanded_kinds = Self::expanded_kinds_for_fragment(
                fragment.style,
                clean_range.clone(),
                &clean_selected,
                clean_marked.as_ref(),
            );

            for kind in &expanded_kinds {
                any_expanded = true;
                let marker = kind.open_marker().to_string();
                let marker_len = marker.len();
                projected_fragments.push(InlineFragment {
                    text: marker,
                    style: fragment.style,
                    html_style: fragment.html_style,
                    link: None,
                    footnote: None,
                    math: None,
                });
                segments.push(ExpandedInlineSegment {
                    display_range: display_cursor..display_cursor + marker_len,
                    clean_range: clean_range.start..clean_range.start,
                    fragment_index,
                    link_group: None,
                    kind: ExpandedInlineSegmentKind::OpeningDelimiter(*kind),
                });
                for _ in 0..marker_len {
                    display_to_clean.push(clean_range.start);
                }
                display_cursor += marker_len;
            }

            let text_segment_kind = if expanded_kinds.is_empty() {
                ExpandedInlineSegmentKind::PlainText
            } else {
                ExpandedInlineSegmentKind::StyledText
            };
            projected_fragments.push(fragment.clone());
            segments.push(ExpandedInlineSegment {
                display_range: display_cursor..display_cursor + fragment_len,
                clean_range: clean_range.clone(),
                fragment_index,
                link_group: None,
                kind: text_segment_kind,
            });
            for offset in 0..=fragment_len {
                clean_to_display_cursor[clean_range.start + offset] = display_cursor + offset;
            }
            for offset in 1..=fragment_len {
                display_to_clean.push(clean_range.start + offset);
            }
            display_cursor += fragment_len;

            for kind in expanded_kinds.iter().rev() {
                let marker = kind.close_marker().to_string();
                let marker_len = marker.len();
                projected_fragments.push(InlineFragment {
                    text: marker,
                    style: fragment.style,
                    html_style: fragment.html_style,
                    link: None,
                    footnote: None,
                    math: None,
                });
                segments.push(ExpandedInlineSegment {
                    display_range: display_cursor..display_cursor + marker_len,
                    clean_range: clean_range.end..clean_range.end,
                    fragment_index,
                    link_group: None,
                    kind: ExpandedInlineSegmentKind::ClosingDelimiter(*kind),
                });
                for _ in 0..marker_len {
                    display_to_clean.push(clean_range.end);
                }
                display_cursor += marker_len;
            }

            clean_cursor = clean_range.end;
            fragment_index += 1;
        }

        if any_expanded {
            for segment in &segments {
                match segment.kind {
                    ExpandedInlineSegmentKind::OpeningDelimiter(ExpandedInlineKind::Code)
                    | ExpandedInlineSegmentKind::OpeningDelimiter(
                        ExpandedInlineKind::Strikethrough,
                    ) => {
                        clean_to_display_cursor[segment.clean_range.start] =
                            segment.display_range.end;
                    }
                    ExpandedInlineSegmentKind::ClosingDelimiter(ExpandedInlineKind::Code)
                    | ExpandedInlineSegmentKind::ClosingDelimiter(
                        ExpandedInlineKind::Strikethrough,
                    ) => {
                        clean_to_display_cursor[segment.clean_range.start] =
                            segment.display_range.start;
                    }
                    _ => {}
                }
            }
        }

        any_expanded.then(|| Self {
            cache: InlineTextTree::from_fragments(projected_fragments).render_cache(),
            segments,
            clean_to_display_cursor,
            display_to_clean,
            link_runs,
            footnote_runs,
        })
    }

    #[allow(dead_code)]
    pub(super) fn pointer_target_offset(&self, offset: usize) -> usize {
        for segment in &self.segments {
            match segment.kind {
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if offset >= segment.display_range.start
                        && offset <= segment.display_range.end =>
                {
                    return segment.display_range.end;
                }
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if offset >= segment.display_range.start
                        && offset <= segment.display_range.end =>
                {
                    return segment.display_range.start;
                }
                ExpandedInlineSegmentKind::MiddleDelimiter(ExpandedInlineKind::Link)
                    if offset >= segment.display_range.start
                        && offset <= segment.display_range.end =>
                {
                    if let Some(link_group) = segment.link_group
                        && let Some(run) = self.link_runs.get(link_group)
                    {
                        return run.target_display_range.start;
                    }
                    return segment.display_range.end;
                }
                _ => {}
            }
        }
        offset
    }

    pub(super) fn collapsed_affinity_for_display_offset(
        &self,
        offset: usize,
    ) -> CollapsedCaretAffinity {
        for segment in &self.segments {
            match segment.kind {
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if offset == segment.display_range.start =>
                {
                    return CollapsedCaretAffinity::OuterStart;
                }
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if offset == segment.display_range.end =>
                {
                    return CollapsedCaretAffinity::OuterEnd;
                }
                _ => {}
            }
        }
        CollapsedCaretAffinity::Default
    }

    pub(super) fn display_offset_for_clean_cursor(
        &self,
        clean: usize,
        affinity: CollapsedCaretAffinity,
    ) -> Option<usize> {
        match affinity {
            CollapsedCaretAffinity::Default => self
                .clean_to_display_cursor
                .get(clean.min(self.clean_to_display_cursor.len().saturating_sub(1)))
                .copied(),
            CollapsedCaretAffinity::OuterStart => self
                .segments
                .iter()
                .find_map(|segment| match segment.kind {
                    ExpandedInlineSegmentKind::OpeningDelimiter(_)
                        if segment.clean_range.start == clean =>
                    {
                        Some(segment.display_range.start)
                    }
                    _ => None,
                })
                .or_else(|| {
                    self.clean_to_display_cursor
                        .get(clean.min(self.clean_to_display_cursor.len().saturating_sub(1)))
                        .copied()
                }),
            CollapsedCaretAffinity::OuterEnd => self
                .segments
                .iter()
                .find_map(|segment| match segment.kind {
                    ExpandedInlineSegmentKind::ClosingDelimiter(_)
                        if segment.clean_range.start == clean =>
                    {
                        Some(segment.display_range.end)
                    }
                    _ => None,
                })
                .or_else(|| {
                    self.clean_to_display_cursor
                        .get(clean.min(self.clean_to_display_cursor.len().saturating_sub(1)))
                        .copied()
                }),
        }
    }

    pub(super) fn move_left_target(
        &self,
        offset: usize,
    ) -> Option<(usize, CollapsedCaretAffinity)> {
        for segment in &self.segments {
            match segment.kind {
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if offset == segment.display_range.end =>
                {
                    return Some((
                        segment.display_range.start,
                        CollapsedCaretAffinity::OuterStart,
                    ));
                }
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if offset == segment.display_range.end =>
                {
                    return Some((segment.display_range.start, CollapsedCaretAffinity::Default));
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn move_right_target(
        &self,
        offset: usize,
    ) -> Option<(usize, CollapsedCaretAffinity)> {
        for segment in &self.segments {
            match segment.kind {
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if offset == segment.display_range.start =>
                {
                    return Some((segment.display_range.end, CollapsedCaretAffinity::Default));
                }
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if offset == segment.display_range.start =>
                {
                    return Some((segment.display_range.end, CollapsedCaretAffinity::OuterEnd));
                }
                _ => {}
            }
        }
        None
    }

    fn expanded_kinds_for_fragment(
        style: InlineStyle,
        fragment_range: Range<usize>,
        clean_selected: &Range<usize>,
        clean_marked: Option<&Range<usize>>,
    ) -> Vec<ExpandedInlineKind> {
        let mut kinds = Vec::new();
        for kind in [ExpandedInlineKind::Strikethrough, ExpandedInlineKind::Code] {
            if kind.applies_to(style)
                && Self::fragment_is_touched(fragment_range.clone(), clean_selected, clean_marked)
            {
                kinds.push(kind);
            }
        }
        kinds.sort_by_key(|kind| kind.projection_rank());
        kinds
    }

    fn fragment_is_touched(
        fragment_range: Range<usize>,
        clean_selected: &Range<usize>,
        clean_marked: Option<&Range<usize>>,
    ) -> bool {
        if let Some(marked_range) = clean_marked
            && !marked_range.is_empty()
            && Self::ranges_overlap(&fragment_range, marked_range)
        {
            return true;
        }

        if !clean_selected.is_empty() {
            return Self::ranges_overlap(&fragment_range, clean_selected);
        }

        let cursor = clean_selected.start;
        fragment_range.start <= cursor && cursor <= fragment_range.end
    }

    fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
        left.start < right.end && right.start < left.end
    }

    pub(super) fn link_run_fully_covering_range(
        &self,
        range: &Range<usize>,
    ) -> Option<&ExpandedLinkRun> {
        self.link_runs.iter().find(|run| {
            run.display_range.start <= range.start && range.end <= run.display_range.end
        })
    }

    pub(super) fn link_run_for_clean_range(
        &self,
        clean_range: &Range<usize>,
    ) -> Option<&ExpandedLinkRun> {
        self.link_runs
            .iter()
            .find(|run| run.clean_range == *clean_range)
    }

    pub(super) fn footnote_run_fully_covering_range(
        &self,
        range: &Range<usize>,
    ) -> Option<&ExpandedFootnoteRun> {
        self.footnote_runs.iter().find(|run| {
            run.display_range.start <= range.start && range.end <= run.display_range.end
        })
    }
}
