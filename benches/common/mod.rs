//! Shared bench fixtures.
//!
//! Each `*.rs` directly under `benches/` is auto-discovered as its own bench
//! binary; subdirectories are not. Per Cargo's rules (same as `tests/`), the
//! helpers below have to live under a *subdirectory* (`common/`) and each
//! bench file pulls them in with `mod common;`.
//!
//! velotype is a bin crate, so benches can't import private items — the
//! mocks below mirror the production types' allocation profile so the
//! algorithmic comparison stays faithful (e.g. `MockTheme` is sized to match
//! the real `Theme`; `MockI18nStrings` has 137 `String` fields like the
//! real `I18nStrings`). Where the production code uses a public, accessible
//! crate (gpui's `SharedString`, `unicode-segmentation`'s `GraphemeCursor`),
//! the bench calls it directly.

#![allow(dead_code)]

use std::ops::Range;

// ---------------------------------------------------------------------------
// Theme-shaped mock — matches production Theme struct allocation profile.
// Real Theme has ~84 `Hsla` colors (16 B each), ~110 `f32` dimensions, ~22
// `f32` typography fields, plus a `String` name and `Placeholders`. Total
// ~2 KB stack + 1 String allocation per clone.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MockTheme {
    pub colors: [(f32, f32, f32, f32); 84],
    pub dimensions: [f32; 110],
    pub typography: [f32; 22],
    pub name: String,
}

impl MockTheme {
    pub fn new() -> Self {
        Self {
            colors: [(0.1, 0.2, 0.3, 1.0); 84],
            dimensions: [12.0; 110],
            typography: [14.0; 22],
            name: String::from("Velotype"),
        }
    }
}

impl Default for MockTheme {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// I18nStrings-shaped mock — 137 `String` fields, each cloned individually
// in the baseline path. Each field clone is one heap allocation.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MockI18nStrings {
    pub fields: [String; 137],
}

impl MockI18nStrings {
    pub fn new() -> Self {
        // Representative short UI strings.
        Self {
            fields: std::array::from_fn(|i| match i % 4 {
                0 => String::from("Open"),
                1 => String::from("Save"),
                2 => String::from("Cancel"),
                _ => String::from("Untitled document"),
            }),
        }
    }
}

impl Default for MockI18nStrings {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Inline span mock for build_text_runs comparison.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
pub struct MockStyle {
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone)]
pub struct MockSpan {
    pub range: Range<usize>,
    pub style: MockStyle,
    pub html_style: Option<u32>,
    pub link: Option<&'static str>,
    pub footnote: Option<&'static str>,
}

pub fn mock_spans(n: usize) -> Vec<MockSpan> {
    (0..n)
        .map(|i| MockSpan {
            range: (i * 10)..((i + 1) * 10),
            style: MockStyle {
                bold: i % 3 == 0,
                italic: i % 4 == 0,
            },
            html_style: (i % 5 == 0).then_some(i as u32),
            link: (i % 6 == 0).then_some("https://example.com"),
            footnote: (i % 7 == 0).then_some("note"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Inline fragment mock for projection_cache build cost simulation.
// ---------------------------------------------------------------------------

pub struct MockFragment {
    pub text: String,
    pub has_link: bool,
}

pub fn mock_fragments(n: usize) -> Vec<MockFragment> {
    (0..n)
        .map(|i| MockFragment {
            text: "fragment ".repeat(2),
            has_link: i % 4 == 0,
        })
        .collect()
}
