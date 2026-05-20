//! Runtime context synchronization for blocks, references, images, and focus.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::*;

impl Editor {
    pub(super) fn current_edit_target_entity_id_from_state(&self, cx: &App) -> Option<EntityId> {
        self.active_entity_id
            .filter(|entity_id| self.focusable_entity_by_id(*entity_id).is_some())
            .or_else(|| {
                self.pending_focus
                    .filter(|entity_id| self.focusable_entity_by_id(*entity_id).is_some())
            })
            .or_else(|| self.first_focusable_entity_id(cx))
    }

    pub(super) fn current_edit_target_from_state(&self, cx: &App) -> Option<Entity<Block>> {
        self.current_edit_target_entity_id_from_state(cx)
            .and_then(|entity_id| self.focusable_entity_by_id(entity_id))
    }

    fn end_block_pointer_selection_sessions_inner(
        &mut self,
        cx: &mut Context<Self>,
        notify: bool,
    ) -> bool {
        let mut changed = false;

        if let Some(target) = self.current_edit_target_from_state(cx) {
            target.update(cx, |block, _cx| {
                changed |= block.end_pointer_selection_session();
            });
        }

        for visible in self.document.visible_blocks() {
            visible.entity.update(cx, |block, _cx| {
                changed |= block.end_pointer_selection_session();
            });
        }

        for binding in self.table_cells.values().cloned().collect::<Vec<_>>() {
            binding.cell.update(cx, |block, _cx| {
                changed |= block.end_pointer_selection_session();
            });
        }

        if changed && notify {
            cx.notify();
        }
        changed
    }

    pub(super) fn end_block_pointer_selection_sessions(&mut self, cx: &mut Context<Self>) -> bool {
        self.end_block_pointer_selection_sessions_inner(cx, true)
    }

    /// Creates a new block entity and subscribes this editor to its
    /// [`BlockEvent`](crate::components::BlockEvent) stream.
    pub(super) fn new_block(cx: &mut Context<Self>, record: BlockRecord) -> Entity<Block> {
        let block = cx.new(|cx| Block::with_record(cx, record));
        cx.subscribe(&block, Self::on_block_event).detach();
        block
    }

    pub(super) fn new_table_cell_block(
        cx: &mut Context<Self>,
        title: InlineTextTree,
        position: TableCellPosition,
        alignment: TableColumnAlignment,
    ) -> Entity<Block> {
        let block = Self::new_block(cx, BlockRecord::new(BlockKind::Paragraph, title));
        block.update(cx, |block, _cx| {
            block.set_table_cell_mode(position, alignment);
        });
        block
    }

    pub(super) fn image_base_dir(&self) -> Option<PathBuf> {
        self.file_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
    }

    pub(super) fn sync_runtime_context_for_block(
        &self,
        block: &Entity<Block>,
        base_dir: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        let next_base_dir = base_dir.map(Path::to_path_buf);
        let image_reference_definitions = self.image_reference_definitions.clone();
        let link_reference_definitions = self.link_reference_definitions.clone();
        let footnote_registry = self.footnote_registry.clone();
        block.update(cx, move |block, cx| {
            block.set_runtime_context(
                next_base_dir.clone(),
                image_reference_definitions.clone(),
                link_reference_definitions.clone(),
                footnote_registry.clone(),
            );
            cx.notify();
        });
    }

    fn reference_definition_source(&self, cx: &App) -> String {
        let mut source = String::new();
        for visible_block in self.document.visible_blocks() {
            let block = visible_block.entity.read(cx);
            if block.kind() != BlockKind::RawMarkdown {
                continue;
            }

            let raw = block
                .record
                .raw_fallback
                .clone()
                .unwrap_or_else(|| block.record.title_markdown());
            if !raw.as_bytes().windows(2).any(|window| window == b"]:") {
                continue;
            }

            if !source.is_empty() {
                source.push('\n');
            }
            source.push_str(&raw);
        }
        source
    }

    fn footnote_signature(&self, cx: &App) -> u64 {
        let mut hasher = DefaultHasher::new();
        for visible_block in self.document.visible_blocks() {
            let block = visible_block.entity.read(cx);
            block.record.id.hash(&mut hasher);
            format!("{:?}", block.kind()).hash(&mut hasher);
            if block.kind() == BlockKind::FootnoteDefinition {
                block.record.title.visible_text().hash(&mut hasher);
                block.record.parent.hash(&mut hasher);
            }
            for fragment in &block.record.title.fragments {
                if let Some(footnote) = fragment.footnote.as_ref() {
                    footnote.id.hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    fn refresh_reference_definition_cache(&mut self, cx: &App) -> bool {
        let source = self.reference_definition_source(cx);
        if source == self.runtime_reference_source_cache {
            return false;
        }

        self.image_reference_definitions = Arc::new(parse_image_reference_definitions(&source));
        self.link_reference_definitions = Arc::new(parse_link_reference_definitions(&source));
        self.runtime_reference_source_cache = source;
        true
    }

    fn refresh_footnote_registry_if_needed(&mut self, cx: &App) -> bool {
        let signature = self.footnote_signature(cx);
        if signature == self.runtime_footnote_signature {
            return false;
        }

        self.rebuild_footnote_registry(cx);
        self.runtime_footnote_signature = signature;
        true
    }

    fn sync_runtime_context_for_table_cells(
        &self,
        table_block: &Entity<Block>,
        base_dir: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        let cells = table_block
            .read(cx)
            .table_runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .header
                    .iter()
                    .chain(runtime.rows.iter().flatten())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for cell in cells {
            self.sync_runtime_context_for_block(&cell, base_dir, cx);
        }
    }

    fn sync_runtime_context_for_visible_blocks(
        &self,
        base_dir: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        for visible_block in self.document.visible_blocks() {
            self.sync_runtime_context_for_block(&visible_block.entity, base_dir, cx);
            if visible_block.entity.read(cx).kind() == BlockKind::Table {
                self.sync_runtime_context_for_table_cells(&visible_block.entity, base_dir, cx);
            }
        }
    }

    pub(super) fn rebuild_footnote_registry(&mut self, cx: &App) {
        let mut definitions = HashMap::new();
        let visible = self.document.visible_blocks();
        for visible_block in visible {
            let block = visible_block.entity.read(cx);
            if block.kind() != BlockKind::FootnoteDefinition {
                continue;
            }

            let allow_definition = self
                .document
                .find_block_location(visible_block.entity.entity_id())
                .is_some_and(|location| {
                    location.parent.is_none()
                        || location
                            .parent
                            .as_ref()
                            .is_some_and(|parent| parent.read(cx).kind().is_quote_container())
                });
            if !allow_definition {
                continue;
            }

            definitions
                .entry(block.record.title.visible_text().to_string())
                .or_insert(visible_block.entity.entity_id());
        }

        let mut bindings = HashMap::<String, FootnoteDefinitionBinding>::new();
        for (id, entity_id) in definitions {
            bindings.insert(
                id,
                FootnoteDefinitionBinding {
                    ordinal: None,
                    definition_entity_id: entity_id,
                    first_reference: None,
                },
            );
        }

        let mut next_ordinal = 1usize;
        let mut occurrence_index = 0usize;
        let mut block_occurrences = HashMap::<uuid::Uuid, Vec<FootnoteResolvedOccurrence>>::new();
        for visible_block in visible {
            let block = visible_block.entity.read(cx);
            let block_id = block.record.id;
            for fragment in &block.record.title.fragments {
                let Some(footnote) = fragment.footnote.as_ref() else {
                    continue;
                };
                let ordinal = if let Some(binding) = bindings.get_mut(&footnote.id) {
                    if binding.ordinal.is_none() {
                        binding.ordinal = Some(next_ordinal);
                        next_ordinal += 1;
                    }
                    if binding.first_reference.is_none() {
                        binding.first_reference = Some(FootnoteReferenceLocation {
                            entity_id: visible_block.entity.entity_id(),
                            occurrence_index,
                        });
                    }
                    binding.ordinal
                } else {
                    None
                };
                block_occurrences
                    .entry(block_id)
                    .or_default()
                    .push(FootnoteResolvedOccurrence {
                        id: footnote.id.clone(),
                        ordinal,
                        occurrence_index,
                    });
                if ordinal.is_none() {
                    occurrence_index += 1;
                    continue;
                }
                occurrence_index += 1;
            }
        }

        self.footnote_registry = Arc::new(FootnoteRegistry {
            bindings,
            block_occurrences,
        });
    }

    pub(super) fn rebuild_image_runtimes(&mut self, cx: &mut Context<Self>) {
        self.rebuild_image_runtimes_inner(None, cx);
    }

    pub(super) fn rebuild_image_runtimes_for_block(
        &mut self,
        block: &Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        self.rebuild_image_runtimes_inner(Some(block), cx);
    }

    fn rebuild_image_runtimes_inner(
        &mut self,
        changed_block: Option<&Entity<Block>>,
        cx: &mut Context<Self>,
    ) {
        let base_dir = self.image_base_dir();
        let base_dir_changed = self.runtime_base_dir != base_dir;
        let references_changed = self.refresh_reference_definition_cache(cx);
        let footnotes_changed = self.refresh_footnote_registry_if_needed(cx);
        self.runtime_base_dir = base_dir.clone();

        if changed_block.is_none() || base_dir_changed || references_changed || footnotes_changed {
            self.sync_runtime_context_for_visible_blocks(base_dir.as_deref(), cx);
            return;
        }

        let changed_block = changed_block.expect("checked");
        self.sync_runtime_context_for_block(changed_block, base_dir.as_deref(), cx);
        if changed_block.read(cx).kind() == BlockKind::Table {
            self.sync_runtime_context_for_table_cells(changed_block, base_dir.as_deref(), cx);
        }
    }

    pub(super) fn focusable_entity_by_id(&self, entity_id: EntityId) -> Option<Entity<Block>> {
        self.document.block_entity_by_id(entity_id).or_else(|| {
            self.table_cells
                .get(&entity_id)
                .map(|binding| binding.cell.clone())
        })
    }

    pub(super) fn first_focusable_entity_id(&self, cx: &App) -> Option<EntityId> {
        let first_root = self.document.first_root()?.clone();
        if first_root.read(cx).kind() == BlockKind::Table {
            return first_root
                .read(cx)
                .table_runtime
                .as_ref()
                .and_then(|runtime| runtime.header.first())
                .map(|cell| cell.entity_id())
                .or_else(|| Some(first_root.entity_id()));
        }
        Some(first_root.entity_id())
    }

    pub(super) fn focused_edit_target_entity_id(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<EntityId> {
        self.document
            .focused_block_entity_id(window, cx)
            .or_else(|| {
                self.table_cells
                    .values()
                    .find(|binding| binding.cell.read(cx).focus_handle.is_focused(window))
                    .map(|binding| binding.cell.entity_id())
            })
    }

    pub(super) fn focused_edit_target(&self, window: &Window, cx: &App) -> Option<Entity<Block>> {
        self.focused_edit_target_entity_id(window, cx)
            .and_then(|entity_id| self.focusable_entity_by_id(entity_id))
    }

    pub(super) fn table_cell_binding(&self, entity_id: EntityId) -> Option<TableCellBinding> {
        self.table_cells.get(&entity_id).cloned()
    }

    pub(super) fn table_block_by_id(&self, entity_id: EntityId, cx: &App) -> Option<Entity<Block>> {
        self.document
            .block_entity_by_id(entity_id)
            .filter(|block| block.read(cx).kind() == BlockKind::Table)
    }
}
