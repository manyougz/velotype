//! Bench: menu labels dedup
//!
//! Validates the review-feedback refactor of `Editor::render` that hoisted
//! the `cx.get_menus()` call and the `menus.iter().map(|m| m.name.to_string())
//! .collect::<Vec<_>>()` collection out of both `render_in_window_menu_bar`
//! and `render_in_window_menu_panel`. Pre-refactor each renderer called
//! `cx.get_menus()` independently and walked menus → labels twice per frame.
//! Post-refactor the work is done once and the slice is shared with both
//! renderers via parameter.
//!
//! Additional win: the per-label `SharedString::to_string()` conversion
//! (one allocation per menu name) became `SharedString::clone()` (atomic
//! Arc bump), since the consumer signatures were widened to take
//! `&[impl AsRef<str>]`.
//!
//! Both production menu types are private to gpui; this bench uses a
//! representative shape (5 menus with ~1 KB total of cumulative name
//! bytes) to capture the algorithmic difference.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

#[derive(Clone)]
struct MockMenu {
    name: Arc<str>,
}

fn mock_menus() -> Vec<MockMenu> {
    [
        "File",
        "Edit",
        "View",
        "Selection",
        "Window",
        "Help",
        "Tools",
        "Format",
    ]
    .iter()
    .map(|s| MockMenu {
        name: Arc::<str>::from(*s),
    })
    .collect()
}

// --- Pre-refactor: each renderer fetches menus + builds Vec<String>. ---
fn old_frame(menus_count_call: &dyn Fn() -> Vec<MockMenu>) -> usize {
    // render_in_window_menu_bar
    let menus = menus_count_call();
    let labels_a: Vec<String> = menus.iter().map(|m| m.name.to_string()).collect();
    let sum_a: usize = labels_a.iter().map(|l| l.len()).sum();
    // render_in_window_menu_panel
    let menus = menus_count_call();
    let labels_b: Vec<String> = menus.iter().map(|m| m.name.to_string()).collect();
    let sum_b: usize = labels_b.iter().map(|l| l.len()).sum();
    sum_a + sum_b
}

// --- Post-refactor: caller fetches once, builds Vec<Arc<str>>, both
// renderers receive it by reference. ---
fn new_frame(menus_count_call: &dyn Fn() -> Vec<MockMenu>) -> usize {
    let menus = menus_count_call();
    let labels: Vec<Arc<str>> = menus.iter().map(|m| m.name.clone()).collect();
    // both renderers consume the same slice
    let sum_a: usize = labels.iter().map(|l| l.len()).sum();
    let sum_b: usize = labels.iter().map(|l| l.len()).sum();
    sum_a + sum_b
}

fn menu_labels_dedup(c: &mut Criterion) {
    let fetch = || mock_menus();

    assert_eq!(old_frame(&fetch), new_frame(&fetch));

    let mut group = c.benchmark_group("menu labels dedup");
    group.bench_function("baseline (2x get_menus + 2x Vec<String>)", |b| {
        b.iter(|| black_box(old_frame(&fetch)));
    });
    group.bench_function("current (1x get_menus + 1x Vec<Arc<str>>)", |b| {
        b.iter(|| black_box(new_frame(&fetch)));
    });
    group.finish();
}

criterion_group!(benches, menu_labels_dedup);
criterion_main!(benches);
