//! Bench: grapheme cursor boundary
//!
//! Validates commit `e3013e2 perf(editor): use GraphemeCursor for prev/next
//! boundary`. Pre-commit, `previous_boundary` and `next_boundary` built a
//! `grapheme_indices(true)` iterator over the entire display text and
//! scanned from one end until passing the cursor — O(distance to nearest
//! end of text) per arrow press. Post-commit, `GraphemeCursor::new(offset,
//! text.len(), true).prev_boundary(text, 0)` jumps to a known offset and
//! walks one boundary in O(grapheme size) ≈ O(1).
//!
//! Uses `unicode-segmentation` directly — same crate the production code
//! depends on, no internals required.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

fn old_previous_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

fn new_previous_boundary(text: &str, offset: usize) -> usize {
    let mut c = GraphemeCursor::new(offset.min(text.len()), text.len(), true);
    c.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
}

fn old_next_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

fn new_next_boundary(text: &str, offset: usize) -> usize {
    let mut c = GraphemeCursor::new(offset.min(text.len()), text.len(), true);
    c.next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

fn grapheme_cursor(c: &mut Criterion) {
    let text = "a".repeat(5_000);

    let mut group = c.benchmark_group("grapheme cursor boundary");
    // prev_boundary worst case: cursor near start ⇒ baseline walks from end
    // backward through the full string.
    let prev_offset = 10;
    assert_eq!(
        old_previous_boundary(&text, prev_offset),
        new_previous_boundary(&text, prev_offset)
    );
    group.bench_with_input(
        BenchmarkId::new("baseline (grapheme_indices.rev)", "prev near start of 5 KB"),
        &(text.as_str(), prev_offset),
        |b, &(t, o)| b.iter(|| black_box(old_previous_boundary(black_box(t), o))),
    );
    group.bench_with_input(
        BenchmarkId::new("current (GraphemeCursor)", "prev near start of 5 KB"),
        &(text.as_str(), prev_offset),
        |b, &(t, o)| b.iter(|| black_box(new_previous_boundary(black_box(t), o))),
    );

    // next_boundary worst case: cursor near end ⇒ baseline walks from start
    // forward through the full string.
    let next_offset = 4_990;
    assert_eq!(
        old_next_boundary(&text, next_offset),
        new_next_boundary(&text, next_offset)
    );
    group.bench_with_input(
        BenchmarkId::new("baseline (grapheme_indices)", "next near end of 5 KB"),
        &(text.as_str(), next_offset),
        |b, &(t, o)| b.iter(|| black_box(old_next_boundary(black_box(t), o))),
    );
    group.bench_with_input(
        BenchmarkId::new("current (GraphemeCursor)", "next near end of 5 KB"),
        &(text.as_str(), next_offset),
        |b, &(t, o)| b.iter(|| black_box(new_next_boundary(black_box(t), o))),
    );

    group.finish();
}

criterion_group!(benches, grapheme_cursor);
criterion_main!(benches);
