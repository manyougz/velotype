//! Bench: editor_text_font fallback caching
//!
//! Validates the review-feedback refactor of `editor_text_font` /
//! `tibetan_font_fallbacks_for_target_os`. The fallback list (~5 font
//! family names) used to be rebuilt per render: 3–5 `String` allocations
//! for compile-time `&'static str` literals + a fresh `Vec` + wrapping in
//! an `Arc<Vec<String>>` (the FontFallbacks shape gpui requires).
//!
//! `editor_text_font()` is called from `Editor::render` on every frame,
//! so for a 30 Hz idle render loop the per-process allocations stack up.
//! Post-refactor: a `OnceLock<FontFallbacks>` caches the whole list once
//! per process; per-frame cost is one `Arc` clone.

use std::hint::black_box;
use std::sync::{Arc, OnceLock};

use criterion::{Criterion, criterion_group, criterion_main};

// Mirror gpui's FontFallbacks shape: Arc<Vec<String>>.
#[derive(Clone)]
#[allow(dead_code)]
struct MockFontFallbacks(Arc<Vec<String>>);

fn tibetan_font_fallbacks() -> Vec<String> {
    [
        "Microsoft Himalaya",
        "Noto Serif Tibetan",
        "Noto Sans Tibetan",
        "BabelStone Tibetan",
        "Kailasa",
    ]
    .iter()
    .map(|f| (*f).to_string())
    .collect()
}

// Baseline: build the Vec<String> + Arc fresh every call.
fn old_make_fallbacks() -> MockFontFallbacks {
    MockFontFallbacks(Arc::new(tibetan_font_fallbacks()))
}

// Current: OnceLock caches the FontFallbacks; per-call is an Arc clone.
fn new_make_fallbacks() -> MockFontFallbacks {
    static CACHE: OnceLock<MockFontFallbacks> = OnceLock::new();
    CACHE
        .get_or_init(|| MockFontFallbacks(Arc::new(tibetan_font_fallbacks())))
        .clone()
}

fn font_fallbacks_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor text font fallbacks");
    group.bench_function("baseline (build per call)", |b| {
        b.iter(|| black_box(old_make_fallbacks()));
    });
    group.bench_function("current (OnceLock + Arc clone)", |b| {
        b.iter(|| black_box(new_make_fallbacks()));
    });
    group.finish();
}

criterion_group!(benches, font_fallbacks_cache);
criterion_main!(benches);
