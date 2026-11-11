//! Editable block runtime and block-local state transitions.

use std::cell::Cell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::*;
use unicode_segmentation::*;

mod code;
mod image;
mod projection;
mod table;

use self::projection::{
    ExpandedInlineProjection, ExpandedInlineSegment, ExpandedInlineSegmentKind, ExpandedLinkRun,
    ProjectedLinkSelectionSnapshot,
};
use super::{
    BlockEvent, BlockKind, BlockRecord, CalloutVariant, FootnoteRegistry, InlineFootnoteHit,
    UndoCaptureKind,
};
use super::{CodeHighlightResult, highlight_code_block};
use super::{
    ImageReferenceDefinitions, ImageResolvedSource, ImageSyntax, LinkReferenceDefinitions,
    parse_standalone_image, resolve_image_source,
};
use crate::components::markdown::inline::{
    InlineFragment, InlineInsertionAttributes, InlineLinkHit, InlineRenderCache, InlineSpan,
    InlineStyle, InlineTextTree, StyleFlag,
};
use crate::components::{
    MermaidDrag, MermaidFrame, MermaidView, TableAxisHighlight, TableAxisMarker, TableCellPosition,
    TableColumnAlignment, TableRuntime,
};

/// Inline formatting command issued by editor actions.
#[derive(Clone, Copy)]
pub(crate) enum InlineFormat {
    /// Toggle bold formatting.
    Bold,
    /// Toggle italic formatting.
    Italic,
    /// Toggle underline formatting.
    Underline,
    /// Toggle inline code formatting.
    Code,
}

/// Editing semantics for the current block.
///
/// Rich blocks edit the attribute-based text tree, while source mode and code
/// blocks edit raw text without inline Markdown normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditMode {
    /// Attribute-based rich text editing for normal rendered blocks.
    RenderedRich,
    /// Raw Markdown editing for source-mode and raw fallback blocks.
    SourceRaw,
    /// Raw text editing for fenced code block contents.
    CodeBlockRaw,
}

impl EditMode {
    /// Chooses the default editing mode for a block kind.
    fn for_kind(kind: &BlockKind) -> Self {
        if kind.is_code_block() {
            Self::CodeBlockRaw
        } else if matches!(
            kind,
            BlockKind::RawMarkdown
                | BlockKind::Comment
                | BlockKind::HtmlBlock
                | BlockKind::MathBlock
                | BlockKind::MermaidBlock
        ) {
            Self::SourceRaw
        } else {
            Self::RenderedRich
        }
    }

    /// Whether edits bypass rich inline parsing.
    fn uses_raw_text_editing(self) -> bool {
        matches!(self, Self::SourceRaw | Self::CodeBlockRaw)
    }

    /// Whether touched inline Markdown syntax may be shown for editing.
    fn supports_inline_projection(self) -> bool {
        matches!(self, Self::RenderedRich)
    }
}

impl EventEmitter<BlockEvent> for Block {}

/// A single editable block in the document tree.
///
/// Each block holds a [`BlockRecord`] containing the persistent data (kind,
/// title, UUIDs) and a [`FocusHandle`] for keyboard routing.  Runtime state
/// such as selection, cursor blink, and layout cache live on the struct.
///
/// Blocks delegate structural operations (split, merge, indent, delete) to
/// the parent editor via `BlockEvent` emissions.
pub struct Block {
    /// Persistent block kind, title, and identity.
    pub record: BlockRecord,
    /// Clean visible-text cache derived from `record.title`.
    pub(crate) render_cache: InlineRenderCache,
    /// Highlight spans for code-block contents, when available.
    code_highlight: Option<CodeHighlightResult>,
    /// Child block entities in document order.
    pub children: Vec<Entity<Block>>,
    /// Focus target for the block body.
    pub focus_handle: FocusHandle,
    /// Independent focus target for a code block's language field.
    pub(crate) code_language_focus_handle: FocusHandle,
    /// Ordered UTF-8 selection in the code-language label.
    pub(crate) code_language_selected_range: Range<usize>,
    /// Whether the active language selection endpoint is its start.
    pub(crate) code_language_selection_reversed: bool,
    /// UTF-8 IME composition range in the code-language label.
    pub(crate) code_language_marked_range: Option<Range<usize>>,
    /// Last shaped line used to hit-test the language label.
    pub(crate) code_language_last_layout: Option<ShapedLine>,
    /// Last painted bounds of the language label.
    pub(crate) code_language_last_bounds: Option<Bounds<Pixels>>,
    /// Whether a pointer drag is selecting language text.
    pub(crate) code_language_is_selecting: bool,
    /// Ordered UTF-8 selection in the current display text.
    pub selected_range: Range<usize>,
    /// Whether the active body selection endpoint is its start.
    pub selection_reversed: bool,
    /// Editor-owned selection spanning multiple blocks, when present.
    pub(crate) editor_selection_range: Option<Range<usize>>,
    /// UTF-8 IME composition range in the current display text.
    pub marked_range: Option<Range<usize>>,
    /// Last wrapped body layout, used for hit testing and caret geometry.
    pub last_layout: Option<Vec<WrappedLine>>,
    /// Last painted body text bounds.
    pub last_bounds: Option<Bounds<Pixels>>,
    /// Line height used with `last_layout`.
    pub last_line_height: Pixels,
    /// Structural indentation used when rendering this block.
    pub render_depth: usize,
    /// Nesting depth of the surrounding quote group.
    pub quote_depth: usize,
    /// Identity of the quote group used for runtime grouping.
    pub(crate) quote_group_anchor: Option<uuid::Uuid>,
    /// Quote depth currently visible in the rendered hierarchy.
    pub(crate) visible_quote_depth: usize,
    /// Anchor of the visible quote group.
    pub(crate) visible_quote_group_anchor: Option<uuid::Uuid>,
    /// Nesting depth of the surrounding callout.
    pub(crate) callout_depth: usize,
    /// Identity of the surrounding callout group.
    pub(crate) callout_anchor: Option<uuid::Uuid>,
    /// Presentation variant of the surrounding callout.
    pub(crate) callout_variant: Option<CalloutVariant>,
    /// Identity of the containing footnote definition.
    pub(crate) footnote_anchor: Option<uuid::Uuid>,
    /// Whether this block is directly inside a list item.
    pub(crate) parent_is_list_item: bool,
    /// Number displayed for a numbered list item.
    pub list_ordinal: Option<usize>,
    /// Whether a pointer drag is selecting body text.
    pub is_selecting: bool,
    /// Last action that reset the cursor blink phase.
    pub cursor_blink_epoch: Instant,
    /// Preferred horizontal position for vertical cursor movement.
    pub vertical_motion_x: Option<Pixels>,
    /// Running cursor repaint loop, cancelled when dropped.
    pub(super) cursor_blink_task: Option<Task<()>>,
    /// Cached projection used to show editable inline delimiters for the
    /// currently touched inline span(s).
    pub(crate) projection: Option<ExpandedInlineProjection>,
    /// Focus and clean ranges that produced the projection, used to skip
    /// unchanged rebuilds.
    projection_cache_key: Option<(bool, Range<usize>, Option<Range<usize>>)>,
    /// Shared copy of the current display text refreshed with the caches.
    cached_display_text: SharedString,
    /// Which side of a projected delimiter owns a collapsed caret.
    collapsed_caret_affinity: CollapsedCaretAffinity,
    /// Active rich, source-raw, or code-raw editing semantics.
    pub(crate) edit_mode: EditMode,
    /// Whether source editing displays line numbers.
    show_source_line_numbers: bool,
    /// Parsed table model for table blocks.
    pub(crate) table_runtime: Option<TableRuntime>,
    /// Cell coordinates when this block edits a table cell.
    pub(crate) table_cell_position: Option<TableCellPosition>,
    /// Alignment of the current table cell.
    pub(crate) table_cell_alignment: Option<TableColumnAlignment>,
    /// Hovered table axis insertion marker.
    pub(crate) table_axis_preview: Option<TableAxisMarker>,
    /// Selected table axis marker.
    pub(crate) table_axis_selection: Option<TableAxisMarker>,
    /// Highlight state for table row or column controls.
    pub(crate) table_axis_highlight: TableAxisHighlight,
    /// Whether the pointer is over the append-column edge.
    pub(crate) table_append_column_edge_hovered: bool,
    /// Whether the pointer is over the append-column control.
    pub(crate) table_append_column_hovered: bool,
    /// Whether the pointer is inside the append-column activation zone.
    pub(crate) table_append_column_zone_hovered: bool,
    /// Whether the append-column button itself is hovered.
    pub(crate) table_append_column_button_hovered: bool,
    /// Deferred task that closes the append-column control.
    pub(crate) table_append_column_close_task: Option<Task<()>>,
    /// Whether the pointer is over the append-row edge.
    pub(crate) table_append_row_edge_hovered: bool,
    /// Whether the pointer is over the append-row control.
    pub(crate) table_append_row_hovered: bool,
    /// Whether the pointer is inside the append-row activation zone.
    pub(crate) table_append_row_zone_hovered: bool,
    /// Whether the append-row button itself is hovered.
    pub(crate) table_append_row_button_hovered: bool,
    /// Deferred task that closes the append-row control.
    pub(crate) table_append_row_close_task: Option<Task<()>>,
    /// Resolved standalone image presentation, when applicable.
    image_runtime: Option<ImageRuntime>,
    /// Whether the image editor is currently expanded.
    image_edit_expanded: bool,
    /// Pending request to expand the image editor.
    image_expand_requested: bool,
    /// Whether an HTML details block is open.
    pub(crate) html_details_open: bool,
    /// Base directory used to resolve relative image sources.
    image_base_dir: Option<PathBuf>,
    /// Image reference definitions available to this block.
    image_reference_definitions: Arc<ImageReferenceDefinitions>,
    /// Link reference definitions available during inline parsing.
    link_reference_definitions: Arc<LinkReferenceDefinitions>,
    /// Resolved footnote occurrences and numbering.
    footnote_registry: Arc<FootnoteRegistry>,
    /// Whether an empty paragraph separates numbered-list groups.
    pub(crate) list_group_separator_candidate: bool,
    /// Pending editor request to restart numbered-list numbering.
    numbered_list_restart_requested: bool,
    /// Pending editor request to reparse quote structure.
    quote_reparse_requested: bool,
    /// Zoom and pan state of the rendered Mermaid preview.
    pub(crate) mermaid_view: MermaidView,
    /// Active pointer press over the Mermaid preview.
    pub(crate) mermaid_drag: Option<MermaidDrag>,
    /// Last prepainted bounds of the Mermaid preview viewport.
    pub(crate) mermaid_viewport_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Geometry of the last rendered Mermaid preview frame.
    pub(crate) mermaid_frame: Option<MermaidFrame>,
    /// Whether the pointer is over the Mermaid preview.
    pub(crate) mermaid_hovered: bool,
    /// Whether the modifier-zoom hint is showing on the Mermaid preview.
    pub(crate) mermaid_zoom_hint_visible: bool,
    /// Deferred task that hides the Mermaid zoom hint.
    pub(crate) mermaid_zoom_hint_task: Option<Task<()>>,
}

/// Cached standalone image presentation state for a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageRuntime {
    /// Image alt text.
    pub(crate) alt: String,
    /// Source spelling before resolution.
    pub(crate) src: String,
    /// Optional image title.
    pub(crate) title: Option<String>,
    /// Resolved local or remote image source.
    pub(crate) resolved_source: ImageResolvedSource,
}

/// How a collapsed caret at an inline projection boundary inherits style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CollapsedCaretAffinity {
    /// Use the normal insertion-attribute lookup.
    #[default]
    Default,
    /// Treat the caret as being just outside the opening delimiter.
    OuterStart,
    /// Treat the caret as being just outside the closing delimiter.
    OuterEnd,
}

impl Block {
    /// Creates runtime state and caches for a persistent block record.
    pub fn with_record(cx: &mut Context<Self>, record: BlockRecord) -> Self {
        let edit_mode = EditMode::for_kind(&record.kind);
        let render_cache = record.title.render_cache();
        let mut block = Self {
            record,
            render_cache,
            code_highlight: None,
            children: Vec::new(),
            focus_handle: cx.focus_handle(),
            code_language_focus_handle: cx.focus_handle(),
            code_language_selected_range: 0..0,
            code_language_selection_reversed: false,
            code_language_marked_range: None,
            code_language_last_layout: None,
            code_language_last_bounds: None,
            code_language_is_selecting: false,
            selected_range: 0..0,
            selection_reversed: false,
            editor_selection_range: None,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_line_height: px(20.0),
            render_depth: 0,
            quote_depth: 0,
            quote_group_anchor: None,
            visible_quote_depth: 0,
            visible_quote_group_anchor: None,
            callout_depth: 0,
            callout_anchor: None,
            callout_variant: None,
            footnote_anchor: None,
            parent_is_list_item: false,
            list_ordinal: None,
            is_selecting: false,
            cursor_blink_epoch: Instant::now(),
            vertical_motion_x: None,
            cursor_blink_task: None,
            projection: None,
            projection_cache_key: None,
            cached_display_text: SharedString::default(),
            collapsed_caret_affinity: CollapsedCaretAffinity::Default,
            edit_mode,
            show_source_line_numbers: false,
            table_runtime: None,
            table_cell_position: None,
            table_cell_alignment: None,
            table_axis_preview: None,
            table_axis_selection: None,
            table_axis_highlight: TableAxisHighlight::None,
            table_append_column_edge_hovered: false,
            table_append_column_hovered: false,
            table_append_column_zone_hovered: false,
            table_append_column_button_hovered: false,
            table_append_column_close_task: None,
            table_append_row_edge_hovered: false,
            table_append_row_hovered: false,
            table_append_row_zone_hovered: false,
            table_append_row_button_hovered: false,
            table_append_row_close_task: None,
            image_runtime: None,
            image_edit_expanded: false,
            image_expand_requested: false,
            html_details_open: false,
            image_base_dir: None,
            image_reference_definitions: Arc::default(),
            link_reference_definitions: Arc::default(),
            footnote_registry: Arc::default(),
            list_group_separator_candidate: false,
            numbered_list_restart_requested: false,
            quote_reparse_requested: false,
            mermaid_view: MermaidView::default(),
            mermaid_drag: None,
            mermaid_viewport_bounds: Rc::default(),
            mermaid_frame: None,
            mermaid_hovered: false,
            mermaid_zoom_hint_visible: false,
            mermaid_zoom_hint_task: None,
        };
        block.sync_code_highlight();
        block.refresh_cached_display_text();
        block
    }

    /// Returns the block's persistent kind.
    pub fn kind(&self) -> BlockKind {
        self.record.kind.clone()
    }

    /// Whether this block is editing source Markdown without rich parsing.
    pub(crate) fn is_source_raw_mode(&self) -> bool {
        self.edit_mode == EditMode::SourceRaw
    }

    /// Whether line numbers are enabled for source editing.
    pub(crate) fn show_source_line_numbers(&self) -> bool {
        self.show_source_line_numbers
    }

    /// Takes and clears the pending quote-structure reparse request.
    pub(crate) fn take_quote_reparse_requested(&mut self) -> bool {
        let requested = self.quote_reparse_requested;
        self.quote_reparse_requested = false;
        requested
    }

    /// Takes and clears the pending numbered-list restart request.
    pub(crate) fn take_numbered_list_restart_requested(&mut self) -> bool {
        let requested = self.numbered_list_restart_requested;
        self.numbered_list_restart_requested = false;
        requested
    }

    /// Updates image, link, and footnote context, refreshing affected caches.
    pub(crate) fn set_runtime_context(
        &mut self,
        base_dir: Option<PathBuf>,
        image_reference_definitions: Arc<ImageReferenceDefinitions>,
        link_reference_definitions: Arc<LinkReferenceDefinitions>,
        footnote_registry: Arc<FootnoteRegistry>,
    ) {
        if self.image_base_dir != base_dir {
            self.image_base_dir = base_dir;
        }
        if self.image_reference_definitions != image_reference_definitions {
            self.image_reference_definitions = image_reference_definitions;
        }
        self.sync_link_reference_definitions(link_reference_definitions);
        self.sync_footnote_registry(footnote_registry);
        self.sync_image_runtime();
    }

    /// Whether text edits bypass rich inline parsing in the current mode.
    pub(crate) fn uses_raw_text_editing(&self) -> bool {
        self.edit_mode.uses_raw_text_editing()
    }

    /// Switches to raw source editing without line numbers.
    pub(crate) fn set_source_raw_mode(&mut self) {
        self.clear_inline_projection();
        self.edit_mode = EditMode::SourceRaw;
        self.show_source_line_numbers = false;
    }

    /// Switches to raw source editing with line numbers.
    pub(crate) fn set_source_document_mode(&mut self) {
        self.set_source_raw_mode();
        self.show_source_line_numbers = true;
    }

    /// Reconciles editing mode with block kind unless source mode is explicit.
    pub(crate) fn sync_edit_mode_from_kind(&mut self) {
        if self.table_cell_position.is_some() {
            self.edit_mode = EditMode::RenderedRich;
            self.show_source_line_numbers = false;
            return;
        }
        if self.edit_mode != EditMode::SourceRaw {
            if self.kind().is_code_block() {
                self.clear_inline_projection();
            }
            self.edit_mode = EditMode::for_kind(&self.record.kind);
            self.show_source_line_numbers = false;
        }
    }

    /// Returns current visible text, including temporary projected delimiters.
    pub fn display_text(&self) -> &str {
        self.current_cache().visible_text()
    }

    /// Cheap clone of the current display text as a `SharedString` (Arc bump)
    /// — avoids a fresh String allocation per render. The cached value is
    /// refreshed by [`Self::refresh_cached_display_text`] whenever the
    /// underlying text might have changed.
    pub(crate) fn shared_display_text(&self) -> SharedString {
        self.cached_display_text.clone()
    }

    /// Refreshes the shared display copy only when visible text changed.
    fn refresh_cached_display_text(&mut self) {
        let current = self.current_cache().visible_text();
        if self.cached_display_text.as_ref() != current {
            self.cached_display_text = SharedString::from(current.to_string());
        }
    }

    /// Parses Markdown using the block's current link references.
    pub(crate) fn inline_tree_from_markdown_with_context(&self, markdown: &str) -> InlineTextTree {
        InlineTextTree::from_markdown_with_link_references(
            markdown,
            &self.link_reference_definitions,
        )
    }

    /// Returns styled spans in the current display cache.
    pub fn inline_spans(&self) -> &[InlineSpan] {
        self.current_cache().spans()
    }

    #[allow(dead_code)]
    /// Returns the inline style at a current-display byte offset.
    pub fn inline_style_at(&self, offset: usize) -> InlineStyle {
        self.current_cache().style_at(offset)
    }

    #[allow(dead_code)]
    /// Returns HTML styling at a current-display byte offset, if present.
    pub(crate) fn inline_html_style_at(
        &self,
        offset: usize,
    ) -> Option<crate::components::HtmlInlineStyle> {
        self.current_cache().html_style_at(offset)
    }

    #[allow(dead_code)]
    /// Returns a link target at a current-display byte offset, if present.
    pub(crate) fn inline_link_at(&self, offset: usize) -> Option<&str> {
        self.current_cache().link_at(offset)
    }

    #[allow(dead_code)]
    /// Returns the link hit at a current-display byte offset, if present.
    pub(crate) fn inline_link_hit_at(&self, offset: usize) -> Option<&InlineLinkHit> {
        self.current_cache().link_hit_at(offset)
    }

    #[allow(dead_code)]
    /// Returns the footnote hit at a current-display byte offset, if present.
    pub(crate) fn inline_footnote_hit_at(&self, offset: usize) -> Option<&InlineFootnoteHit> {
        self.current_cache().footnote_hit_at(offset)
    }

    #[allow(dead_code)]
    /// Returns inline math at a current-display byte offset, if present.
    pub(crate) fn inline_math_at(&self, offset: usize) -> Option<&crate::components::InlineMath> {
        self.current_cache().inline_math_at(offset)
    }

    /// Whether the clean title mixes text with separately rendered inline content.
    pub(crate) fn has_mixed_inline_visuals(&self) -> bool {
        self.record.title.has_mixed_inline_visuals()
    }

    /// Returns a footnote definition's visible identifier, if applicable.
    pub(crate) fn footnote_definition_id(&self) -> Option<String> {
        self.kind()
            .is_footnote_definition()
            .then(|| self.record.title.visible_text())
    }

    /// Returns the registry ordinal for this footnote definition.
    pub(crate) fn footnote_definition_ordinal(&self) -> Option<usize> {
        self.footnote_definition_id()
            .as_deref()
            .and_then(|id| self.footnote_registry.ordinal(id))
    }

    /// Whether this footnote definition has a resolved first reference.
    pub(crate) fn footnote_definition_has_backref(&self) -> bool {
        self.footnote_definition_id().as_deref().is_some_and(|id| {
            self.footnote_registry
                .binding(id)
                .and_then(|binding| binding.first_reference.as_ref())
                .is_some()
        })
    }

    /// Finds a footnote occurrence in current-display UTF-8 coordinates.
    pub(crate) fn current_range_for_footnote_occurrence(
        &self,
        occurrence_index: usize,
    ) -> Option<Range<usize>> {
        let mut clean_offset = 0usize;
        for fragment in &self.record.title.fragments {
            let len = fragment.text.len();
            if fragment
                .footnote
                .as_ref()
                .is_some_and(|footnote| footnote.occurrence_index == occurrence_index)
            {
                return Some(self.clean_to_current_range(clean_offset..clean_offset + len));
            }
            clean_offset += len;
        }
        None
    }

    /// Whether the currently displayed text is empty.
    pub fn is_empty(&self) -> bool {
        self.display_text().is_empty()
    }

    /// Whether this non-list block sits directly inside a list item.
    pub fn is_direct_list_child(&self) -> bool {
        self.parent_is_list_item && !self.kind().is_list_item()
    }

    /// Whether this list item is nested beneath another list item.
    pub fn is_nested_list_item(&self) -> bool {
        self.parent_is_list_item && self.kind().is_list_item()
    }

    /// Whether list nesting commands apply to this block.
    pub fn can_adjust_list_nesting(&self) -> bool {
        (self.kind().is_list_item() || self.parent_is_list_item) && !self.kind().is_code_block()
    }

    /// Whether this block can be outdented from its list context.
    pub fn can_outdent_list_nesting(&self) -> bool {
        self.kind().is_list_item() || self.parent_is_list_item
    }

    /// Returns the current display text length in UTF-8 bytes.
    pub(crate) fn visible_len(&self) -> usize {
        self.current_cache().visible_len()
    }

    /// Splits the clean title at a current-display UTF-8 offset.
    pub(crate) fn split_title(&self, offset: usize) -> (InlineTextTree, InlineTextTree) {
        self.record
            .title
            .split_at(self.current_to_clean_offset(offset))
    }

    /// Forgets the preferred x position after nonvertical movement.
    fn clear_vertical_motion(&mut self) {
        self.vertical_motion_x = None;
    }

    /// Rebuilds clean caches while preserving selection, IME mark, and projection.
    pub(crate) fn sync_render_cache(&mut self) {
        let clean_selected = self.current_to_clean_range(self.selected_range.clone());
        let clean_marked = self
            .marked_range
            .clone()
            .map(|range| self.current_to_clean_range(range));
        let (clean_anchor, clean_focus) = self.clean_selection_anchor_focus();
        let (anchor_affinity, focus_affinity) = self.selection_endpoint_affinities();
        let collapsed_affinity = self.current_collapsed_caret_affinity();
        let keep_projection =
            self.projection.is_some() && self.edit_mode.supports_inline_projection();
        self.render_cache = self.record.title.render_cache();
        self.sync_code_highlight();
        self.sync_image_runtime();
        self.projection = None;
        self.projection_cache_key = None;
        if keep_projection {
            self.rebuild_inline_projection(clean_selected.clone(), clean_marked.clone());
            if clean_selected.is_empty() {
                let offset = self.clean_to_current_cursor_offset_with_affinity(
                    clean_selected.start,
                    collapsed_affinity,
                );
                self.assign_collapsed_selection_offset(offset, collapsed_affinity, None);
            } else {
                self.set_selection_from_clean_anchor_focus(
                    clean_anchor,
                    clean_focus,
                    anchor_affinity,
                    focus_affinity,
                );
                self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
            }
            self.marked_range = clean_marked.map(|range| self.clean_to_current_range(range));
        } else {
            self.set_selection_from_anchor_focus(clean_anchor, clean_focus);
            self.marked_range = clean_marked;
            self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
        }
        self.refresh_cached_display_text();
    }

    /// Rebinds link references and restores selection through Markdown offsets.
    fn sync_link_reference_definitions(
        &mut self,
        link_reference_definitions: Arc<LinkReferenceDefinitions>,
    ) {
        if self.link_reference_definitions == link_reference_definitions {
            return;
        }

        let selected_markdown = (!self.uses_raw_text_editing())
            .then(|| self.current_range_to_markdown_range(self.selected_range.clone()));
        let marked_markdown = (!self.uses_raw_text_editing())
            .then(|| {
                self.marked_range
                    .clone()
                    .map(|range| self.current_range_to_markdown_range(range))
            })
            .flatten();
        let selection_reversed = self.selection_reversed;
        let collapsed_affinity = self.current_collapsed_caret_affinity();
        let had_projection = self.projection.is_some();

        self.link_reference_definitions = link_reference_definitions;
        if self.uses_raw_text_editing() {
            return;
        }

        let markdown = self.record.title.serialize_markdown();
        let next_title = InlineTextTree::from_markdown_with_link_references(
            &markdown,
            &self.link_reference_definitions,
        );
        if self.record.title == next_title {
            return;
        }

        self.record.set_title(next_title);
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();

        if let Some(selected_markdown) = selected_markdown {
            let restored = self.markdown_range_to_current_range(selected_markdown);
            if restored.is_empty() {
                self.assign_collapsed_selection_offset(
                    restored.start,
                    collapsed_affinity,
                    self.vertical_motion_x,
                );
            } else {
                self.selected_range = restored;
                self.selection_reversed = selection_reversed;
                self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
            }
        }

        self.marked_range =
            marked_markdown.map(|range| self.markdown_range_to_current_range(range));

        if had_projection {
            self.sync_inline_projection_for_focus(true);
        }
    }

    /// Rebinds footnote occurrences and restores selection through Markdown offsets.
    fn sync_footnote_registry(&mut self, footnote_registry: Arc<FootnoteRegistry>) {
        if self.footnote_registry == footnote_registry {
            return;
        }

        let selected_markdown = (!self.uses_raw_text_editing())
            .then(|| self.current_range_to_markdown_range(self.selected_range.clone()));
        let marked_markdown = (!self.uses_raw_text_editing())
            .then(|| {
                self.marked_range
                    .clone()
                    .map(|range| self.current_range_to_markdown_range(range))
            })
            .flatten();
        let selection_reversed = self.selection_reversed;
        let collapsed_affinity = self.current_collapsed_caret_affinity();
        let had_projection = self.projection.is_some();

        self.footnote_registry = footnote_registry;
        if self.uses_raw_text_editing() || !self.record.title.has_footnote_references() {
            return;
        }

        let mut next_title = self.record.title.clone();
        let mut occurrence_iter = self
            .footnote_registry
            .occurrences_for_block(self.record.id)
            .unwrap_or(&[])
            .iter();
        next_title.apply_footnote_reference_state(|id| {
            let occurrence = occurrence_iter.next()?;
            if occurrence.id != id {
                return None;
            }
            Some((occurrence.ordinal?, occurrence.occurrence_index))
        });
        if self.record.title == next_title {
            return;
        }

        self.record.set_title(next_title);
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();

        if let Some(selected_markdown) = selected_markdown {
            let restored = self.markdown_range_to_current_range(selected_markdown);
            if restored.is_empty() {
                self.assign_collapsed_selection_offset(
                    restored.start,
                    collapsed_affinity,
                    self.vertical_motion_x,
                );
            } else {
                self.selected_range = restored;
                self.selection_reversed = selection_reversed;
                self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
            }
        }

        self.marked_range =
            marked_markdown.map(|range| self.markdown_range_to_current_range(range));

        if had_projection {
            self.sync_inline_projection_for_focus(true);
        }
    }

    /// Whether source-preserving links require edits in serialized Markdown.
    fn should_use_markdown_space_link_edit(&self) -> bool {
        !self.uses_raw_text_editing() && self.record.title.has_source_preserving_links()
    }

    /// Applies a visible edit through serialized Markdown to preserve link syntax.
    fn apply_markdown_space_title_edit(
        &mut self,
        visible_range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        let old_visible_len = self.record.title.visible_text().len();
        let markdown_range = self.current_range_to_markdown_range(visible_range.clone());
        let mut markdown = self.record.title.serialize_markdown();
        let replaced_text = markdown[markdown_range.clone()].to_string();
        markdown.replace_range(markdown_range.clone(), new_text);

        let next_title = InlineTextTree::from_markdown_with_link_references(
            &markdown,
            &self.link_reference_definitions,
        );
        let map = next_title.markdown_offset_map();
        let selected_markdown = selected_range_relative.as_ref().map(|relative| {
            markdown_range.start + relative.start..markdown_range.start + relative.end
        });
        let cursor_markdown = selected_markdown
            .as_ref()
            .map(|range| range.end)
            .unwrap_or(markdown_range.start + new_text.len());
        let marked_markdown = if mark_inserted_text && !new_text.is_empty() {
            Some(markdown_range.start..markdown_range.start + new_text.len())
        } else {
            None
        };
        let selected_clean = selected_markdown
            .as_ref()
            .map(|range| map.markdown_to_visible_range(range.clone()));
        let marked_clean = marked_markdown
            .as_ref()
            .map(|range| map.markdown_to_visible_range(range.clone()));
        let cursor_clean = map.markdown_to_visible_offset(cursor_markdown);

        let quote_structure_edit = self.quote_depth > 0
            && (new_text.contains('\n')
                || replaced_text.contains('\n')
                || (self.kind() == BlockKind::Quote
                    && Self::multiline_quote_edit_requires_reparse(&next_title.visible_text())));
        if quote_structure_edit {
            self.quote_reparse_requested = true;
        }

        // Typing a closing marker (for example the `)` that completes a link)
        // absorbs that markup into a span, so the clean text grows by less than
        // the inserted text. Flag it so the caret is placed just past the new
        // closing delimiter instead of landing inside the span.
        let caret_may_have_closed_span = !new_text.is_empty()
            && !mark_inserted_text
            && next_title.visible_text().len() < old_visible_len + new_text.len();

        self.apply_title_edit(
            next_title,
            cursor_clean,
            marked_clean,
            selected_clean.clone(),
            selected_clean
                .as_ref()
                .and_then(|range| (!range.is_empty()).then_some(false)),
            caret_may_have_closed_span,
            cx,
        );
    }

    /// Returns the projected display cache, or the clean render cache.
    pub(crate) fn current_cache(&self) -> &InlineRenderCache {
        self.projection
            .as_ref()
            .map(|projection| &projection.cache)
            .unwrap_or(&self.render_cache)
    }

    /// Rebuilds touched inline syntax while focused, preserving link selections.
    ///
    /// Reuses the projection when clean selection and mark are unchanged. A
    /// projected link selection keeps its display-relative byte offsets and is
    /// normalized against the rebuilt display text.
    pub(crate) fn sync_inline_projection_for_focus(&mut self, focused: bool) {
        let supports_projection = self.edit_mode.supports_inline_projection();
        if !focused || !supports_projection {
            self.clear_inline_projection();
            return;
        }

        let projected_link_selection = self.projection.as_ref().and_then(|projection| {
            projection
                .link_run_fully_covering_range(&self.selected_range)
                .map(|run| ProjectedLinkSelectionSnapshot {
                    clean_range: run.clean_range.clone(),
                    display_relative_range: self
                        .selected_range
                        .start
                        .saturating_sub(run.display_range.start)
                        ..self
                            .selected_range
                            .end
                            .saturating_sub(run.display_range.start),
                    selection_reversed: self.selection_reversed,
                })
        });
        let clean_selected = self.current_to_clean_range(self.selected_range.clone());
        let clean_marked = self
            .marked_range
            .clone()
            .map(|range| self.current_to_clean_range(range));
        if self.projection_cache_key.as_ref()
            == Some(&(
                supports_projection,
                clean_selected.clone(),
                clean_marked.clone(),
            ))
        {
            return;
        }
        let (clean_anchor, clean_focus) = self.clean_selection_anchor_focus();
        let (anchor_affinity, focus_affinity) = self.selection_endpoint_affinities();
        let collapsed_affinity = self.current_collapsed_caret_affinity();
        self.rebuild_inline_projection(clean_selected.clone(), clean_marked.clone());
        if let Some(snapshot) = projected_link_selection
            && let Some(run) = self
                .projection
                .as_ref()
                .and_then(|projection| projection.link_run_for_clean_range(&snapshot.clean_range))
        {
            let start = run.display_range.start
                + snapshot
                    .display_relative_range
                    .start
                    .min(run.display_range.len());
            let end = run.display_range.start
                + snapshot
                    .display_relative_range
                    .end
                    .min(run.display_range.len());
            self.selected_range =
                crate::text_offsets::normalize_utf8_range(self.display_text(), start..end);
            self.selection_reversed = snapshot.selection_reversed;
            self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
        } else if clean_selected.is_empty() {
            let offset = self.clean_to_current_cursor_offset_with_affinity(
                clean_selected.start,
                collapsed_affinity,
            );
            self.assign_collapsed_selection_offset(offset, collapsed_affinity, None);
        } else {
            self.set_selection_from_clean_anchor_focus(
                clean_anchor,
                clean_focus,
                anchor_affinity,
                focus_affinity,
            );
            self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
        }
        self.marked_range = clean_marked.map(|range| self.clean_to_current_range(range));
    }

    /// Returns to clean text while retaining the selection and IME mark.
    pub(crate) fn clear_inline_projection(&mut self) {
        if self.projection.is_none() {
            self.projection_cache_key = None;
            return;
        }

        let clean_marked = self
            .marked_range
            .clone()
            .map(|range| self.current_to_clean_range(range));
        let (clean_anchor, clean_focus) = self.clean_selection_anchor_focus();
        self.projection = None;
        self.projection_cache_key = None;
        self.set_selection_from_anchor_focus(clean_anchor, clean_focus);
        self.marked_range = clean_marked;
        self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
        self.refresh_cached_display_text();
    }

    /// Recomputes the temporary display and cache key from clean UTF-8 ranges.
    fn rebuild_inline_projection(
        &mut self,
        clean_selected: Range<usize>,
        clean_marked: Option<Range<usize>>,
    ) {
        self.projection_cache_key = Some((
            self.edit_mode.supports_inline_projection(),
            clean_selected.clone(),
            clean_marked.clone(),
        ));
        self.projection = ExpandedInlineProjection::build(
            &self.record.title.fragments,
            clean_selected,
            clean_marked,
        );
        self.refresh_cached_display_text();
    }

    /// Returns projected display segments, or an empty slice when collapsed.
    fn projection_segments(&self) -> &[ExpandedInlineSegment] {
        self.projection
            .as_ref()
            .map(|projection| projection.segments.as_slice())
            .unwrap_or(&[])
    }

    /// Finds a projected link enclosing a current-display byte range.
    fn projected_link_run_fully_covering_range(
        &self,
        range: &Range<usize>,
    ) -> Option<&ExpandedLinkRun> {
        self.projection
            .as_ref()
            .and_then(|projection| projection.link_run_fully_covering_range(range))
    }

    /// Determines which side of a projected delimiter owns this display caret.
    fn collapsed_caret_affinity_for_display_offset(&self, offset: usize) -> CollapsedCaretAffinity {
        self.projection
            .as_ref()
            .map(|projection| projection.collapsed_affinity_for_display_offset(offset))
            .unwrap_or(CollapsedCaretAffinity::Default)
    }

    /// Affinity of the current selection's anchor and focus, used to restore
    /// each endpoint accurately when the projection is rebuilt.
    fn selection_endpoint_affinities(&self) -> (CollapsedCaretAffinity, CollapsedCaretAffinity) {
        let (anchor, focus) = self.selection_anchor_focus();
        (
            self.collapsed_caret_affinity_for_display_offset(anchor),
            self.collapsed_caret_affinity_for_display_offset(focus),
        )
    }

    /// Reads caret affinity from projection geometry or the stored fallback.
    fn current_collapsed_caret_affinity(&self) -> CollapsedCaretAffinity {
        if !self.selected_range.is_empty() {
            return CollapsedCaretAffinity::Default;
        }

        self.projection
            .as_ref()
            .map(|projection| {
                projection.collapsed_affinity_for_display_offset(self.cursor_offset())
            })
            .unwrap_or(self.collapsed_caret_affinity)
    }

    /// Refreshes stored affinity after a selection change.
    fn sync_collapsed_caret_affinity(&mut self) {
        self.collapsed_caret_affinity = if self.selected_range.is_empty() {
            self.projection
                .as_ref()
                .map(|projection| {
                    projection.collapsed_affinity_for_display_offset(self.cursor_offset())
                })
                .unwrap_or(CollapsedCaretAffinity::Default)
        } else {
            CollapsedCaretAffinity::Default
        };
    }

    /// Sets a scalar-aligned display caret and its boundary affinity.
    pub(crate) fn assign_collapsed_selection_offset(
        &mut self,
        offset: usize,
        affinity: CollapsedCaretAffinity,
        preferred_x: Option<Pixels>,
    ) {
        let clamped_offset = crate::text_offsets::floor_char_boundary(self.display_text(), offset);
        self.selected_range = clamped_offset..clamped_offset;
        self.selection_reversed = false;
        self.vertical_motion_x = preferred_x;
        self.collapsed_caret_affinity = affinity;
        self.sync_collapsed_caret_affinity();
    }

    /// Maps a clean UTF-8 caret to the default current-display position.
    fn clean_to_current_cursor_offset(&self, clean: usize) -> usize {
        let Some(projection) = &self.projection else {
            return clean;
        };
        projection
            .clean_to_display_cursor
            .get(clean.min(projection.clean_to_display_cursor.len().saturating_sub(1)))
            .copied()
            .unwrap_or(clean)
    }

    /// Maps a clean caret to display space, honoring delimiter-side affinity.
    fn clean_to_current_cursor_offset_with_affinity(
        &self,
        clean: usize,
        affinity: CollapsedCaretAffinity,
    ) -> usize {
        let Some(projection) = &self.projection else {
            return clean;
        };
        projection
            .display_offset_for_clean_cursor(clean, affinity)
            .unwrap_or_else(|| self.clean_to_current_cursor_offset(clean))
    }

    /// Maps a clean range start to current-display byte coordinates.
    fn clean_to_current_range_start(&self, clean: usize) -> usize {
        self.clean_to_current_cursor_offset(clean)
    }

    /// Maps a clean range end to current-display byte coordinates.
    fn clean_to_current_range_end(&self, clean: usize) -> usize {
        self.clean_to_current_cursor_offset(clean)
    }

    /// Maps and scalar-normalizes a clean visible-text range into display space.
    pub(crate) fn clean_to_current_range(&self, range: Range<usize>) -> Range<usize> {
        let range =
            crate::text_offsets::normalize_utf8_range(self.render_cache.visible_text(), range);
        if range.is_empty() {
            let offset = self.clean_to_current_cursor_offset(range.start);
            let offset = crate::text_offsets::floor_char_boundary(self.display_text(), offset);
            offset..offset
        } else {
            crate::text_offsets::normalize_utf8_range(
                self.display_text(),
                self.clean_to_current_range_start(range.start)
                    ..self.clean_to_current_range_end(range.end),
            )
        }
    }

    /// Maps a display range to scalar-aligned clean visible-text bytes.
    pub(crate) fn current_to_clean_range(&self, range: Range<usize>) -> Range<usize> {
        let range = crate::text_offsets::normalize_utf8_range(self.display_text(), range);
        crate::text_offsets::normalize_utf8_range(
            self.render_cache.visible_text(),
            self.current_to_clean_offset(range.start)..self.current_to_clean_offset(range.end),
        )
    }

    /// Maps a display byte offset to a scalar-aligned clean byte offset.
    pub(crate) fn current_to_clean_offset(&self, offset: usize) -> usize {
        let offset = crate::text_offsets::floor_char_boundary(self.display_text(), offset);
        crate::text_offsets::floor_char_boundary(
            self.render_cache.visible_text(),
            self.unexpand_offset(offset),
        )
    }

    #[allow(dead_code)]
    /// Snaps projected delimiter pointer hits to editable boundaries.
    pub(crate) fn pointer_target_offset(&self, offset: usize) -> usize {
        self.projection
            .as_ref()
            .map(|projection| projection.pointer_target_offset(offset))
            .unwrap_or(offset)
    }

    /// Returns the left-arrow jump across a projected delimiter, if any.
    pub(crate) fn projected_move_left_target(
        &self,
        offset: usize,
    ) -> Option<(usize, CollapsedCaretAffinity)> {
        self.projection
            .as_ref()
            .and_then(|projection| projection.move_left_target(offset))
    }

    /// Returns the right-arrow jump across a projected delimiter, if any.
    pub(crate) fn projected_move_right_target(
        &self,
        offset: usize,
    ) -> Option<(usize, CollapsedCaretAffinity)> {
        self.projection
            .as_ref()
            .and_then(|projection| projection.move_right_target(offset))
    }

    /// Returns the body selection in clean visible-text byte coordinates.
    pub(crate) fn selection_clean_range(&self) -> Range<usize> {
        self.current_to_clean_range(self.selected_range.clone())
    }

    /// Maps a display byte range into serialized Markdown byte coordinates.
    ///
    /// Expanded links and footnotes use their projected source runs; raw edit
    /// modes already share the Markdown and display coordinate space.
    pub(crate) fn current_range_to_markdown_range(&self, range: Range<usize>) -> Range<usize> {
        if self.uses_raw_text_editing() || self.kind().is_code_block() {
            return crate::text_offsets::normalize_utf8_range(self.display_text(), range);
        }

        if let Some(link_run) = self.projected_link_run_fully_covering_range(&range) {
            let map = self.record.title.markdown_offset_map();
            let label_markdown_start = map.visible_to_markdown_offset(link_run.clean_range.start);
            let run_markdown_start =
                label_markdown_start.saturating_sub(link_run.link.open_marker().len());
            let start = run_markdown_start
                + range
                    .start
                    .saturating_sub(link_run.display_range.start)
                    .min(link_run.display_range.len());
            let end = run_markdown_start
                + range
                    .end
                    .saturating_sub(link_run.display_range.start)
                    .min(link_run.display_range.len());
            return start..end;
        }

        if let Some(footnote_run) = self
            .projection
            .as_ref()
            .and_then(|projection| projection.footnote_run_fully_covering_range(&range))
        {
            let raw = footnote_run.footnote.raw_markdown();
            let raw_len = raw.len();
            let local_start = range
                .start
                .saturating_sub(footnote_run.display_range.start)
                .min(footnote_run.display_range.len());
            let local_end = range
                .end
                .saturating_sub(footnote_run.display_range.start)
                .min(footnote_run.display_range.len());
            let mapped_start = (raw_len * local_start) / footnote_run.display_range.len().max(1);
            let mapped_end = (raw_len * local_end) / footnote_run.display_range.len().max(1);
            let map = self.record.title.markdown_offset_map();
            let run_markdown_start = map.visible_to_markdown_offset(footnote_run.clean_range.start);
            let mapped = crate::text_offsets::normalize_utf8_range(&raw, mapped_start..mapped_end);
            return run_markdown_start + mapped.start..run_markdown_start + mapped.end;
        }

        let clean_range = self.current_to_clean_range(range);
        self.record
            .title
            .markdown_offset_map()
            .visible_to_markdown_range(clean_range)
    }

    /// Maps a Markdown byte range into current display coordinates.
    pub(crate) fn markdown_range_to_current_range(&self, range: Range<usize>) -> Range<usize> {
        if self.uses_raw_text_editing() || self.kind().is_code_block() {
            return crate::text_offsets::normalize_utf8_range(self.display_text(), range);
        }

        let clean_range = self
            .record
            .title
            .markdown_offset_map()
            .markdown_to_visible_range(range);
        self.clean_to_current_range(clean_range)
    }

    /// Maps a Markdown byte offset to a current-display caret position.
    pub(crate) fn markdown_offset_to_current_offset(&self, offset: usize) -> usize {
        self.markdown_range_to_current_range(offset..offset).start
    }

    /// Asks the editor to capture undo state before a block edit.
    pub(crate) fn prepare_undo_capture(&self, kind: UndoCaptureKind, cx: &mut Context<Self>) {
        cx.emit(BlockEvent::PrepareUndo { kind });
    }

    /// Converts a UTF-8 byte position to UTF-16 after flooring to a scalar boundary.
    pub(super) fn utf8_to_utf16_in(text: &str, offset: usize) -> usize {
        crate::text_offsets::utf8_to_utf16(text, offset)
    }

    /// Converts an IME range to UTF-8 bytes without splitting Unicode scalars.
    pub(super) fn utf16_range_to_utf8_in(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
        crate::text_offsets::utf16_range_to_utf8(text, range_utf16)
    }

    /// Converts an internal byte range to an ordered UTF-16 range.
    pub(super) fn utf8_range_to_utf16_in(text: &str, range: &Range<usize>) -> Range<usize> {
        crate::text_offsets::utf8_range_to_utf16(text, range)
    }

    /// Detect Markdown shortcut prefixes in the edited title and convert the
    /// block's kind accordingly (e.g. `"- " -> BulletedListItem`).
    ///
    /// Applies general shortcuts to paragraphs and task-list prefixes to
    /// bulleted items, but never to table cells or raw text. Returns the kind,
    /// title, cursor, and removed prefix length in UTF-8 bytes.
    fn normalize_after_title_edit(
        &self,
        mut next_title: InlineTextTree,
        cursor: usize,
    ) -> (BlockKind, InlineTextTree, usize, usize) {
        if self.is_table_cell() {
            return (self.kind(), next_title, cursor, 0);
        }

        if !self.uses_raw_text_editing() && self.kind() == BlockKind::Paragraph {
            let visible_text = next_title.visible_text();
            if let Some((kind, prefix_len)) = BlockKind::detect_markdown_shortcut(&visible_text) {
                next_title.remove_visible_prefix(prefix_len);
                return (
                    kind,
                    next_title,
                    cursor.saturating_sub(prefix_len),
                    prefix_len,
                );
            }
        }

        if !self.uses_raw_text_editing() && self.kind() == BlockKind::BulletedListItem {
            let visible_text = next_title.visible_text();
            if let Some((checked, prefix_len)) =
                BlockKind::parse_task_list_item_prefix(&visible_text)
            {
                next_title.remove_visible_prefix(prefix_len);
                return (
                    BlockKind::TaskListItem { checked },
                    next_title,
                    cursor.saturating_sub(prefix_len),
                    prefix_len,
                );
            }
        }

        (self.kind(), next_title, cursor, 0)
    }

    /// Whether a quote line begins syntax requiring a structural reparse.
    fn quote_line_starts_block_syntax(line: &str) -> bool {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            return false;
        }

        let leading_spaces = trimmed_end.bytes().take_while(|b| *b == b' ').count();
        if leading_spaces >= 4 {
            return true;
        }

        BlockKind::detect_markdown_shortcut(&format!("{trimmed_end} "))
            .is_some_and(|(kind, _)| kind != BlockKind::Paragraph)
            || BlockKind::parse_code_fence_opening(trimmed_end).is_some()
            || BlockKind::parse_separator_line(trimmed_end)
            || BlockKind::parse_atx_heading_line(trimmed_end).is_some()
    }

    /// Checks lines after the first for block syntax introduced inside a quote.
    fn multiline_quote_edit_requires_reparse(text: &str) -> bool {
        text.split('\n')
            .skip(1)
            .any(Self::quote_line_starts_block_syntax)
    }

    /// Shifts a UTF-8 range left by a consumed shortcut prefix.
    fn adjust_range_for_shortcut(range: &Range<usize>, removed_prefix_len: usize) -> Range<usize> {
        range.start.saturating_sub(removed_prefix_len)..range.end.saturating_sub(removed_prefix_len)
    }

    /// Collects styles whose visible delimiters intersect a replacement range.
    fn projected_styles_touching_display_range(
        &self,
        display_range: &Range<usize>,
    ) -> Vec<(usize, StyleFlag)> {
        let mut targets = Vec::new();
        for segment in self.projection_segments() {
            let touches = display_range.start < segment.display_range.end
                && segment.display_range.start < display_range.end;
            if touches
                && matches!(
                    segment.kind,
                    ExpandedInlineSegmentKind::OpeningDelimiter(_)
                        | ExpandedInlineSegmentKind::ClosingDelimiter(_)
                )
            {
                let kind = match segment.kind {
                    ExpandedInlineSegmentKind::OpeningDelimiter(kind)
                    | ExpandedInlineSegmentKind::ClosingDelimiter(kind) => kind,
                    _ => continue,
                };
                if let Some(flag) = kind.style_flag() {
                    let target = (segment.fragment_index, flag);
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
            }
        }
        targets
    }

    /// Counts clean visible-text bytes before a source fragment.
    fn clean_offset_before_fragment_index(fragments: &[InlineFragment], index: usize) -> usize {
        fragments
            .iter()
            .take(index)
            .map(|fragment| fragment.text.len())
            .sum()
    }

    /// Whether every replacement fragment belongs to the same link.
    fn replacement_is_pure_link_run(fragments: &[InlineFragment]) -> bool {
        let Some(first_link) = fragments
            .first()
            .and_then(|fragment| fragment.link.as_ref())
        else {
            return false;
        };

        fragments
            .iter()
            .all(|fragment| fragment.link.as_ref() == Some(first_link))
    }

    /// Edits an expanded inline link while preserving its source syntax.
    ///
    /// A replacement that remains one link keeps the projected selection;
    /// other replacements fall back to the clean title-edit path.
    fn apply_link_projection_edit(
        &mut self,
        link_run: &ExpandedLinkRun,
        visible_range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        let local_visible_range = visible_range.start - link_run.display_range.start
            ..visible_range.end - link_run.display_range.start;
        let local_display_text = self.display_text()[link_run.display_range.clone()].to_string();
        let local_tree = InlineTextTree::plain(local_display_text);
        let local_result = local_tree.replace_visible_range_with_link_references(
            local_visible_range.clone(),
            new_text,
            InlineInsertionAttributes::default(),
            &self.link_reference_definitions,
        );
        let replacement_fragments = local_result.tree.fragments.clone();

        let replacement_start = link_run.start_fragment_index;
        let replacement_clean_start = Self::clean_offset_before_fragment_index(
            &self.record.title.fragments,
            replacement_start,
        );
        let mut next_title = self.record.title.clone();
        next_title.replace_fragment_range(
            link_run.start_fragment_index..link_run.end_fragment_index,
            replacement_fragments.clone(),
        );

        if Self::replacement_is_pure_link_run(&replacement_fragments) {
            let old_kind = self.record.kind.clone();
            let old_title = self.record.title.clone();
            self.record.set_title(next_title.clone());
            self.sync_edit_mode_from_kind();
            self.sync_render_cache();

            let replacement_visible_len = replacement_fragments
                .iter()
                .map(|fragment| fragment.text.len())
                .sum::<usize>();
            let selected_clean =
                replacement_clean_start..replacement_clean_start + replacement_visible_len;
            self.rebuild_inline_projection(selected_clean.clone(), None);

            let local_selected = selected_range_relative.clone().unwrap_or_else(|| {
                let cursor = local_visible_range.start + new_text.len();
                cursor..cursor
            });
            if let Some(projected_link_run) = self.projection.as_ref().and_then(|projection| {
                projection
                    .link_runs
                    .iter()
                    .find(|run| run.clean_range == selected_clean)
            }) {
                let start = projected_link_run.display_range.start
                    + local_selected
                        .start
                        .min(projected_link_run.display_range.len());
                let end = projected_link_run.display_range.start
                    + local_selected
                        .end
                        .min(projected_link_run.display_range.len());
                self.selected_range =
                    crate::text_offsets::normalize_utf8_range(self.display_text(), start..end);
                self.selection_reversed = false;
                self.marked_range = if mark_inserted_text && !new_text.is_empty() {
                    Some(self.selected_range.clone())
                } else {
                    None
                };
                self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
                self.cursor_blink_epoch = Instant::now();
                self.clear_vertical_motion();
                if self.record.kind != old_kind || self.record.title != old_title {
                    cx.emit(BlockEvent::Changed);
                }
                cx.notify();
                return;
            }
        }

        let local_selected = selected_range_relative.as_ref().map(|relative| {
            let absolute = local_visible_range.start + relative.start
                ..local_visible_range.start + relative.end;
            local_result.map_range(&absolute)
        });
        let cursor = local_selected
            .as_ref()
            .map(|range| range.end)
            .unwrap_or_else(|| local_result.map_offset(local_visible_range.start + new_text.len()));
        let prefix = replacement_clean_start;
        let selected_clean = local_selected.map(|range| prefix + range.start..prefix + range.end);
        let marked_clean = if mark_inserted_text && !new_text.is_empty() {
            let inserted_range =
                local_visible_range.start..local_visible_range.start + new_text.len();
            let mapped = local_result.map_range(&inserted_range);
            Some(prefix + mapped.start..prefix + mapped.end)
        } else {
            None
        };
        self.apply_title_edit(
            next_title,
            prefix + cursor,
            marked_clean,
            selected_clean.clone(),
            selected_clean
                .as_ref()
                .and_then(|range| (!range.is_empty()).then_some(false)),
            false,
            cx,
        );
    }

    /// Chooses rich-text attributes for insertion at a display byte offset.
    ///
    /// Delimiters use their adjacent span inside the style and plain attributes
    /// just outside it; link targets also use plain attributes.
    fn insertion_attributes_for_current_offset(
        &self,
        current_offset: usize,
    ) -> InlineInsertionAttributes {
        if self.uses_raw_text_editing() {
            return InlineInsertionAttributes::default();
        }

        if self.projection.is_none() {
            return self
                .record
                .title
                .attributes_for_insertion_at(current_offset);
        }

        for segment in self.projection_segments() {
            match segment.kind {
                ExpandedInlineSegmentKind::StyledText
                    if current_offset >= segment.display_range.start
                        && current_offset <= segment.display_range.end =>
                {
                    let fragment = &self.record.title.fragments[segment.fragment_index];
                    return InlineInsertionAttributes {
                        style: fragment.style,
                        html_style: fragment.html_style,
                        link: fragment.link.clone(),
                        footnote: fragment.footnote.clone(),
                        math: None,
                    };
                }
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if current_offset == segment.display_range.end =>
                {
                    let fragment = &self.record.title.fragments[segment.fragment_index];
                    return InlineInsertionAttributes {
                        style: fragment.style,
                        html_style: fragment.html_style,
                        link: fragment.link.clone(),
                        footnote: fragment.footnote.clone(),
                        math: None,
                    };
                }
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if current_offset == segment.display_range.start =>
                {
                    let fragment = &self.record.title.fragments[segment.fragment_index];
                    return InlineInsertionAttributes {
                        style: fragment.style,
                        html_style: fragment.html_style,
                        link: fragment.link.clone(),
                        footnote: fragment.footnote.clone(),
                        math: None,
                    };
                }
                // Caret just outside a span: after a closing delimiter or before
                // an opening one. Insert plain text so it isn't absorbed back into
                // the span, matching how code and strikethrough already behave.
                ExpandedInlineSegmentKind::ClosingDelimiter(_)
                    if current_offset == segment.display_range.end =>
                {
                    return InlineInsertionAttributes::default();
                }
                ExpandedInlineSegmentKind::OpeningDelimiter(_)
                    if current_offset == segment.display_range.start =>
                {
                    return InlineInsertionAttributes::default();
                }
                ExpandedInlineSegmentKind::LinkTargetText => {
                    if let Some(link_group) = segment.link_group
                        && let Some(link_run) = self
                            .projection
                            .as_ref()
                            .and_then(|projection| projection.link_runs.get(link_group))
                        && current_offset >= link_run.target_display_range.start
                        && current_offset <= link_run.target_display_range.end
                    {
                        return InlineInsertionAttributes::default();
                    }
                }
                _ => {}
            }
        }

        self.record
            .title
            .attributes_for_insertion_at(self.current_to_clean_offset(current_offset))
    }

    /// Copies insertion style, link, and footnote metadata from a fragment.
    fn attributes_for_fragment(fragment: &InlineFragment) -> InlineInsertionAttributes {
        InlineInsertionAttributes {
            style: fragment.style,
            html_style: fragment.html_style,
            link: fragment.link.clone(),
            footnote: fragment.footnote.clone(),
            math: None,
        }
    }

    /// Chooses insertion attributes for a display-space replacement.
    fn replacement_attributes_for_visible_range(
        &self,
        visible_range: &Range<usize>,
    ) -> InlineInsertionAttributes {
        if self.uses_raw_text_editing() {
            return InlineInsertionAttributes::default();
        }

        if visible_range.is_empty() {
            return self.insertion_attributes_for_current_offset(visible_range.start);
        }

        if self.projection.is_some() {
            return self
                .projected_replacement_attributes_for_visible_range(visible_range)
                .unwrap_or_default();
        }

        self.fragment_attributes_for_clean_range(self.current_to_clean_range(visible_range.clone()))
            .unwrap_or_default()
    }

    /// Inherits attributes only from a single containing styled display span.
    fn projected_replacement_attributes_for_visible_range(
        &self,
        visible_range: &Range<usize>,
    ) -> Option<InlineInsertionAttributes> {
        self.projection_segments().iter().find_map(|segment| {
            (segment.kind == ExpandedInlineSegmentKind::StyledText
                && segment.display_range.start <= visible_range.start
                && visible_range.end <= segment.display_range.end)
                .then(|| {
                    self.record
                        .title
                        .fragments
                        .get(segment.fragment_index)
                        .map(Self::attributes_for_fragment)
                })
                .flatten()
        })
    }

    /// Inherits attributes only from one fragment containing a clean range.
    fn fragment_attributes_for_clean_range(
        &self,
        clean_range: Range<usize>,
    ) -> Option<InlineInsertionAttributes> {
        if clean_range.is_empty() {
            return None;
        }

        let mut cursor = 0usize;
        for fragment in &self.record.title.fragments {
            let fragment_start = cursor;
            let fragment_end = fragment_start + fragment.text.len();
            if fragment_start <= clean_range.start && clean_range.end <= fragment_end {
                return Some(Self::attributes_for_fragment(fragment));
            }
            cursor = fragment_end;
        }

        None
    }

    /// Whether the collapsed caret would insert inline-code text.
    pub(super) fn collapsed_caret_inherits_inline_code_style(&self) -> bool {
        self.selected_range.is_empty()
            && !self.uses_raw_text_editing()
            && self
                .insertion_attributes_for_current_offset(self.cursor_offset())
                .style
                .code
    }

    /// Apply a new title to the block, running shortcut detection and
    /// updating the render cache, cursor, and selection state.  Emits
    /// [`BlockEvent::Changed`] if the kind or title actually changed.
    pub(super) fn apply_title_edit(
        &mut self,
        next_title: InlineTextTree,
        cursor_clean: usize,
        marked_range_clean: Option<Range<usize>>,
        selected_range_clean: Option<Range<usize>>,
        selected_range_reversed: Option<bool>,
        caret_may_have_closed_span: bool,
        cx: &mut Context<Self>,
    ) {
        let old_kind = self.record.kind.clone();
        let old_title = self.record.title.clone();
        let old_title_was_empty = old_title.visible_text().is_empty();
        let mut collapsed_affinity = self.current_collapsed_caret_affinity();
        let keep_projection =
            self.projection.is_some() && self.edit_mode.supports_inline_projection();

        let (next_kind, normalized_title, adjusted_cursor, shortcut_removed_len) =
            self.normalize_after_title_edit(next_title, cursor_clean);
        let should_restart_numbered_list = old_kind == BlockKind::Paragraph
            && old_title_was_empty
            && self.list_group_separator_candidate
            && next_kind == BlockKind::NumberedListItem;

        let next_marked_clean = marked_range_clean
            .as_ref()
            .map(|range| Self::adjust_range_for_shortcut(range, shortcut_removed_len));
        let next_selected_clean = selected_range_clean
            .as_ref()
            .map(|range| Self::adjust_range_for_shortcut(range, shortcut_removed_len))
            .unwrap_or_else(|| adjusted_cursor..adjusted_cursor);

        self.record.kind = next_kind;
        self.record.set_title(normalized_title);
        self.numbered_list_restart_requested = should_restart_numbered_list;
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        // Rebuild when a projection already existed, or when this edit may have
        // closed a delimiter, creating a span whose markers now need projecting.
        if self.edit_mode.supports_inline_projection()
            && (keep_projection || caret_may_have_closed_span)
        {
            self.rebuild_inline_projection(next_selected_clean.clone(), next_marked_clean.clone());
        }

        // If the edit closed a span (its delimiters were absorbed), place the
        // caret after the new closing marker so typing continues as plain text.
        if caret_may_have_closed_span
            && next_selected_clean.is_empty()
            && self.projection.as_ref().is_some_and(|projection| {
                projection.caret_closes_span_at_clean(next_selected_clean.start)
            })
        {
            collapsed_affinity = CollapsedCaretAffinity::OuterEnd;
        }

        self.marked_range = next_marked_clean
            .clone()
            .map(|range| self.clean_to_current_range(range));
        if next_selected_clean.is_empty() {
            let offset = self.clean_to_current_cursor_offset_with_affinity(
                next_selected_clean.start,
                collapsed_affinity,
            );
            self.assign_collapsed_selection_offset(offset, collapsed_affinity, None);
        } else {
            self.selected_range = self.clean_to_current_range(next_selected_clean);
            self.selection_reversed = selected_range_reversed.unwrap_or(self.selection_reversed);
            self.collapsed_caret_affinity = CollapsedCaretAffinity::Default;
        }
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();

        if self.record.kind != old_kind || self.record.title != old_title {
            cx.emit(BlockEvent::Changed);
        }
        cx.notify();
    }

    /// Replaces a current-display UTF-8 range and updates selection and IME state.
    ///
    /// `selected_range_relative` is in UTF-8 bytes of `new_text`; a nonempty
    /// insertion is marked when `mark_inserted_text` is true. Rich edits map
    /// projected delimiters to clean text or serialized Markdown as needed to
    /// preserve inline links and styles; raw modes edit literal text.
    pub(crate) fn replace_text_in_visible_range(
        &mut self,
        visible_range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        if self.kind().is_separator() && !self.uses_raw_text_editing() {
            return;
        }

        let visible_range =
            crate::text_offsets::normalize_utf8_range(self.display_text(), visible_range);
        let selected_range_relative = selected_range_relative
            .map(|range| crate::text_offsets::normalize_utf8_range(new_text, range));

        let inserted_attributes = self.replacement_attributes_for_visible_range(&visible_range);

        // Inline `[label](url)` links round-trip through their projected source,
        // so edit them via the link projection even when the block is otherwise
        // source-preserving (for example it also contains inline math). This keeps
        // a link's anchor text editable the same way in every block; reference and
        // autolink links stay on the markdown-space path below, which preserves
        // their original source spelling.
        if !self.uses_raw_text_editing()
            && let Some(link_run) = self
                .projected_link_run_fully_covering_range(&visible_range)
                .filter(|run| !run.link.is_source_preserving())
                .cloned()
        {
            self.apply_link_projection_edit(
                &link_run,
                visible_range,
                new_text,
                selected_range_relative,
                mark_inserted_text,
                cx,
            );
            return;
        }

        if self.should_use_markdown_space_link_edit() {
            self.apply_markdown_space_title_edit(
                visible_range,
                new_text,
                selected_range_relative,
                mark_inserted_text,
                cx,
            );
            return;
        }

        // Editing outside an inline link's run would otherwise re-derive the
        // inline tree from collapsed visible text, which no longer contains the
        // `[label](url)` markers and silently drops the link. Edit in markdown
        // space (as source-preserving links already do) so the link round-trips.
        if !self.uses_raw_text_editing() && self.record.title.has_inline_links() {
            self.apply_markdown_space_title_edit(
                visible_range,
                new_text,
                selected_range_relative,
                mark_inserted_text,
                cx,
            );
            return;
        }

        let clean_range = self.current_to_clean_range(visible_range.clone());
        let mut base_title = self.record.title.clone();
        let overlaps_delimiters = self.projection.is_some() && !self.uses_raw_text_editing();
        if overlaps_delimiters {
            let touched_styles = self.projected_styles_touching_display_range(&visible_range);
            if !touched_styles.is_empty() {
                base_title.unwrap_styles_on_fragments(&touched_styles);
            }
        }

        let base_visible_len = base_title.visible_text().len();
        let replaced_text = self.display_text()[visible_range.clone()].to_string();
        let result = if self.uses_raw_text_editing() {
            base_title.replace_visible_range_raw(
                clean_range.clone(),
                new_text,
                InlineInsertionAttributes::default(),
            )
        } else {
            base_title.replace_visible_range_with_link_references(
                clean_range.clone(),
                new_text,
                inserted_attributes,
                &self.link_reference_definitions,
            )
        };

        // A span was closed when re-parsing absorbed delimiters into a style,
        // leaving the clean text shorter than expected. Skip IME and deletions.
        let expected_visible_len =
            base_visible_len.saturating_sub(clean_range.len()) + new_text.len();
        let caret_may_have_closed_span = !self.uses_raw_text_editing()
            && !new_text.is_empty()
            && !mark_inserted_text
            && result.tree.visible_text().len() < expected_visible_len;
        let quote_structure_edit = !self.uses_raw_text_editing()
            && self.quote_depth > 0
            && (new_text.contains('\n')
                || replaced_text.contains('\n')
                || (self.kind() == BlockKind::Quote
                    && Self::multiline_quote_edit_requires_reparse(&result.tree.visible_text())));
        if quote_structure_edit {
            self.quote_reparse_requested = true;
        }
        let inserted_range = clean_range.start..clean_range.start + new_text.len();
        let marked_range = if mark_inserted_text && !new_text.is_empty() {
            Some(result.map_range(&inserted_range))
        } else {
            None
        };
        let selected_range = selected_range_relative.as_ref().map(|relative| {
            let absolute = clean_range.start + relative.start..clean_range.start + relative.end;
            result.map_range(&absolute)
        });
        let cursor = selected_range
            .as_ref()
            .map(|range| range.end)
            .unwrap_or_else(|| result.map_offset(clean_range.start + new_text.len()));

        self.apply_title_edit(
            result.tree,
            cursor,
            marked_range,
            selected_range.clone(),
            selected_range
                .as_ref()
                .and_then(|range| (!range.is_empty()).then_some(false)),
            caret_may_have_closed_span,
            cx,
        );
    }

    /// Refreshes runtime caches and emits a block change event.
    pub(super) fn mark_changed(&mut self, cx: &mut Context<Self>) {
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();
        cx.emit(BlockEvent::Changed);
        cx.notify();
    }

    /// Converts this block to a heading of `level` when it is an editable
    /// paragraph or heading. Returns whether the kind changed.
    pub(crate) fn set_heading_level(&mut self, level: u8, cx: &mut Context<Self>) -> bool {
        let current_kind = self.kind();
        if self.uses_raw_text_editing()
            || !(1..=6).contains(&level)
            || !self.children.is_empty()
            || !matches!(
                &current_kind,
                BlockKind::Paragraph | BlockKind::Heading { .. }
            )
            || matches!(current_kind, BlockKind::Heading { level: current } if current == level)
        {
            return false;
        }

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.record.kind = BlockKind::Heading { level };
        self.record.raw_fallback = None;
        self.quote_reparse_requested = false;
        self.mark_changed(cx);
        true
    }

    /// Converts an editable heading back to a paragraph. Returns whether the
    /// kind changed.
    pub(crate) fn remove_heading(&mut self, cx: &mut Context<Self>) -> bool {
        if self.uses_raw_text_editing()
            || !self.children.is_empty()
            || !matches!(self.kind(), BlockKind::Heading { .. })
        {
            return false;
        }

        self.convert_to_paragraph(cx);
        true
    }

    /// Converts this block to a paragraph with an undo boundary.
    pub(crate) fn convert_to_paragraph(&mut self, cx: &mut Context<Self>) {
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.record.kind = BlockKind::Paragraph;
        self.record.raw_fallback = None;
        self.quote_reparse_requested = false;
        self.mark_changed(cx);
    }

    /// Converts this block to a separator with undo and change events.
    pub(crate) fn convert_to_separator(&mut self, cx: &mut Context<Self>) {
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.make_separator();
        cx.emit(BlockEvent::Changed);
        cx.notify();
    }

    /// Turns this block into a separator in place without emitting events or
    /// capturing undo, so editor-level flows that already manage those can
    /// reuse the conversion.
    pub(crate) fn make_separator(&mut self) {
        self.clear_inline_projection();
        self.record.kind = BlockKind::Separator;
        self.record.raw_fallback = None;
        self.record.set_title(InlineTextTree::plain(String::new()));
        self.quote_reparse_requested = false;
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        self.assign_collapsed_selection_offset(0, CollapsedCaretAffinity::Default, None);
        self.marked_range = None;
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();
    }

    /// Replaces this block with an empty code block using the given language.
    pub(crate) fn enter_code_block(
        &mut self,
        language: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.clear_inline_projection();
        self.record.kind = BlockKind::CodeBlock { language };
        self.record.raw_fallback = None;
        self.record.set_title(InlineTextTree::plain(String::new()));
        self.quote_reparse_requested = false;
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        self.assign_collapsed_selection_offset(0, CollapsedCaretAffinity::Default, None);
        self.marked_range = None;
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();
        cx.emit(BlockEvent::Changed);
        cx.notify();
    }

    /// Convert the current paragraph into a display-math block. `body` becomes
    /// the formula source between the fences (empty for a fresh `$$` block), and
    /// the caret lands at the start of that body line.
    pub(crate) fn enter_math_block(&mut self, body: &str, cx: &mut Context<Self>) {
        let source = format!("$$\n{body}\n$$");
        let cursor = "$$\n".len();

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.clear_inline_projection();
        self.record.kind = BlockKind::MathBlock;
        self.record.set_title(InlineTextTree::plain(source));
        self.quote_reparse_requested = false;
        self.sync_edit_mode_from_kind();
        self.sync_render_cache();
        self.assign_collapsed_selection_offset(cursor, CollapsedCaretAffinity::Default, None);
        self.marked_range = None;
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();
        cx.emit(BlockEvent::Changed);
        cx.notify();
    }

    /// Toggle a style flag directly on the fragment tree without ever
    /// manipulating raw marker characters.  The selection range determines
    /// which fragments have their [`InlineStyle`] flag flipped.
    ///
    /// Serializers later translate these flags back to markers on export.
    pub(crate) fn toggle_inline_format(&mut self, format: InlineFormat, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() || self.uses_raw_text_editing() {
            return;
        }

        let mut next_title = self.record.title.clone();
        let selection = self.selection_clean_range();
        let changed = match format {
            InlineFormat::Bold => next_title.toggle_bold(selection.clone()),
            InlineFormat::Italic => next_title.toggle_italic(selection.clone()),
            InlineFormat::Underline => next_title.toggle_underline(selection.clone()),
            InlineFormat::Code => next_title.toggle_code(selection.clone()),
        };
        if !changed {
            return;
        }

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.apply_title_edit(
            next_title,
            selection.end,
            None,
            Some(selection),
            Some(self.selection_reversed),
            false,
            cx,
        );
    }

    /// Returns the laid-out hard line containing the display caret.
    fn current_line_layout_and_offset(&self) -> Option<(&WrappedLine, usize)> {
        let lines = self.last_layout.as_ref()?;
        let text = self.display_text();
        let ranges = super::element::hard_line_ranges(text);
        let (line_idx, offset_in_line) =
            super::element::line_index_for_offset(&ranges, self.cursor_offset());
        Some((lines.get(line_idx)?, offset_in_line))
    }

    /// Returns the stored or measured x position for vertical caret movement.
    pub(super) fn vertical_anchor_x(&self) -> Pixels {
        self.vertical_motion_x
            .or_else(|| {
                self.current_line_layout_and_offset()
                    .and_then(|(layout, offset_in_line)| {
                        super::element::position_for_offset(
                            layout,
                            offset_in_line,
                            self.last_line_height,
                            true,
                        )
                        .map(|position| position.x)
                    })
            })
            .unwrap_or(px(0.0))
    }

    /// Attempt to move the cursor up (direction < 0) or down one visual line
    /// within the current block.  Returns false if the cursor is already at
    /// the first or last line, so the editor can transfer focus instead.
    pub(super) fn move_cursor_vertically(
        &mut self,
        direction: i32,
        preferred_x: Pixels,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(lines) = self.last_layout.as_ref() else {
            return false;
        };

        let text = self.display_text();
        let ranges = super::element::hard_line_ranges(text);
        let (current_line_idx, offset_in_line) =
            super::element::line_index_for_offset(&ranges, self.cursor_offset());
        let Some(current_layout) = lines.get(current_line_idx) else {
            return false;
        };
        let Some(current_position) = super::element::position_for_offset(
            current_layout,
            offset_in_line,
            self.last_line_height,
            true,
        ) else {
            return false;
        };

        let current_y =
            super::element::wrapped_line_top(lines, self.last_line_height, current_line_idx)
                + current_position.y;
        let target_y = if direction < 0 {
            current_y - self.last_line_height + self.last_line_height / 2.0
        } else {
            current_y + self.last_line_height + self.last_line_height / 2.0
        };
        if target_y < px(0.0) {
            return false;
        }

        let total_height = lines.iter().fold(px(0.0), |height, line| {
            height + super::element::wrapped_line_height(line, self.last_line_height)
        });
        if target_y >= total_height {
            return false;
        }

        let Some((target_line_idx, target_y_in_line)) =
            super::element::wrapped_line_for_y(lines, self.last_line_height, target_y)
        else {
            return false;
        };
        let target_layout = &lines[target_line_idx];
        let target_point = point(preferred_x, target_y_in_line);
        let target_offset_in_line =
            match target_layout.closest_index_for_position(target_point, self.last_line_height) {
                Ok(idx) | Err(idx) => idx,
            };

        let flat_offset = ranges[target_line_idx].start + target_offset_in_line;
        self.move_to_with_preferred_x(flat_offset, Some(preferred_x), cx);
        true
    }

    /// Compute the character offset where the cursor should land when focus
    /// enters this block from above or below. Uses `preferred_x` when supplied
    /// so the horizontal position carries across different-height blocks.
    pub fn entry_offset_for_vertical_focus(
        &self,
        prefer_last_line: bool,
        preferred_x: Option<Pixels>,
    ) -> usize {
        let Some(lines) = self.last_layout.as_ref() else {
            return if prefer_last_line {
                self.visible_len()
            } else {
                0
            };
        };

        let text = self.display_text();
        let ranges = super::element::hard_line_ranges(text);
        let target_line_idx = if prefer_last_line { lines.len() - 1 } else { 0 };
        let target_layout = &lines[target_line_idx];
        let target_x = preferred_x.unwrap_or(px(0.0));
        let target_y = if prefer_last_line {
            super::element::wrapped_line_height(target_layout, self.last_line_height)
                - self.last_line_height / 2.0
        } else {
            self.last_line_height / 2.0
        };

        let offset_in_line = match target_layout
            .closest_index_for_position(point(target_x, target_y), self.last_line_height)
        {
            Ok(idx) | Err(idx) => idx,
        };
        ranges[target_line_idx].start + offset_in_line
    }

    /// Collapses the display selection and retains an optional vertical x anchor.
    pub fn move_to_with_preferred_x(
        &mut self,
        offset: usize,
        preferred_x: Option<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.assign_collapsed_selection_offset(
            offset,
            CollapsedCaretAffinity::Default,
            preferred_x,
        );
        self.cursor_blink_epoch = Instant::now();
        cx.notify();
    }

    /// Starts a 33 ms cursor repaint loop while blink opacity is animating.
    /// During the first 0.5 s after an epoch reset, opacity stays at 1.0 and
    /// the timer skips unnecessary repaints.
    ///
    /// The blink task is automatically cancelled when the block loses focus
    /// (the task handle is dropped in [`Block::render`]).
    pub(super) fn start_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink_epoch = Instant::now();
        self.cursor_blink_task = Some(cx.spawn(
            async |this: WeakEntity<Block>, cx: &mut AsyncApp| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if this
                    .update(cx, |this: &mut Block, cx: &mut Context<Block>| {
                        if this.cursor_blink_epoch.elapsed().as_secs_f32() >= 0.5 {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            },
        ));
    }

    /// Cosine blink opacity: fully opaque for 0.5 s, then a 1 s period.
    pub fn cursor_opacity(&self) -> f32 {
        let elapsed = self.cursor_blink_epoch.elapsed().as_secs_f32();
        if elapsed < 0.5 {
            return 1.0;
        }
        let t = elapsed - 0.5;
        (f32::cos(t * std::f32::consts::TAU) + 1.0) / 2.0
    }

    /// Returns the active selection endpoint as a display UTF-8 byte offset.
    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    /// Ends body and language pointer drags, returning whether either was active.
    pub(crate) fn end_pointer_selection_session(&mut self) -> bool {
        let changed = self.is_selecting || self.code_language_is_selecting;
        self.is_selecting = false;
        self.code_language_is_selecting = false;
        changed
    }

    /// Returns body selection endpoints in anchor/focus order.
    fn selection_anchor_focus(&self) -> (usize, usize) {
        if self.selection_reversed {
            (self.selected_range.end, self.selected_range.start)
        } else {
            (self.selected_range.start, self.selected_range.end)
        }
    }

    /// Returns anchor/focus positions in clean visible-text byte coordinates.
    fn clean_selection_anchor_focus(&self) -> (usize, usize) {
        let (anchor, focus) = self.selection_anchor_focus();
        (
            self.current_to_clean_offset(anchor),
            self.current_to_clean_offset(focus),
        )
    }

    /// Stores scalar-aligned display endpoints and their direction.
    fn set_selection_from_anchor_focus(&mut self, anchor: usize, focus: usize) {
        let text = self.display_text();
        let clamped_anchor = crate::text_offsets::floor_char_boundary(text, anchor);
        let clamped_focus = crate::text_offsets::floor_char_boundary(text, focus);
        self.selected_range = clamped_anchor.min(clamped_focus)..clamped_anchor.max(clamped_focus);
        self.selection_reversed = !self.selected_range.is_empty() && clamped_focus < clamped_anchor;
    }

    /// Restores clean endpoints through the projection with separate affinities.
    fn set_selection_from_clean_anchor_focus(
        &mut self,
        anchor: usize,
        focus: usize,
        anchor_affinity: CollapsedCaretAffinity,
        focus_affinity: CollapsedCaretAffinity,
    ) {
        // Map each endpoint back through its own affinity. Several display
        // positions can share one clean offset (a trailing link's `](url)`
        // delimiters all collapse onto the anchor-text end), so the plain
        // clean->display cursor map would snap an endpoint that sat after the
        // closing delimiter back to just inside it. Honoring the captured
        // affinity keeps such endpoints in place across a projection rebuild.
        self.set_selection_from_anchor_focus(
            self.clean_to_current_cursor_offset_with_affinity(anchor, anchor_affinity),
            self.clean_to_current_cursor_offset_with_affinity(focus, focus_affinity),
        );
    }

    /// Collapses the body selection at a display UTF-8 byte offset.
    pub fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.move_to_with_preferred_x(offset, None, cx);
    }

    /// Extends the body selection to a scalar-aligned display byte offset.
    pub fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let clamped_offset = crate::text_offsets::floor_char_boundary(self.display_text(), offset);
        if self.selection_reversed {
            self.selected_range.start = clamped_offset;
        } else {
            self.selected_range.end = clamped_offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.cursor_blink_epoch = Instant::now();
        self.clear_vertical_motion();
        self.sync_collapsed_caret_affinity();
        cx.notify();
    }

    /// Converts a current-display byte range to GPUI's UTF-16 code units.
    pub(super) fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        Self::utf8_range_to_utf16_in(self.display_text(), range)
    }

    /// Converts an IME range to scalar-aligned display UTF-8 bytes.
    pub(super) fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        Self::utf16_range_to_utf8_in(self.display_text(), range_utf16)
    }

    /// Finds the preceding grapheme boundary in current display text.
    pub fn previous_boundary(&self, offset: usize) -> usize {
        let text = self.display_text();
        let mut cursor = GraphemeCursor::new(
            crate::text_offsets::floor_char_boundary(text, offset),
            text.len(),
            true,
        );
        cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
    }

    /// Finds the following grapheme boundary in current display text.
    pub fn next_boundary(&self, offset: usize) -> usize {
        let text = self.display_text();
        let mut cursor = GraphemeCursor::new(
            crate::text_offsets::floor_char_boundary(text, offset),
            text.len(),
            true,
        );
        cursor
            .next_boundary(text, 0)
            .ok()
            .flatten()
            .unwrap_or(text.len())
    }

    /// UTF-8 offset of the preceding Unicode word start, or zero.
    pub fn previous_word_start(&self, offset: usize) -> usize {
        let text = self.display_text();
        let offset = offset.min(text.len());
        text.unicode_word_indices()
            .map(|(start, _)| start)
            .take_while(|start| *start < offset)
            .last()
            .unwrap_or(0)
    }

    /// UTF-8 offset of the next Unicode word start, or the display text length.
    pub fn next_word_start(&self, offset: usize) -> usize {
        let text = self.display_text();
        let offset = offset.min(text.len());
        text.unicode_word_indices()
            .map(|(start, _)| start)
            .find(|start| *start > offset)
            .unwrap_or(text.len())
    }

    /// Maps a projected display byte offset back to clean visible text.
    fn unexpand_offset(&self, expanded: usize) -> usize {
        let Some(projection) = &self.projection else {
            return expanded;
        };
        projection
            .display_to_clean
            .get(expanded.min(projection.display_to_clean.len().saturating_sub(1)))
            .copied()
            .unwrap_or(expanded)
    }

    /// Hit-tests the last body layout to a scalar-aligned display byte offset.
    pub fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.display_text().is_empty() {
            return 0;
        }

        let (Some(bounds), Some(lines)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.visible_len();
        }

        let text = self.display_text();
        let ranges = super::element::hard_line_ranges(text);
        let relative_y = position.y - bounds.top();
        let Some((line_idx, y_in_line)) =
            super::element::wrapped_line_for_y(lines, self.last_line_height, relative_y)
        else {
            return 0;
        };
        let (Some(layout), Some(range)) = (lines.get(line_idx), ranges.get(line_idx)) else {
            return 0;
        };
        let origin_x = super::element::aligned_line_left(layout, *bounds, self.text_align());

        let offset_in_line = match layout.closest_index_for_position(
            point(position.x - origin_x, y_in_line),
            self.last_line_height,
        ) {
            Ok(idx) | Err(idx) => idx,
        };
        crate::text_offsets::floor_char_boundary(
            text,
            range.start + offset_in_line.min(range.len()),
        )
    }

    /// Returns painted bounds of the IME mark, selection, or collapsed caret.
    pub(crate) fn active_range_or_cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let lines = self.last_layout.as_ref()?;
        let line_height = self.last_line_height;
        let text = self.display_text();
        let active_range = self
            .marked_range
            .clone()
            .unwrap_or_else(|| self.selected_range.clone());

        if active_range.is_empty() {
            return super::element::cursor_bounds_for_offset(
                lines,
                bounds,
                line_height,
                text,
                self.cursor_offset(),
                self.text_align(),
                px(1.0),
            );
        }

        super::element::range_bounds(
            lines,
            bounds,
            line_height,
            text,
            active_range,
            self.text_align(),
        )
    }
}

#[cfg(test)]
mod tests;
