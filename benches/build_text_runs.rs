//! Bench: build text runs
//!
//! Validates commit `52c5eb0 perf(editor): walk inline spans monotonically
//! in build_text_runs`. The pre-commit version did four linear
//! `spans.iter().find(...)` lookups per boundary pair (N+2 boundaries) — an
//! O(N²) walk in span count. Post-commit a single monotonic `span_idx`
//! advances through the sorted spans alongside the sorted boundary list,
//! dropping the total to O(N).
//!
//! The production function is private; both algorithms are reproduced here
//! against a synthetic span dataset so the comparison is faithful to the
//! actual change. Two parameterized sizes (30 and 100 spans) demonstrate
//! the scaling difference.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

mod common;
use common::{MockSpan, MockStyle, mock_spans};

/// Pre-commit algorithm — four O(N) lookups per boundary.
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
        let style = find_active(spans, start)
            .map(|s| s.style)
            .unwrap_or_default();
        let html = find_active(spans, start).and_then(|s| s.html_style);
        let is_link = find_active(spans, start).and_then(|s| s.link).is_some();
        let is_foot = find_active(spans, start).and_then(|s| s.footnote).is_some();
        acc += style.bold as usize
            + style.italic as usize
            + html.unwrap_or(0) as usize
            + is_link as usize
            + is_foot as usize;
    }
    acc
}

/// Post-commit algorithm — single monotonic span_idx.
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

fn find_active(spans: &[MockSpan], offset: usize) -> Option<&MockSpan> {
    spans
        .iter()
        .find(|s| s.range.start <= offset && offset < s.range.end)
}

fn build_text_runs(c: &mut Criterion) {
    let mut group = c.benchmark_group("build text runs");
    for &n in &[30usize, 100usize] {
        let spans = mock_spans(n);
        let text_len = spans.last().unwrap().range.end;
        // Sanity: both algorithms agree.
        assert_eq!(
            old_build_text_runs(&spans, text_len),
            new_build_text_runs(&spans, text_len)
        );
        group.bench_with_input(
            BenchmarkId::new("baseline (O(N^2))", n),
            &(&spans, text_len),
            |b, &(spans, text_len)| {
                b.iter(|| black_box(old_build_text_runs(black_box(spans), text_len)));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current  (O(N))", n),
            &(&spans, text_len),
            |b, &(spans, text_len)| {
                b.iter(|| black_box(new_build_text_runs(black_box(spans), text_len)));
            },
        );
    }
    group.finish();
}

// Silences "unused" on test-only struct fields that the algorithms touch
// only via field access rather than constructor matching.
fn _force_used() {
    let _ = MockStyle::default();
}

criterion_group!(benches, build_text_runs);
criterion_main!(benches);
