//! Bench: exporter inline walker
//!
//! Validates the review-feedback refactor of the unguarded
//! `let ch = line[index..].chars().next().unwrap();` fallback inside
//! `export::html::rewrite_inline_math_line`. The new code is a `let-else`
//! that breaks the loop on EOF instead of panicking.
//!
//! Per the corresponding MUST review item the change is a safety win, not
//! a performance one — the surrounding `while index < line.len()` already
//! guarantees there's at least one char, so the unwrap is dead-code-eliminated
//! by the optimizer. This bench measures the per-char copy loop to confirm
//! there's no regression on long ASCII / multi-byte inline strings.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

// OLD: pre-refactor — unwrap on the per-char peek inside the copy fallback.
fn old_copy_chars(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        let ch = line[index..].chars().next().unwrap();
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

// NEW: post-refactor — let-else exits the loop instead of panicking at EOF.
fn new_copy_chars(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        let Some(ch) = line[index..].chars().next() else {
            break;
        };
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn exporter_inline_walker(c: &mut Criterion) {
    let inputs = [
        (
            "ascii ~1 KB",
            "the quick brown fox jumps over the lazy dog. ".repeat(22),
        ),
        (
            "multibyte ~1 KB",
            "日本語のテキストと絵文字 🎉 と code `x` と markdown **bold** ".repeat(12),
        ),
    ];

    for (label, text) in &inputs {
        assert_eq!(
            old_copy_chars(text),
            new_copy_chars(text),
            "exporter walker diverged on input: {label}"
        );
    }

    let mut group = c.benchmark_group("exporter inline walker");
    for (label, text) in &inputs {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("baseline (unwrap)", label),
            text.as_str(),
            |b, t| b.iter(|| black_box(old_copy_chars(black_box(t)))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (let-else)", label),
            text.as_str(),
            |b, t| b.iter(|| black_box(new_copy_chars(black_box(t)))),
        );
    }
    group.finish();
}

criterion_group!(benches, exporter_inline_walker);
criterion_main!(benches);
