//! Action definitions and key bindings for both block editing and app-level
//! window/menu commands.
//!
//! Text-editing actions are scoped to the `"BlockEditor"` key context on each
//! block. Window and menu commands use global bindings so they remain
//! available even when focus is on non-block UI such as dialogs or buttons.

use std::collections::{BTreeMap, BTreeSet};

use gpui::*;
use schemars::JsonSchema;
use serde::Deserialize;

actions!(
    velotype,
    [
        Newline,
        DeleteBack,
        Delete,
        WordDeleteBack,
        WordDeleteForward,
        FocusPrev,
        FocusNext,
        MoveLeft,
        MoveRight,
        WordMoveLeft,
        WordMoveRight,
        Home,
        End,
        BlockUp,
        BlockDown,
        SelectLeft,
        SelectRight,
        WordSelectLeft,
        WordSelectRight,
        SelectHome,
        SelectEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        BoldSelection,
        ItalicSelection,
        UnderlineSelection,
        CodeSelection,
        IndentBlock,
        OutdentBlock,
        ExitCodeBlock,
        SaveDocument,
        NewWindow,
        OpenFile,
        OpenPreferences,
        NoRecentFiles,
        SaveDocumentAs,
        ExportHtml,
        ExportPdf,
        AddLanguageConfig,
        AddThemeConfig,
        QuitApplication,
        CheckForUpdates,
        ShowAbout,
        DismissTransientUi,
        ToggleViewMode,
        ToggleWorkspace,
    ]
);

/// Selects a theme from the app-level theme registry.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = velotype)]
#[serde(deny_unknown_fields)]
pub struct SelectTheme {
    /// Stable theme id from the built-in theme catalog.
    pub theme_id: String,
}

/// Selects a UI language from the app-level language registry.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = velotype)]
#[serde(deny_unknown_fields)]
pub struct SelectLanguage {
    /// Stable language id from the built-in language catalog.
    pub language_id: String,
}

/// Opens a previously recorded Markdown file path.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
#[action(namespace = velotype)]
#[serde(deny_unknown_fields)]
pub struct OpenRecentFile {
    /// Path stored in Velotype's recent-file history.
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShortcutCategory {
    File,
    Edit,
    Navigation,
    Formatting,
    Block,
    Other,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShortcutCommand {
    Newline,
    DeleteBack,
    Delete,
    WordDeleteBack,
    WordDeleteForward,
    FocusPrev,
    FocusNext,
    MoveLeft,
    MoveRight,
    WordMoveLeft,
    WordMoveRight,
    Home,
    End,
    BlockUp,
    BlockDown,
    SelectLeft,
    SelectRight,
    WordSelectLeft,
    WordSelectRight,
    SelectHome,
    SelectEnd,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    BoldSelection,
    ItalicSelection,
    UnderlineSelection,
    CodeSelection,
    IndentBlock,
    OutdentBlock,
    ExitCodeBlock,
    SaveDocument,
    SaveDocumentAs,
    NewWindow,
    OpenFile,
    QuitApplication,
    DismissTransientUi,
    ToggleViewMode,
    ToggleWorkspace,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShortcutDefinition {
    pub(crate) command: ShortcutCommand,
    pub(crate) id: &'static str,
    pub(crate) category: ShortcutCategory,
    pub(crate) default_keys: &'static [&'static str],
    pub(crate) context: Option<&'static str>,
}

const BLOCK_CONTEXT: Option<&str> = Some("BlockEditor");

const SHORTCUT_DEFINITIONS: &[ShortcutDefinition] = &[
    ShortcutDefinition {
        command: ShortcutCommand::Newline,
        id: "newline",
        category: ShortcutCategory::Block,
        default_keys: &["enter"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::DeleteBack,
        id: "delete_back",
        category: ShortcutCategory::Edit,
        default_keys: &["backspace"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Delete,
        id: "delete",
        category: ShortcutCategory::Edit,
        default_keys: &["delete"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordDeleteBack,
        id: "word_delete_back",
        category: ShortcutCategory::Edit,
        default_keys: &["ctrl-backspace", "alt-backspace"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordDeleteForward,
        id: "word_delete_forward",
        category: ShortcutCategory::Edit,
        default_keys: &["ctrl-delete", "alt-delete"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FocusPrev,
        id: "focus_prev",
        category: ShortcutCategory::Navigation,
        default_keys: &["up"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::FocusNext,
        id: "focus_next",
        category: ShortcutCategory::Navigation,
        default_keys: &["down"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::MoveLeft,
        id: "move_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::MoveRight,
        id: "move_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordMoveLeft,
        id: "word_move_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-left", "alt-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordMoveRight,
        id: "word_move_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-right", "alt-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Home,
        id: "home",
        category: ShortcutCategory::Navigation,
        default_keys: &["home"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::End,
        id: "end",
        category: ShortcutCategory::Navigation,
        default_keys: &["end"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BlockUp,
        id: "block_up",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-up", "alt-up"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BlockDown,
        id: "block_down",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-down", "alt-down"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectLeft,
        id: "select_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectRight,
        id: "select_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordSelectLeft,
        id: "word_select_left",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-shift-left", "alt-shift-left"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::WordSelectRight,
        id: "word_select_right",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-shift-right", "alt-shift-right"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectHome,
        id: "select_home",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-home"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectEnd,
        id: "select_end",
        category: ShortcutCategory::Navigation,
        default_keys: &["shift-end"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SelectAll,
        id: "select_all",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-a", "ctrl-a"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Copy,
        id: "copy",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-c", "ctrl-c"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Cut,
        id: "cut",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-x", "ctrl-x"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Paste,
        id: "paste",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-v", "ctrl-v"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::Undo,
        id: "undo",
        category: ShortcutCategory::Edit,
        default_keys: &["cmd-z", "ctrl-z"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::BoldSelection,
        id: "bold_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-b", "ctrl-b"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ItalicSelection,
        id: "italic_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-i", "ctrl-i"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::UnderlineSelection,
        id: "underline_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-u", "ctrl-u"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::CodeSelection,
        id: "code_selection",
        category: ShortcutCategory::Formatting,
        default_keys: &["cmd-`", "ctrl-`"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::IndentBlock,
        id: "indent_block",
        category: ShortcutCategory::Block,
        default_keys: &["tab"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OutdentBlock,
        id: "outdent_block",
        category: ShortcutCategory::Block,
        default_keys: &["shift-tab"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ExitCodeBlock,
        id: "exit_code_block",
        category: ShortcutCategory::Block,
        default_keys: &["cmd-enter", "ctrl-enter"],
        context: BLOCK_CONTEXT,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SaveDocument,
        id: "save_document",
        category: ShortcutCategory::File,
        default_keys: &["cmd-s", "ctrl-s"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::SaveDocumentAs,
        id: "save_document_as",
        category: ShortcutCategory::File,
        default_keys: &["cmd-shift-s", "ctrl-shift-s"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::NewWindow,
        id: "new_window",
        category: ShortcutCategory::File,
        default_keys: &["cmd-n", "ctrl-n"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::OpenFile,
        id: "open_file",
        category: ShortcutCategory::File,
        default_keys: &["cmd-o", "ctrl-o"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::QuitApplication,
        id: "quit_application",
        category: ShortcutCategory::File,
        default_keys: &["cmd-q", "ctrl-q"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::DismissTransientUi,
        id: "dismiss_transient_ui",
        category: ShortcutCategory::Other,
        default_keys: &["escape"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleViewMode,
        id: "toggle_view_mode",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-tab", "cmd-tab"],
        context: None,
    },
    ShortcutDefinition {
        command: ShortcutCommand::ToggleWorkspace,
        id: "toggle_workspace",
        category: ShortcutCategory::Navigation,
        default_keys: &["ctrl-w", "cmd-w"],
        context: None,
    },
];

pub(crate) fn shortcut_definitions() -> &'static [ShortcutDefinition] {
    SHORTCUT_DEFINITIONS
}

pub(crate) fn normalize_shortcut_keys(keys: &[String]) -> Option<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for key in keys {
        let parsed = Keystroke::parse(key.trim()).ok()?;
        if parsed.is_ime_in_progress() {
            return None;
        }
        let key = parsed.unparse();
        if seen.insert(key.clone()) {
            normalized.push(key);
        }
    }
    (!normalized.is_empty()).then_some(normalized)
}

fn default_keys(definition: ShortcutDefinition) -> Vec<String> {
    definition
        .default_keys
        .iter()
        .map(|key| key.to_string())
        .collect()
}

fn shortcuts_conflict(
    left: ShortcutDefinition,
    left_keys: &[String],
    right: ShortcutDefinition,
    right_keys: &[String],
) -> bool {
    left.context == right.context && left_keys.iter().any(|key| right_keys.contains(key))
}

pub(crate) fn normalize_shortcut_config(
    config: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut effective: BTreeMap<&'static str, (bool, Vec<String>)> = BTreeMap::new();
    for definition in SHORTCUT_DEFINITIONS {
        let custom = config
            .get(definition.id)
            .and_then(|keys| normalize_shortcut_keys(keys));
        effective.insert(
            definition.id,
            match custom {
                Some(keys) if keys != default_keys(*definition) => (true, keys),
                _ => (false, default_keys(*definition)),
            },
        );
    }

    loop {
        let mut conflicted = BTreeSet::new();
        for (index, left) in SHORTCUT_DEFINITIONS.iter().enumerate() {
            let (left_custom, left_keys) = effective.get(left.id).expect("known shortcut");
            for right in SHORTCUT_DEFINITIONS.iter().skip(index + 1) {
                let (right_custom, right_keys) = effective.get(right.id).expect("known shortcut");
                if shortcuts_conflict(*left, left_keys, *right, right_keys) {
                    if *left_custom {
                        conflicted.insert(left.id);
                    }
                    if *right_custom {
                        conflicted.insert(right.id);
                    }
                }
            }
        }

        if conflicted.is_empty() {
            break;
        }

        for id in conflicted {
            if let Some(definition) = SHORTCUT_DEFINITIONS
                .iter()
                .find(|definition| definition.id == id)
            {
                effective.insert(definition.id, (false, default_keys(*definition)));
            }
        }
    }

    effective
        .into_iter()
        .filter_map(|(id, (custom, keys))| custom.then_some((id.to_string(), keys)))
        .collect()
}

pub(crate) fn resolved_shortcut_keys(
    config: &BTreeMap<String, Vec<String>>,
    command: ShortcutCommand,
) -> Vec<String> {
    let normalized = normalize_shortcut_config(config);
    let definition = SHORTCUT_DEFINITIONS
        .iter()
        .find(|definition| definition.command == command)
        .expect("known shortcut command");
    normalized
        .get(definition.id)
        .cloned()
        .unwrap_or_else(|| default_keys(*definition))
}

pub(crate) fn shortcut_conflict_for(
    command: ShortcutCommand,
    proposed_keys: &[String],
    config: &BTreeMap<String, Vec<String>>,
) -> Option<ShortcutDefinition> {
    let definition = SHORTCUT_DEFINITIONS
        .iter()
        .find(|definition| definition.command == command)?;
    let proposed_keys = normalize_shortcut_keys(proposed_keys)?;
    for other in SHORTCUT_DEFINITIONS
        .iter()
        .filter(|other| other.command != command)
    {
        let other_keys = resolved_shortcut_keys(config, other.command);
        if shortcuts_conflict(*definition, &proposed_keys, *other, &other_keys) {
            return Some(*other);
        }
    }
    None
}

fn key_binding_for(
    command: ShortcutCommand,
    key: &str,
    context: Option<&'static str>,
) -> KeyBinding {
    match command {
        ShortcutCommand::Newline => KeyBinding::new(key, Newline, context),
        ShortcutCommand::DeleteBack => KeyBinding::new(key, DeleteBack, context),
        ShortcutCommand::Delete => KeyBinding::new(key, Delete, context),
        ShortcutCommand::WordDeleteBack => KeyBinding::new(key, WordDeleteBack, context),
        ShortcutCommand::WordDeleteForward => KeyBinding::new(key, WordDeleteForward, context),
        ShortcutCommand::FocusPrev => KeyBinding::new(key, FocusPrev, context),
        ShortcutCommand::FocusNext => KeyBinding::new(key, FocusNext, context),
        ShortcutCommand::MoveLeft => KeyBinding::new(key, MoveLeft, context),
        ShortcutCommand::MoveRight => KeyBinding::new(key, MoveRight, context),
        ShortcutCommand::WordMoveLeft => KeyBinding::new(key, WordMoveLeft, context),
        ShortcutCommand::WordMoveRight => KeyBinding::new(key, WordMoveRight, context),
        ShortcutCommand::Home => KeyBinding::new(key, Home, context),
        ShortcutCommand::End => KeyBinding::new(key, End, context),
        ShortcutCommand::BlockUp => KeyBinding::new(key, BlockUp, context),
        ShortcutCommand::BlockDown => KeyBinding::new(key, BlockDown, context),
        ShortcutCommand::SelectLeft => KeyBinding::new(key, SelectLeft, context),
        ShortcutCommand::SelectRight => KeyBinding::new(key, SelectRight, context),
        ShortcutCommand::WordSelectLeft => KeyBinding::new(key, WordSelectLeft, context),
        ShortcutCommand::WordSelectRight => KeyBinding::new(key, WordSelectRight, context),
        ShortcutCommand::SelectHome => KeyBinding::new(key, SelectHome, context),
        ShortcutCommand::SelectEnd => KeyBinding::new(key, SelectEnd, context),
        ShortcutCommand::SelectAll => KeyBinding::new(key, SelectAll, context),
        ShortcutCommand::Copy => KeyBinding::new(key, Copy, context),
        ShortcutCommand::Cut => KeyBinding::new(key, Cut, context),
        ShortcutCommand::Paste => KeyBinding::new(key, Paste, context),
        ShortcutCommand::Undo => KeyBinding::new(key, Undo, context),
        ShortcutCommand::BoldSelection => KeyBinding::new(key, BoldSelection, context),
        ShortcutCommand::ItalicSelection => KeyBinding::new(key, ItalicSelection, context),
        ShortcutCommand::UnderlineSelection => KeyBinding::new(key, UnderlineSelection, context),
        ShortcutCommand::CodeSelection => KeyBinding::new(key, CodeSelection, context),
        ShortcutCommand::IndentBlock => KeyBinding::new(key, IndentBlock, context),
        ShortcutCommand::OutdentBlock => KeyBinding::new(key, OutdentBlock, context),
        ShortcutCommand::ExitCodeBlock => KeyBinding::new(key, ExitCodeBlock, context),
        ShortcutCommand::SaveDocument => KeyBinding::new(key, SaveDocument, context),
        ShortcutCommand::SaveDocumentAs => KeyBinding::new(key, SaveDocumentAs, context),
        ShortcutCommand::NewWindow => KeyBinding::new(key, NewWindow, context),
        ShortcutCommand::OpenFile => KeyBinding::new(key, OpenFile, context),
        ShortcutCommand::QuitApplication => KeyBinding::new(key, QuitApplication, context),
        ShortcutCommand::DismissTransientUi => KeyBinding::new(key, DismissTransientUi, context),
        ShortcutCommand::ToggleViewMode => KeyBinding::new(key, ToggleViewMode, context),
        ShortcutCommand::ToggleWorkspace => KeyBinding::new(key, ToggleWorkspace, context),
    }
}

pub(crate) fn resolved_keybindings(config: &BTreeMap<String, Vec<String>>) -> Vec<KeyBinding> {
    let normalized = normalize_shortcut_config(config);
    let mut bindings = Vec::new();
    for definition in SHORTCUT_DEFINITIONS {
        let keys = normalized
            .get(definition.id)
            .cloned()
            .unwrap_or_else(|| default_keys(*definition));
        bindings.extend(
            keys.iter()
                .map(|key| key_binding_for(definition.command, key, definition.context)),
        );
    }
    bindings
}

pub(crate) fn install_keybindings(cx: &mut App, config: &BTreeMap<String, Vec<String>>) {
    cx.bind_keys(resolved_keybindings(config));
}

/// Register key bindings for the block editor.
#[allow(dead_code)]
pub fn init(cx: &mut App) {
    install_keybindings(cx, &BTreeMap::new());
}

pub(crate) fn init_with_keybindings(cx: &mut App, config: &BTreeMap<String, Vec<String>>) {
    install_keybindings(cx, config);
}

#[cfg(test)]
mod tests {
    use super::{
        ShortcutCommand, normalize_shortcut_config, resolved_shortcut_keys, shortcut_conflict_for,
    };
    use std::collections::BTreeMap;

    #[test]
    fn custom_shortcut_replaces_command_defaults() {
        let mut config = BTreeMap::new();
        config.insert("save_document".to_string(), vec!["ctrl-alt-s".to_string()]);

        assert_eq!(
            resolved_shortcut_keys(&config, ShortcutCommand::SaveDocument),
            vec!["ctrl-alt-s".to_string()]
        );
    }

    #[test]
    fn toggle_view_mode_has_default_shortcuts() {
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleViewMode),
            vec!["ctrl-tab".to_string(), "cmd-tab".to_string()]
        );
    }

    #[test]
    fn toggle_workspace_has_default_shortcuts() {
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleWorkspace),
            vec!["ctrl-w".to_string(), "cmd-w".to_string()]
        );
    }

    #[test]
    fn word_and_block_shortcuts_have_ctrl_and_alt_defaults() {
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordMoveLeft),
            vec!["ctrl-left".to_string(), "alt-left".to_string()]
        );
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordDeleteBack),
            vec!["ctrl-backspace".to_string(), "alt-backspace".to_string()]
        );
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::BlockUp),
            vec!["ctrl-up".to_string(), "alt-up".to_string()]
        );
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordSelectRight),
            vec![
                "ctrl-shift-right".to_string(),
                "alt-shift-right".to_string()
            ]
        );
    }

    #[test]
    fn invalid_or_empty_shortcuts_fall_back_to_defaults() {
        let mut config = BTreeMap::new();
        config.insert("save_document".to_string(), vec!["".to_string()]);
        config.insert("open_file".to_string(), vec!["a".to_string()]);

        let normalized = normalize_shortcut_config(&config);
        assert!(!normalized.contains_key("save_document"));
        assert!(!normalized.contains_key("open_file"));
    }

    #[test]
    fn conflicting_custom_shortcut_falls_back_to_default() {
        let mut config = BTreeMap::new();
        config.insert("copy".to_string(), vec!["ctrl-x".to_string()]);

        let normalized = normalize_shortcut_config(&config);
        assert!(!normalized.contains_key("copy"));
        assert_eq!(
            resolved_shortcut_keys(&config, ShortcutCommand::Copy),
            vec!["cmd-c".to_string(), "ctrl-c".to_string()]
        );
    }

    #[test]
    fn detects_shortcut_conflicts_for_preferences_drafts() {
        let conflict = shortcut_conflict_for(
            ShortcutCommand::Copy,
            &["ctrl-x".to_string()],
            &BTreeMap::new(),
        )
        .expect("copy should conflict with cut");

        assert_eq!(conflict.id, "cut");
    }
}
