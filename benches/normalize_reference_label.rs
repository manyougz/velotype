//! Bench: normalize_reference_label
//!
//! Validates the review-feedback refactor of
//! `components::markdown::image::normalize_reference_label` that replaced
//! `label.split_whitespace().collect::<Vec<_>>().join(" ")` (a two-pass
//! collect + join with an intermediate Vec<&str>) with a single-pass
//! String::push_str loop.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn old_normalize(label: &str) -> Option<String> {
    let normalized = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

fn new_normalize(label: &str) -> Option<String> {
    let mut normalized = String::with_capacity(label.len());
    for word in label.split_whitespace() {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(word);
    }
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_lowercase())
    }
}

fn normalize_reference_label(c: &mut Criterion) {
    let inputs = [
        ("short", "Hello World  Foo".to_string()),
        (
            "medium",
            "   Lorem   ipsum   dolor   sit   amet,  consectetur   adipiscing   ".to_string(),
        ),
        (
            "long",
            "The   quick   brown   fox  jumps  over  the  lazy  dog ".repeat(10),
        ),
    ];

    for (label, input) in &inputs {
        assert_eq!(
            old_normalize(input),
            new_normalize(input),
            "normalize diverged on input: {label}"
        );
    }

    let mut group = c.benchmark_group("normalize reference label");
    for (label, input) in &inputs {
        group.bench_with_input(
            BenchmarkId::new("baseline (collect+join)", label),
            input.as_str(),
            |b, s| b.iter(|| black_box(old_normalize(black_box(s)))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (single-pass push_str)", label),
            input.as_str(),
            |b, s| b.iter(|| black_box(new_normalize(black_box(s)))),
        );
    }
    group.finish();
}

criterion_group!(benches, normalize_reference_label);
criterion_main!(benches);
