//! Lightweight workspace panel state, file-tree scanning, and outline parsing.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gpui::*;

use super::{BlockKind, Editor};
use crate::i18n::I18nStrings;
use crate::theme::Theme;

const FOLDER_ICON: &str = "icon/workspace/folder.svg";
const MARKDOWN_ICON: &str = "icon/workspace/markdown.svg";
const WORKSPACE_PANEL_TARGET_RATIO: f32 = 0.15;
const WORKSPACE_PANEL_MIN_WIDTH: f32 = 240.0;
const WORKSPACE_PANEL_MAX_WIDTH: f32 = 360.0;
const WORKSPACE_NODE_HEIGHT: f32 = 28.0;
const WORKSPACE_NODE_INDENT: f32 = 18.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WorkspaceTab {
    #[default]
    Files,
    Outline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceTreeKind {
    Directory(PathBuf),
    MarkdownFile(PathBuf),
    Heading { line: usize, level: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceTreeNode {
    id: String,
    label: String,
    kind: WorkspaceTreeKind,
    children: Vec<WorkspaceTreeNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum WorkspaceSelection {
    File(PathBuf),
    Outline(String),
}

#[derive(Default)]
pub(super) struct WorkspaceState {
    is_open: bool,
    active_tab: WorkspaceTab,
    root: Option<PathBuf>,
    file_tree: Option<WorkspaceTreeNode>,
    file_error: Option<String>,
    outline_tree: Vec<WorkspaceTreeNode>,
    outline_source: Option<String>,
    expanded: HashSet<String>,
    selected: Option<WorkspaceSelection>,
}

impl Editor {
    pub(crate) fn toggle_workspace_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.is_open {
            self.workspace.is_open = false;
        } else {
            self.close_menu_bar(cx);
            self.dismiss_contextual_overlays(cx);
            self.workspace.is_open = true;
            self.sync_workspace_models(cx);
            window.activate_window();
        }
        cx.notify();
    }

    pub(crate) fn on_toggle_workspace_action(
        &mut self,
        _: &crate::components::ToggleWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_workspace_drawer(window, cx);
    }

    pub(super) fn sync_workspace_after_document_path_change(&mut self, cx: &mut Context<Self>) {
        self.workspace.root = None;
        self.workspace.file_tree = None;
        self.workspace.file_error = None;
        self.workspace.outline_source = None;
        if self.workspace.is_open {
            self.sync_workspace_models(cx);
        }
    }

    fn sync_workspace_models(&mut self, cx: &mut Context<Self>) {
        self.sync_workspace_file_tree();
        self.sync_workspace_outline(cx);
    }

    fn workspace_root_for_current_file(&self) -> Option<PathBuf> {
        self.file_path.as_ref()?.parent().map(Path::to_path_buf)
    }

    fn sync_workspace_file_tree(&mut self) {
        let next_root = self.workspace_root_for_current_file();
        if self.workspace.root == next_root && self.workspace.file_tree.is_some() {
            self.workspace.selected = self
                .file_path
                .as_ref()
                .map(|path| WorkspaceSelection::File(path.clone()));
            return;
        }

        self.workspace.root = next_root.clone();
        self.workspace.file_tree = None;
        self.workspace.file_error = None;

        let Some(root) = next_root else {
            self.workspace.selected = None;
            return;
        };

        // Validate the root path
        if root.as_os_str().is_empty() {
            self.workspace.file_error = Some("Invalid workspace path: empty path".to_string());
            self.workspace.selected = None;
            return;
        }

        match scan_workspace_dir(&root) {
            Ok(tree) => {
                self.workspace.expanded.insert(tree.id.clone());
                self.workspace.file_tree = Some(tree);
                self.workspace.selected = self
                    .file_path
                    .as_ref()
                    .map(|path| WorkspaceSelection::File(path.clone()));
            }
            Err(err) => {
                self.workspace.file_error = Some(err.to_string());
            }
        }
    }

    fn sync_workspace_outline(&mut self, cx: &mut Context<Self>) {
        let source = self.serialized_document_text(cx);
        if self.workspace.outline_source.as_deref() == Some(source.as_str()) {
            return;
        }

        let outline = build_outline_tree(&source);
        prune_outline_state(&mut self.workspace, &outline);
        self.workspace.outline_tree = outline;
        self.workspace.outline_source = Some(source);
    }

    fn set_workspace_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        if self.workspace.active_tab != tab {
            self.workspace.active_tab = tab;
            self.sync_workspace_models(cx);
            cx.notify();
        }
    }

    fn toggle_workspace_node(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.workspace.expanded.remove(id) {
            self.workspace.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    fn select_outline_node(&mut self, id: String, cx: &mut Context<Self>) {
        self.workspace.selected = Some(WorkspaceSelection::Outline(id));
        cx.notify();
    }

    fn open_workspace_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.selected = Some(WorkspaceSelection::File(path.clone()));
        self.request_dropped_markdown_replace(path, window, cx);
    }

    pub(super) fn render_workspace_panel(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        panel_width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.workspace.is_open {
            return None;
        }

        self.sync_workspace_models(cx);
        let editor = cx.entity().downgrade();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;

        let tab = |label: String, tab: WorkspaceTab, active: bool| {
            let tab_editor = editor.clone();
            let tab_id = match tab {
                WorkspaceTab::Files => "workspace-tab-files",
                WorkspaceTab::Outline => "workspace-tab-outline",
            };
            div()
                .id(tab_id)
                .h(px(30.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(if active {
                    c.selection
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .hover(|this| this.bg(c.dialog_secondary_button_hover))
                .cursor_pointer()
                .text_size(px(t.text_size * 0.88))
                .text_color(if active {
                    c.text_default
                } else {
                    c.dialog_muted
                })
                .child(label)
                .on_click(move |_event, _window, cx| {
                    let _ = tab_editor.update(cx, |editor, cx| {
                        editor.set_workspace_tab(tab, cx);
                    });
                })
        };

        let body = match self.workspace.active_tab {
            WorkspaceTab::Files => self.render_workspace_files_tree(theme, strings, &editor),
            WorkspaceTab::Outline => self.render_workspace_outline_tree(theme, strings, &editor),
        };

        Some(
            div()
                .id("workspace-panel")
                .h_full()
                .w(px(panel_width))
                .flex()
                .flex_col()
                .flex_shrink_0()
                .bg(c.dialog_surface)
                .border_r(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .child(
                    div()
                        .px(px(12.0))
                        .pt(px(12.0))
                        .pb(px(8.0))
                        .flex()
                        .gap(px(8.0))
                        .border_b(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .child(tab(
                            strings.workspace_tab_files.clone(),
                            WorkspaceTab::Files,
                            self.workspace.active_tab == WorkspaceTab::Files,
                        ))
                        .child(tab(
                            strings.workspace_tab_outline.clone(),
                            WorkspaceTab::Outline,
                            self.workspace.active_tab == WorkspaceTab::Outline,
                        )),
                )
                .child(
                    div()
                        .id("workspace-panel-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .px(px(8.0))
                        .py(px(10.0))
                        .child(body),
                )
                .into_any_element(),
        )
    }

    fn render_workspace_files_tree(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        if self.workspace.root.is_none() {
            return self.render_workspace_empty_state(
                &strings.workspace_no_file_title,
                &strings.workspace_no_file_message,
                theme,
            );
        }

        if let Some(error) = self.workspace.file_error.as_ref() {
            return self.render_workspace_empty_state(
                &strings.workspace_scan_failed_title,
                error,
                theme,
            );
        }

        let Some(root) = self.workspace.file_tree.as_ref() else {
            return self.render_workspace_empty_state("", &strings.workspace_empty_files, theme);
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .children(self.render_workspace_nodes(std::slice::from_ref(root), 0, theme, editor))
            .into_any_element()
    }

    fn render_workspace_outline_tree(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        if self.workspace.outline_tree.is_empty() {
            return self.render_workspace_empty_state("", &strings.workspace_empty_outline, theme);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .children(self.render_workspace_nodes(&self.workspace.outline_tree, 0, theme, editor))
            .into_any_element()
    }

    fn render_workspace_empty_state(
        &self,
        title: &str,
        message: &str,
        theme: &Theme,
    ) -> AnyElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let title = (!title.is_empty()).then(|| {
            div()
                .text_size(px(t.text_size))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text_default)
                .child(title.to_string())
        });

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(22.0))
            .text_align(TextAlign::Center)
            .children(title)
            .child(
                div()
                    .text_size(px(t.text_size * 0.9))
                    .line_height(px(t.text_size * t.text_line_height))
                    .text_color(c.dialog_muted)
                    .child(message.to_string()),
            )
            .into_any_element()
    }

    fn render_workspace_nodes(
        &self,
        nodes: &[WorkspaceTreeNode],
        depth: usize,
        theme: &Theme,
        editor: &WeakEntity<Editor>,
    ) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for node in nodes {
            elements.push(self.render_workspace_node(node, depth, theme, editor));
            if !node.children.is_empty() && self.workspace.expanded.contains(&node.id) {
                elements.extend(self.render_workspace_nodes(
                    &node.children,
                    depth + 1,
                    theme,
                    editor,
                ));
            }
        }
        elements
    }

    fn render_workspace_node(
        &self,
        node: &WorkspaceTreeNode,
        depth: usize,
        theme: &Theme,
        editor: &WeakEntity<Editor>,
    ) -> AnyElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let is_expanded = self.workspace.expanded.contains(&node.id);
        let has_children = !node.children.is_empty();
        let selected = match (&self.workspace.selected, &node.kind) {
            (Some(WorkspaceSelection::File(selected)), WorkspaceTreeKind::MarkdownFile(path)) => {
                selected == path
            }
            (Some(WorkspaceSelection::Outline(selected)), _) => selected == &node.id,
            _ => false,
        };
        let node_id = node.id.clone();
        let click_editor = editor.clone();
        let click_kind = node.kind.clone();
        let arrow_node_id = node.id.clone();
        let arrow_editor = editor.clone();
        let arrow = if has_children {
            if is_expanded { "v" } else { ">" }
        } else {
            ""
        };

        let icon = match &node.kind {
            WorkspaceTreeKind::Directory(_) => Some((FOLDER_ICON, Hsla::from(rgba(0xf59e0bff)))),
            WorkspaceTreeKind::MarkdownFile(_) => {
                Some((MARKDOWN_ICON, Hsla::from(rgba(0x2563ebff))))
            }
            WorkspaceTreeKind::Heading { .. } => None,
        };

        let label_color = if selected {
            c.text_default
        } else {
            c.dialog_muted
        };

        let mut arrow_el = div()
            .w(px(14.0))
            .h(px(18.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(c.dialog_muted)
            .child(arrow);
        if has_children {
            arrow_el = arrow_el.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                move |_event, _window, cx| {
                    let _ = arrow_editor.update(cx, |editor, cx| {
                        editor.toggle_workspace_node(&arrow_node_id, cx);
                    });
                    cx.stop_propagation();
                },
            );
        }

        div()
            .id(("workspace-node", stable_node_hash(&node.id)))
            .h(px(WORKSPACE_NODE_HEIGHT))
            .w_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(8.0 + depth as f32 * WORKSPACE_NODE_INDENT))
            .pr(px(8.0))
            .rounded(px(6.0))
            .bg(if selected {
                c.selection
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .cursor_pointer()
            .child(arrow_el)
            .children(icon.map(|(path, color)| {
                svg()
                    .path(path)
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(color)
                    .into_any_element()
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .text_size(px(t.text_size * 0.9))
                    .line_height(px(t.text_size * t.text_line_height))
                    .text_color(label_color)
                    .child(node.label.clone()),
            )
            .on_click(move |_event, window, cx| {
                let node_id = node_id.clone();
                let click_kind = click_kind.clone();
                let _ = click_editor.update(cx, |editor, cx| match click_kind {
                    WorkspaceTreeKind::Directory(_) => editor.toggle_workspace_node(&node_id, cx),
                    WorkspaceTreeKind::MarkdownFile(path) => {
                        editor.open_workspace_file(path, window, cx);
                    }
                    WorkspaceTreeKind::Heading { .. } => editor.select_outline_node(node_id, cx),
                });
            })
            .into_any_element()
    }
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("md"))
}

fn scan_workspace_dir(path: &Path) -> Result<WorkspaceTreeNode> {
    let mut children = Vec::new();
    for entry in
        fs::read_dir(path).with_context(|| format!("failed to read '{}'", path.display()))?
    {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            children.push(scan_workspace_dir(&entry_path)?);
        } else if file_type.is_file() && is_markdown_file(&entry_path) {
            children.push(WorkspaceTreeNode {
                id: file_node_id(&entry_path),
                label: file_label(&entry_path),
                kind: WorkspaceTreeKind::MarkdownFile(entry_path),
                children: Vec::new(),
            });
        }
    }

    children.sort_by(|left, right| {
        let left_dir = matches!(left.kind, WorkspaceTreeKind::Directory(_));
        let right_dir = matches!(right.kind, WorkspaceTreeKind::Directory(_));
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });

    Ok(WorkspaceTreeNode {
        id: file_node_id(path),
        label: file_label(path),
        kind: WorkspaceTreeKind::Directory(path.to_path_buf()),
        children,
    })
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn file_node_id(path: &Path) -> String {
    format!("file:{}", path.to_string_lossy())
}

fn stable_node_hash(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn workspace_panel_width_for_viewport(viewport_width: f32) -> f32 {
    let target = viewport_width * WORKSPACE_PANEL_TARGET_RATIO;
    target.clamp(WORKSPACE_PANEL_MIN_WIDTH, WORKSPACE_PANEL_MAX_WIDTH)
}

fn prune_outline_state(workspace: &mut WorkspaceState, outline: &[WorkspaceTreeNode]) {
    let mut current_ids = HashSet::new();
    collect_node_ids(outline, &mut current_ids);
    workspace
        .expanded
        .retain(|id| !is_outline_node_id(id) || current_ids.contains(id));

    if matches!(
        &workspace.selected,
        Some(WorkspaceSelection::Outline(id)) if !current_ids.contains(id)
    ) {
        workspace.selected = None;
    }
}

fn collect_node_ids(nodes: &[WorkspaceTreeNode], ids: &mut HashSet<String>) {
    for node in nodes {
        ids.insert(node.id.clone());
        collect_node_ids(&node.children, ids);
    }
}

fn is_outline_node_id(id: &str) -> bool {
    id.starts_with("outline:")
}

fn build_outline_tree(markdown: &str) -> Vec<WorkspaceTreeNode> {
    let mut roots = Vec::new();
    let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();
    let mut fence: Option<(char, usize)> = None;

    for (line_index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some((marker, len)) = fence {
            if is_closing_fence(trimmed, marker, len) {
                fence = None;
            }
            continue;
        }

        if let Some(next_fence) = opening_fence(trimmed) {
            fence = Some(next_fence);
            continue;
        }

        let Some((level, title)) = BlockKind::parse_atx_heading_line(line) else {
            continue;
        };

        while stack
            .last()
            .is_some_and(|(parent_level, _)| *parent_level >= level)
        {
            stack.pop();
        }

        let node = WorkspaceTreeNode {
            id: format!("outline:{line_index}"),
            label: title,
            kind: WorkspaceTreeKind::Heading {
                line: line_index,
                level,
            },
            children: Vec::new(),
        };

        let siblings = if let Some((_, parent_path)) = stack.last() {
            children_at_path_mut(&mut roots, parent_path)
        } else {
            &mut roots
        };
        siblings.push(node);

        let mut node_path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        node_path.push(siblings.len() - 1);
        stack.push((level, node_path));
    }

    roots
}

fn children_at_path_mut<'a>(
    nodes: &'a mut Vec<WorkspaceTreeNode>,
    path: &[usize],
) -> &'a mut Vec<WorkspaceTreeNode> {
    let mut current = nodes;
    for &index in path {
        current = &mut current[index].children;
    }
    current
}

fn opening_fence(trimmed: &str) -> Option<(char, usize)> {
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    (len >= 3).then_some((marker, len))
}

fn is_closing_fence(trimmed: &str, marker: char, len: usize) -> bool {
    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    count >= len && trimmed[count..].trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::{
        WorkspaceSelection, WorkspaceState, WorkspaceTreeKind, build_outline_tree,
        prune_outline_state, scan_workspace_dir, workspace_panel_width_for_viewport,
    };
    use std::fs;

    #[test]
    fn workspace_scan_keeps_dirs_and_md_files_only() {
        let root =
            std::env::temp_dir().join(format!("velotype-workspace-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested")).expect("create dirs");
        fs::write(root.join("a.md"), "a").expect("write md");
        fs::write(root.join("a.txt"), "ignored").expect("write txt");
        fs::write(root.join("nested").join("b.md"), "b").expect("write nested md");

        let tree = scan_workspace_dir(&root).expect("scan tree");
        let labels = tree
            .children
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["nested", "a.md"]);
        assert!(matches!(
            tree.children[0].kind,
            WorkspaceTreeKind::Directory(_)
        ));
        assert!(matches!(
            tree.children[1].kind,
            WorkspaceTreeKind::MarkdownFile(_)
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn outline_tree_skips_headings_inside_fenced_code() {
        let outline = build_outline_tree(
            "# Root\n\n```md\n# ignored\n```\n\n## Child\n\n### Grandchild\n\n# Next",
        );

        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].label, "Root");
        assert_eq!(outline[0].children[0].label, "Child");
        assert_eq!(outline[0].children[0].children[0].label, "Grandchild");
        assert_eq!(outline[1].label, "Next");
    }

    #[test]
    fn outline_expansion_state_is_not_auto_populated_and_prunes_stale_ids() {
        let outline = build_outline_tree("# Root\n\n## Child\n\n# Next");
        let mut fresh = WorkspaceState::default();
        prune_outline_state(&mut fresh, &outline);
        assert!(fresh.expanded.is_empty());

        let mut existing = WorkspaceState::default();
        existing.expanded.insert("outline:0".to_string());
        existing.expanded.insert("outline:999".to_string());
        existing
            .expanded
            .insert("workspace-dir:C:/docs".to_string());
        existing.selected = Some(WorkspaceSelection::Outline("outline:999".to_string()));

        prune_outline_state(&mut existing, &outline);

        assert!(existing.expanded.contains("outline:0"));
        assert!(existing.expanded.contains("workspace-dir:C:/docs"));
        assert!(!existing.expanded.contains("outline:999"));
        assert_eq!(existing.selected, None);
    }

    #[test]
    fn workspace_panel_width_uses_ratio_with_bounds() {
        assert_eq!(workspace_panel_width_for_viewport(1000.0), 240.0);
        assert_eq!(workspace_panel_width_for_viewport(2000.0), 300.0);
        assert_eq!(workspace_panel_width_for_viewport(4000.0), 360.0);
    }
}
