//! Persistent app preferences and the preferences window.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use futures::channel::oneshot;

use anyhow::Context as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde::{Deserialize, Serialize};

use super::{VelotypeConfigDirs, read_recent_files};
use crate::components::single_line_input::{InputChanged, SingleLineInput, SingleLineOverflow};
use crate::components::{
    ShortcutCategory, ShortcutCommand, ShortcutDefinition, install_keybindings,
    normalize_shortcut_config, normalize_shortcut_keys, resolved_shortcut_keys,
    shortcut_conflict_for, shortcut_definitions, switch::Switch,
};
use crate::fonts::{FontCatalog, FontPreferences, FontSettings, FontStackParseError};
use crate::i18n::{I18nManager, I18nStrings, language_id_for_locale_preferences};
use crate::theme::{Theme, ThemeCatalogEntry, ThemeManager};
use crate::window_chrome::{
    custom_titlebar_height, render_custom_titlebar, velotype_window_options,
};

/// Built-in dark theme used when a configured theme is absent or unavailable.
const DEFAULT_THEME_ID: &str = "velotype";
/// Language fallback for ordinary preference reads without locale detection.
const DEFAULT_LANGUAGE_ID: &str = "en-US";

/// A user-configurable button shown in the status bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StatusBarButton {
    /// Stable identifier for the configured button.
    pub id: String,
    /// Text displayed on the button.
    pub label: String,
    /// Action identifier stored with the button configuration.
    pub action_id: String,
}

/// Status bar visibility and component toggles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusBarPreferences {
    /// Whether the status bar is visible.
    pub enabled: bool,
    /// Whether to display the document word count.
    pub show_word_count: bool,
    /// Whether to display the cursor's line and column.
    pub show_cursor_position: bool,
    /// Whether to display the workspace drawer toggle.
    pub show_sidebar_toggle: bool,
    /// Whether to display the source/rendered mode switch.
    pub show_mode_switch: bool,
    /// Additional button definitions retained in the preferences file.
    pub custom_buttons: Vec<StatusBarButton>,
}

impl Default for StatusBarPreferences {
    /// Enables the bar and all built-in items, with no custom buttons.
    fn default() -> Self {
        Self {
            enabled: true,
            show_word_count: true,
            show_cursor_position: true,
            show_sidebar_toggle: true,
            show_mode_switch: true,
            custom_buttons: Vec::new(),
        }
    }
}

/// Startup document selection stored in `config.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupOpenPreference {
    /// Start with an empty document.
    NewFile,
    /// Try the first existing recent file, falling back to an empty document.
    LastOpenedFile,
}

impl StartupOpenPreference {
    /// Returns the stable TOML value for this startup mode.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NewFile => "new_file",
            Self::LastOpenedFile => "last_opened_file",
        }
    }

    /// Parses a persisted value, using a new document for unrecognized values.
    fn from_str(value: &str) -> Self {
        match value {
            "last_opened_file" => Self::LastOpenedFile,
            _ => Self::NewFile,
        }
    }
}

/// Where pasted clipboard images should be stored before inserting Markdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImagePasteBehavior {
    /// Keep existing image paths; write clipboard image bytes into the paste root.
    None,
    /// Copy into the document directory, or the working directory for unsaved files.
    CopyToDocumentFolder,
    /// Copy into an `assets` subdirectory of the document or working directory.
    CopyToAssetsFolder,
    /// Copy into `<document-stem>.assets`, choosing an unused `untitle` name if unsaved.
    CopyToNamedAssetsFolder,
    /// Upload through the configured local PicGo server.
    UsePicGo,
}

impl ImagePasteBehavior {
    /// Returns the stable TOML value for this image storage policy.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CopyToDocumentFolder => "copy_to_document_folder",
            Self::CopyToAssetsFolder => "copy_to_assets_folder",
            Self::CopyToNamedAssetsFolder => "copy_to_named_assets_folder",
            Self::UsePicGo => "use_picgo",
        }
    }

    /// Parses a persisted policy, disabling file copying for unrecognized values.
    fn from_str(value: &str) -> Self {
        match value {
            "copy_to_document_folder" => Self::CopyToDocumentFolder,
            "copy_to_assets_folder" => Self::CopyToAssetsFolder,
            "copy_to_named_assets_folder" => Self::CopyToNamedAssetsFolder,
            "use_picgo" => Self::UsePicGo,
            _ => Self::None,
        }
    }

    /// A failed upload must fall back to a local policy, never retry PicGo.
    fn local_only(self) -> Self {
        if self == Self::UsePicGo {
            Self::None
        } else {
            self
        }
    }
}

/// The default loopback endpoint of PicGo's built-in server.
pub(crate) const DEFAULT_PICGO_SERVER_URL: &str = "http://127.0.0.1:36677";

/// User preferences persisted under the app config directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppPreferences {
    /// Document selection policy used at startup.
    pub(crate) startup_open: StartupOpenPreference,
    /// Selected built-in or imported UI language identifier.
    pub(crate) default_language_id: String,
    /// Selected built-in or imported theme identifier.
    pub(crate) default_theme_id: String,
    /// Whether table top rows receive header styling.
    pub(crate) show_table_headers: bool,
    /// Storage policy for images inserted from the clipboard.
    pub(crate) image_paste_behavior: ImagePasteBehavior,
    /// Last chosen local policy used when a PicGo upload fails.
    pub(crate) image_paste_fallback: ImagePasteBehavior,
    /// User-supplied loopback PicGo Server address.
    pub(crate) picgo_server_url: String,
    /// Whether new stable releases may be downloaded automatically after startup checks.
    pub(crate) auto_download_updates: bool,
    /// Command identifiers mapped to user-supplied shortcut lists.
    pub(crate) keybindings: BTreeMap<String, Vec<String>>,
    /// Status bar visibility, built-in items, and stored custom buttons.
    pub(crate) status_bar: StatusBarPreferences,
    /// Unresolved family lists for body, code, and UI text.
    pub(crate) fonts: FontPreferences,
}

impl Default for AppPreferences {
    /// Uses a new document, English, the dark theme, and built-in editor defaults.
    fn default() -> Self {
        Self {
            startup_open: StartupOpenPreference::NewFile,
            default_language_id: DEFAULT_LANGUAGE_ID.into(),
            default_theme_id: DEFAULT_THEME_ID.into(),
            show_table_headers: true,
            image_paste_behavior: ImagePasteBehavior::None,
            image_paste_fallback: ImagePasteBehavior::None,
            picgo_server_url: DEFAULT_PICGO_SERVER_URL.into(),
            auto_download_updates: false,
            keybindings: BTreeMap::new(),
            status_bar: StatusBarPreferences::default(),
            fonts: FontPreferences::default(),
        }
    }
}

/// In-memory visibility flags read by the status bar renderer.
struct StatusBarSettings {
    /// Visibility of the entire status bar.
    status_bar_enabled: bool,
    /// Visibility of the word count item.
    status_bar_show_word_count: bool,
    /// Visibility of the cursor position item.
    status_bar_show_cursor_position: bool,
    /// Visibility of the workspace drawer toggle.
    status_bar_show_sidebar_toggle: bool,
    /// Visibility of the source/rendered mode switch.
    status_bar_show_mode_switch: bool,
}

/// Runtime-accessible editor settings mirrored from [`AppPreferences`] so the
/// render path can read them without touching disk. Toggling persists the new
/// value back to the preferences file.
pub struct EditorSettings {
    /// Whether native tables style their top row as a header.
    show_table_headers: bool,
    /// Status bar flags mirrored from persisted preferences.
    status_bar_settings: StatusBarSettings,
}

impl Global for EditorSettings {}

impl EditorSettings {
    /// Installs table styling and status bar flags, defaulting unreadable bar settings.
    pub fn init(cx: &mut App, show_table_headers: bool) {
        let status_bar = read_app_preferences()
            .ok()
            .map(|p| p.status_bar)
            .unwrap_or_default();
        Self::set_global(cx, show_table_headers, &status_bar);
    }

    /// Replaces the runtime flags without reading or writing preferences.
    fn set_global(cx: &mut App, show_table_headers: bool, status_bar: &StatusBarPreferences) {
        cx.set_global(Self {
            show_table_headers,
            status_bar_settings: StatusBarSettings {
                status_bar_enabled: status_bar.enabled,
                status_bar_show_word_count: status_bar.show_word_count,
                status_bar_show_cursor_position: status_bar.show_cursor_position,
                status_bar_show_sidebar_toggle: status_bar.show_sidebar_toggle,
                status_bar_show_mode_switch: status_bar.show_mode_switch,
            },
        });
    }

    /// Whether table top rows are styled as headers. Defaults to `true` when
    /// the global has not been installed (e.g. in unit tests).
    pub fn show_table_headers(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.show_table_headers)
            .unwrap_or(true)
    }

    /// Updates table styling immediately and attempts to persist it, logging failures.
    pub fn set_show_table_headers(cx: &mut App, show_table_headers: bool) {
        let status_bar = cx
            .try_global::<Self>()
            .map(|s| StatusBarPreferences {
                enabled: s.status_bar_settings.status_bar_enabled,
                show_word_count: s.status_bar_settings.status_bar_show_word_count,
                show_cursor_position: s.status_bar_settings.status_bar_show_cursor_position,
                show_sidebar_toggle: s.status_bar_settings.status_bar_show_sidebar_toggle,
                show_mode_switch: s.status_bar_settings.status_bar_show_mode_switch,
                custom_buttons: Vec::new(),
            })
            .unwrap_or_default();
        Self::set_global(cx, show_table_headers, &status_bar);
        match read_app_preferences() {
            Ok(mut preferences) => {
                preferences.show_table_headers = show_table_headers;
                if let Err(err) = save_app_preferences(&preferences) {
                    eprintln!("failed to save table header preference: {err}");
                }
            }
            Err(err) => eprintln!("failed to read table header preference: {err}"),
        }
    }

    /// Returns runtime bar flags or defaults; custom buttons are not stored globally.
    pub fn status_bar_preferences(cx: &App) -> StatusBarPreferences {
        cx.try_global::<Self>()
            .map(|s| StatusBarPreferences {
                enabled: s.status_bar_settings.status_bar_enabled,
                show_word_count: s.status_bar_settings.status_bar_show_word_count,
                show_cursor_position: s.status_bar_settings.status_bar_show_cursor_position,
                show_sidebar_toggle: s.status_bar_settings.status_bar_show_sidebar_toggle,
                show_mode_switch: s.status_bar_settings.status_bar_show_mode_switch,
                custom_buttons: Vec::new(),
            })
            .unwrap_or_default()
    }
}

/// TOML output shape for normalized app preferences.
#[derive(Serialize)]
struct PreferencesFile {
    /// Startup document selection table.
    startup: StartupPreferencesFile,
    /// Default UI language table.
    language: LanguagePreferencesFile,
    /// Default theme table.
    theme: ThemePreferencesFile,
    /// Editor behavior table.
    editor: EditorPreferencesFile,
    /// Optional background update download policy.
    updates: UpdatePreferencesFile,
    /// Status bar configuration table.
    status_bar: StatusBarPreferencesFile,
    /// Normalized user shortcut overrides.
    keybindings: BTreeMap<String, Vec<String>>,
    /// Font family lists table.
    fonts: FontPreferencesFile,
}

/// Persisted update policy; installation always requires an explicit click.
#[derive(Serialize)]
struct UpdatePreferencesFile {
    auto_download: bool,
}

/// Persisted font family lists, kept as user-editable strings.
#[derive(Serialize)]
struct FontPreferencesFile {
    /// Body text family list.
    body_stack: String,
    /// Code text family list.
    code_stack: String,
    /// Application UI family list.
    ui_stack: String,
}

impl From<&FontPreferences> for FontPreferencesFile {
    /// Copies unresolved font preferences into their TOML representation.
    fn from(value: &FontPreferences) -> Self {
        Self {
            body_stack: value.body_stack.clone(),
            code_stack: value.code_stack.clone(),
            ui_stack: value.ui_stack.clone(),
        }
    }
}

/// Persisted startup document policy.
#[derive(Serialize)]
struct StartupPreferencesFile {
    /// Serialized startup mode identifier.
    open: String,
}

/// Persisted editor rendering and paste settings.
#[derive(Serialize)]
struct EditorPreferencesFile {
    /// Whether to style the first table row as a header.
    show_table_headers: bool,
    /// Serialized clipboard image storage policy.
    image_paste_behavior: String,
    /// Persisted local fallback policy for PicGo failures.
    image_paste_fallback: String,
    /// PicGo's local HTTP server URL.
    picgo_server_url: String,
}

/// Persisted default UI language selection.
#[derive(Serialize)]
struct LanguagePreferencesFile {
    /// Built-in or imported language identifier.
    default_language_id: String,
}

/// Persisted default theme selection.
#[derive(Serialize)]
struct ThemePreferencesFile {
    /// Built-in or imported theme identifier.
    default_theme_id: String,
}

/// TOML status bar settings, omitting an empty custom button list.
#[derive(Serialize)]
struct StatusBarPreferencesFile {
    /// Visibility of the entire bar.
    enabled: bool,
    /// Visibility of the word count.
    show_word_count: bool,
    /// Visibility of the cursor position.
    show_cursor_position: bool,
    /// Visibility of the workspace drawer toggle.
    show_sidebar_toggle: bool,
    /// Visibility of the source/rendered mode switch.
    show_mode_switch: bool,
    /// Stored custom button definitions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    custom_buttons: Vec<StatusBarButton>,
}

impl From<&StatusBarPreferences> for StatusBarPreferencesFile {
    /// Copies visibility flags and custom buttons into the TOML output shape.
    fn from(value: &StatusBarPreferences) -> Self {
        Self {
            enabled: value.enabled,
            show_word_count: value.show_word_count,
            show_cursor_position: value.show_cursor_position,
            show_sidebar_toggle: value.show_sidebar_toggle,
            show_mode_switch: value.show_mode_switch,
            custom_buttons: value.custom_buttons.clone(),
        }
    }
}

impl From<&AppPreferences> for PreferencesFile {
    /// Converts enum values and normalizes shortcuts for TOML serialization.
    fn from(value: &AppPreferences) -> Self {
        Self {
            startup: StartupPreferencesFile {
                open: value.startup_open.as_str().into(),
            },
            language: LanguagePreferencesFile {
                default_language_id: value.default_language_id.clone(),
            },
            theme: ThemePreferencesFile {
                default_theme_id: value.default_theme_id.clone(),
            },
            editor: EditorPreferencesFile {
                show_table_headers: value.show_table_headers,
                image_paste_behavior: value.image_paste_behavior.as_str().into(),
                image_paste_fallback: value.image_paste_fallback.local_only().as_str().into(),
                picgo_server_url: value.picgo_server_url.clone(),
            },
            status_bar: StatusBarPreferencesFile::from(&value.status_bar),
            updates: UpdatePreferencesFile {
                auto_download: value.auto_download_updates,
            },
            keybindings: normalize_shortcut_config(&value.keybindings),
            fonts: FontPreferencesFile::from(&value.fonts),
        }
    }
}

/// Reads system preferences without writing, using English for a missing language.
pub(crate) fn read_app_preferences() -> anyhow::Result<AppPreferences> {
    read_app_preferences_with_dirs(&VelotypeConfigDirs::from_system()?)
}

/// Reads preferences from the supplied root; missing or malformed TOML uses defaults.
pub(crate) fn read_app_preferences_with_dirs(
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<AppPreferences> {
    let path = dirs.app_config_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppPreferences::default());
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Ok(AppPreferences::default());
    };

    Ok(app_preferences_from_toml_value(&value, DEFAULT_LANGUAGE_ID))
}

/// Loads and rewrites normalized preferences, detecting an unset language from locales.
pub(crate) fn load_or_create_app_preferences() -> anyhow::Result<AppPreferences> {
    let dirs = VelotypeConfigDirs::from_system()?;
    load_or_create_app_preferences_with_dirs_and_locales(&dirs, sys_locale::get_locales())
}

/// Extracts known settings with per-field defaults and independent font validation.
fn app_preferences_from_toml_value(
    value: &toml::Value,
    fallback_language_id: &str,
) -> AppPreferences {
    let startup_open = value
        .get("startup")
        .and_then(|startup| startup.get("open"))
        .and_then(|open| open.as_str())
        .map(StartupOpenPreference::from_str)
        .unwrap_or(StartupOpenPreference::NewFile);
    let default_language_id = value
        .get("language")
        .and_then(|language| language.get("default_language_id"))
        .and_then(|id| id.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(fallback_language_id)
        .to_string();
    let default_theme_id = value
        .get("theme")
        .and_then(|theme| theme.get("default_theme_id"))
        .and_then(|id| id.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_THEME_ID)
        .to_string();
    let keybindings = value
        .get("keybindings")
        .and_then(|keybindings| keybindings.as_table())
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, value)| {
                    let keys = value
                        .as_array()?
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    Some((key.clone(), keys))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .map(|keybindings| normalize_shortcut_config(&keybindings))
        .unwrap_or_default();

    let show_table_headers = value
        .get("editor")
        .and_then(|editor| editor.get("show_table_headers"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let image_paste_behavior = value
        .get("editor")
        .and_then(|editor| editor.get("image_paste_behavior"))
        .and_then(|value| value.as_str())
        .map(ImagePasteBehavior::from_str)
        .unwrap_or(ImagePasteBehavior::None);
    let image_paste_fallback = value
        .get("editor")
        .and_then(|editor| editor.get("image_paste_fallback"))
        .and_then(|value| value.as_str())
        .map(ImagePasteBehavior::from_str)
        .unwrap_or(image_paste_behavior)
        .local_only();
    let picgo_server_url = value
        .get("editor")
        .and_then(|editor| editor.get("picgo_server_url"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_PICGO_SERVER_URL)
        .to_string();

    let status_bar = value
        .get("status_bar")
        .map(|sb| {
            let enabled = sb.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let show_word_count = sb
                .get("show_word_count")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_cursor_position = sb
                .get("show_cursor_position")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_sidebar_toggle = sb
                .get("show_sidebar_toggle")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_mode_switch = sb
                .get("show_mode_switch")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let custom_buttons = sb
                .get("custom_buttons")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let id = item.get("id")?.as_str()?.to_string();
                            let label = item.get("label")?.as_str()?.to_string();
                            Some(StatusBarButton {
                                id,
                                label,
                                action_id: item
                                    .get("action_id")
                                    .and_then(|a| a.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            StatusBarPreferences {
                enabled,
                show_word_count,
                show_cursor_position,
                show_sidebar_toggle,
                show_mode_switch,
                custom_buttons,
            }
        })
        .unwrap_or_default();

    let default_fonts = FontPreferences::default();
    let font_value = |key: &str, fallback: &str| {
        let candidate = value
            .get("fonts")
            .and_then(|fonts| fonts.get(key))
            .and_then(|value| value.as_str())
            .unwrap_or(fallback);
        if crate::fonts::parse_font_stack(candidate).is_ok() {
            candidate.to_string()
        } else {
            fallback.to_string()
        }
    };
    let fonts = FontPreferences {
        body_stack: font_value("body_stack", &default_fonts.body_stack),
        code_stack: font_value("code_stack", &default_fonts.code_stack),
        ui_stack: font_value("ui_stack", &default_fonts.ui_stack),
    };

    AppPreferences {
        startup_open,
        default_language_id,
        default_theme_id,
        show_table_headers,
        image_paste_behavior,
        image_paste_fallback,
        picgo_server_url,
        auto_download_updates: value
            .get("updates")
            .and_then(|updates| updates.get("auto_download"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        keybindings,
        status_bar,
        fonts,
    }
}

/// Maps preferred system locales to a supported built-in language.
fn detected_language_id_from_locales<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    language_id_for_locale_preferences(locales)
}

/// Loads or repairs preferences using locale fallback, then saves the normalized result.
fn load_or_create_app_preferences_with_dirs_and_locales<I, S>(
    dirs: &VelotypeConfigDirs,
    locales: I,
) -> anyhow::Result<AppPreferences>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let detected_language_id = detected_language_id_from_locales(locales);
    let path = dirs.app_config_file();
    let preferences = match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<toml::Value>(&text)
            .map(|value| app_preferences_from_toml_value(&value, detected_language_id))
            .unwrap_or_else(|_| AppPreferences {
                default_language_id: detected_language_id.into(),
                ..AppPreferences::default()
            }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => AppPreferences {
            default_language_id: detected_language_id.into(),
            ..AppPreferences::default()
        },
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };
    save_app_preferences_with_dirs(&preferences, dirs)?;
    Ok(preferences)
}

/// Writes normalized preferences to the platform configuration directory.
pub(crate) fn save_app_preferences(preferences: &AppPreferences) -> anyhow::Result<()> {
    save_app_preferences_with_dirs(preferences, &VelotypeConfigDirs::from_system()?)
}

/// Creates the config directory as needed and writes pretty-printed TOML preferences.
pub(crate) fn save_app_preferences_with_dirs(
    preferences: &AppPreferences,
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<()> {
    let path = dirs.app_config_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let text = toml::to_string_pretty(&PreferencesFile::from(preferences))?;
    std::fs::write(&path, text).with_context(|| format!("failed to write '{}'", path.display()))
}

/// Returns the first history path that is a file, without checking its extension.
/// Unreadable history yields no startup file.
pub(crate) fn first_existing_recent_markdown_file() -> Option<PathBuf> {
    let recent_files = read_recent_files().ok()?;
    recent_files.into_iter().find(|path| path.is_file())
}

/// Activates and persists a recognized language, returning whether its ID changed.
/// Unknown IDs return `false`; persistence errors occur after runtime selection.
pub(crate) fn apply_configured_language(cx: &mut App, language_id: &str) -> anyhow::Result<bool> {
    let mut applied = false;
    let changed = cx.update_global::<I18nManager, _>(|i18n_manager, _cx| {
        let changed = i18n_manager.set_language_by_id(language_id);
        applied = changed || i18n_manager.current_language_id() == language_id;
        changed
    });
    if !applied {
        return Ok(false);
    }
    update_app_preferences(|preferences| {
        preferences.default_language_id = language_id.into();
    })?;
    Ok(changed)
}

/// Activates and persists a theme, returning `true` for any recognized ID.
/// Persistence errors occur after runtime selection; unknown IDs return `false`.
pub(crate) fn apply_configured_theme(cx: &mut App, theme_id: &str) -> anyhow::Result<bool> {
    let mut applied = false;
    let changed = cx.update_global::<ThemeManager, _>(|theme_manager, cx| {
        theme_manager.set_system_appearance(cx.window_appearance());
        let changed = theme_manager.set_theme_by_id(theme_id);
        applied = changed || theme_manager.current_theme_id() == theme_id;
        changed
    });
    if !applied {
        return Ok(false);
    }
    update_app_preferences(|preferences| {
        preferences.default_theme_id = theme_id.into();
    })?;
    Ok(changed)
}

/// Imports and activates a language pack, then persists its ID as the default.
pub(crate) fn import_language_config_and_select(
    cx: &mut App,
    path: impl AsRef<std::path::Path>,
) -> anyhow::Result<String> {
    let imported_id = cx.update_global::<I18nManager, _>(|i18n_manager, _cx| {
        i18n_manager.import_language_config(path)
    })?;
    update_app_preferences(|preferences| {
        preferences.default_language_id = imported_id.clone();
    })?;
    Ok(imported_id)
}

/// Imports and activates a theme pack, then persists its ID as the default.
pub(crate) fn import_theme_config_and_select(
    cx: &mut App,
    path: impl AsRef<std::path::Path>,
) -> anyhow::Result<String> {
    let imported_id = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
        theme_manager.import_theme_config(path)
    })?;
    update_app_preferences(|preferences| {
        preferences.default_theme_id = imported_id.clone();
    })?;
    Ok(imported_id)
}

/// Persists editable window settings while retaining language and table-header settings.
pub(crate) fn save_preferences_from_window(
    startup_open: StartupOpenPreference,
    default_theme_id: &str,
    image_paste_behavior: ImagePasteBehavior,
    image_paste_fallback: ImagePasteBehavior,
    picgo_server_url: &str,
    auto_download_updates: bool,
    keybindings: BTreeMap<String, Vec<String>>,
    status_bar: &StatusBarPreferences,
    fonts: &FontPreferences,
) -> anyhow::Result<AppPreferences> {
    let dirs = VelotypeConfigDirs::from_system()?;
    save_preferences_from_window_with_dirs(
        startup_open,
        default_theme_id,
        image_paste_behavior,
        image_paste_fallback,
        picgo_server_url,
        auto_download_updates,
        keybindings,
        status_bar,
        fonts,
        &dirs,
    )
}

/// Updates window-owned fields in the supplied config root and returns the saved values.
fn save_preferences_from_window_with_dirs(
    startup_open: StartupOpenPreference,
    default_theme_id: &str,
    image_paste_behavior: ImagePasteBehavior,
    image_paste_fallback: ImagePasteBehavior,
    picgo_server_url: &str,
    auto_download_updates: bool,
    keybindings: BTreeMap<String, Vec<String>>,
    status_bar: &StatusBarPreferences,
    fonts: &FontPreferences,
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<AppPreferences> {
    let mut preferences =
        load_or_create_app_preferences_with_dirs_and_locales(dirs, sys_locale::get_locales())?;
    preferences.startup_open = startup_open;
    preferences.default_theme_id = default_theme_id.into();
    preferences.image_paste_behavior = image_paste_behavior;
    preferences.image_paste_fallback = image_paste_fallback.local_only();
    preferences.picgo_server_url = picgo_server_url.to_string();
    preferences.auto_download_updates = auto_download_updates;
    preferences.keybindings = normalize_shortcut_config(&keybindings);
    preferences.status_bar = status_bar.clone();
    preferences.fonts = fonts.clone();
    save_app_preferences_with_dirs(&preferences, dirs)?;
    Ok(preferences)
}

/// Loads normalized preferences, applies one mutation, and saves the result.
fn update_app_preferences(
    update: impl FnOnce(&mut AppPreferences),
) -> anyhow::Result<AppPreferences> {
    let mut preferences = load_or_create_app_preferences()?;
    update(&mut preferences);
    save_app_preferences(&preferences)?;
    Ok(preferences)
}

/// Preferences page currently displayed in the navigation pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreferencesNav {
    /// Application update download policy.
    Settings,
    /// Startup document selection.
    File,
    /// Theme selection and font family lists.
    Theme,
    /// Clipboard image storage policy.
    Image,
    /// Keyboard shortcut overrides.
    Shortcuts,
    /// Status bar visibility controls.
    StatusBar,
}

/// Non-uploading connectivity check for the configured local PicGo Server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PicGoStatus {
    Checking,
    Available,
    Legacy,
    Unavailable,
}

/// Checks body, code, and UI availability; syntax errors are handled separately.
fn font_uses_system_default(fonts: &FontPreferences, catalog: &FontCatalog) -> [bool; 3] {
    [
        FontSettings::stack_uses_system_default(&fonts.body_stack, catalog, std::env::consts::OS)
            .unwrap_or(false),
        FontSettings::stack_uses_system_default(&fonts.code_stack, catalog, std::env::consts::OS)
            .unwrap_or(false),
        FontSettings::stack_uses_system_default(&fonts.ui_stack, catalog, std::env::consts::OS)
            .unwrap_or(false),
    ]
}

/// Independent preferences window view.
pub(crate) struct PreferencesWindow {
    /// Active page in the navigation pane.
    nav: PreferencesNav,
    /// Draft startup document policy.
    startup_open: StartupOpenPreference,
    /// Draft theme selection.
    selected_theme_id: String,
    /// Draft clipboard image storage policy.
    image_paste_behavior: ImagePasteBehavior,
    /// Local policy to use if PicGo cannot upload a batch.
    image_paste_fallback: ImagePasteBehavior,
    /// Draft PicGo Server URL, edited in a separate input.
    picgo_server_url: String,
    /// Draft automatic download policy and its last saved value.
    auto_download_updates: bool,
    saved_auto_download_updates: bool,
    /// Effective OS association state, independent of the unsaved preference draft.
    file_associations: FileAssociationStatus,
    /// Keeps the current check alive without doing OS work during rendering.
    file_association_task: Option<Task<()>>,
    /// Draft shortcut overrides.
    keybindings: BTreeMap<String, Vec<String>>,
    /// Startup policy at the last successful save.
    saved_startup_open: StartupOpenPreference,
    /// Theme ID at the last successful save.
    saved_theme_id: String,
    /// Image policy at the last successful save.
    saved_image_paste_behavior: ImagePasteBehavior,
    /// Saved fallback and server address for dirty-state comparison.
    saved_image_paste_fallback: ImagePasteBehavior,
    saved_picgo_server_url: String,
    /// Result of the latest local PicGo Server probe.
    picgo_status: PicGoStatus,
    /// Discards results from checks started before the latest URL change.
    picgo_check_generation: u64,
    /// Cancels superseded checks while the address field is being edited.
    picgo_check_task: Option<Task<()>>,
    /// Shortcut overrides used as the dirty-state baseline.
    saved_keybindings: BTreeMap<String, Vec<String>>,
    /// Theme catalog snapshot captured when this window opens.
    theme_options: Vec<ThemeCatalogEntry>,
    /// Window-level focus target and shortcut recording receiver.
    focus_handle: FocusHandle,
    /// Whether startup policy options are expanded.
    startup_dropdown_open: bool,
    /// Whether theme options are expanded.
    theme_dropdown_open: bool,
    /// Whether image storage options are expanded.
    image_dropdown_open: bool,
    /// Command awaiting a replacement keystroke.
    recording_shortcut: Option<ShortcutCommand>,
    /// Localized invalid-key or conflict message for shortcut recording.
    shortcut_error: Option<String>,
    /// Draft visibility of the status bar.
    status_bar_enabled: bool,
    /// Draft visibility of word count.
    status_bar_show_word_count: bool,
    /// Draft visibility of cursor position.
    status_bar_show_cursor_position: bool,
    /// Draft visibility of the workspace toggle.
    status_bar_show_sidebar_toggle: bool,
    /// Draft visibility of the view-mode switch.
    status_bar_show_mode_switch: bool,
    /// Saved bar visibility for dirty-state comparison.
    saved_status_bar_enabled: bool,
    /// Saved word-count visibility for dirty-state comparison.
    saved_status_bar_show_word_count: bool,
    /// Saved cursor-position visibility for dirty-state comparison.
    saved_status_bar_show_cursor_position: bool,
    /// Saved workspace-toggle visibility for dirty-state comparison.
    saved_status_bar_show_sidebar_toggle: bool,
    /// Saved mode-switch visibility for dirty-state comparison.
    saved_status_bar_show_mode_switch: bool,
    /// Draft family lists synchronized from the text inputs.
    fonts: FontPreferences,
    /// Font family lists at the last successful save.
    saved_fonts: FontPreferences,
    /// Available family snapshot used to validate local availability.
    font_catalog: FontCatalog,
    /// Flags for stacks with no available requested family, in body, code, UI order.
    font_uses_system_default: [bool; 3],
    /// Editable body font family list.
    body_font_input: Entity<SingleLineInput>,
    /// Editable code font family list.
    code_font_input: Entity<SingleLineInput>,
    /// Editable application UI font family list.
    ui_font_input: Entity<SingleLineInput>,
    /// Editable PicGo Server URL.
    picgo_server_input: Entity<SingleLineInput>,
}

/// File association is an immediate OS action, not a persisted application preference.
#[derive(Debug, PartialEq, Eq)]
enum FileAssociationStatus {
    Checking,
    Associating,
    Associated,
    NotAssociated,
    NeedsConfirmation,
    Failed(String),
}

impl FileAssociationStatus {
    fn can_associate(&self) -> bool {
        matches!(
            self,
            Self::NotAssociated | Self::NeedsConfirmation | Self::Failed(_)
        )
    }
    fn is_busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Associating)
    }
}

impl PreferencesWindow {
    /// Builds the draft, saved baselines, and subscribed font inputs from preferences.
    fn new(
        preferences: AppPreferences,
        theme_options: Vec<ThemeCatalogEntry>,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected_theme_id = if theme_options
            .iter()
            .any(|entry| entry.id == preferences.default_theme_id)
        {
            preferences.default_theme_id
        } else {
            DEFAULT_THEME_ID.into()
        };
        let startup_open = preferences.startup_open;
        let image_paste_behavior = preferences.image_paste_behavior;
        let image_paste_fallback = if image_paste_behavior == ImagePasteBehavior::UsePicGo {
            preferences.image_paste_fallback.local_only()
        } else {
            image_paste_behavior
        };
        let picgo_server_url = preferences.picgo_server_url.clone();
        let keybindings = preferences.keybindings;
        let fonts = preferences.fonts.clone();
        let font_catalog = FontCatalog::current(cx);
        let font_uses_system_default = font_uses_system_default(&fonts, &font_catalog);
        let body_font_input = cx.new(|cx| {
            SingleLineInput::new(fonts.body_stack.clone(), "e.g. .SystemUIFont", cx)
                .with_overflow(SingleLineOverflow::Ellipsis)
        });
        let code_font_input = cx.new(|cx| {
            SingleLineInput::new(fonts.code_stack.clone(), "e.g. Consolas, monospace", cx)
                .with_overflow(SingleLineOverflow::Ellipsis)
        });
        let ui_font_input = cx.new(|cx| {
            SingleLineInput::new(fonts.ui_stack.clone(), "e.g. .SystemUIFont", cx)
                .with_overflow(SingleLineOverflow::Ellipsis)
        });
        let picgo_server_input = cx.new(|cx| {
            SingleLineInput::new(picgo_server_url.clone(), DEFAULT_PICGO_SERVER_URL, cx)
                .with_overflow(SingleLineOverflow::Ellipsis)
        });
        cx.subscribe(&picgo_server_input, |this, input, _: &InputChanged, cx| {
            this.picgo_server_url = input.read(cx).value().to_string();
            if this.image_paste_behavior == ImagePasteBehavior::UsePicGo {
                this.check_picgo_server(cx);
            }
            cx.notify();
        })
        .detach();
        cx.subscribe(&body_font_input, |this, input, _: &InputChanged, cx| {
            this.fonts.body_stack = input.read(cx).value().to_string();
            this.refresh_font_availability(0);
            cx.notify();
        })
        .detach();
        cx.subscribe(&code_font_input, |this, input, _: &InputChanged, cx| {
            this.fonts.code_stack = input.read(cx).value().to_string();
            this.refresh_font_availability(1);
            cx.notify();
        })
        .detach();
        cx.subscribe(&ui_font_input, |this, input, _: &InputChanged, cx| {
            this.fonts.ui_stack = input.read(cx).value().to_string();
            this.refresh_font_availability(2);
            cx.notify();
        })
        .detach();
        Self {
            nav: PreferencesNav::Settings,
            startup_open,
            selected_theme_id: selected_theme_id.clone(),
            image_paste_behavior,
            image_paste_fallback,
            picgo_server_url: picgo_server_url.clone(),
            keybindings: keybindings.clone(),
            saved_startup_open: startup_open,
            saved_theme_id: selected_theme_id,
            saved_image_paste_behavior: image_paste_behavior,
            saved_image_paste_fallback: image_paste_fallback,
            saved_picgo_server_url: picgo_server_url,
            auto_download_updates: preferences.auto_download_updates,
            saved_auto_download_updates: preferences.auto_download_updates,
            file_associations: FileAssociationStatus::Checking,
            file_association_task: None,
            picgo_status: PicGoStatus::Checking,
            picgo_check_generation: 0,
            picgo_check_task: None,
            saved_keybindings: keybindings,
            theme_options,
            focus_handle: cx.focus_handle(),
            startup_dropdown_open: false,
            theme_dropdown_open: false,
            image_dropdown_open: false,
            recording_shortcut: None,
            shortcut_error: None,
            status_bar_enabled: preferences.status_bar.enabled,
            status_bar_show_word_count: preferences.status_bar.show_word_count,
            status_bar_show_cursor_position: preferences.status_bar.show_cursor_position,
            status_bar_show_sidebar_toggle: preferences.status_bar.show_sidebar_toggle,
            status_bar_show_mode_switch: preferences.status_bar.show_mode_switch,
            saved_status_bar_enabled: preferences.status_bar.enabled,
            saved_status_bar_show_word_count: preferences.status_bar.show_word_count,
            saved_status_bar_show_cursor_position: preferences.status_bar.show_cursor_position,
            saved_status_bar_show_sidebar_toggle: preferences.status_bar.show_sidebar_toggle,
            saved_status_bar_show_mode_switch: preferences.status_bar.show_mode_switch,
            fonts: fonts.clone(),
            saved_fonts: fonts,
            font_catalog,
            font_uses_system_default,
            body_font_input,
            code_font_input,
            ui_font_input,
            picgo_server_input,
        }
    }

    /// Returns the first localized font syntax error in body, code, UI order.
    fn font_validation_error(&self, strings: &I18nStrings) -> Option<String> {
        [
            (&strings.preferences_font_body, &self.fonts.body_stack),
            (&strings.preferences_font_code, &self.fonts.code_stack),
            (&strings.preferences_font_ui, &self.fonts.ui_stack),
        ]
        .into_iter()
        .find_map(|(label, value)| {
            crate::fonts::parse_font_stack(value).err().map(|error| {
                let message = match error {
                    FontStackParseError::Empty => &strings.preferences_font_error_empty,
                    FontStackParseError::UnterminatedQuote => {
                        &strings.preferences_font_error_unterminated_quote
                    }
                };
                format!("{label}: {message}")
            })
        })
    }

    /// Rechecks one indexed font stack; invalid syntax does not show an availability warning.
    fn refresh_font_availability(&mut self, index: usize) {
        let input = match index {
            0 => &self.fonts.body_stack,
            1 => &self.fonts.code_stack,
            2 => &self.fonts.ui_stack,
            _ => return,
        };
        self.font_uses_system_default[index] = FontSettings::stack_uses_system_default(
            input,
            &self.font_catalog,
            std::env::consts::OS,
        )
        .unwrap_or(false);
    }

    /// Looks up the draft theme's display name, falling back to localized system or `Velotype`.
    fn selected_theme_name(&self, strings: &crate::i18n::I18nStrings) -> String {
        if self.selected_theme_id == crate::theme::THEME_SYSTEM_ID {
            return strings.theme_system.clone();
        }
        self.theme_options
            .iter()
            .find(|entry| entry.id == self.selected_theme_id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| "Velotype".into())
    }

    /// Compares draft values with saved baselines, normalizing shortcut overrides.
    fn has_unsaved_changes(&self) -> bool {
        self.startup_open != self.saved_startup_open
            || self.selected_theme_id != self.saved_theme_id
            || self.image_paste_behavior != self.saved_image_paste_behavior
            || self.image_paste_fallback != self.saved_image_paste_fallback
            || self.picgo_server_url != self.saved_picgo_server_url
            || self.auto_download_updates != self.saved_auto_download_updates
            || normalize_shortcut_config(&self.keybindings)
                != normalize_shortcut_config(&self.saved_keybindings)
            || self.status_bar_enabled != self.saved_status_bar_enabled
            || self.status_bar_show_word_count != self.saved_status_bar_show_word_count
            || self.status_bar_show_cursor_position != self.saved_status_bar_show_cursor_position
            || self.status_bar_show_sidebar_toggle != self.saved_status_bar_show_sidebar_toggle
            || self.status_bar_show_mode_switch != self.saved_status_bar_show_mode_switch
            || self.fonts != self.saved_fonts
    }

    /// Shows general settings while dismissing page-local editing controls.
    fn set_nav_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::Settings;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.recording_shortcut = None;
        cx.notify();
    }

    /// Serializes background checks/actions and refreshes after the system default-app UI returns.
    fn update_file_associations(
        &mut self,
        associate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_associations.is_busy() && self.file_association_task.is_some() {
            return;
        }
        self.file_associations = if associate {
            FileAssociationStatus::Associating
        } else {
            FileAssociationStatus::Checking
        };
        let handle = window.window_handle();
        let task = cx.background_executor().spawn(async move {
            let settings = if associate {
                crate::file_associations::associate()?
            } else {
                None
            };
            // User confirmation comes before rechecking; a failed query must not block Settings.
            if settings.is_some() {
                return anyhow::Ok((false, settings));
            }
            let associated = crate::file_associations::is_associated()?;
            anyhow::ensure!(
                !associate || associated,
                "the system did not apply the Markdown file associations"
            );
            anyhow::Ok((associated, settings))
        });
        self.file_association_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let result = task.await;
                let _ = cx.update_window(handle, |_, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.file_associations = match result {
                            Ok((true, _)) => FileAssociationStatus::Associated,
                            Ok((false, Some(url))) => {
                                cx.open_url(url);
                                FileAssociationStatus::NeedsConfirmation
                            }
                            Ok((false, None)) => FileAssociationStatus::NotAssociated,
                            Err(error) => {
                                let strings = cx.global::<I18nManager>().strings_arc();
                                let message = strings
                                    .preferences_file_associations_failed
                                    .replace("{error}", &format!("{error:#}"));
                                if associate {
                                    let _ = window.prompt(
                                        PromptLevel::Critical,
                                        &strings.preferences_file_associations,
                                        Some(&message),
                                        &[strings.info_dialog_ok.as_str()],
                                        cx,
                                    );
                                }
                                FileAssociationStatus::Failed(message)
                            }
                        };
                        cx.notify();
                    });
                });
            },
        ));
        cx.notify();
    }

    /// Leaves already-associated and in-flight states inert even if invoked outside the button.
    fn associate_file_formats(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_associations.can_associate() {
            self.update_file_associations(true, window, cx);
        }
    }

    /// Shows startup settings and closes dropdowns and shortcut recording.
    fn set_nav_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::File;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.recording_shortcut = None;
        cx.notify();
    }

    /// Shows theme/font settings and closes dropdowns and shortcut recording.
    fn set_nav_theme(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::Theme;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.recording_shortcut = None;
        cx.notify();
    }

    /// Shows image settings and closes dropdowns and shortcut recording.
    fn set_nav_image(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::Image;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.recording_shortcut = None;
        if self.image_paste_behavior == ImagePasteBehavior::UsePicGo {
            self.check_picgo_server(cx);
        }
        cx.notify();
    }

    /// Probes the configured loopback server without uploading clipboard data.
    fn check_picgo_server(&mut self, cx: &mut Context<Self>) {
        self.picgo_check_generation = self.picgo_check_generation.wrapping_add(1);
        let generation = self.picgo_check_generation;
        let url = self.picgo_server_url.clone();
        self.picgo_status = PicGoStatus::Checking;
        self.picgo_check_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let (sender, receiver) = oneshot::channel();
                thread::spawn(move || {
                    let result = crate::editor::picgo::probe_server(&url);
                    let _ = sender.send(result);
                });
                let result = receiver.await.unwrap_or(Ok(false));
                let _ = this.update(cx, |this, cx| {
                    if this.picgo_check_generation != generation {
                        return;
                    }
                    this.picgo_status = match result {
                        Ok(true) => PicGoStatus::Available,
                        Ok(false) => PicGoStatus::Legacy,
                        Err(_) => PicGoStatus::Unavailable,
                    };
                    cx.notify();
                });
            },
        ));
    }

    /// Shows shortcut settings, closes dropdowns, and clears the recording error.
    fn set_nav_shortcuts(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::Shortcuts;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.shortcut_error = None;
        cx.notify();
    }

    /// Shows status bar settings and closes dropdowns and shortcut recording.
    fn set_nav_status_bar(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::StatusBar;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        self.recording_shortcut = None;
        cx.notify();
    }

    /// Toggles startup options and closes the other dropdowns.
    fn toggle_startup_dropdown(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.startup_dropdown_open = !self.startup_dropdown_open;
        self.theme_dropdown_open = false;
        self.image_dropdown_open = false;
        cx.notify();
    }

    /// Toggles theme options and closes the other dropdowns.
    fn toggle_theme_dropdown(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.theme_dropdown_open = !self.theme_dropdown_open;
        self.startup_dropdown_open = false;
        self.image_dropdown_open = false;
        cx.notify();
    }

    /// Toggles image storage options and closes the other dropdowns.
    fn toggle_image_dropdown(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.image_dropdown_open = !self.image_dropdown_open;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        cx.notify();
    }

    /// Closes the window without persisting the draft.
    fn cancel(&mut self, _: &ClickEvent, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    /// Closes without saving when the custom titlebar receives a standard click.
    fn on_titlebar_close(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if event.standard_click() {
            window.remove_window();
        }
    }

    /// Validates and saves a changed draft, prompting on write failure before applying it.
    fn save(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_unsaved_changes() {
            return;
        }

        let strings = cx.global::<I18nManager>().strings();
        if self.font_validation_error(strings).is_some() {
            cx.notify();
            return;
        }

        let preferences = match save_preferences_from_window(
            self.startup_open,
            &self.selected_theme_id,
            self.image_paste_behavior,
            self.image_paste_fallback,
            &self.picgo_server_url,
            self.auto_download_updates,
            self.keybindings.clone(),
            &StatusBarPreferences {
                enabled: self.status_bar_enabled,
                show_word_count: self.status_bar_show_word_count,
                show_cursor_position: self.status_bar_show_cursor_position,
                show_sidebar_toggle: self.status_bar_show_sidebar_toggle,
                show_mode_switch: self.status_bar_show_mode_switch,
                custom_buttons: Vec::new(),
            },
            &self.fonts,
        ) {
            Ok(preferences) => preferences,
            Err(err) => {
                let strings = cx.global::<I18nManager>().strings().clone();
                let ok = strings.info_dialog_ok;
                let buttons = [ok.as_str()];
                let _ = window.prompt(
                    PromptLevel::Critical,
                    &strings.preferences_save_failed_title,
                    Some(&err.to_string()),
                    &buttons,
                    cx,
                );
                return;
            }
        };

        self.apply_saved_preferences(preferences, window, cx);
    }

    /// Applies saved theme, shortcuts, bar flags, and fonts, then resets dirty baselines.
    /// Keeps this window open, active, and focused.
    fn apply_saved_preferences(
        &mut self,
        preferences: AppPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme_changed = cx.update_global::<ThemeManager, _>(|theme_manager, cx| {
            theme_manager.set_system_appearance(cx.window_appearance());
            theme_manager.set_theme_by_id(&preferences.default_theme_id)
        });
        if !theme_changed {
            let _ = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
                theme_manager.set_theme_by_id(DEFAULT_THEME_ID)
            });
        }
        cx.clear_key_bindings();
        install_keybindings(cx, &preferences.keybindings);
        crate::app_menu::install_menus(cx);
        cx.update_global::<EditorSettings, _>(|settings, _cx| {
            settings.status_bar_settings.status_bar_enabled = preferences.status_bar.enabled;
            settings.status_bar_settings.status_bar_show_word_count =
                preferences.status_bar.show_word_count;
            settings.status_bar_settings.status_bar_show_cursor_position =
                preferences.status_bar.show_cursor_position;
            settings.status_bar_settings.status_bar_show_sidebar_toggle =
                preferences.status_bar.show_sidebar_toggle;
            settings.status_bar_settings.status_bar_show_mode_switch =
                preferences.status_bar.show_mode_switch;
        });
        FontSettings::update(cx, preferences.fonts.clone())
            .expect("saved font stacks were validated");
        cx.refresh_windows();
        window.activate_window();
        self.focus_handle.focus(window, cx);
        self.saved_startup_open = self.startup_open;
        self.saved_theme_id = self.selected_theme_id.clone();
        self.saved_image_paste_behavior = self.image_paste_behavior;
        self.saved_image_paste_fallback = self.image_paste_fallback;
        self.saved_picgo_server_url = self.picgo_server_url.clone();
        self.saved_auto_download_updates = self.auto_download_updates;
        crate::updater::set_auto_download(cx, preferences.auto_download_updates);
        self.saved_keybindings = normalize_shortcut_config(&self.keybindings);
        self.saved_status_bar_enabled = self.status_bar_enabled;
        self.saved_status_bar_show_word_count = self.status_bar_show_word_count;
        self.saved_status_bar_show_cursor_position = self.status_bar_show_cursor_position;
        self.saved_status_bar_show_sidebar_toggle = self.status_bar_show_sidebar_toggle;
        self.saved_status_bar_show_mode_switch = self.status_bar_show_mode_switch;
        self.saved_fonts = self.fonts.clone();
        cx.notify();
    }

    /// Builds a navigation button with selected-page styling and a click listener.
    fn nav_button(
        &self,
        id: &'static str,
        label: String,
        selected: bool,
        theme: &Theme,
        on_click: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .h(px(34.0))
            .w(px(156.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_end()
            .rounded(px(d.menu_item_radius))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(if selected {
                c.dialog_primary_button_text
            } else {
                c.dialog_body
            })
            .bg(if selected {
                c.dialog_primary_button_bg
            } else {
                c.dialog_secondary_button_bg
            })
            .hover(move |this| {
                this.bg(if selected {
                    c.dialog_primary_button_hover
                } else {
                    c.dialog_secondary_button_hover
                })
            })
            .id(id)
            .child(label)
            .on_click(cx.listener(on_click))
    }

    /// Builds the current-value button that opens a preferences dropdown.
    fn dropdown_button(
        id: &'static str,
        label: String,
        theme: &Theme,
        on_click: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .w_full()
            .min_w(px(0.0))
            .min_h(px(36.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_secondary_button_bg)
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .child(label)
            .child("v")
            .on_click(cx.listener(on_click))
    }

    /// Builds a selectable dropdown row with current-selection highlighting.
    fn dropdown_item(
        id: impl Into<ElementId>,
        label: String,
        selected: bool,
        theme: &Theme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .w_full()
            .min_w(px(0.0))
            .min_h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(d.menu_item_radius))
            .cursor_pointer()
            .bg(if selected {
                c.selection
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .child(label)
            .on_click(cx.listener(on_click))
    }

    /// Stacks a field label above its full-width control.
    fn labeled_row(&self, label: &str, control: impl IntoElement, theme: &Theme) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(t.dialog_body_size))
                    .font_weight(t.dialog_button_weight.to_font_weight())
                    .text_color(c.dialog_title)
                    .child(SharedString::from(label.to_string())),
            )
            .child(div().w_full().min_w(px(0.0)).child(control))
    }

    /// Renders the draft startup document policy dropdown.
    fn render_startup_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = match self.startup_open {
            StartupOpenPreference::NewFile => strings.preferences_startup_new_file.clone(),
            StartupOpenPreference::LastOpenedFile => {
                strings.preferences_startup_last_opened_file.clone()
            }
        };
        let mut dropdown = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(Self::dropdown_button(
                "preferences-startup-dropdown",
                selected,
                theme,
                Self::toggle_startup_dropdown,
                cx,
            ));
        if self.startup_dropdown_open {
            let new_file_label = strings.preferences_startup_new_file.clone();
            let last_file_label = strings.preferences_startup_last_opened_file.clone();
            dropdown = dropdown
                .child(Self::dropdown_item(
                    "preferences-startup-new-file",
                    new_file_label,
                    self.startup_open == StartupOpenPreference::NewFile,
                    theme,
                    |this, _, _, cx| {
                        this.startup_open = StartupOpenPreference::NewFile;
                        this.startup_dropdown_open = false;
                        cx.notify();
                    },
                    cx,
                ))
                .child(Self::dropdown_item(
                    "preferences-startup-last-opened-file",
                    last_file_label,
                    self.startup_open == StartupOpenPreference::LastOpenedFile,
                    theme,
                    |this, _, _, cx| {
                        this.startup_open = StartupOpenPreference::LastOpenedFile;
                        this.startup_dropdown_open = false;
                        cx.notify();
                    },
                    cx,
                ));
        }
        self.labeled_row(&strings.preferences_startup_option, dropdown, theme)
    }

    /// Renders theme selection, font inputs, syntax errors, and availability warnings.
    fn render_theme_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut dropdown = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(Self::dropdown_button(
                "preferences-theme-dropdown",
                self.selected_theme_name(strings),
                theme,
                Self::toggle_theme_dropdown,
                cx,
            ));
        if self.theme_dropdown_open {
            let mut list = div()
                .id("preferences-theme-dropdown-list")
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0));

            for (index, entry) in self.theme_options.clone().into_iter().enumerate() {
                let selected = entry.id == self.selected_theme_id;
                let display_name = if entry.id == crate::theme::THEME_SYSTEM_ID {
                    strings.theme_system.clone()
                } else {
                    entry.name
                };
                list = list.child(Self::dropdown_item(
                    ("preferences-theme-option", index),
                    display_name,
                    selected,
                    theme,
                    move |this, _, _, cx| {
                        this.selected_theme_id = entry.id.clone();
                        this.theme_dropdown_open = false;
                        cx.notify();
                    },
                    cx,
                ));
            }
            dropdown = dropdown.child(list);
        }
        let c = &theme.colors;
        let t = &theme.typography;
        let font_error = self.font_validation_error(strings);
        let warning = |uses_system_default: bool| {
            if !uses_system_default {
                div().into_any_element()
            } else {
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px((t.dialog_body_size - 2.).max(10.)))
                    .text_color(c.callout_warning_border)
                    .child(strings.preferences_font_unavailable_template.clone())
                    .into_any_element()
            }
        };
        let fields = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.labeled_row(
                &strings.preferences_font_body,
                self.body_font_input.clone(),
                theme,
            ))
            .child(warning(
                font_error.is_none() && self.font_uses_system_default[0],
            ))
            .child(self.labeled_row(
                &strings.preferences_font_code,
                self.code_font_input.clone(),
                theme,
            ))
            .child(warning(
                font_error.is_none() && self.font_uses_system_default[1],
            ))
            .child(self.labeled_row(
                &strings.preferences_font_ui,
                self.ui_font_input.clone(),
                theme,
            ))
            .child(warning(
                font_error.is_none() && self.font_uses_system_default[2],
            ))
            .when_some(font_error, |this, error| {
                this.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .text_size(px(t.dialog_body_size))
                        .text_color(c.dialog_danger_button_bg)
                        .child(error),
                )
            });
        div()
            .id("preferences-theme-scroll")
            .w_full()
            .h_full()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(18.))
            .pr(px(4.0))
            .child(self.labeled_row(&strings.preferences_local_theme, dropdown, theme))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px((t.dialog_body_size - 2.).max(10.)))
                    .text_color(c.dialog_muted)
                    .child(strings.preferences_font_help.clone()),
            )
            .child(fields)
    }

    /// Resolves the localized label for an image storage policy.
    fn image_paste_behavior_label(
        behavior: ImagePasteBehavior,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match behavior {
            ImagePasteBehavior::None => strings.preferences_image_paste_none.clone(),
            ImagePasteBehavior::CopyToDocumentFolder => strings
                .preferences_image_paste_copy_to_document_folder
                .clone(),
            ImagePasteBehavior::CopyToAssetsFolder => strings
                .preferences_image_paste_copy_to_assets_folder
                .clone(),
            ImagePasteBehavior::CopyToNamedAssetsFolder => strings
                .preferences_image_paste_copy_to_named_assets_folder
                .clone(),
            ImagePasteBehavior::UsePicGo => strings.preferences_image_paste_picgo.clone(),
        }
    }

    /// Renders the clipboard image storage policy dropdown.
    fn render_image_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let options = [
            ImagePasteBehavior::None,
            ImagePasteBehavior::CopyToDocumentFolder,
            ImagePasteBehavior::CopyToAssetsFolder,
            ImagePasteBehavior::CopyToNamedAssetsFolder,
            ImagePasteBehavior::UsePicGo,
        ];
        let mut dropdown = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(Self::dropdown_button(
                "preferences-image-dropdown",
                Self::image_paste_behavior_label(self.image_paste_behavior, strings),
                theme,
                Self::toggle_image_dropdown,
                cx,
            ));
        if self.image_dropdown_open {
            for (index, behavior) in options.into_iter().enumerate() {
                let selected = behavior == self.image_paste_behavior;
                let label = Self::image_paste_behavior_label(behavior, strings);
                dropdown = dropdown.child(Self::dropdown_item(
                    ("preferences-image-option", index),
                    label,
                    selected,
                    theme,
                    move |this, _, _, cx| {
                        if behavior != ImagePasteBehavior::UsePicGo {
                            this.image_paste_fallback = behavior;
                        }
                        this.image_paste_behavior = behavior;
                        this.image_dropdown_open = false;
                        if behavior == ImagePasteBehavior::UsePicGo {
                            this.check_picgo_server(cx);
                        }
                        cx.notify();
                    },
                    cx,
                ));
            }
        }
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(self.labeled_row(&strings.preferences_image_insert_behavior, dropdown, theme));
        if self.image_paste_behavior == ImagePasteBehavior::UsePicGo {
            let status = match self.picgo_status {
                PicGoStatus::Checking => &strings.preferences_picgo_checking,
                PicGoStatus::Available => &strings.preferences_picgo_available,
                PicGoStatus::Legacy => &strings.preferences_picgo_legacy,
                PicGoStatus::Unavailable => &strings.preferences_picgo_unavailable,
            };
            content = content
                .child(self.labeled_row(
                    &strings.preferences_picgo_server_url,
                    self.picgo_server_input.clone(),
                    theme,
                ))
                .child(
                    div()
                        .text_size(px(theme.typography.dialog_body_size))
                        .text_color(theme.colors.dialog_muted)
                        .child(status.clone()),
                );
        }
        content
    }

    /// Resolves the localized heading for a shortcut category.
    fn shortcut_category_label(
        category: ShortcutCategory,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match category {
            ShortcutCategory::File => strings.preferences_shortcuts_group_file.clone(),
            ShortcutCategory::Edit => strings.preferences_shortcuts_group_edit.clone(),
            ShortcutCategory::Navigation => strings.preferences_shortcuts_group_navigation.clone(),
            ShortcutCategory::Formatting => strings.preferences_shortcuts_group_formatting.clone(),
            ShortcutCategory::Block => strings.preferences_shortcuts_group_block.clone(),
            ShortcutCategory::Other => strings.preferences_shortcuts_group_other.clone(),
        }
    }

    /// Resolves the localized preferences label for a shortcut command.
    fn shortcut_command_label(
        command: ShortcutCommand,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match command {
            ShortcutCommand::Newline => strings.preferences_shortcut_newline.clone(),
            ShortcutCommand::DeleteBack => strings.preferences_shortcut_delete_back.clone(),
            ShortcutCommand::Delete => strings.preferences_shortcut_delete.clone(),
            ShortcutCommand::WordDeleteBack => {
                strings.preferences_shortcut_word_delete_back.clone()
            }
            ShortcutCommand::WordDeleteForward => {
                strings.preferences_shortcut_word_delete_forward.clone()
            }
            ShortcutCommand::FocusPrev => strings.preferences_shortcut_focus_prev.clone(),
            ShortcutCommand::FocusNext => strings.preferences_shortcut_focus_next.clone(),
            ShortcutCommand::MoveLeft => strings.preferences_shortcut_move_left.clone(),
            ShortcutCommand::MoveRight => strings.preferences_shortcut_move_right.clone(),
            ShortcutCommand::WordMoveLeft => strings.preferences_shortcut_word_move_left.clone(),
            ShortcutCommand::WordMoveRight => strings.preferences_shortcut_word_move_right.clone(),
            ShortcutCommand::Home => strings.preferences_shortcut_home.clone(),
            ShortcutCommand::End => strings.preferences_shortcut_end.clone(),
            ShortcutCommand::BlockUp => strings.preferences_shortcut_block_up.clone(),
            ShortcutCommand::BlockDown => strings.preferences_shortcut_block_down.clone(),
            ShortcutCommand::PageUp => strings.preferences_shortcut_page_up.clone(),
            ShortcutCommand::PageDown => strings.preferences_shortcut_page_down.clone(),
            ShortcutCommand::JumpToTop => strings.preferences_shortcut_jump_to_top.clone(),
            ShortcutCommand::JumpToBottom => strings.preferences_shortcut_jump_to_bottom.clone(),
            ShortcutCommand::SelectLeft => strings.preferences_shortcut_select_left.clone(),
            ShortcutCommand::SelectRight => strings.preferences_shortcut_select_right.clone(),
            ShortcutCommand::WordSelectLeft => {
                strings.preferences_shortcut_word_select_left.clone()
            }
            ShortcutCommand::WordSelectRight => {
                strings.preferences_shortcut_word_select_right.clone()
            }
            ShortcutCommand::SelectHome => strings.preferences_shortcut_select_home.clone(),
            ShortcutCommand::SelectEnd => strings.preferences_shortcut_select_end.clone(),
            ShortcutCommand::SelectAll => strings.preferences_shortcut_select_all.clone(),
            ShortcutCommand::Copy => strings.preferences_shortcut_copy.clone(),
            ShortcutCommand::Cut => strings.preferences_shortcut_cut.clone(),
            ShortcutCommand::Paste => strings.preferences_shortcut_paste.clone(),
            ShortcutCommand::Undo => strings.preferences_shortcut_undo.clone(),
            ShortcutCommand::Redo => strings.preferences_shortcut_redo.clone(),
            ShortcutCommand::BoldSelection => strings.preferences_shortcut_bold_selection.clone(),
            ShortcutCommand::ItalicSelection => {
                strings.preferences_shortcut_italic_selection.clone()
            }
            ShortcutCommand::UnderlineSelection => {
                strings.preferences_shortcut_underline_selection.clone()
            }
            ShortcutCommand::CodeSelection => strings.preferences_shortcut_code_selection.clone(),
            ShortcutCommand::IndentBlock => strings.preferences_shortcut_indent_block.clone(),
            ShortcutCommand::OutdentBlock => strings.preferences_shortcut_outdent_block.clone(),
            ShortcutCommand::ExitCodeBlock => strings.preferences_shortcut_exit_code_block.clone(),
            ShortcutCommand::SaveDocument => strings.preferences_shortcut_save_document.clone(),
            ShortcutCommand::SaveDocumentAs => {
                strings.preferences_shortcut_save_document_as.clone()
            }
            ShortcutCommand::NewWindow => strings.preferences_shortcut_new_window.clone(),
            ShortcutCommand::OpenFile => strings.preferences_shortcut_open_file.clone(),
            ShortcutCommand::QuitApplication => {
                strings.preferences_shortcut_quit_application.clone()
            }
            ShortcutCommand::CloseWindow => strings.preferences_shortcut_close_window.clone(),
            ShortcutCommand::DismissTransientUi => {
                strings.preferences_shortcut_dismiss_transient_ui.clone()
            }
            ShortcutCommand::ToggleViewMode => {
                strings.preferences_shortcut_toggle_view_mode.clone()
            }
            ShortcutCommand::ToggleWorkspace => {
                strings.preferences_shortcut_toggle_workspace.clone()
            }
        }
    }

    /// Replaces every occurrence of one placeholder in a localized message.
    fn format_template(template: &str, key: &str, value: &str) -> String {
        template.replace(key, value)
    }

    /// Arms a command for key capture and focuses the window-level receiver.
    fn begin_recording_shortcut(
        &mut self,
        command: ShortcutCommand,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording_shortcut = Some(command);
        self.shortcut_error = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Removes a command override and clears any recording state for that command.
    fn reset_shortcut(
        &mut self,
        command: ShortcutCommand,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(definition) = shortcut_definitions()
            .iter()
            .find(|definition| definition.command == command)
        {
            self.keybindings.remove(definition.id);
        }
        if self.recording_shortcut == Some(command) {
            self.recording_shortcut = None;
        }
        self.shortcut_error = None;
        cx.notify();
    }

    /// Captures a nonrepeated key, rejecting invalid/conflicting bindings; Escape cancels.
    fn capture_shortcut_key(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self.recording_shortcut else {
            return;
        };
        cx.stop_propagation();
        if event.is_held {
            return;
        }

        let key = event.keystroke.unparse();
        if key == "escape" {
            self.recording_shortcut = None;
            self.shortcut_error = None;
            cx.notify();
            return;
        }

        let Some(keys) = normalize_shortcut_keys(std::slice::from_ref(&key)) else {
            let strings = cx.global::<I18nManager>().strings();
            self.shortcut_error = Some(Self::format_template(
                &strings.preferences_shortcut_invalid_template,
                "{shortcut}",
                &key,
            ));
            cx.notify();
            return;
        };

        if let Some(conflict) = shortcut_conflict_for(command, &keys, &self.keybindings) {
            let strings = cx.global::<I18nManager>().strings();
            let label = Self::shortcut_command_label(conflict.command, strings);
            self.shortcut_error = Some(Self::format_template(
                &strings.preferences_shortcut_conflict_template,
                "{command}",
                &label,
            ));
            cx.notify();
            return;
        }

        if let Some(definition) = shortcut_definitions()
            .iter()
            .find(|definition| definition.command == command)
        {
            let defaults = definition
                .default_keys
                .iter()
                .map(|key| key.to_string())
                .collect::<Vec<_>>();
            if keys == defaults {
                self.keybindings.remove(definition.id);
            } else {
                self.keybindings.insert(definition.id.to_string(), keys);
            }
        }
        self.recording_shortcut = None;
        self.shortcut_error = None;
        cx.notify();
    }

    /// Builds a compact shortcut-key or recording-state label.
    fn shortcut_chip(label: &str, theme: &Theme) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .min_w(px(58.0))
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px((d.menu_item_radius - 1.0).max(0.0)))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.code_bg)
            .text_size(px((t.dialog_body_size - 1.0).max(10.0)))
            .text_color(c.code_text)
            .child(SharedString::from(label.to_string()))
    }

    /// Builds a shortcut row's record or reset button.
    fn shortcut_action_button(
        id: impl Into<ElementId>,
        label: String,
        theme: &Theme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .id(id)
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px((d.dialog_radius - 5.0).max(0.0)))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_secondary_button_bg)
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .cursor_pointer()
            .text_size(px((t.dialog_button_size - 1.0).max(10.0)))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(c.dialog_secondary_button_text)
            .child(label)
            .on_click(cx.listener(on_click))
    }

    /// Renders one command's resolved keys and record/reset controls.
    fn render_shortcut_row(
        &self,
        definition: ShortcutDefinition,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let is_recording = self.recording_shortcut == Some(definition.command);
        let keys = resolved_shortcut_keys(&self.keybindings, definition.command);
        let label = Self::shortcut_command_label(definition.command, strings);
        let command = definition.command;

        let mut chips = div().flex().flex_wrap().gap(px(6.0));
        if is_recording {
            chips = chips.child(Self::shortcut_chip(
                &strings.preferences_shortcut_recording,
                theme,
            ));
        } else {
            for key in keys {
                chips = chips.child(Self::shortcut_chip(&key, theme));
            }
        }

        div()
            .w_full()
            .min_h(px(42.0))
            .px(px(10.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(d.menu_item_radius))
            .bg(c.dialog_surface)
            .child(
                div()
                    .min_w(px(144.0))
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_body)
                    .child(label),
            )
            .child(div().flex_1().child(chips))
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(Self::shortcut_action_button(
                        ("preferences-shortcut-record", definition.command as u32),
                        strings.preferences_shortcut_record.clone(),
                        theme,
                        move |this, event, window, cx| {
                            this.begin_recording_shortcut(command, event, window, cx)
                        },
                        cx,
                    ))
                    .child(Self::shortcut_action_button(
                        ("preferences-shortcut-reset", definition.command as u32),
                        strings.preferences_shortcut_reset.clone(),
                        theme,
                        move |this, event, window, cx| {
                            this.reset_shortcut(command, event, window, cx)
                        },
                        cx,
                    )),
            )
    }

    /// Renders categorized shortcut rows and the current validation error.
    fn render_shortcuts_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let mut content = div()
            .id("preferences-shortcuts-scroll")
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .pr(px(4.0));

        let categories = [
            ShortcutCategory::File,
            ShortcutCategory::Edit,
            ShortcutCategory::Navigation,
            ShortcutCategory::Formatting,
            ShortcutCategory::Block,
            ShortcutCategory::Other,
        ];

        for category in categories {
            let mut group = div().w_full().flex().flex_col().gap(px(8.0)).child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(t.dialog_body_size))
                            .font_weight(t.dialog_button_weight.to_font_weight())
                            .text_color(c.dialog_title)
                            .child(Self::shortcut_category_label(category, strings)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(d.dialog_border_width.max(1.0)))
                            .bg(c.dialog_border),
                    ),
            );
            for definition in shortcut_definitions()
                .iter()
                .copied()
                .filter(|definition| definition.category == category)
            {
                group = group.child(self.render_shortcut_row(definition, theme, strings, cx));
            }
            content = content.child(group);
        }

        let mut page = div()
            .w_full()
            .h_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0));
        if let Some(error) = &self.shortcut_error {
            page = page.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_danger_button_bg)
                    .child(error.clone()),
            );
        }
        page.child(content)
    }

    /// Uses the existing controlled switch for the opt-in download setting.
    fn render_settings_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(strings.preferences_updates_auto_download.clone()),
                    )
                    .child(
                        Switch::new("preferences-auto-download-updates")
                            .checked(self.auto_download_updates)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auto_download_updates = !this.auto_download_updates;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(theme.typography.dialog_body_size))
                    .text_color(theme.colors.dialog_muted)
                    .child(strings.preferences_updates_auto_download_hint.clone()),
            )
            .child({
                let enabled = self.file_associations.can_associate();
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(strings.preferences_file_associations.clone()),
                    )
                    .child(
                        div()
                            .id("preferences-associate-formats")
                            .h(px(theme.dimensions.dialog_button_height))
                            .px(px(theme.dimensions.dialog_button_padding_x))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px((theme.dimensions.dialog_radius - 4.0).max(0.0)))
                            .border(px(theme.dimensions.dialog_border_width))
                            .border_color(theme.colors.dialog_border)
                            .bg(theme.colors.dialog_secondary_button_bg)
                            .text_size(px(theme.typography.dialog_button_size))
                            .font_weight(theme.typography.dialog_button_weight.to_font_weight())
                            .text_color(if enabled {
                                theme.colors.dialog_secondary_button_text
                            } else {
                                theme.colors.dialog_muted
                            })
                            .when(enabled, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(theme.colors.dialog_secondary_button_hover)
                                    })
                                    .on_click(cx.listener(Self::associate_file_formats))
                            })
                            .child(strings.preferences_associate_formats.clone()),
                    )
            })
            .children(
                match &self.file_associations {
                    FileAssociationStatus::Checking | FileAssociationStatus::Associating => {
                        Some(strings.preferences_file_associations_checking.clone())
                    }
                    FileAssociationStatus::Associated => {
                        Some(strings.preferences_file_associations_associated.clone())
                    }
                    FileAssociationStatus::NeedsConfirmation => {
                        Some(strings.preferences_file_associations_pending.clone())
                    }
                    FileAssociationStatus::Failed(error) => Some(error.clone()),
                    FileAssociationStatus::NotAssociated => None,
                }
                .map(|message| {
                    div()
                        .text_size(px(theme.typography.dialog_body_size))
                        .text_color(theme.colors.dialog_muted)
                        .child(message)
                }),
            )
    }

    /// Renders the draft status bar and built-in item visibility switches.
    fn render_status_bar_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;

        let switch_row = |label: &str,
                          checked: bool,
                          on_click: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
                          cx: &mut Context<Self>| {
            div()
                .w(px(280.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(t.dialog_body_size))
                        .text_color(c.dialog_body)
                        .child(SharedString::from(label.to_string())),
                )
                .child(
                    Switch::new(ElementId::Name(
                        format!("preferences-toggle-{}", label).into(),
                    ))
                    .checked(checked)
                    .on_click(cx.listener(on_click)),
                )
        };

        let items = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(switch_row(
                &strings.preferences_status_bar_enabled,
                self.status_bar_enabled,
                |this, _, _, cx| {
                    this.status_bar_enabled = !this.status_bar_enabled;
                    cx.notify();
                },
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_word_count,
                self.status_bar_show_word_count,
                |this, _, _, cx| {
                    this.status_bar_show_word_count = !this.status_bar_show_word_count;
                    cx.notify();
                },
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_cursor_position,
                self.status_bar_show_cursor_position,
                |this, _, _, cx| {
                    this.status_bar_show_cursor_position = !this.status_bar_show_cursor_position;
                    cx.notify();
                },
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_sidebar_toggle,
                self.status_bar_show_sidebar_toggle,
                |this, _, _, cx| {
                    this.status_bar_show_sidebar_toggle = !this.status_bar_show_sidebar_toggle;
                    cx.notify();
                },
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_mode_switch,
                self.status_bar_show_mode_switch,
                |this, _, _, cx| {
                    this.status_bar_show_mode_switch = !this.status_bar_show_mode_switch;
                    cx.notify();
                },
                cx,
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .child(items)
    }
}

impl Render for PreferencesWindow {
    /// Renders navigation, the active page, and draft-aware save/cancel controls.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let can_save = self.has_unsaved_changes();
        let window_title =
            SharedString::from(format!("Velotype - {}", strings.preferences_window_title));
        window.set_window_title(window_title.as_ref());
        let titlebar_height = custom_titlebar_height(window, d);

        let content = div()
            .size_full()
            .pt(px(titlebar_height))
            .flex()
            .key_context("Preferences")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::capture_shortcut_key))
            .bg(c.editor_background)
            .text_color(c.dialog_body)
            .font(FontSettings::ui_font(cx))
            .child(
                div()
                    .w(relative(0.3))
                    .h_full()
                    .pr(px(20.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .border_r(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(self.nav_button(
                                "preferences-nav-settings",
                                strings.preferences_nav_settings.clone(),
                                self.nav == PreferencesNav::Settings,
                                &theme,
                                Self::set_nav_settings,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-file",
                                strings.preferences_nav_file.clone(),
                                self.nav == PreferencesNav::File,
                                &theme,
                                Self::set_nav_file,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-theme",
                                strings.preferences_nav_theme.clone(),
                                self.nav == PreferencesNav::Theme,
                                &theme,
                                Self::set_nav_theme,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-image",
                                strings.preferences_nav_image.clone(),
                                self.nav == PreferencesNav::Image,
                                &theme,
                                Self::set_nav_image,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-shortcuts",
                                strings.preferences_nav_shortcuts.clone(),
                                self.nav == PreferencesNav::Shortcuts,
                                &theme,
                                Self::set_nav_shortcuts,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-status-bar",
                                strings.preferences_nav_status_bar.clone(),
                                self.nav == PreferencesNav::StatusBar,
                                &theme,
                                Self::set_nav_status_bar,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .w(relative(0.7))
                    .h_full()
                    .p(px(d.dialog_padding))
                    .flex()
                    .flex_col()
                    .gap(px(d.dialog_gap))
                    .child(
                        div()
                            .w_full()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(d.dialog_gap * 1.5))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(match self.nav {
                                        PreferencesNav::Settings => {
                                            strings.preferences_nav_settings.clone()
                                        }
                                        PreferencesNav::File => {
                                            strings.preferences_nav_file.clone()
                                        }
                                        PreferencesNav::Theme => {
                                            strings.preferences_nav_theme.clone()
                                        }
                                        PreferencesNav::Image => {
                                            strings.preferences_nav_image.clone()
                                        }
                                        PreferencesNav::Shortcuts => {
                                            strings.preferences_nav_shortcuts.clone()
                                        }
                                        PreferencesNav::StatusBar => {
                                            strings.preferences_nav_status_bar.clone()
                                        }
                                    }),
                            )
                            .child(match self.nav {
                                PreferencesNav::Settings => div()
                                    .id("preferences-settings-scroll")
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .child(self.render_settings_page(&theme, &strings, cx))
                                    .into_any_element(),
                                PreferencesNav::File => div()
                                    .id("preferences-file-scroll")
                                    .w_full()
                                    .h_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .min_w(px(0.0))
                                    .overflow_y_scroll()
                                    .pr(px(4.0))
                                    .child(self.render_startup_page(&theme, &strings, cx))
                                    .into_any_element(),
                                PreferencesNav::Theme => div()
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .child(self.render_theme_page(&theme, &strings, cx))
                                    .into_any_element(),
                                PreferencesNav::Image => div()
                                    .id("preferences-image-scroll")
                                    .w_full()
                                    .h_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .min_w(px(0.0))
                                    .overflow_y_scroll()
                                    .pr(px(4.0))
                                    .child(self.render_image_page(&theme, &strings, cx))
                                    .into_any_element(),
                                PreferencesNav::Shortcuts => div()
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .child(self.render_shortcuts_page(&theme, &strings, cx))
                                    .into_any_element(),
                                PreferencesNav::StatusBar => div()
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(self.render_status_bar_page(&theme, &strings, cx))
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .gap(px(d.dialog_button_gap))
                            .child(
                                div()
                                    .id("preferences-cancel")
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(d.dialog_border_width))
                                    .border_color(c.dialog_border)
                                    .bg(c.dialog_secondary_button_bg)
                                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                    .cursor_pointer()
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(c.dialog_secondary_button_text)
                                    .child(strings.preferences_cancel.clone())
                                    .on_click(cx.listener(Self::cancel)),
                            )
                            .child(
                                div()
                                    .id("preferences-save")
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(if can_save { 0.0 } else { d.dialog_border_width }))
                                    .border_color(c.dialog_border)
                                    .bg(if can_save {
                                        c.dialog_primary_button_bg
                                    } else {
                                        c.dialog_secondary_button_bg
                                    })
                                    .hover(move |this| {
                                        if can_save {
                                            this.bg(c.dialog_primary_button_hover)
                                        } else {
                                            this.bg(c.dialog_secondary_button_bg)
                                        }
                                    })
                                    .when(can_save, |this| this.cursor_pointer())
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(if can_save {
                                        c.dialog_primary_button_text
                                    } else {
                                        c.dialog_secondary_button_text
                                    })
                                    .child(strings.preferences_save.clone())
                                    .on_click(cx.listener(Self::save)),
                            ),
                    ),
            );

        let root = div()
            .size_full()
            .relative()
            .bg(c.editor_background)
            .font(FontSettings::ui_font(cx))
            .child(content);

        if let Some(titlebar) = render_custom_titlebar(
            "preferences-titlebar",
            window_title,
            &theme,
            window,
            cx,
            Self::on_titlebar_close,
        ) {
            root.child(titlebar)
        } else {
            root
        }
    }
}

/// Opens a preferences draft and installs the input filter before activation and focus.
/// Panics if the window cannot be opened or its initial update fails.
fn open_preferences_window_with_state(
    cx: &mut App,
    preferences: AppPreferences,
    theme_options: Vec<ThemeCatalogEntry>,
    title: String,
) -> WindowHandle<PreferencesWindow> {
    let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
    let window_title = SharedString::from(format!("Velotype - {title}"));
    let handle = cx
        .open_window(
            velotype_window_options(window_title, bounds),
            move |_window, cx| {
                cx.new(move |cx| PreferencesWindow::new(preferences, theme_options, cx))
            },
        )
        .expect("preferences window should open");

    handle
        .update(cx, |preferences, window, cx| {
            crate::ime::install_window_input_filter(window);
            window.activate_window();
            preferences.focus_handle.focus(window, cx);
            crate::theme::install_window_appearance_observer(window, cx);
            cx.observe_window_activation(window, |preferences, window, cx| {
                if window.is_window_active() {
                    preferences.update_file_associations(false, window, cx);
                }
            })
            .detach();
            preferences.update_file_associations(false, window, cx);
        })
        .expect("newly opened preferences window should be updateable");

    handle
}

/// Opens a new preferences window using persisted settings and current global catalogs.
pub(crate) fn open_preferences_window(cx: &mut App) -> WindowHandle<PreferencesWindow> {
    let preferences = match read_app_preferences() {
        Ok(preferences) => preferences,
        Err(err) => {
            eprintln!("failed to read app preferences: {err}");
            AppPreferences::default()
        }
    };
    let theme_options = cx.global::<ThemeManager>().available_themes().to_vec();
    let title = cx
        .global::<I18nManager>()
        .strings()
        .preferences_window_title
        .clone();
    open_preferences_window_with_state(cx, preferences, theme_options, title)
}

#[cfg(test)]
mod tests {
    use super::{
        AppPreferences, DEFAULT_PICGO_SERVER_URL, EditorSettings, ImagePasteBehavior,
        StartupOpenPreference, StatusBarPreferences,
        load_or_create_app_preferences_with_dirs_and_locales, open_preferences_window_with_state,
        read_app_preferences_with_dirs, save_app_preferences_with_dirs,
        save_preferences_from_window_with_dirs,
    };
    use crate::config::VelotypeConfigDirs;
    use crate::fonts::FontPreferences;
    use crate::i18n::I18nManager;
    use crate::theme::{ThemeCatalogEntry, ThemeManager};
    use gpui::TestAppContext;
    use std::collections::BTreeMap;

    fn init_preferences_test_app(cx: &mut TestAppContext) {
        cx.update(|cx| {
            I18nManager::init_with_language_id(cx, "en-US");
            ThemeManager::init_with_theme_id(cx, "velotype");
            crate::components::init(cx);
            EditorSettings::init(cx, true);
        });
    }

    fn default_theme_options() -> Vec<ThemeCatalogEntry> {
        vec![ThemeCatalogEntry {
            id: "velotype".into(),
            name: "Velotype".into(),
        }]
    }

    #[test]
    fn file_association_button_is_disabled_when_associated_or_busy() {
        use super::FileAssociationStatus::*;
        for state in [Checking, Associating, Associated] {
            assert!(!state.can_associate());
        }
        for state in [
            NotAssociated,
            NeedsConfirmation,
            Failed("unavailable".into()),
        ] {
            assert!(state.can_associate());
            assert!(!state.is_busy());
        }
        assert!(!Associated.is_busy());
        assert!(Checking.is_busy());
        assert!(Associating.is_busy());
    }

    #[test]
    fn missing_preferences_file_returns_defaults() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-missing-{}",
            uuid::Uuid::new_v4()
        ));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let preferences =
            read_app_preferences_with_dirs(&dirs).expect("missing preferences should load");
        assert_eq!(preferences, AppPreferences::default());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn updater_preferences_are_opt_in_and_invalid_values_remain_disabled() {
        for text in [
            "",
            "[updates]\nauto_download = 'yes'",
            "[updates]\nauto_download = false",
        ] {
            let value: toml::Value = toml::from_str(text).unwrap();
            assert!(!super::app_preferences_from_toml_value(&value, "en-US").auto_download_updates);
        }
        let value: toml::Value = toml::from_str("[updates]\nauto_download = true").unwrap();
        let preferences = super::app_preferences_from_toml_value(&value, "en-US");
        assert!(preferences.auto_download_updates);
        let text = toml::to_string(&super::PreferencesFile::from(&preferences)).unwrap();
        assert!(text.contains("[updates]\nauto_download = true"));
    }

    #[test]
    fn partial_or_invalid_preferences_fall_back_by_field() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-partial-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::write(
            dirs.app_config_file(),
            r#"
                [startup]
                open = "not-valid"

                [theme]
                default_theme_id = "velotype-light"
            "#,
        )
        .expect("preferences should be written");

        let preferences =
            read_app_preferences_with_dirs(&dirs).expect("partial preferences should load");
        assert_eq!(preferences.startup_open, StartupOpenPreference::NewFile);
        assert_eq!(preferences.default_language_id, "en-US");
        assert_eq!(preferences.default_theme_id, "velotype-light");
        assert_eq!(preferences.image_paste_behavior, ImagePasteBehavior::None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_font_stacks_fall_back_independently() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-invalid-font-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::write(
            dirs.app_config_file(),
            r#"
                [fonts]
                body_stack = "Georgia, serif"
                code_stack = ""
                ui_stack = "Inter, sans-serif"
            "#,
        )
        .expect("preferences should be written");

        let preferences = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
        assert_eq!(preferences.fonts.body_stack, "Georgia, serif");
        assert_eq!(
            preferences.fonts.code_stack,
            FontPreferences::default().code_stack
        );
        assert_eq!(preferences.fonts.ui_stack, "Inter, sans-serif");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_image_paste_behavior_falls_back_to_none() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-image-invalid-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::write(
            dirs.app_config_file(),
            r#"
                [editor]
                image_paste_behavior = "somewhere-dangerous"
            "#,
        )
        .expect("preferences should be written");

        let preferences = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
        assert_eq!(preferences.image_paste_behavior, ImagePasteBehavior::None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn picgo_settings_round_trip_and_never_use_picgo_as_fallback() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-picgo-{}",
            uuid::Uuid::new_v4()
        ));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let preferences = AppPreferences {
            image_paste_behavior: ImagePasteBehavior::UsePicGo,
            image_paste_fallback: ImagePasteBehavior::CopyToAssetsFolder,
            picgo_server_url: "http://localhost:4444".into(),
            ..AppPreferences::default()
        };
        save_app_preferences_with_dirs(&preferences, &dirs).unwrap();
        assert_eq!(read_app_preferences_with_dirs(&dirs).unwrap(), preferences);
        let text = std::fs::read_to_string(dirs.app_config_file()).unwrap();
        assert!(text.contains("image_paste_behavior = \"use_picgo\""));
        assert!(text.contains("image_paste_fallback = \"copy_to_assets_folder\""));
        assert!(text.contains("picgo_server_url = \"http://localhost:4444\""));
        std::fs::write(dirs.app_config_file(), "[editor]\nimage_paste_behavior = \"use_picgo\"\nimage_paste_fallback = \"use_picgo\"\n").unwrap();
        assert_eq!(
            read_app_preferences_with_dirs(&dirs)
                .unwrap()
                .image_paste_fallback,
            ImagePasteBehavior::None
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn damaged_preferences_file_returns_defaults() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-damaged-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::write(dirs.app_config_file(), "not = [valid")
            .expect("preferences should be written");

        let preferences =
            read_app_preferences_with_dirs(&dirs).expect("damaged preferences should load");
        assert_eq!(preferences, AppPreferences::default());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saves_and_reads_preferences() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-save-{}",
            uuid::Uuid::new_v4()
        ));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let preferences = AppPreferences {
            startup_open: StartupOpenPreference::LastOpenedFile,
            default_language_id: "zh-CN".into(),
            default_theme_id: "velotype-light".into(),
            show_table_headers: false,
            image_paste_behavior: ImagePasteBehavior::CopyToAssetsFolder,
            image_paste_fallback: ImagePasteBehavior::CopyToAssetsFolder,
            picgo_server_url: DEFAULT_PICGO_SERVER_URL.into(),
            auto_download_updates: true,
            keybindings: BTreeMap::new(),
            status_bar: StatusBarPreferences::default(),
            fonts: FontPreferences {
                body_stack: "Segoe UI, sans-serif".into(),
                code_stack: "Consolas, monospace".into(),
                ui_stack: ".SystemUIFont".into(),
            },
        };

        save_app_preferences_with_dirs(&preferences, &dirs)
            .expect("preferences should save to config.toml");
        let loaded = read_app_preferences_with_dirs(&dirs).expect("preferences should read back");
        assert_eq!(loaded, preferences);

        let text =
            std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
        assert!(text.contains("open = \"last_opened_file\""));
        assert!(text.contains("default_language_id = \"zh-CN\""));
        assert!(text.contains("default_theme_id = \"velotype-light\""));
        assert!(text.contains("show_table_headers = false"));
        assert!(text.contains("image_paste_behavior = \"copy_to_assets_folder\""));
        assert!(text.contains("[fonts]"));
        assert!(text.contains("body_stack = \"Segoe UI, sans-serif\""));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_preferences_file_is_created_with_detected_language() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-create-{}",
            uuid::Uuid::new_v4()
        ));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let preferences = load_or_create_app_preferences_with_dirs_and_locales(&dirs, ["zh-HK"])
            .expect("preferences should be created");
        assert_eq!(preferences.default_language_id, "zh-CN");
        assert!(dirs.app_config_file().exists());
        let text =
            std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
        assert!(text.contains("[language]"));
        assert!(text.contains("default_language_id = \"zh-CN\""));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_preferences_are_normalized_with_language() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-legacy-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::write(
            dirs.app_config_file(),
            r#"
                [startup]
                open = "last_opened_file"

                [theme]
                default_theme_id = "velotype-light"
            "#,
        )
        .expect("legacy preferences should be written");

        let preferences = load_or_create_app_preferences_with_dirs_and_locales(&dirs, ["en-GB"])
            .expect("legacy preferences should normalize");
        assert_eq!(
            preferences.startup_open,
            StartupOpenPreference::LastOpenedFile
        );
        assert_eq!(preferences.default_language_id, "en-US");
        assert_eq!(preferences.default_theme_id, "velotype-light");
        let text =
            std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
        assert!(text.contains("[language]"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saving_preferences_window_preserves_language() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-window-{}",
            uuid::Uuid::new_v4()
        ));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let preferences = AppPreferences {
            startup_open: StartupOpenPreference::NewFile,
            default_language_id: "zh-CN".into(),
            default_theme_id: "velotype".into(),
            show_table_headers: true,
            image_paste_behavior: ImagePasteBehavior::None,
            image_paste_fallback: ImagePasteBehavior::None,
            picgo_server_url: DEFAULT_PICGO_SERVER_URL.into(),
            auto_download_updates: false,
            keybindings: BTreeMap::new(),
            status_bar: StatusBarPreferences::default(),
            fonts: FontPreferences::default(),
        };
        save_app_preferences_with_dirs(&preferences, &dirs)
            .expect("preferences should save to config.toml");

        let saved = save_preferences_from_window_with_dirs(
            StartupOpenPreference::LastOpenedFile,
            "velotype-light",
            ImagePasteBehavior::CopyToNamedAssetsFolder,
            ImagePasteBehavior::CopyToNamedAssetsFolder,
            DEFAULT_PICGO_SERVER_URL,
            true,
            BTreeMap::from([("save_document".to_string(), vec!["ctrl-alt-s".to_string()])]),
            &StatusBarPreferences::default(),
            &FontPreferences::default(),
            &dirs,
        )
        .expect("window preferences should save");
        assert_eq!(saved.default_language_id, "zh-CN");
        assert_eq!(saved.startup_open, StartupOpenPreference::LastOpenedFile);
        assert_eq!(saved.default_theme_id, "velotype-light");
        assert_eq!(
            saved.image_paste_behavior,
            ImagePasteBehavior::CopyToNamedAssetsFolder
        );
        assert_eq!(
            saved.keybindings.get("save_document"),
            Some(&vec!["ctrl-alt-s".to_string()])
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preferences_saves_and_loads_adaptive_system_theme() {
        let root = std::env::temp_dir().join(format!(
            "velotype-preferences-system-theme-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp root should exist");
        let dirs = VelotypeConfigDirs::from_root(&root);

        let saved = save_preferences_from_window_with_dirs(
            StartupOpenPreference::NewFile,
            crate::theme::THEME_SYSTEM_ID,
            ImagePasteBehavior::None,
            ImagePasteBehavior::None,
            DEFAULT_PICGO_SERVER_URL,
            true,
            BTreeMap::new(),
            &StatusBarPreferences::default(),
            &FontPreferences::default(),
            &dirs,
        )
        .expect("window preferences should save");
        assert_eq!(saved.default_theme_id, crate::theme::THEME_SYSTEM_ID);

        let loaded = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
        assert_eq!(loaded.default_theme_id, crate::theme::THEME_SYSTEM_ID);

        let toml_text =
            std::fs::read_to_string(dirs.app_config_file()).expect("config file exists");
        assert!(toml_text.contains("default_theme_id = \"system\""));

        let _ = std::fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn preferences_window_activates_and_focuses_on_open(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);

        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
        assert_eq!(active_window.window_id(), handle.window_id());
        handle
            .update(cx, |preferences, _window, _cx| {
                assert_eq!(preferences.nav, super::PreferencesNav::Settings);
            })
            .expect("preferences window should be updateable");
        assert!(
            handle
                .update(cx, |preferences, window, _cx| preferences
                    .focus_handle
                    .is_focused(window))
                .expect("preferences window should be updateable")
        );
        assert!(
            !handle
                .update(cx, |preferences, _window, _cx| preferences
                    .has_unsaved_changes())
                .expect("preferences window should be updateable")
        );
    }

    #[cfg(target_os = "windows")]
    #[gpui::test]
    async fn preferences_file_association_check_completes_in_background(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();
        handle
            .update(cx, |preferences, _window, _cx| {
                assert!(
                    matches!(
                        preferences.file_associations,
                        super::FileAssociationStatus::Associated
                            | super::FileAssociationStatus::NotAssociated
                    ),
                    "unexpected association check result: {:?}",
                    preferences.file_associations
                );
                assert!(!preferences.has_unsaved_changes());
            })
            .unwrap();
    }

    #[gpui::test]
    async fn preferences_dirty_state_tracks_draft_changes(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);

        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, _window, _cx| {
                assert!(!preferences.has_unsaved_changes());
                preferences.file_associations = super::FileAssociationStatus::Associated;
                assert!(!preferences.has_unsaved_changes());
                preferences.file_associations = super::FileAssociationStatus::NotAssociated;
                assert!(!preferences.has_unsaved_changes());
                preferences.startup_open = StartupOpenPreference::LastOpenedFile;
                assert!(preferences.has_unsaved_changes());
                preferences.startup_open = StartupOpenPreference::NewFile;
                assert!(!preferences.has_unsaved_changes());

                preferences.image_paste_behavior = ImagePasteBehavior::CopyToAssetsFolder;
                assert!(preferences.has_unsaved_changes());
                preferences.image_paste_behavior = ImagePasteBehavior::None;
                assert!(!preferences.has_unsaved_changes());

                preferences.image_paste_behavior = ImagePasteBehavior::UsePicGo;
                assert!(preferences.has_unsaved_changes());
                preferences.image_paste_behavior = ImagePasteBehavior::None;
                preferences.picgo_server_url = "http://localhost:40000".into();
                assert!(preferences.has_unsaved_changes());
                preferences.picgo_server_url = DEFAULT_PICGO_SERVER_URL.into();
                assert!(!preferences.has_unsaved_changes());

                preferences.auto_download_updates = true;
                assert!(preferences.has_unsaved_changes());
                preferences.auto_download_updates = false;
                assert!(!preferences.has_unsaved_changes());

                preferences
                    .keybindings
                    .insert("save_document".into(), vec!["ctrl-alt-s".into()]);
                assert!(preferences.has_unsaved_changes());
            })
            .expect("preferences window should be updateable");
    }

    #[gpui::test]
    async fn font_input_changes_update_the_preferences_draft(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);
        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();
        let input = handle
            .update(cx, |preferences, _window, _cx| {
                preferences.body_font_input.clone()
            })
            .expect("preferences window should be updateable");
        input.update(cx, |input, cx| input.set_value("Georgia, serif", cx));
        cx.run_until_parked();
        handle
            .update(cx, |preferences, _window, _cx| {
                assert_eq!(preferences.fonts.body_stack, "Georgia, serif");
                assert!(preferences.has_unsaved_changes());
            })
            .expect("preferences window should be updateable");

        input.update(cx, |input, cx| input.set_value("", cx));
        cx.run_until_parked();
        handle
            .update(cx, |preferences, _window, cx| {
                let strings = cx.global::<I18nManager>().strings();
                assert_eq!(
                    preferences.font_validation_error(strings).as_deref(),
                    Some("Body font: Enter at least one font family.")
                );
            })
            .expect("preferences window should expose live font validation");
    }

    #[gpui::test]
    async fn applying_saved_preferences_keeps_window_open_and_focused(cx: &mut TestAppContext) {
        init_preferences_test_app(cx);

        let handle = cx.update(|cx| {
            open_preferences_window_with_state(
                cx,
                AppPreferences::default(),
                default_theme_options(),
                "Preferences".into(),
            )
        });
        cx.run_until_parked();

        handle
            .update(cx, |preferences, window, cx| {
                preferences.startup_open = StartupOpenPreference::LastOpenedFile;
                assert!(preferences.has_unsaved_changes());
                let saved = AppPreferences {
                    startup_open: StartupOpenPreference::LastOpenedFile,
                    ..AppPreferences::default()
                };
                preferences.apply_saved_preferences(saved, window, cx);
            })
            .expect("preferences window should be updateable");
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
        assert_eq!(active_window.window_id(), handle.window_id());
        assert!(
            handle
                .update(cx, |preferences, window, _cx| preferences
                    .focus_handle
                    .is_focused(window))
                .expect("preferences window should remain updateable")
        );
        assert!(
            !handle
                .update(cx, |preferences, _window, _cx| preferences
                    .has_unsaved_changes())
                .expect("preferences window should remain updateable")
        );
    }
}
