//! Velotype - a block-based Markdown editor built with GPUI.
//!
//! Reads file paths from command-line arguments and opens one GPUI window per
//! file. With no arguments, a single empty window is created.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use gpui::*;

mod app_identity;
mod app_menu;
mod components;
mod config;
mod editor;
mod export;
mod i18n;
mod net;
mod theme;

use app_menu::{init as init_app_menu, open_editor_window};
use components::init as init_editor;
use i18n::I18nManager;
use theme::ThemeManager;

fn main() {
    let input_paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();

    Application::new().run(move |cx: &mut App| {
        let preferences = config::load_or_create_app_preferences().unwrap_or_else(|err| {
            eprintln!("failed to initialize app preferences: {err}");
            Default::default()
        });
        I18nManager::init_with_language_id(cx, &preferences.default_language_id);
        ThemeManager::init_with_theme_id(cx, &preferences.default_theme_id);
        net::install_http_client(cx);
        init_editor(cx);
        init_app_menu(cx);

        if input_paths.is_empty() {
            if preferences.startup_open == config::StartupOpenPreference::LastOpenedFile {
                if let Some(path) = config::first_existing_recent_markdown_file() {
                    match std::fs::read_to_string(&path) {
                        Ok(markdown) => {
                            open_editor_window(cx, markdown, Some(path));
                            return;
                        }
                        Err(err) => {
                            eprintln!(
                                "failed to read last opened file '{}': {err}",
                                path.display()
                            );
                        }
                    }
                }
            }
            open_editor_window(cx, String::new(), None);
            return;
        }

        for path in &input_paths {
            let markdown = match std::fs::read_to_string(path) {
                Ok(content) => {
                    if let Err(err) = config::record_recent_file(path) {
                        eprintln!("failed to update recent file history: {err}");
                    }
                    content
                }
                Err(err) => {
                    eprintln!(
                        "failed to read '{}': {err}. opened as empty document.",
                        path.display()
                    );
                    String::new()
                }
            };
            open_editor_window(cx, markdown, Some(path.clone()));
        }
        app_menu::install_menus(cx);
        cx.refresh_windows();
    });
}
