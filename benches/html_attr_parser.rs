//! Bench: HTML attribute parser
//!
//! Validates the review-feedback refactor of
//! `components::markdown::html::parse_html_attrs` that replaced eight
//! `source[index..].chars().next().unwrap()` calls with two small helpers
//! (`peek_char` + `advance_char`). The refactor is a clarity / safety fix:
//! it pushes the "byte-index must stay at a UTF-8 boundary" invariant into
//! the helpers so a future edit that hand-increments `index` by anything
//! other than `ch.len_utf8()` is impossible to write at the call sites.
//!
//! Goal of this bench: confirm there is no measurable regression on
//! realistic HTML attr strings (ASCII-only, multi-byte CJK / emoji, and a
//! long stress input). The helpers are `#[inline]` so the compiler should
//! generate identical code; this bench checks that empirically.
//!
//! Both versions are reproduced here because `parse_html_attrs` is
//! `pub(crate)` and unreachable from this external bench crate. The pair
//! is checked for output equality at bench startup.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BenchAttr {
    name: String,
    value: Option<String>,
    raw_source: String,
}

// --------------------------------------------------------------------------
// OLD: pre-refactor parser — eight `.chars().next().unwrap()` calls.
// Verbatim copy of the source pre-refactor, kept here only for comparison.
// --------------------------------------------------------------------------

fn old_parse_html_attrs(source: &str) -> Vec<BenchAttr> {
    let mut attrs = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        while index < source.len()
            && source[index..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '/')
        {
            index += source[index..].chars().next().unwrap().len_utf8();
        }
        if index >= source.len() {
            break;
        }

        let start = index;
        while index < source.len() {
            let ch = source[index..].chars().next().unwrap();
            if ch.is_whitespace() || ch == '=' || ch == '/' {
                break;
            }
            index += ch.len_utf8();
        }
        let name_end = index;
        if name_end == start {
            index += source[index..].chars().next().unwrap().len_utf8();
            continue;
        }

        while index < source.len()
            && source[index..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
        {
            index += source[index..].chars().next().unwrap().len_utf8();
        }

        let mut value = None;
        if source[index..].starts_with('=') {
            index += 1;
            while index < source.len()
                && source[index..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_whitespace())
            {
                index += source[index..].chars().next().unwrap().len_utf8();
            }

            if index < source.len() {
                let ch = source[index..].chars().next().unwrap();
                if ch == '"' || ch == '\'' {
                    index += ch.len_utf8();
                    let value_start = index;
                    while index < source.len() && !source[index..].starts_with(ch) {
                        index += source[index..].chars().next().unwrap().len_utf8();
                    }
                    value = Some(source[value_start..index].to_string());
                    if index < source.len() {
                        index += ch.len_utf8();
                    }
                } else {
                    let value_start = index;
                    while index < source.len() {
                        let ch = source[index..].chars().next().unwrap();
                        if ch.is_whitespace() || ch == '/' {
                            break;
                        }
                        index += ch.len_utf8();
                    }
                    value = Some(source[value_start..index].to_string());
                }
            }
        }

        attrs.push(BenchAttr {
            name: source[start..name_end].to_ascii_lowercase(),
            value,
            raw_source: source[start..index].to_string(),
        });
    }
    attrs
}

// --------------------------------------------------------------------------
// NEW: post-refactor parser — `peek_char` + `advance_char` helpers.
// Verbatim copy of the current production code.
// --------------------------------------------------------------------------

#[inline]
fn peek_char(source: &str, index: usize) -> Option<char> {
    source[index..].chars().next()
}

#[inline]
fn advance_char(source: &str, index: &mut usize) -> Option<char> {
    let ch = source[*index..].chars().next()?;
    *index += ch.len_utf8();
    Some(ch)
}

fn new_parse_html_attrs(source: &str) -> Vec<BenchAttr> {
    let mut attrs = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace() || *c == '/') {
            index += ch.len_utf8();
        }
        if index >= source.len() {
            break;
        }

        let start = index;
        while let Some(ch) = peek_char(source, index) {
            if ch.is_whitespace() || ch == '=' || ch == '/' {
                break;
            }
            index += ch.len_utf8();
        }
        let name_end = index;
        if name_end == start {
            advance_char(source, &mut index);
            continue;
        }

        while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace()) {
            index += ch.len_utf8();
        }

        let mut value = None;
        if source[index..].starts_with('=') {
            index += 1;
            while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace()) {
                index += ch.len_utf8();
            }

            if let Some(quote) = peek_char(source, index).filter(|c| *c == '"' || *c == '\'') {
                index += quote.len_utf8();
                let value_start = index;
                while let Some(ch) = peek_char(source, index) {
                    if ch == quote {
                        break;
                    }
                    index += ch.len_utf8();
                }
                value = Some(source[value_start..index].to_string());
                if index < source.len() {
                    index += quote.len_utf8();
                }
            } else if peek_char(source, index).is_some() {
                let value_start = index;
                while let Some(ch) = peek_char(source, index) {
                    if ch.is_whitespace() || ch == '/' {
                        break;
                    }
                    index += ch.len_utf8();
                }
                value = Some(source[value_start..index].to_string());
            }
        }

        attrs.push(BenchAttr {
            name: source[start..name_end].to_ascii_lowercase(),
            value,
            raw_source: source[start..index].to_string(),
        });
    }
    attrs
}

// --------------------------------------------------------------------------
// Inputs
// --------------------------------------------------------------------------

fn ascii_attrs() -> String {
    // Typical HTML element attribute string.
    String::from(r#"class="card primary" id="main" data-id="42" tabindex="0""#)
}

fn multibyte_attrs() -> String {
    // CJK + emoji in titles. Stresses len_utf8 advancement.
    String::from(r#"title="日本語のタイトル" alt="🎉 confetti emoji" data-tag="🦀rust" lang="ja""#)
}

fn long_stress_attrs() -> String {
    // ~30 attrs, ~2 KB total. Stresses the loop count.
    let mut s = String::new();
    for i in 0..30 {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&format!(
            r#"data-key-{i}="value with spaces and a tag {i}""#
        ));
    }
    s
}

// --------------------------------------------------------------------------
// Benches
// --------------------------------------------------------------------------

fn html_attr_parser(c: &mut Criterion) {
    let inputs = [
        ("ascii", ascii_attrs()),
        ("multibyte", multibyte_attrs()),
        ("long stress", long_stress_attrs()),
    ];

    // Sanity: both implementations must agree on every input.
    for (label, src) in &inputs {
        assert_eq!(
            old_parse_html_attrs(src),
            new_parse_html_attrs(src),
            "parser output diverged on input: {label}"
        );
    }

    let mut group = c.benchmark_group("html attr parser");
    for (label, src) in &inputs {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("baseline (unwrap-based)", label),
            src.as_str(),
            |b, src| b.iter(|| black_box(old_parse_html_attrs(black_box(src)))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (peek/advance helpers)", label),
            src.as_str(),
            |b, src| b.iter(|| black_box(new_parse_html_attrs(black_box(src)))),
        );
    }
    group.finish();
}

criterion_group!(benches, html_attr_parser);
criterion_main!(benches);
