//! Bench: blink throttle
//!
//! Validates commit `cc7a1c4 perf(editor): suppress blink notify while
//! cursor is steady`. This commit does NOT speed up a function — it changes
//! WHETHER the blink task calls `cx.notify()`. Pre-commit, notify fires
//! 30×/sec while focused, scheduling a full render of every visible block
//! each tick. Post-commit, notify fires only after the cursor blink epoch
//! has elapsed ≥ 0.5 s; arrow keys / typing reset that epoch, so during
//! active editing no blink-driven notify fires at all.
//!
//! The microbench can only measure the *cost added* by the gate (a single
//! `Instant::elapsed()` comparison). The *savings* is the entire
//! `Block::render` × every visible block per suppressed tick — only
//! observable in a real frame-time benchmark with gpui standing up (see
//! `render_loop.rs` for a per-frame simulation that includes this gate).

use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};

fn blink_throttle(c: &mut Criterion) {
    let mut group = c.benchmark_group("blink throttle");
    let epoch = Instant::now();
    group.bench_function("baseline (always notify)", |b| {
        b.iter(|| {
            // Pre-commit: unconditional "yes, do the work" decision.
            let should_notify = true;
            black_box(should_notify);
        });
    });
    group.bench_function("current (elapsed gate)", |b| {
        b.iter(|| {
            // Post-commit: skip work while cursor opacity is pinned to 1.0.
            let should_notify = epoch.elapsed().as_secs_f32() >= 0.5;
            black_box(should_notify);
        });
    });
    group.finish();
}

criterion_group!(benches, blink_throttle);
criterion_main!(benches);
