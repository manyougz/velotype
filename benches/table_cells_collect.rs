//! Bench: table_cells values collect
//!
//! Validates the review-feedback refactor of
//! `Editor::end_block_pointer_selection_sessions_inner` that swapped
//! `self.table_cells.values().cloned().collect::<Vec<_>>()` (clones the
//! whole `TableCellBinding` per entry — three fields: a table-block
//! `Entity<Block>`, a cell `Entity<Block>`, and a `TableCellPosition`)
//! for `.values().map(|b| b.cell.clone()).collect::<Vec<Entity<Block>>>()`.
//!
//! Both impls pay the cost of cloning the cell `Entity` (one Arc bump
//! per cell) so the `update` loop can iterate without holding `&self`.
//! The new version skips cloning the second `Entity` and the
//! `TableCellPosition` per binding — wasted work the loop body never
//! reads.
//!
//! `TableCellBinding` is private to `editor/mod.rs`; the bench reproduces
//! its shape via `MockBinding` (one tagged Arc + a second tagged Arc + a
//! 32-byte position struct) so the per-entry clone cost matches.

use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[derive(Clone)]
struct MockEntity(Arc<u64>);

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct MockTableCellPosition {
    row: u32,
    col: u32,
    is_header: bool,
    _padding: [u8; 24],
}

#[derive(Clone)]
#[allow(dead_code)]
struct MockBinding {
    table_block: MockEntity,
    cell: MockEntity,
    position: MockTableCellPosition,
}

fn mock_table_cells(n: usize) -> HashMap<u64, MockBinding> {
    (0..n as u64)
        .map(|i| {
            (
                i,
                MockBinding {
                    table_block: MockEntity(Arc::new(i / 9)),
                    cell: MockEntity(Arc::new(i)),
                    position: MockTableCellPosition {
                        row: (i / 9) as u32,
                        col: (i % 9) as u32,
                        is_header: i < 9,
                        _padding: [0; 24],
                    },
                },
            )
        })
        .collect()
}

// --- Baseline: clone the whole binding (Arc + Arc + position) per entry. ---
fn old_collect(cells: &HashMap<u64, MockBinding>) -> usize {
    let bindings: Vec<MockBinding> = cells.values().cloned().collect();
    let mut sum = 0usize;
    for b in bindings {
        sum = sum.wrapping_add(*b.cell.0 as usize);
    }
    sum
}

// --- Current: clone only the cell entity (one Arc) per entry. ---
fn new_collect(cells: &HashMap<u64, MockBinding>) -> usize {
    let cells: Vec<MockEntity> = cells.values().map(|b| b.cell.clone()).collect();
    let mut sum = 0usize;
    for c in cells {
        sum = sum.wrapping_add(*c.0 as usize);
    }
    sum
}

fn table_cells_collect(c: &mut Criterion) {
    let mut group = c.benchmark_group("table_cells values collect");
    for &n in &[9usize, 81, 200] {
        let cells = mock_table_cells(n);
        assert_eq!(old_collect(&cells), new_collect(&cells));
        group.bench_with_input(
            BenchmarkId::new("baseline (clone full binding)", n),
            &cells,
            |b, cells| b.iter(|| black_box(old_collect(black_box(cells)))),
        );
        group.bench_with_input(
            BenchmarkId::new("current (clone only cell entity)", n),
            &cells,
            |b, cells| b.iter(|| black_box(new_collect(black_box(cells)))),
        );
    }
    group.finish();
}

// Suppress unused-field warning while keeping the type's shape realistic.
fn _used() {
    let b = MockBinding {
        table_block: MockEntity(Arc::new(0)),
        cell: MockEntity(Arc::new(0)),
        position: MockTableCellPosition {
            row: 0,
            col: 0,
            is_header: false,
            _padding: [0; 24],
        },
    };
    let _ = b.table_block;
}

criterion_group!(benches, table_cells_collect);
criterion_main!(benches);
