//! Bench: render loop
//!
//! Higher-level benchmark that simulates one frame of velotype's editor
//! re-rendering a document of N visible blocks. Every commit on
//! `perf/editor-render` (theme/i18n Arc clone, SharedString display text,
//! GraphemeCursor, blink throttle, projection cache, monotonic build text
//! runs) participates in the per-frame hot path. This bench combines all
//! of them so the speedup represents the realistic frame-time win on a
//! large document, not the isolated per-operation win.
//!
//! What one simulated frame does per block:
//!  1. Clone the global Theme   (Arc bump vs ~2 KB deep clone).
//!  2. Clone the global I18n    (Arc bump vs 137 String allocs).
//!  3. Clone the display text   (Arc bump vs full String alloc + into).
//!  4. Check the projection key (3-tuple PartialEq vs full rebuild).
//!  5. Decide blink notify      (elapsed gate vs unconditional).
//!  6. Build text runs          (monotonic span_idx vs 4× per-boundary find).
//!
//! Two block counts (50 and 200) bracket the typical / heavy document
//! scenarios the user observed in debug mode. The "current" path is
//! whatever the perf/editor-render branch leaves in place; the "baseline"
//! is the pre-commit version of each step.

use std::hint::black_box;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gpui::SharedString;

mod common;
use common::{MockFragment, MockI18nStrings, MockSpan, MockTheme, mock_fragments, mock_spans};

/// What one block's worth of per-frame work costs in the *pre-commit* code.
fn frame_step_baseline(
    theme: &MockTheme,
    strings: &MockI18nStrings,
    display_text: &str,
    spans: &[MockSpan],
    fragments: &[MockFragment],
    text_len: usize,
) -> usize {
    // 1. Theme deep clone.
    let _t = black_box(theme.clone());
    // 2. I18nStrings deep clone (137 String allocs).
    let _s = black_box(strings.clone());
    // 3. Display text: fresh String + conversion to SharedString.
    let owned: String = display_text.to_string();
    let _shared: SharedString = owned.into();
    // 4. Projection: unconditional full rebuild.
    let _proj = simulate_projection_build(fragments);
    // 5. Blink tick: unconditional "do work".
    let _notify = true;
    // 6. build_text_runs: four linear span scans per boundary.
    old_build_text_runs(spans, text_len)
}

/// What one block's worth of per-frame work costs in the *post-commit* code.
fn frame_step_current(
    theme_arc: &Arc<MockTheme>,
    strings_arc: &Arc<MockI18nStrings>,
    cached_text: &SharedString,
    cached_key: &(bool, Range<usize>, Option<Range<usize>>),
    current_key: &(bool, Range<usize>, Option<Range<usize>>),
    epoch: Instant,
    spans: &[MockSpan],
    text_len: usize,
) -> usize {
    // 1. Theme Arc clone.
    let _t = black_box(theme_arc.clone());
    // 2. I18nStrings Arc clone.
    let _s = black_box(strings_arc.clone());
    // 3. Display text Arc bump.
    let _shared = black_box(cached_text.clone());
    // 4. Projection cache hit ⇒ no rebuild.
    let _hit = black_box(cached_key) == black_box(current_key);
    // 5. Blink throttle gate.
    let _notify = epoch.elapsed().as_secs_f32() >= 0.5;
    // 6. build_text_runs: monotonic span_idx.
    new_build_text_runs(spans, text_len)
}

// --- inlined algorithms (copies from the per-commit benches so this file
// can run standalone without a workspace-shared helper) ---

fn old_build_text_runs(spans: &[MockSpan], text_len: usize) -> usize {
    let mut boundaries = vec![0, text_len];
    for s in spans {
        boundaries.push(s.range.start);
        boundaries.push(s.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut acc = 0usize;
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        let active = |o: usize| spans.iter().find(|s| s.range.start <= o && o < s.range.end);
        let style = active(start).map(|s| s.style).unwrap_or_default();
        let html = active(start).and_then(|s| s.html_style);
        let is_link = active(start).and_then(|s| s.link).is_some();
        let is_foot = active(start).and_then(|s| s.footnote).is_some();
        acc += style.bold as usize
            + style.italic as usize
            + html.unwrap_or(0) as usize
            + is_link as usize
            + is_foot as usize;
    }
    acc
}

fn new_build_text_runs(spans: &[MockSpan], text_len: usize) -> usize {
    let mut boundaries = vec![0, text_len];
    for s in spans {
        boundaries.push(s.range.start);
        boundaries.push(s.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut acc = 0usize;
    let mut span_idx = 0usize;
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        while span_idx < spans.len() && spans[span_idx].range.end <= start {
            span_idx += 1;
        }
        let active = spans
            .get(span_idx)
            .filter(|s| s.range.start <= start && start < s.range.end);
        let style = active.map(|s| s.style).unwrap_or_default();
        let html = active.and_then(|s| s.html_style);
        let is_link = active.and_then(|s| s.link).is_some();
        let is_foot = active.and_then(|s| s.footnote).is_some();
        acc += style.bold as usize
            + style.italic as usize
            + html.unwrap_or(0) as usize
            + is_link as usize
            + is_foot as usize;
    }
    acc
}

fn simulate_projection_build(fragments: &[MockFragment]) -> Option<(usize, Vec<usize>)> {
    let clean_len: usize = fragments.iter().map(|f| f.text.len()).sum();
    let mut display_to_clean: Vec<usize> = Vec::with_capacity(clean_len + 1);
    let mut clean_to_display: Vec<usize> = vec![0; clean_len + 1];
    let mut display_cursor = 0usize;
    let mut clean_cursor = 0usize;
    let mut any_expanded = false;
    for f in fragments {
        let len = f.text.len();
        if f.has_link {
            for _ in 0..2 {
                display_to_clean.push(clean_cursor);
            }
            display_cursor += 2;
            any_expanded = true;
        }
        for offset in 0..=len {
            clean_to_display[clean_cursor + offset] = display_cursor + offset;
        }
        for offset in 1..=len {
            display_to_clean.push(clean_cursor + offset);
        }
        display_cursor += len;
        clean_cursor += len;
    }
    any_expanded.then_some((display_cursor, clean_to_display))
}

fn render_loop(c: &mut Criterion) {
    // Per-block fixtures.
    let theme_owned = MockTheme::new();
    let theme_arc = Arc::new(MockTheme::new());
    let strings_owned = MockI18nStrings::new();
    let strings_arc = Arc::new(MockI18nStrings::new());
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(8);
    let cached_text = SharedString::from(text.clone());
    let spans = mock_spans(30);
    let span_text_len = spans.last().unwrap().range.end;
    let fragments = mock_fragments(20);
    let cached_key: (bool, Range<usize>, Option<Range<usize>>) = (true, 5..5, None);
    let current_key = cached_key.clone();
    let epoch = Instant::now();

    let mut group = c.benchmark_group("render loop (per frame)");
    for &n_blocks in &[50usize, 200usize] {
        group.bench_with_input(
            BenchmarkId::new("baseline", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for _ in 0..n_blocks {
                        acc += frame_step_baseline(
                            &theme_owned,
                            &strings_owned,
                            &text,
                            &spans,
                            &fragments,
                            span_text_len,
                        );
                    }
                    black_box(acc);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for _ in 0..n_blocks {
                        acc += frame_step_current(
                            &theme_arc,
                            &strings_arc,
                            &cached_text,
                            &cached_key,
                            &current_key,
                            epoch,
                            &spans,
                            span_text_len,
                        );
                    }
                    black_box(acc);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, render_loop);
criterion_main!(benches);
