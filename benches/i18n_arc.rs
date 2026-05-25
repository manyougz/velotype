//! Bench: I18n Arc clone
//!
//! Validates commit `d221136 perf(theme,i18n): wrap Theme and I18nStrings in
//! Arc` for the I18n path. Pre-commit, `cx.global::<I18nManager>().strings()
//! .clone()` cloned an `I18nStrings` struct containing 137 `String` fields —
//! each clone is a separate heap allocation, so a single `strings().clone()`
//! was 137 allocations. Post-commit, `strings_arc()` returns an
//! `Arc<I18nStrings>` clone (atomic increment).
//!
//! The real `I18nStrings` is private to the velotype bin; this bench uses
//! `MockI18nStrings` (137 `String` fields, same allocation profile).

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

mod common;
use common::MockI18nStrings;

fn i18n_arc_clone(c: &mut Criterion) {
    let owned = MockI18nStrings::new();
    let shared = Arc::new(MockI18nStrings::new());

    let mut group = c.benchmark_group("I18n Arc clone");
    group.bench_function("baseline (137 String allocs)", |b| {
        b.iter(|| black_box(owned.clone()));
    });
    group.bench_function("current (Arc bump)", |b| {
        b.iter(|| black_box(shared.clone()));
    });
    group.finish();
}

criterion_group!(benches, i18n_arc_clone);
criterion_main!(benches);
