//! Bench: shared display text
//!
//! Validates commit `1a21dd5 perf(editor): cache display text as
//! SharedString`. Pre-commit, `BlockTextElement::request_layout` did
//! `let content = input.display_text().to_string();` then `content.into()`
//! to convert `String -> SharedString` on every render frame — two
//! allocations sized to the block's visible text. Post-commit,
//! `input.shared_display_text()` returns a cached `SharedString` clone
//! (atomic Arc bump), recomputed only when the text actually changes.
//!
//! Uses gpui's public `SharedString` directly — no mocks needed.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gpui::SharedString;

fn shared_display_text(c: &mut Criterion) {
    let inputs = [
        (
            "short ~56 B",
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".to_string(),
        ),
        (
            "long ~4 KB",
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(72),
        ),
    ];

    let mut group = c.benchmark_group("shared display text");
    for (label, text) in &inputs {
        let cached: SharedString = SharedString::from(text.clone());
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("baseline (String alloc + into)", label),
            text,
            |b, text| {
                b.iter(|| {
                    let owned: String = text.clone();
                    let s: SharedString = owned.into();
                    black_box(s);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current (Arc bump)", label),
            &cached,
            |b, cached| {
                b.iter(|| {
                    let s: SharedString = cached.clone();
                    black_box(s);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, shared_display_text);
criterion_main!(benches);
