//! Bench: OsStr::to_string_lossy() double allocation
//!
//! Validates the review-feedback refactor at two `app_menu.rs` sites where
//! `path.to_string_lossy().to_string()` was used to produce an owned
//! `String`. `OsStr::to_string_lossy` returns `Cow<str>`; calling
//! `.to_string()` on the Cow allocates a fresh String even when the OS
//! string is already valid UTF-8 (the common case). Two replacements:
//!   - `window_title`: borrow the Cow directly into `format!` — its Display
//!     impl writes the borrowed bytes straight in.
//!   - recent-files label: `into_owned()` — reuses the Cow's Owned variant
//!     when the underlying OS string is valid UTF-8.

use std::ffi::OsStr;
use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};

// --- window_title shape: format!("Velotype - {}", lossy().to_string())
//                        vs format!("Velotype - {}", lossy()) ---

fn old_format_title(path: &Path) -> String {
    format!(
        "Velotype - {}",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    )
}

fn new_format_title(path: &Path) -> String {
    format!(
        "Velotype - {}",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| path.to_string_lossy())
    )
}

// --- recent-files shape: lossy().to_string() vs lossy().into_owned() ---

fn old_label(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn new_label(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn os_string_lossy(c: &mut Criterion) {
    let path = Path::new("/Users/me/Documents/notes/today.md");

    assert_eq!(old_format_title(path), new_format_title(path));
    assert_eq!(old_label(path), new_label(path));

    let mut group = c.benchmark_group("OsStr::to_string_lossy");
    group.bench_function("baseline / format_title (.to_string())", |b| {
        b.iter(|| black_box(old_format_title(black_box(path))));
    });
    group.bench_function("current  / format_title (borrowed Cow)", |b| {
        b.iter(|| black_box(new_format_title(black_box(path))));
    });
    group.bench_function("baseline / label (.to_string())", |b| {
        b.iter(|| black_box(old_label(black_box(path))));
    });
    group.bench_function("current  / label (.into_owned())", |b| {
        b.iter(|| black_box(new_label(black_box(path))));
    });
    group.finish();

    // OsStr stays alive on its own — silence "unused crate" if it grows.
    let _: &OsStr = path.as_os_str();
}

criterion_group!(benches, os_string_lossy);
criterion_main!(benches);
