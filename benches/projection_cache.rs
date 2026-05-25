//! Bench: projection cache
//!
//! Validates commit `7c1a86c perf(editor): cache projection rebuild inputs`.
//! Pre-commit, `Block::sync_inline_projection_for_focus` ran on every
//! render — every cursor move and every 30 Hz blink tick — and
//! unconditionally invoked `ExpandedInlineProjection::build`, a full
//! O(fragments + text) walk that allocates several parallel index `Vec`s.
//! Post-commit, the function short-circuits when
//! `(supports_projection, clean_selected, clean_marked)` matches the
//! cached key from the last build.
//!
//! `ExpandedInlineProjection::build` is `pub(super)` and unreachable from
//! this external crate, so the baseline is a fragment-walk simulation
//! with the same allocation profile (two parallel `Vec<usize>` index
//! tables sized to the total text length). The cache-hit path is a
//! 3-tuple `PartialEq`. Cache-miss runs the rebuild, so it costs the
//! same as baseline; the commit's value is the ~100 % hit rate during
//! cursor blink and intra-fragment movement.

use std::hint::black_box;
use std::ops::Range;

use criterion::{Criterion, criterion_group, criterion_main};

mod common;
use common::{MockFragment, mock_fragments};

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

fn projection_cache(c: &mut Criterion) {
    let fragments = mock_fragments(40);
    let cached_key: (bool, Range<usize>, Option<Range<usize>>) = (true, 5..5, None);
    let current_key = cached_key.clone();

    let mut group = c.benchmark_group("projection cache");
    group.bench_function("baseline (always rebuild)", |b| {
        b.iter(|| {
            let result = simulate_projection_build(black_box(&fragments));
            black_box(result);
        });
    });
    group.bench_function("current (cache hit)", |b| {
        b.iter(|| {
            let hit = black_box(&cached_key) == black_box(&current_key);
            black_box(hit);
        });
    });
    group.finish();
}

criterion_group!(benches, projection_cache);
criterion_main!(benches);
