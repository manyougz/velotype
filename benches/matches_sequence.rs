//! Bench: matches_sequence
//!
//! Validates the review-feedback refactor of
//! `components::markdown::inline::matches_sequence` that removed a
//! `sequence.chars().collect::<Vec<_>>()` allocation from a tight inline
//! parsing loop. The function is called up to 9× per token by
//! `escaped_sequence_token_len`, which itself runs per character during
//! markdown inline parsing — so even a single `Vec` allocation per call
//! compounds quickly on long inline strings with frequent escape candidates.
//!
//! ```text
//! // before
//! fn matches_sequence(tokens, index, sequence) -> bool {
//!     let chars = sequence.chars().collect::<Vec<_>>();   // <- alloc
//!     if index + chars.len() > tokens.len() { return false; }
//!     chars.iter().enumerate()
//!         .all(|(o, ch)| tokens[index + o].ch == *ch)
//! }
//!
//! // after
//! fn matches_sequence(tokens, index, sequence) -> bool {
//!     sequence.chars().enumerate().all(|(o, ch)| {
//!         tokens.get(index + o).is_some_and(|t| t.ch == ch)
//!     })
//! }
//! ```
//!
//! `CharToken` is private to `inline.rs`; this bench uses a minimal
//! `MockCharToken` (one `char` field — the only field both implementations
//! touch). The pair is checked for output equality at bench startup.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[derive(Clone, Copy)]
struct MockCharToken {
    ch: char,
}

// --------------------------------------------------------------------------
// OLD: pre-refactor — allocates a Vec<char> per call.
// --------------------------------------------------------------------------

fn old_matches_sequence(tokens: &[MockCharToken], index: usize, sequence: &str) -> bool {
    let chars = sequence.chars().collect::<Vec<_>>();
    if index + chars.len() > tokens.len() {
        return false;
    }
    chars
        .iter()
        .enumerate()
        .all(|(offset, ch)| tokens[index + offset].ch == *ch)
}

// --------------------------------------------------------------------------
// NEW: post-refactor — zero allocations; iterates directly.
// --------------------------------------------------------------------------

fn new_matches_sequence(tokens: &[MockCharToken], index: usize, sequence: &str) -> bool {
    sequence
        .chars()
        .enumerate()
        .all(|(offset, ch)| tokens.get(index + offset).is_some_and(|t| t.ch == ch))
}

// --------------------------------------------------------------------------
// Realistic caller: escaped_sequence_token_len fires up to 9 matches_sequence
// calls per token. This wrapper simulates one token's worth of work.
// --------------------------------------------------------------------------

const SEQUENCES: &[&str] = &[
    "</strong>",
    "<strong>",
    "</em>",
    "<em>",
    "</u>",
    "<u>",
    "\\",
    "*",
];

fn old_escaped_sequence_token_len(tokens: &[MockCharToken], index: usize) -> Option<usize> {
    if index + 1 >= tokens.len() {
        return None;
    }
    let next = index + 1;
    if old_matches_sequence(tokens, next, SEQUENCES[0]) {
        Some(9)
    } else if old_matches_sequence(tokens, next, SEQUENCES[1]) {
        Some(8)
    } else if old_matches_sequence(tokens, next, SEQUENCES[2]) {
        Some(5)
    } else if old_matches_sequence(tokens, next, SEQUENCES[3]) {
        Some(4)
    } else if old_matches_sequence(tokens, next, SEQUENCES[4]) {
        Some(4)
    } else if old_matches_sequence(tokens, next, SEQUENCES[5]) {
        Some(3)
    } else if old_matches_sequence(tokens, next, SEQUENCES[6])
        || old_matches_sequence(tokens, next, SEQUENCES[7])
    {
        Some(1)
    } else {
        None
    }
}

fn new_escaped_sequence_token_len(tokens: &[MockCharToken], index: usize) -> Option<usize> {
    if index + 1 >= tokens.len() {
        return None;
    }
    let next = index + 1;
    if new_matches_sequence(tokens, next, SEQUENCES[0]) {
        Some(9)
    } else if new_matches_sequence(tokens, next, SEQUENCES[1]) {
        Some(8)
    } else if new_matches_sequence(tokens, next, SEQUENCES[2]) {
        Some(5)
    } else if new_matches_sequence(tokens, next, SEQUENCES[3]) {
        Some(4)
    } else if new_matches_sequence(tokens, next, SEQUENCES[4]) {
        Some(4)
    } else if new_matches_sequence(tokens, next, SEQUENCES[5]) {
        Some(3)
    } else if new_matches_sequence(tokens, next, SEQUENCES[6])
        || new_matches_sequence(tokens, next, SEQUENCES[7])
    {
        Some(1)
    } else {
        None
    }
}

// --------------------------------------------------------------------------
// Inputs
// --------------------------------------------------------------------------

fn tokens_from(text: &str) -> Vec<MockCharToken> {
    text.chars().map(|ch| MockCharToken { ch }).collect()
}

fn plain_paragraph() -> Vec<MockCharToken> {
    // No escape candidates ⇒ each call walks all 9 sequences and fails fast
    // on the first character. Worst case for "allocation per call".
    tokens_from(&"the quick brown fox jumps over the lazy dog. ".repeat(20))
}

fn html_heavy() -> Vec<MockCharToken> {
    // Frequent <strong>/<em>/<u> ⇒ matches_sequence walks deeper before
    // accepting / rejecting.
    tokens_from(
        &"This <strong>bolded</strong> word and <em>italic</em> here, with \\<u>under</u>. "
            .repeat(15),
    )
}

// --------------------------------------------------------------------------
// Benches
// --------------------------------------------------------------------------

fn matches_sequence(c: &mut Criterion) {
    let inputs = [
        ("plain paragraph", plain_paragraph()),
        ("html heavy", html_heavy()),
    ];

    // Sanity: both implementations agree at every position.
    for (label, tokens) in &inputs {
        for i in 0..tokens.len().saturating_sub(10) {
            assert_eq!(
                old_escaped_sequence_token_len(tokens, i),
                new_escaped_sequence_token_len(tokens, i),
                "matches_sequence diverged at index {i} of input: {label}",
            );
        }
    }

    let mut group = c.benchmark_group("matches_sequence");

    // Direct (single-call) bench.
    let html_tokens = html_heavy();
    let sequence = "</strong>";
    group.bench_with_input(
        BenchmarkId::new("baseline (Vec<char> alloc)", "</strong>"),
        &(html_tokens.as_slice(), sequence),
        |b, &(tokens, seq)| b.iter(|| black_box(old_matches_sequence(black_box(tokens), 0, seq))),
    );
    group.bench_with_input(
        BenchmarkId::new("current (no alloc)", "</strong>"),
        &(html_tokens.as_slice(), sequence),
        |b, &(tokens, seq)| b.iter(|| black_box(new_matches_sequence(black_box(tokens), 0, seq))),
    );

    // Realistic caller — escaped_sequence_token_len across the whole input.
    for (label, tokens) in &inputs {
        group.bench_with_input(
            BenchmarkId::new("baseline / escaped_sequence_token_len whole input", label),
            tokens.as_slice(),
            |b, tokens| {
                b.iter(|| {
                    let mut accepted = 0usize;
                    for i in 0..tokens.len() {
                        if old_escaped_sequence_token_len(tokens, i).is_some() {
                            accepted += 1;
                        }
                    }
                    black_box(accepted);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current  / escaped_sequence_token_len whole input", label),
            tokens.as_slice(),
            |b, tokens| {
                b.iter(|| {
                    let mut accepted = 0usize;
                    for i in 0..tokens.len() {
                        if new_escaped_sequence_token_len(tokens, i).is_some() {
                            accepted += 1;
                        }
                    }
                    black_box(accepted);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, matches_sequence);
criterion_main!(benches);
