//! Bench: Theme Arc clone
//!
//! Validates commit `d221136 perf(theme,i18n): wrap Theme and I18nStrings in
//! Arc` for the Theme path. Pre-commit, `cx.global::<ThemeManager>().current()
//! .clone()` did a deep clone of a ~2 KB struct + a String allocation for the
//! theme name on every render frame, for every visible block. Post-commit,
//! `current_arc()` returns an `Arc<Theme>` clone (atomic increment).
//!
//! The real `Theme` type is private to the velotype bin crate, so this bench
//! uses `MockTheme` (`benches/common/mod.rs`) with the same allocation
//! profile — the algorithmic comparison (deep clone vs Arc bump) is what
//! moves.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

mod common;
use common::MockTheme;

fn theme_arc_clone(c: &mut Criterion) {
    let owned = MockTheme::new();
    let shared = Arc::new(MockTheme::new());

    let mut group = c.benchmark_group("Theme Arc clone");
    group.bench_function("baseline (deep clone)", |b| {
        b.iter(|| black_box(owned.clone()));
    });
    group.bench_function("current (Arc bump)", |b| {
        b.iter(|| black_box(shared.clone()));
    });
    group.finish();
}

criterion_group!(benches, theme_arc_clone);
criterion_main!(benches);
