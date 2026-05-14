//! Persistent app preferences and the preferences window.

use std::path::PathBuf;

use anyhow::Context as _;
use gpui::*;
use serde::Serialize;

use super::{VelotypeConfigDirs, read_recent_files};
use crate::app_identity::VELOTYPE_APP_ID;
use crate::i18n::{I18nManager, language_id_for_locale_preferences};
use crate::theme::{Theme, ThemeCatalogEntry, ThemeManager};

const DEFAULT_THEME_ID: &str = "velotype";
const DEFAULT_LANGUAGE_ID: &str = "en-US";

/// Startup document selection stored in `config.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupOpenPreference {
    NewFile,
    LastOpenedFile,
}

impl StartupOpenPreference {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NewFile => "new_file",
            Self::LastOpenedFile => "last_opened_file",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "last_opened_file" => Self::LastOpenedFile,
            _ => Self::NewFile,
        }
    }
}

/// User preferences persisted under the app config directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppPreferences {
    pub(crate) startup_open: StartupOpenPreference,
    pub(crate) default_language_id: String,
    pub(crate) default_theme_id: String,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            startup_open: StartupOpenPreference::NewFile,
            default_language_id: DEFAULT_LANGUAGE_ID.into(),
            default_theme_id: DEFAULT_THEME_ID.into(),
        }
    }
}

#[derive(Serialize)]
struct PreferencesFile {
    startup: StartupPreferencesFile,
    language: LanguagePreferencesFile,
    theme: ThemePreferencesFile,
}

#[derive(Serialize)]
struct StartupPreferencesFile {
    open: String,
}

#[derive(Serialize)]
struct LanguagePreferencesFile {
    default_language_id: String,
}

#[derive(Serialize)]
struct ThemePreferencesFile {
    default_theme_id: String,
}

impl From<&AppPreferences> for PreferencesFile {
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
        }
    }
}

pub(crate) fn read_app_preferences() -> anyhow::Result<AppPreferences> {
    read_app_preferences_with_dirs(&VelotypeConfigDirs::from_system()?)
}

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

pub(crate) fn load_or_create_app_preferences() -> anyhow::Result<AppPreferences> {
    let dirs = VelotypeConfigDirs::from_system()?;
    load_or_create_app_preferences_with_dirs_and_locales(&dirs, sys_locale::get_locales())
}

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

    AppPreferences {
        startup_open,
        default_language_id,
        default_theme_id,
    }
}

fn detected_language_id_from_locales<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    language_id_for_locale_preferences(locales)
}

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

pub(crate) fn save_app_preferences(preferences: &AppPreferences) -> anyhow::Result<()> {
    save_app_preferences_with_dirs(preferences, &VelotypeConfigDirs::from_system()?)
}

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

pub(crate) fn first_existing_recent_markdown_file() -> Option<PathBuf> {
    let recent_files = read_recent_files().ok()?;
    recent_files.into_iter().find(|path| path.is_file())
}

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

pub(crate) fn apply_configured_theme(cx: &mut App, theme_id: &str) -> anyhow::Result<bool> {
    let mut applied = false;
    let changed = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
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

pub(crate) fn save_preferences_from_window(
    startup_open: StartupOpenPreference,
    default_theme_id: &str,
) -> anyhow::Result<AppPreferences> {
    let dirs = VelotypeConfigDirs::from_system()?;
    save_preferences_from_window_with_dirs(startup_open, default_theme_id, &dirs)
}

fn save_preferences_from_window_with_dirs(
    startup_open: StartupOpenPreference,
    default_theme_id: &str,
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<AppPreferences> {
    let mut preferences =
        load_or_create_app_preferences_with_dirs_and_locales(dirs, sys_locale::get_locales())?;
    preferences.startup_open = startup_open;
    preferences.default_theme_id = default_theme_id.into();
    save_app_preferences_with_dirs(&preferences, dirs)?;
    Ok(preferences)
}

fn update_app_preferences(
    update: impl FnOnce(&mut AppPreferences),
) -> anyhow::Result<AppPreferences> {
    let mut preferences = load_or_create_app_preferences()?;
    update(&mut preferences);
    save_app_preferences(&preferences)?;
    Ok(preferences)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreferencesNav {
    File,
    Theme,
}

/// Independent preferences window view.
pub(crate) struct PreferencesWindow {
    nav: PreferencesNav,
    startup_open: StartupOpenPreference,
    selected_theme_id: String,
    theme_options: Vec<ThemeCatalogEntry>,
    startup_dropdown_open: bool,
    theme_dropdown_open: bool,
}

impl PreferencesWindow {
    fn new(preferences: AppPreferences, theme_options: Vec<ThemeCatalogEntry>) -> Self {
        let selected_theme_id = if theme_options
            .iter()
            .any(|entry| entry.id == preferences.default_theme_id)
        {
            preferences.default_theme_id
        } else {
            DEFAULT_THEME_ID.into()
        };
        Self {
            nav: PreferencesNav::File,
            startup_open: preferences.startup_open,
            selected_theme_id,
            theme_options,
            startup_dropdown_open: false,
            theme_dropdown_open: false,
        }
    }

    fn selected_theme_name(&self) -> String {
        self.theme_options
            .iter()
            .find(|entry| entry.id == self.selected_theme_id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| "Velotype".into())
    }

    fn set_nav_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::File;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        cx.notify();
    }

    fn set_nav_theme(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.nav = PreferencesNav::Theme;
        self.startup_dropdown_open = false;
        self.theme_dropdown_open = false;
        cx.notify();
    }

    fn toggle_startup_dropdown(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.startup_dropdown_open = !self.startup_dropdown_open;
        self.theme_dropdown_open = false;
        cx.notify();
    }

    fn toggle_theme_dropdown(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.theme_dropdown_open = !self.theme_dropdown_open;
        self.startup_dropdown_open = false;
        cx.notify();
    }

    fn cancel(&mut self, _: &ClickEvent, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    fn save(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let preferences =
            match save_preferences_from_window(self.startup_open, &self.selected_theme_id) {
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

        let theme_changed = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            theme_manager.set_theme_by_id(&preferences.default_theme_id)
        });
        if !theme_changed {
            let _ = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
                theme_manager.set_theme_by_id(DEFAULT_THEME_ID)
            });
        }
        crate::app_menu::install_menus(cx);
        cx.refresh_windows();
        window.remove_window();
    }

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
            .w(px(280.0))
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
            .w(px(280.0))
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

    fn labeled_row(&self, label: String, control: impl IntoElement, theme: &Theme) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(280.0))
                    .text_size(px(t.dialog_body_size))
                    .font_weight(t.dialog_button_weight.to_font_weight())
                    .text_color(c.dialog_title)
                    .child(label),
            )
            .child(control)
    }

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
        self.labeled_row(strings.preferences_startup_option.clone(), dropdown, theme)
    }

    fn render_theme_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut dropdown = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(Self::dropdown_button(
                "preferences-theme-dropdown",
                self.selected_theme_name(),
                theme,
                Self::toggle_theme_dropdown,
                cx,
            ));
        if self.theme_dropdown_open {
            for (index, entry) in self.theme_options.clone().into_iter().enumerate() {
                let selected = entry.id == self.selected_theme_id;
                dropdown = dropdown.child(Self::dropdown_item(
                    ("preferences-theme-option", index),
                    entry.name,
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
        }
        self.labeled_row(strings.preferences_local_theme.clone(), dropdown, theme)
    }
}

impl Render for PreferencesWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;

        div()
            .size_full()
            .bg(c.editor_background)
            .text_color(c.dialog_body)
            .flex()
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
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(d.dialog_gap * 1.5))
                            .child(
                                div()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(match self.nav {
                                        PreferencesNav::File => {
                                            strings.preferences_nav_file.clone()
                                        }
                                        PreferencesNav::Theme => {
                                            strings.preferences_nav_theme.clone()
                                        }
                                    }),
                            )
                            .child(match self.nav {
                                PreferencesNav::File => self
                                    .render_startup_page(&theme, &strings, cx)
                                    .into_any_element(),
                                PreferencesNav::Theme => self
                                    .render_theme_page(&theme, &strings, cx)
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
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
                                    .bg(c.dialog_primary_button_bg)
                                    .hover(|this| this.bg(c.dialog_primary_button_hover))
                                    .cursor_pointer()
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(c.dialog_primary_button_text)
                                    .child(strings.preferences_save.clone())
                                    .on_click(cx.listener(Self::save)),
                            ),
                    ),
            )
    }
}

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
    let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
    cx.open_window(
        WindowOptions {
            app_id: Some(VELOTYPE_APP_ID.to_string()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(format!("Velotype - {title}").into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        },
        move |_window, cx| cx.new(move |_cx| PreferencesWindow::new(preferences, theme_options)),
    )
    .expect("preferences window should open")
}

#[cfg(test)]
mod tests {
    use super::{
        AppPreferences, StartupOpenPreference,
        load_or_create_app_preferences_with_dirs_and_locales, read_app_preferences_with_dirs,
        save_app_preferences_with_dirs, save_preferences_from_window_with_dirs,
    };
    use crate::config::VelotypeConfigDirs;

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
        };
        save_app_preferences_with_dirs(&preferences, &dirs)
            .expect("preferences should save to config.toml");

        let saved = save_preferences_from_window_with_dirs(
            StartupOpenPreference::LastOpenedFile,
            "velotype-light",
            &dirs,
        )
        .expect("window preferences should save");
        assert_eq!(saved.default_language_id, "zh-CN");
        assert_eq!(saved.startup_open, StartupOpenPreference::LastOpenedFile);
        assert_eq!(saved.default_theme_id, "velotype-light");
        let _ = std::fs::remove_dir_all(root);
    }
}
