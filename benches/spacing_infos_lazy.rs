//! Bench: spacing_infos lazy lookup
//!
//! Validates the review-feedback refactor of `Editor::render` that replaced
//! the eager `let spacing_infos = visible_blocks.iter().map(...).collect()`
//! with an on-demand `spacing_for(index)` closure.
//!
//! Pre-refactor: per frame, every visible block's spacing metadata was read
//! into a `Vec<RenderedRowSpacingInfo>` before the layout loop began —
//! 7-field struct × N blocks (~120 B each) + a `Vec` allocation. For a
//! 200-block document that's ~24 KB allocated and freed every frame.
//!
//! Post-refactor: lookups happen on-demand inside the layout loop. Plain
//! documents read each block once (same N reads as before, no Vec). Inputs
//! with callouts / footnotes scan ahead and re-read the boundary blocks
//! (slightly more reads than eager); the trade is fewer reads vs a sizable
//! heap allocation. For the typical document the no-Vec wins.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct MockSpacingInfo {
    quote_group_anchor: Option<u128>,
    visible_quote_group_anchor: Option<u128>,
    callout_anchor: Option<u128>,
    callout_variant: Option<u8>,
    is_callout_header: bool,
    footnote_anchor: Option<u128>,
    is_footnote_header: bool,
}

struct MockBlock {
    quote_group_anchor: Option<u128>,
    visible_quote_group_anchor: Option<u128>,
    callout_anchor: Option<u128>,
    callout_variant: Option<u8>,
    is_callout: bool,
    footnote_anchor: Option<u128>,
    is_footnote_definition: bool,
}

impl MockSpacingInfo {
    #[inline]
    fn from_block(b: &MockBlock) -> Self {
        Self {
            quote_group_anchor: b.quote_group_anchor,
            visible_quote_group_anchor: b.visible_quote_group_anchor,
            callout_anchor: b.callout_anchor,
            callout_variant: b.callout_variant,
            is_callout_header: b.is_callout,
            footnote_anchor: b.footnote_anchor,
            is_footnote_header: b.is_footnote_definition,
        }
    }
}

fn mock_document(n: usize, with_callouts: bool) -> Vec<MockBlock> {
    (0..n)
        .map(|i| MockBlock {
            quote_group_anchor: (i % 7 == 0).then_some(i as u128),
            visible_quote_group_anchor: (i % 7 == 0).then_some(i as u128),
            callout_anchor: if with_callouts && i % 8 < 4 {
                Some((i / 8) as u128)
            } else {
                None
            },
            callout_variant: (with_callouts && i % 8 < 4).then_some(1),
            is_callout: with_callouts && i % 8 == 0,
            footnote_anchor: (with_callouts && i % 16 == 0).then_some((i / 16) as u128),
            is_footnote_definition: with_callouts && i % 16 == 0,
        })
        .collect()
}

// --- Eager: collect a Vec<MockSpacingInfo>, then index into it. ---
fn eager_frame(blocks: &[MockBlock]) -> u64 {
    let spacing_infos: Vec<MockSpacingInfo> =
        blocks.iter().map(MockSpacingInfo::from_block).collect();
    // Simulate the editor's layout-loop access pattern: walk blocks, look
    // up own + previous spacing for the gap calculation, and scan ahead in
    // callout / footnote groups.
    let mut sum = 0u64;
    let mut i = 0;
    while i < blocks.len() {
        let here = spacing_infos[i];
        sum += here.callout_variant.unwrap_or(0) as u64;
        if let Some(anchor) = here.callout_anchor {
            let mut j = i;
            while j < blocks.len() && spacing_infos[j].callout_anchor == Some(anchor) {
                sum += spacing_infos[j].is_callout_header as u64;
                j += 1;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    sum
}

// --- Lazy: closure reads each spacing on demand. ---
fn lazy_frame(blocks: &[MockBlock]) -> u64 {
    let spacing_for = |index: usize| MockSpacingInfo::from_block(&blocks[index]);
    let mut sum = 0u64;
    let mut i = 0;
    while i < blocks.len() {
        let here = spacing_for(i);
        sum += here.callout_variant.unwrap_or(0) as u64;
        if let Some(anchor) = here.callout_anchor {
            let mut j = i;
            while j < blocks.len() && spacing_for(j).callout_anchor == Some(anchor) {
                sum += spacing_for(j).is_callout_header as u64;
                j += 1;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    sum
}

fn spacing_infos_lazy(c: &mut Criterion) {
    let plain_200 = mock_document(200, false);
    let callout_200 = mock_document(200, true);
    let plain_50 = mock_document(50, false);

    for (label, blocks) in &[
        ("plain 50 blocks", &plain_50),
        ("plain 200 blocks", &plain_200),
        ("callout-heavy 200 blocks", &callout_200),
    ] {
        assert_eq!(
            eager_frame(blocks),
            lazy_frame(blocks),
            "frame sum diverged on input: {label}",
        );
    }

    let mut group = c.benchmark_group("spacing_infos lazy");
    for (label, blocks) in [
        ("plain 50 blocks", &plain_50),
        ("plain 200 blocks", &plain_200),
        ("callout-heavy 200 blocks", &callout_200),
    ] {
        group.bench_with_input(
            BenchmarkId::new("baseline (eager collect)", label),
            blocks.as_slice(),
            |b, blocks| b.iter(|| black_box(eager_frame(black_box(blocks)))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (lazy spacing_for)", label),
            blocks.as_slice(),
            |b, blocks| b.iter(|| black_box(lazy_frame(black_box(blocks)))),
        );
    }
    group.finish();
}

criterion_group!(benches, spacing_infos_lazy);
criterion_main!(benches);
