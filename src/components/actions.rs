//! Action definitions and key bindings for both block editing and app-level
//! window/menu commands.
//!
//! Text-editing actions are scoped to the `"BlockEditor"` key context on each
//! block. Window and menu commands use global bindings so they remain
//! available even when focus is on non-block UI such as dialogs or buttons.

use gpui::*;
use schemars::JsonSchema;
use serde::Deserialize;

actions!(
    velotype,
    [
        Newline,
        DeleteBack,
        Delete,
        FocusPrev,
        FocusNext,
        MoveLeft,
        MoveRight,
        Home,
        End,
        SelectLeft,
        SelectRight,
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

/// Register key bindings for the block editor.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Newline, Some("BlockEditor")),
        KeyBinding::new("backspace", DeleteBack, Some("BlockEditor")),
        KeyBinding::new("delete", Delete, Some("BlockEditor")),
        KeyBinding::new("up", FocusPrev, Some("BlockEditor")),
        KeyBinding::new("down", FocusNext, Some("BlockEditor")),
        KeyBinding::new("left", MoveLeft, Some("BlockEditor")),
        KeyBinding::new("right", MoveRight, Some("BlockEditor")),
        KeyBinding::new("home", Home, Some("BlockEditor")),
        KeyBinding::new("end", End, Some("BlockEditor")),
        KeyBinding::new("shift-left", SelectLeft, Some("BlockEditor")),
        KeyBinding::new("shift-right", SelectRight, Some("BlockEditor")),
        KeyBinding::new("shift-home", SelectHome, Some("BlockEditor")),
        KeyBinding::new("shift-end", SelectEnd, Some("BlockEditor")),
        KeyBinding::new("cmd-a", SelectAll, Some("BlockEditor")),
        KeyBinding::new("cmd-c", Copy, Some("BlockEditor")),
        KeyBinding::new("cmd-x", Cut, Some("BlockEditor")),
        KeyBinding::new("cmd-v", Paste, Some("BlockEditor")),
        KeyBinding::new("cmd-z", Undo, Some("BlockEditor")),
        KeyBinding::new("cmd-b", BoldSelection, Some("BlockEditor")),
        KeyBinding::new("cmd-i", ItalicSelection, Some("BlockEditor")),
        KeyBinding::new("cmd-u", UnderlineSelection, Some("BlockEditor")),
        KeyBinding::new("cmd-`", CodeSelection, Some("BlockEditor")),
        KeyBinding::new("ctrl-`", CodeSelection, Some("BlockEditor")),
        KeyBinding::new("tab", IndentBlock, Some("BlockEditor")),
        KeyBinding::new("shift-tab", OutdentBlock, Some("BlockEditor")),
        KeyBinding::new("cmd-enter", ExitCodeBlock, Some("BlockEditor")),
        KeyBinding::new("ctrl-enter", ExitCodeBlock, Some("BlockEditor")),
        KeyBinding::new("ctrl-a", SelectAll, Some("BlockEditor")),
        KeyBinding::new("ctrl-c", Copy, Some("BlockEditor")),
        KeyBinding::new("ctrl-x", Cut, Some("BlockEditor")),
        KeyBinding::new("ctrl-v", Paste, Some("BlockEditor")),
        KeyBinding::new("ctrl-z", Undo, Some("BlockEditor")),
        KeyBinding::new("ctrl-b", BoldSelection, Some("BlockEditor")),
        KeyBinding::new("ctrl-i", ItalicSelection, Some("BlockEditor")),
        KeyBinding::new("ctrl-u", UnderlineSelection, Some("BlockEditor")),
        KeyBinding::new("cmd-s", SaveDocument, None),
        KeyBinding::new("ctrl-s", SaveDocument, None),
        KeyBinding::new("cmd-shift-s", SaveDocumentAs, None),
        KeyBinding::new("ctrl-shift-s", SaveDocumentAs, None),
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("ctrl-n", NewWindow, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("ctrl-o", OpenFile, None),
        KeyBinding::new("cmd-q", QuitApplication, None),
        KeyBinding::new("ctrl-q", QuitApplication, None),
        KeyBinding::new("escape", DismissTransientUi, None),
    ]);
}
