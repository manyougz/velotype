//! User-configurable system font stacks shared by native rendering and export.

use gpui::{App, Font, FontFallbacks, Global, font};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(crate) const SYSTEM_UI_FONT: &str = ".SystemUIFont";
pub(crate) const DEFAULT_BODY_STACK: &str = ".SystemUIFont";
pub(crate) const DEFAULT_UI_STACK: &str = ".SystemUIFont";
pub(crate) const DEFAULT_CODE_STACK: &str =
    "SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace";

#[derive(Clone, Debug)]
pub(crate) struct FontCatalog {
    families: Arc<HashMap<String, String>>,
}

impl Global for FontCatalog {}

impl FontCatalog {
    pub(crate) fn from_names(names: impl IntoIterator<Item = String>) -> Self {
        let mut families = HashMap::new();
        for name in names {
            families.entry(name.to_lowercase()).or_insert(name);
        }
        families
            .entry(SYSTEM_UI_FONT.to_lowercase())
            .or_insert_with(|| SYSTEM_UI_FONT.to_string());
        Self {
            families: Arc::new(families),
        }
    }

    fn canonical_name(&self, family: &str) -> Option<&str> {
        self.families
            .get(&family.to_lowercase())
            .map(String::as_str)
    }

    pub(crate) fn current(cx: &App) -> Self {
        cx.try_global::<Self>()
            .cloned()
            .unwrap_or_else(|| Self::from_names(std::iter::once(SYSTEM_UI_FONT.to_string())))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontPreferences {
    pub body_stack: String,
    pub code_stack: String,
    pub ui_stack: String,
}

impl Default for FontPreferences {
    fn default() -> Self {
        Self {
            body_stack: DEFAULT_BODY_STACK.into(),
            code_stack: DEFAULT_CODE_STACK.into(),
            ui_stack: DEFAULT_UI_STACK.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FontStackParseError {
    Empty,
    UnterminatedQuote,
}

impl std::fmt::Display for FontStackParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "enter at least one font family"),
            Self::UnterminatedQuote => write!(f, "font family has an unterminated quote"),
        }
    }
}

/// Parses a CSS-like comma-separated family list. Quotes group a family but
/// are not retained; duplicate matching is case-insensitive.
pub(crate) fn parse_font_stack(input: &str) -> Result<Vec<String>, FontStackParseError> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    // hand-writing text parser
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            continue;
        }
        if matches!(ch, '\'' | '"') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                Some(_) => current.push(ch),
            }
            continue;
        }
        if ch == ',' && quote.is_none() {
            push_family(&mut families, &mut current);
        } else {
            current.push(ch);
        }
    }
    if quote.is_some() {
        return Err(FontStackParseError::UnterminatedQuote);
    }
    if escaped {
        current.push('\\');
    }
    push_family(&mut families, &mut current);

    let mut seen = HashSet::new();
    families.retain(|family| seen.insert(family.to_lowercase()));
    if families.is_empty() {
        Err(FontStackParseError::Empty)
    } else {
        Ok(families)
    }
}

fn push_family(families: &mut Vec<String>, current: &mut String) {
    let family = current.trim();
    if !family.is_empty() {
        families.push(family.to_string());
    }
    current.clear();
}

fn generic_families(name: &str, target_os: &str) -> Option<&'static [&'static str]> {
    match (name.to_ascii_lowercase().as_str(), target_os) {
        ("sans-serif", "windows") => Some(&["Segoe UI", "Arial"]),
        ("sans-serif", "macos") => Some(&["Helvetica Neue", "Helvetica"]),
        ("sans-serif", _) => Some(&["Noto Sans", "DejaVu Sans", "Liberation Sans"]),
        ("serif", "windows") => Some(&["Times New Roman", "Georgia"]),
        ("serif", "macos") => Some(&["Times", "Georgia"]),
        ("serif", _) => Some(&["Noto Serif", "DejaVu Serif", "Liberation Serif"]),
        ("monospace", "windows") => Some(&["Cascadia Mono", "Consolas", "Courier New"]),
        ("monospace", "macos") => Some(&["SFMono-Regular", "Menlo", "Monaco"]),
        ("monospace", _) => Some(&["Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono"]),
        _ => None,
    }
}

pub(crate) fn tibetan_font_families(target_os: &str) -> &'static [&'static str] {
    match target_os {
        "windows" => &[
            "Microsoft Himalaya",
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "BabelStone Tibetan",
        ],
        "macos" => &["Kailasa", "Noto Serif Tibetan", "Noto Sans Tibetan"],
        _ => &[
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "Microsoft Himalaya",
            "Kailasa",
            "BabelStone Tibetan",
        ],
    }
}

const CSS_TIBETAN_FONT_FAMILIES: &[&str] = &[
    "Noto Serif Tibetan",
    "Noto Sans Tibetan",
    "Microsoft Himalaya",
    "Kailasa",
    "BabelStone Tibetan",
];

fn expand_generics(families: &[String], target_os: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for family in families {
        if let Some(expanded) = generic_families(family, target_os) {
            for candidate in expanded {
                if seen.insert(candidate.to_lowercase()) {
                    result.push((*candidate).to_string());
                }
            }
        } else if seen.insert(family.to_lowercase()) {
            result.push(family.clone());
        }
    }
    result
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedFontStack {
    #[cfg(test)]
    native_families: Vec<String>,
    /// True when none of the user-provided families can be used locally.
    /// The renderer will then use its built-in fallback chain, ending in the
    /// system UI font.
    pub uses_system_default: bool,
    css_families: Vec<String>,
    native_font: Font,
}

impl ResolvedFontStack {
    fn resolve(
        input: &str,
        catalog: &FontCatalog,
        target_os: &str,
        tibetan: bool,
    ) -> Result<Self, FontStackParseError> {
        let requested = parse_font_stack(input)?;
        let expanded = expand_generics(&requested, target_os);
        let mut native_families = Vec::new();
        let mut has_available_requested_family = false;
        for family in &expanded {
            if let Some(canonical) = catalog.canonical_name(family) {
                push_unique(&mut native_families, canonical);
                has_available_requested_family = true;
            } else {
                // A missing entry is normal for a cross-platform font stack.
                // Keep it in the CSS export stack below, but do not surface it
                // unless no requested family can be used at all.
            }
        }
        if !has_available_requested_family {
            push_unique(&mut native_families, SYSTEM_UI_FONT);
        }
        let mut css_families = requested;
        if tibetan {
            for family in tibetan_font_families(target_os) {
                if let Some(canonical) = catalog.canonical_name(family) {
                    push_unique(&mut native_families, canonical);
                }
            }
            for family in CSS_TIBETAN_FONT_FAMILIES {
                push_unique(&mut css_families, family);
            }
        }
        push_unique(&mut native_families, SYSTEM_UI_FONT);
        push_unique(&mut css_families, SYSTEM_UI_FONT);
        let native_font = native_font(&native_families);
        Ok(Self {
            #[cfg(test)]
            native_families,
            uses_system_default: !has_available_requested_family,
            css_families,
            native_font,
        })
    }

    pub(crate) fn font(&self) -> Font {
        self.native_font.clone()
    }

    pub(crate) fn css_font_family(&self) -> String {
        self.css_families
            .iter()
            .map(|family| {
                if family.eq_ignore_ascii_case(SYSTEM_UI_FONT) {
                    "system-ui".to_string()
                } else if is_css_generic_family(family) {
                    family.to_ascii_lowercase()
                } else {
                    css_quote_family(family)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn native_font(families: &[String]) -> Font {
    let mut result = font(
        families
            .first()
            .cloned()
            .unwrap_or_else(|| SYSTEM_UI_FONT.into()),
    );
    if families.len() > 1 {
        result.fallbacks = Some(FontFallbacks::from_fonts(families[1..].to_vec()));
    }
    result
}

fn is_css_generic_family(family: &str) -> bool {
    matches!(
        family.to_ascii_lowercase().as_str(),
        "serif" | "sans-serif" | "monospace"
    )
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

fn css_quote_family(family: &str) -> String {
    let escaped = family
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '<' => "\\3c ".chars().collect(),
            '>' => "\\3e ".chars().collect(),
            '\n' | '\r' | '\u{000c}' => " ".chars().collect(),
            ch if ch.is_control() => format!("\\{:x} ", ch as u32).chars().collect(),
            ch => vec![ch],
        })
        .collect::<String>();
    format!("\"{escaped}\"")
}

#[derive(Clone, Debug)]
pub(crate) struct FontSettings {
    pub body: ResolvedFontStack,
    pub code: ResolvedFontStack,
    pub ui: ResolvedFontStack,
}

impl Global for FontSettings {}

impl FontSettings {
    pub(crate) fn stack_uses_system_default(
        input: &str,
        catalog: &FontCatalog,
        target_os: &str,
    ) -> Result<bool, FontStackParseError> {
        Ok(ResolvedFontStack::resolve(input, catalog, target_os, true)?.uses_system_default)
    }

    pub(crate) fn resolve(
        preferences: FontPreferences,
        catalog: &FontCatalog,
        target_os: &str,
    ) -> Result<Self, FontStackParseError> {
        Ok(Self {
            body: ResolvedFontStack::resolve(&preferences.body_stack, catalog, target_os, true)?,
            code: ResolvedFontStack::resolve(&preferences.code_stack, catalog, target_os, true)?,
            ui: ResolvedFontStack::resolve(&preferences.ui_stack, catalog, target_os, true)?,
        })
    }

    pub(crate) fn init(cx: &mut App, preferences: FontPreferences) {
        let catalog = FontCatalog::from_names(cx.text_system().all_font_names());
        let settings =
            Self::resolve(preferences, &catalog, std::env::consts::OS).unwrap_or_else(|_| {
                Self::resolve(FontPreferences::default(), &catalog, std::env::consts::OS)
                    .expect("default font stacks are valid")
            });
        cx.set_global(catalog);
        cx.set_global(settings);
    }

    pub(crate) fn update(
        cx: &mut App,
        preferences: FontPreferences,
    ) -> Result<(), FontStackParseError> {
        let catalog = FontCatalog::current(cx);
        let settings = Self::resolve(preferences, &catalog, std::env::consts::OS)?;
        cx.set_global(settings);
        Ok(())
    }

    pub(crate) fn current(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_else(|| {
            Self::resolve(
                FontPreferences::default(),
                &FontCatalog::current(cx),
                std::env::consts::OS,
            )
            .expect("default font stacks are valid")
        })
    }

    pub(crate) fn body_font(cx: &App) -> Font {
        Self::current_stack_font(cx, |settings| &settings.body)
    }

    pub(crate) fn code_font(cx: &App) -> Font {
        Self::current_stack_font(cx, |settings| &settings.code)
    }

    pub(crate) fn ui_font(cx: &App) -> Font {
        Self::current_stack_font(cx, |settings| &settings.ui)
    }

    fn current_stack_font(cx: &App, select: impl FnOnce(&Self) -> &ResolvedFontStack) -> Font {
        if let Some(settings) = cx.try_global::<Self>() {
            select(settings).font()
        } else {
            let fallback = Self::current(cx);
            select(&fallback).font()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_whitespace_empty_items_and_duplicates() {
        assert_eq!(
            parse_font_stack("  'Noto Sans', Arial, , \"Noto Sans\", serif").unwrap(),
            vec!["Noto Sans", "Arial", "serif"]
        );
    }

    #[test]
    fn rejects_empty_and_unterminated_stacks() {
        assert_eq!(parse_font_stack(" ,  "), Err(FontStackParseError::Empty));
        assert_eq!(
            parse_font_stack("\"Noto Sans"),
            Err(FontStackParseError::UnterminatedQuote)
        );
    }

    #[test]
    fn resolves_generics_and_only_uses_system_default_when_none_are_available() {
        let catalog = FontCatalog::from_names(vec![
            "Segoe UI".into(),
            "Arial".into(),
            "Microsoft Himalaya".into(),
            SYSTEM_UI_FONT.into(),
        ]);
        let stack =
            ResolvedFontStack::resolve("Missing, sans-serif", &catalog, "windows", true).unwrap();
        assert!(!stack.uses_system_default);
        assert_eq!(&stack.native_families[..2], &["Segoe UI", "Arial"]);
        assert_eq!(stack.native_families.last().unwrap(), SYSTEM_UI_FONT);
        assert!(stack.css_font_family().contains("\"Missing\""));

        let missing_only =
            ResolvedFontStack::resolve("Missing", &catalog, "windows", true).unwrap();
        assert!(missing_only.uses_system_default);
        assert_eq!(
            missing_only.native_families.first().unwrap(),
            SYSTEM_UI_FONT
        );
        assert_eq!(
            missing_only.native_families.get(1).unwrap(),
            "Microsoft Himalaya"
        );
    }

    #[test]
    fn css_serialization_escapes_user_controlled_names() {
        let stack = ResolvedFontStack::resolve(
            "'A\\\"; color:red;/*</style>'",
            &FontCatalog::from_names(Vec::new()),
            "linux",
            false,
        )
        .unwrap();
        let css = stack.css_font_family();
        assert!(css.starts_with("\"A\\\"; color:red;/*"));
        assert!(css.contains("\\3c /style\\3e "));
        assert!(!css.contains("</style>"));
        assert!(css.ends_with("system-ui"));
    }

    #[test]
    fn css_preserves_generic_families_for_cross_platform_fallback() {
        let stack = ResolvedFontStack::resolve(
            "Local Code, monospace",
            &FontCatalog::from_names(vec!["Local Code".into()]),
            "macos",
            false,
        )
        .unwrap();
        assert_eq!(
            stack.css_font_family(),
            "\"Local Code\", monospace, system-ui"
        );
    }

    #[test]
    fn catalog_restores_the_installed_fonts_canonical_name() {
        let stack = ResolvedFontStack::resolve(
            "iNtEr",
            &FontCatalog::from_names(vec!["Inter".into()]),
            "linux",
            false,
        )
        .unwrap();
        assert_eq!(
            stack.native_families.first().map(String::as_str),
            Some("Inter")
        );
        assert_eq!(stack.font().family.to_string(), "Inter");
    }
}
