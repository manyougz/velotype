//! Bench: known-length Vec collection
//!
//! Validates the review-feedback refactor at `document::parse_fenced_code`
//! that replaced `Vec::new() + while-push` with `slice[start..end].to_vec()`
//! when the final length is known.
//!
//! `Vec::new()` starts at capacity 0 and doubles on each push that overflows
//! (4, 8, 16, …), so for an N-line code fence the inner loop does
//! O(log N) reallocations + memcpys before the final push. `slice.to_vec()`
//! pre-allocates the exact capacity and copies all elements in one pass.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn make_lines(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("    println!({i});")).collect()
}

// Pre-refactor: empty Vec + per-line clone + per-line push.
fn old_collect(lines: &[String], start: usize, end: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

// Post-refactor: slice + to_vec().
fn new_collect(lines: &[String], start: usize, end: usize) -> Vec<String> {
    lines[start..end].to_vec()
}

fn known_length_collect(c: &mut Criterion) {
    let mut group = c.benchmark_group("known length collect");
    for &n in &[10usize, 50, 200] {
        let lines = make_lines(n);
        assert_eq!(old_collect(&lines, 0, n), new_collect(&lines, 0, n));
        group.bench_with_input(
            BenchmarkId::new("baseline (Vec::new + while push)", n),
            &lines,
            |b, lines| b.iter(|| black_box(old_collect(black_box(lines), 0, n))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (slice.to_vec())", n),
            &lines,
            |b, lines| b.iter(|| black_box(new_collect(black_box(lines), 0, n))),
        );
    }
    group.finish();
}

criterion_group!(benches, known_length_collect);
criterion_main!(benches);
