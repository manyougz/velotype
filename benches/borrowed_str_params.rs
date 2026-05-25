//! Bench: borrowed-str helper signatures
//!
//! Validates the review-feedback refactor that switched five helper-function
//! parameters from owned `String` to `&str`:
//!
//!   - components::markdown::link::normalize_link_destination
//!   - components::markdown::image::normalize_image_source
//!   - app_menu::show_window_prompt          (title arg)
//!   - config::preferences::labeled_row      (label arg)
//!   - config::preferences::shortcut_chip    (label arg)
//!
//! All five had call sites that immediately did `.to_string()` or `.clone()`
//! on a `&str` they already held — one allocation per call, eliminated by
//! the signature change.
//!
//! The bench models a typical call site: caller holds a `&str` reference to
//! some text. Baseline allocates a `String` and passes it to a fn that takes
//! `String`. Current passes the `&str` directly.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

// --- Reproduced helpers (verbatim shape; production functions are pub(crate)
// or private, so unreachable from this external crate). The shape matters
// for the bench (allocation profile), not the exact body. ---

fn old_normalize(destination: String) -> String {
    // Mimics the unescape + autolink-unwrap shape: one input scan, a
    // bounded amount of work, returns owned.
    let scanned = destination
        .chars()
        .filter(|&c| c != '\\')
        .collect::<String>();
    if scanned.starts_with('<') && scanned.ends_with('>') && scanned.len() >= 2 {
        scanned[1..scanned.len() - 1].to_string()
    } else {
        scanned
    }
}

fn new_normalize(destination: &str) -> String {
    let scanned: String = destination.chars().filter(|&c| c != '\\').collect();
    if scanned.starts_with('<') && scanned.ends_with('>') && scanned.len() >= 2 {
        scanned[1..scanned.len() - 1].to_string()
    } else {
        scanned
    }
}

// --- Inputs ---

fn typical_url() -> &'static str {
    "https://example.com/path/to/resource?key=value&other=1"
}

fn long_path() -> String {
    "https://example.com/very/long/path/with/many/segments/and/parameters?".repeat(8)
}

fn borrowed_str_params(c: &mut Criterion) {
    let short = typical_url();
    let long = long_path();

    // Sanity.
    assert_eq!(old_normalize(short.to_string()), new_normalize(short));
    assert_eq!(old_normalize(long.clone()), new_normalize(&long));

    let mut group = c.benchmark_group("borrowed str params");

    // The realistic call-site pattern is: caller holds a &str (e.g. from a
    // bracketed slice of source markdown). Baseline must clone to pass
    // owned; current passes the borrow directly.
    for (label, input) in [
        ("typical url ~55 B", short),
        ("long path ~570 B", long.as_str()),
    ] {
        group.bench_with_input(
            BenchmarkId::new("baseline (caller .to_string())", label),
            input,
            |b, s| {
                b.iter(|| {
                    let owned: String = s.to_string();
                    black_box(old_normalize(owned));
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current (caller &str)", label),
            input,
            |b, s| {
                b.iter(|| {
                    black_box(new_normalize(s));
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, borrowed_str_params);
criterion_main!(benches);
