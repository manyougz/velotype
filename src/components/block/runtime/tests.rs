use std::ops::Range;
use std::sync::Arc;

use super::projection::{
    expanded_display_cursor_offset_for_clean, expanded_display_offset_for_clean,
};
use crate::components::markdown::code_highlight::CodeLanguageKey;
use crate::components::markdown::inline::{
    InlineFragment, InlineInsertionAttributes, InlineLinkHit, InlineStyle, InlineTextTree,
};
use crate::components::markdown::link::parse_link_reference_definitions;
use crate::components::{Block, BlockKind, BlockRecord, Newline, TableCellPosition};
use gpui::{AppContext, EntityInputHandler, TestAppContext};

fn assert_only_code_range(block: &Block, expected: Range<usize>) {
    let code_ranges = block
        .inline_spans()
        .iter()
        .filter(|span| span.style.code)
        .map(|span| span.range.clone())
        .collect::<Vec<_>>();
    assert_eq!(code_ranges, vec![expected]);
}

#[test]
fn expanded_code_cursor_offset_stays_before_closing_backtick() {
    let fragments = vec![InlineFragment {
        text: "123".to_string(),
        style: InlineStyle {
            code: true,
            ..InlineStyle::default()
        },
        html_style: None,
        link: None,
        footnote: None,
        math: None,
    }];

    assert_eq!(expanded_display_offset_for_clean(&fragments, 0), 1);
    assert_eq!(expanded_display_offset_for_clean(&fragments, 3), 5);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 0), 1);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 3), 4);
}

#[test]
fn expanded_code_cursor_offset_keeps_plain_text_boundaries() {
    let fragments = vec![
        InlineFragment {
            text: "a".to_string(),
            style: InlineStyle::default(),
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        },
        InlineFragment {
            text: "bc".to_string(),
            style: InlineStyle {
                code: true,
                ..InlineStyle::default()
            },
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        },
    ];

    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 1), 1);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 3), 4);
}

#[test]
fn typing_inside_manual_backticks_keeps_cursor_inside_code_span() {
    let tree = InlineTextTree::plain("``");
    let result = tree.replace_visible_range(1..1, "1", InlineInsertionAttributes::default());

    assert_eq!(result.tree.visible_text(), "1");
    assert_eq!(
        result.tree.fragments,
        vec![InlineFragment {
            text: "1".to_string(),
            style: InlineStyle {
                code: true,
                ..InlineStyle::default()
            },
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        }]
    );

    let clean_cursor = result.map_offset(2);
    assert_eq!(clean_cursor, 1);
    assert_eq!(
        expanded_display_cursor_offset_for_clean(&result.tree.fragments, clean_cursor),
        2
    );
}

#[gpui::test]
async fn enter_inside_multiline_inline_code_inserts_hard_line_without_splitting(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`line 1\nline 2`"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "line 1\n".len();
        block.selected_range = offset..offset;
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        let text = "line 1\n\nline 2";
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.display_text(), text);
        assert_eq!(block.selected_range, "line 1\n\n".len().."line 1\n\n".len());
        assert!(
            block
                .inline_spans()
                .iter()
                .any(|span| { span.style.code && span.range == (0..text.len()) })
        );
    });
}

#[gpui::test]
async fn inline_math_focus_uses_markdown_source_then_reparses_on_blur(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**bold** $x^2$"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        assert_eq!(block.display_text(), "bold $x^2$");
        assert!(block.sync_inline_math_source_edit_for_focus(true));
        assert!(block.uses_raw_text_editing());
        assert_eq!(block.display_text(), "**bold** $x^2$");
        assert!(block.sync_inline_math_source_edit_for_focus(false));
        assert!(!block.uses_raw_text_editing());
        assert_eq!(block.display_text(), "bold $x^2$");
        assert_eq!(block.record.title.serialize_markdown(), "**bold** $x^2$");
    });
}

#[gpui::test]
async fn mermaid_block_uses_raw_text_editing(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let markdown = "```mermaid\nflowchart LR\nA --> B\n```";
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::mermaid(markdown)));

    block.update(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::MermaidBlock);
        assert!(block.uses_raw_text_editing());
        assert_eq!(block.display_text(), markdown);
        assert_eq!(block.record.markdown_line(0, None), markdown);
    });
}

#[gpui::test]
async fn enter_inside_projected_inline_code_inserts_hard_line_without_splitting(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`line 1\nline 2`"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "line 1\n".len();
        block.selected_range = offset..offset;
        block.sync_inline_projection_for_focus(true);
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        let text = "line 1\n\nline 2";
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.record.title.visible_text(), text);
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .any(|span| span.style.code && span.range == (0..text.len()))
        );
    });
}

#[gpui::test]
async fn enter_outside_inline_code_still_splits_paragraph(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = "alpha".len().."alpha".len();
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.display_text(), "alpha");
        assert_eq!(block.selected_range, "alpha".len().."alpha".len());
    });
}

#[gpui::test]
async fn enter_inside_comment_block_inserts_hard_line_without_splitting(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::comment("<!--\n**not bold** [not link](https://example.com)\n-->"),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "<!--\n".len();
        block.selected_range = offset..offset;
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::Comment);
        assert_eq!(
            block.display_text(),
            "<!--\n\n**not bold** [not link](https://example.com)\n-->"
        );
        assert_eq!(block.inline_spans().len(), 1);
        assert_eq!(block.inline_spans()[0].range, 0..block.display_text().len());
        assert_eq!(block.inline_spans()[0].style, InlineStyle::default());
    });
}

#[gpui::test]
async fn paragraph_shortcut_creates_task_item_directly(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(
            InlineTextTree::plain("- [x] task"),
            10,
            None,
            None,
            None,
            cx,
        );
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::TaskListItem { checked: true });
    assert_eq!(text, "task");
}

#[gpui::test]
async fn paragraph_shortcut_creates_parenthesized_numbered_list_directly(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(InlineTextTree::plain("1) item"), 7, None, None, None, cx);
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::NumberedListItem);
    assert_eq!(text, "item");
}

#[gpui::test]
async fn bullet_shortcut_upgrades_to_task_item_after_box_prefix(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(InlineTextTree::plain("- "), 2, None, None, None, cx);
    });
    let kind = block.read_with(cx, |block, _cx| block.kind());
    assert_eq!(kind, BlockKind::BulletedListItem);

    block.update(cx, |block, cx| {
        block.apply_title_edit(InlineTextTree::plain("[ ] "), 4, None, None, None, cx);
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::TaskListItem { checked: false });
    assert_eq!(text, "");
}

#[gpui::test]
async fn inline_code_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `code` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a code b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a `code` b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 9..9;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a code b"
    );
}

#[gpui::test]
async fn inline_code_projection_expands_only_the_selected_code_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`one` and `two`"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "`one` and two"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 10..10;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "one and `two`"
    );
}

#[gpui::test]
async fn strikethrough_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~gone~~ b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a gone b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a ~~gone~~ b"
    );
}

#[gpui::test]
async fn inline_code_projection_right_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `123` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 5..5;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 6);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_right_target(block.cursor_offset())
            .expect("inner end should jump to outer end");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a `123` b"
    );
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 7);

    block.update(cx, |block, _cx| {
        let target = block.next_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 8);
}

#[gpui::test]
async fn inline_code_projection_left_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `123` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 3);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_left_target(block.cursor_offset())
            .expect("inner start should jump to outer start");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 2);

    block.update(cx, |block, _cx| {
        let target = block.previous_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 1);
}

#[gpui::test]
async fn strikethrough_projection_right_escape_stays_outside_after_rebuild(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~123~~ b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 5..5;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 7);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_right_target(block.cursor_offset())
            .expect("inner end should jump to outer end");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 9);

    block.update(cx, |block, _cx| {
        let target = block.next_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 10);
}

#[gpui::test]
async fn strikethrough_projection_left_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~bc~~ d"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 4);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_left_target(block.cursor_offset())
            .expect("expected projected move left target");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 2);

    block.update(cx, |block, _cx| {
        let target = block.previous_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 1);
}

#[gpui::test]
async fn inline_link_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a link b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a [link](https://example.com) b"
    );
}

#[gpui::test]
async fn reference_style_link_resolves_and_expands_preserving_raw_syntax(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-link]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "reference link"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_at(0).map(str::to_string)),
        Some("https://example.com".to_string())
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "[reference link][ref-link]"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        "[reference link][ref-link]"
    );
}

#[gpui::test]
async fn reference_style_link_hit_exposes_raw_prompt_and_resolved_open_target(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-links]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-links]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_hit_at(0).cloned()),
        Some(InlineLinkHit {
            prompt_target: "ref-links".to_string(),
            open_target: "https://example.com".to_string(),
        })
    );
}

#[gpui::test]
async fn inline_link_with_title_expands_title_but_opens_destination(cx: &mut TestAppContext) {
    let markdown = "[ABC](https://abc.com \"https://abc.com\")";
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown(markdown),
            ),
        )
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_hit_at(0).cloned()),
        Some(InlineLinkHit {
            prompt_target: "https://abc.com".to_string(),
            open_target: "https://abc.com".to_string(),
        })
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        markdown
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        markdown
    );
}

#[gpui::test]
async fn autolink_expands_with_angle_brackets_when_touched(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("<https://example.com>"),
            ),
        )
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "https://example.com"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "<https://example.com>"
    );
}

#[gpui::test]
async fn projected_reference_target_stays_link_hit_testable(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-link]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    let target_offset = block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        block
            .display_text()
            .find("ref-link")
            .expect("projection should expose reference target")
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block
            .inline_link_hit_at(target_offset)
            .cloned()),
        Some(InlineLinkHit {
            prompt_target: "ref-link".to_string(),
            open_target: "https://example.com".to_string(),
        })
    );
}

#[gpui::test]
async fn projected_reference_syntax_maps_full_delimiter_range_back_to_markdown(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-link]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    let display_len = block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        block.display_text().len()
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| {
            block.current_range_to_markdown_range(0..display_len)
        }),
        0.."[reference link][ref-link]".len()
    );
}

#[gpui::test]
async fn editing_link_destination_inside_projection_preserves_link(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        let expanded = block.display_text().to_string();
        let insert_at = expanded
            .find("example.com")
            .expect("expanded link should expose its destination");
        block.replace_text_in_visible_range(insert_at..insert_at, "docs.", None, false, cx);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        "a [link](https://docs.example.com) b"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a [link](https://docs.example.com) b"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_at(3).map(str::to_string)),
        Some("https://docs.example.com".to_string())
    );
}

#[gpui::test]
async fn deleting_adjacent_text_preserves_reference_style_link_syntax(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[ref][ref-link]a"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    block.update(cx, |block, cx| {
        block.replace_text_in_visible_range(3..4, "", None, false, cx);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        "[ref][ref-link]"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "ref"
    );
}

#[gpui::test]
async fn deleting_adjacent_text_preserves_autolink_syntax(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("<ref2>a"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.replace_text_in_visible_range(4..5, "", None, false, cx);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        "<ref2>"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "ref2"
    );
}

#[gpui::test]
async fn link_projection_preserves_cursor_inside_destination_after_rebuild(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        let expanded = block.display_text().to_string();
        let destination_offset = expanded
            .find("example.com")
            .expect("expanded link should expose destination text");
        block.move_to_with_preferred_x(destination_offset, None, cx);
        block.sync_inline_projection_for_focus(true);
    });

    let destination_offset = block.read_with(cx, |block, _cx| {
        block
            .display_text()
            .find("example.com")
            .expect("expanded link should expose destination text")
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.cursor_offset()),
        destination_offset
    );
}

#[gpui::test]
async fn link_projection_preserves_selection_inside_destination_after_rebuild(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    let selected_range = block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        let expanded = block.display_text().to_string();
        let destination_offset = expanded
            .find("example.com")
            .expect("expanded link should expose destination text");
        let selected_range = destination_offset..destination_offset + "example".len();
        block.selected_range = selected_range.clone();
        block.selection_reversed = false;
        block.sync_inline_projection_for_focus(true);
        selected_range
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        selected_range
    );
}

#[gpui::test]
async fn link_middle_delimiter_click_snaps_to_destination_start(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    let destination_offset = block.update(cx, |block, cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        let expanded = block.display_text().to_string();
        let middle = expanded
            .find("](")
            .expect("expanded link should expose middle delimiter");
        let destination_offset = expanded
            .find("https://")
            .expect("expanded link should expose destination start");
        let click_target = block.pointer_target_offset(middle + 1);
        block.move_to_with_preferred_x(click_target, None, cx);
        block.sync_inline_projection_for_focus(true);
        destination_offset
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.cursor_offset()),
        destination_offset
    );
}

#[gpui::test]
async fn reversed_selection_survives_projection_focus_refresh(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..7;
        block.selection_reversed = true;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        1..7
    );
    assert!(block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn reversed_selection_survives_render_cache_refresh(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..7;
        block.selection_reversed = true;
        block.sync_render_cache();
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        1..7
    );
    assert!(block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn reversed_selection_survives_clear_inline_projection(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`code`"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        block.selected_range = 1..5;
        block.selection_reversed = true;
        block.clear_inline_projection();
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "code"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        0..4
    );
    assert!(block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn reversed_selection_inside_link_destination_survives_focus_refresh(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    let expected = block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
        let expanded = block.display_text().to_string();
        let destination_offset = expanded
            .find("example.com")
            .expect("expanded link should expose destination text");
        let expected = destination_offset..destination_offset + "example".len();
        block.selected_range = expected.clone();
        block.selection_reversed = true;
        block.sync_inline_projection_for_focus(true);
        expected
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        expected
    );
    assert!(block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn ime_selected_text_range_reports_reversed_for_right_to_left_selection(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..7;
        block.selection_reversed = true;
    });

    let selection = cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::selected_text_range(block, false, window, block_cx)
                .expect("selection")
        })
    });

    assert_eq!(selection.range, 1..7);
    assert!(selection.reversed);
}

#[gpui::test]
async fn ime_replace_text_replaces_right_to_left_selection_in_source_raw_mode(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        );
        block.set_source_raw_mode();
        block
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..7;
        block.selection_reversed = true;
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "Z", window, block_cx,
            );
        });
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "aZeta"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        2..2
    );
    assert!(!block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn ime_replace_and_mark_text_replaces_right_to_left_selection_in_table_cell(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("alpha")),
        );
        block.set_table_cell_mode(
            TableCellPosition { row: 0, column: 0 },
            crate::components::TableColumnAlignment::Left,
        );
        block
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..4;
        block.selection_reversed = true;
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_and_mark_text_in_range(
                block,
                None,
                "XY",
                Some(0..1),
                window,
                block_cx,
            );
        });
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "aXYa"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        1..2
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.marked_range.clone()),
        Some(1..3)
    );
    assert!(!block.read_with(cx, |block, _cx| block.selection_reversed));
}

#[gpui::test]
async fn ime_commit_inside_inline_code_preserves_code_style(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("aaa`hello world`aaa"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        let cursor = "aaahello".len();
        block.selected_range = cursor..cursor;
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_and_mark_text_in_range(
                block,
                None,
                "ni",
                Some(2..2),
                window,
                block_cx,
            );
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "你", window, block_cx,
            );
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "aaahello你 worldaaa");
        assert_eq!(
            block.record.title.serialize_markdown(),
            "aaa`hello你 world`aaa"
        );
        assert_only_code_range(block, "aaa".len().."aaahello你 world".len());
    });
}

#[gpui::test]
async fn ime_commit_inside_projected_inline_code_preserves_code_style(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("aaa`hello world`aaa"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        let cursor = "aaahello".len();
        block.selected_range = cursor..cursor;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "aaa`hello world`aaa");
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_and_mark_text_in_range(
                block,
                None,
                "ni",
                Some(2..2),
                window,
                block_cx,
            );
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "你", window, block_cx,
            );
        });
    });

    block.update(cx, |block, _cx| {
        assert_eq!(
            block.record.title.serialize_markdown(),
            "aaa`hello你 world`aaa"
        );
        block.clear_inline_projection();
        assert_eq!(block.display_text(), "aaahello你 worldaaa");
        assert_only_code_range(block, "aaa".len().."aaahello你 world".len());
    });
}

#[gpui::test]
async fn replacing_selection_inside_inline_code_preserves_code_style(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("aaa`hello world`aaa"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        let start = "aaahello ".len();
        let end = "aaahello world".len();
        block.selected_range = start..end;
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "你", window, block_cx,
            );
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "aaahello 你aaa");
        assert_eq!(block.record.title.serialize_markdown(), "aaa`hello 你`aaa");
        assert_only_code_range(block, "aaa".len().."aaahello 你".len());
    });
}

#[gpui::test]
async fn replacing_selection_across_inline_code_boundary_stays_plain(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("aaa`hello`bbb"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = "aaahel".len().."aaahellobb".len();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "你", window, block_cx,
            );
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "aaahel你b");
        assert_eq!(block.record.title.serialize_markdown(), "aaa`hel`你b");
        assert_only_code_range(block, "aaa".len().."aaahel".len());
    });
}

#[test]
fn ime_utf16_ranges_keep_multilingual_boundaries() {
    let text = "中文😀かな";
    let emoji_utf8 = "中文".len().."中文😀".len();
    assert_eq!(Block::utf16_range_to_utf8_in(text, &(2..4)), emoji_utf8);
    assert_eq!(Block::utf8_range_to_utf16_in(text, &emoji_utf8), 2..4);
}

#[gpui::test]
async fn ime_replace_text_handles_cjk_and_emoji_utf16_ranges(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::plain("中文😀かな".to_string()),
            ),
        );
        block.set_source_raw_mode();
        block
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::replace_text_in_range(
                block,
                Some(2..4),
                "語",
                window,
                block_cx,
            );
        });
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "中文語かな"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.selected_range.clone()),
        "中文語".len().."中文語".len()
    );
}

#[gpui::test]
async fn ime_selection_ignores_editor_external_selection(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..1;
        block.editor_selection_range = Some(0..block.visible_len());
    });

    let selection = cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <Block as EntityInputHandler>::selected_text_range(block, false, window, block_cx)
                .expect("selection")
        })
    });

    assert_eq!(selection.range, 1..1);
    assert!(!selection.reversed);
}

#[gpui::test]
async fn focusing_rendered_image_does_not_auto_expand(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("![diagram](./assets/diagram.png)"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.sync_render_cache();
        assert!(block.showing_rendered_image());
        assert!(!block.image_edit_expanded);

        assert!(!block.sync_image_focus_state(true));
        assert!(block.showing_rendered_image());
        assert!(!block.image_edit_expanded);
    });
}

#[gpui::test]
async fn requested_rendered_image_expansion_enters_raw_markdown_editing(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("![diagram](./assets/diagram.png)"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.sync_render_cache();
        block.request_image_edit_expansion();
        assert!(block.sync_image_focus_state(true));
        assert!(block.image_edit_expanded);
        assert!(!block.showing_rendered_image());
        assert_eq!(block.cursor_offset(), block.visible_len());
    });
}

#[gpui::test]
async fn blurred_valid_rendered_image_recovers_image_presentation(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("![diagram](./assets/diagram.png)"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.sync_render_cache();
        block.request_image_edit_expansion();
        assert!(block.sync_image_focus_state(true));
        assert!(block.image_edit_expanded);

        assert!(block.sync_image_focus_state(false));
        assert!(!block.image_edit_expanded);
        assert!(block.showing_rendered_image());
    });
}

#[gpui::test]
async fn broken_rendered_image_syntax_blurs_back_to_plain_text(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("![diagram](./assets/diagram.png)"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.sync_render_cache();
        block.request_image_edit_expansion();
        assert!(block.sync_image_focus_state(true));

        block
            .record
            .set_title(InlineTextTree::from_markdown("not an image anymore"));
        block.sync_render_cache();
        assert!(!block.sync_image_focus_state(false));
        assert!(block.image_runtime().is_none());
        assert!(!block.image_edit_expanded);
        assert!(!block.showing_rendered_image());
        assert_eq!(block.display_text(), "not an image anymore");
    });
}

#[gpui::test]
async fn code_block_cache_builds_rust_highlight_spans(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("fn main() {\n    let value: i32 = 42;\n}\n"),
            ),
        )
    });

    let highlight = block
        .read_with(cx, |block, _cx| block.code_highlight_result().cloned())
        .expect("code block should cache a highlight result");
    assert_eq!(highlight.language, CodeLanguageKey::Rust);
    assert!(!highlight.spans.is_empty());
}

#[gpui::test]
async fn code_block_cache_updates_when_language_changes(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("fn main() {\n    let value = 42;\n}\n"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.record.kind = BlockKind::CodeBlock {
            language: Some("text".into()),
        };
        block.sync_render_cache();
    });

    let highlight = block
        .read_with(cx, |block, _cx| block.code_highlight_result().cloned())
        .expect("known plain fallback should still cache a result");
    assert_eq!(highlight.language, CodeLanguageKey::PlainText);
    assert!(highlight.spans.is_empty());
}

#[gpui::test]
async fn code_block_language_setter_updates_highlight_without_changing_content(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("print('hello')"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let range = 0..block.code_language_text().len();
        block.replace_code_language_text_in_range(range, "python", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.code_language_text(), "python");
        assert_eq!(block.display_text(), "print('hello')");
        assert_eq!(
            block
                .code_highlight_result()
                .expect("python should highlight")
                .language,
            CodeLanguageKey::Python
        );
    });
}

#[gpui::test]
async fn code_block_language_accepts_unknown_language_as_plain_rendering(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("fn main() {}"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let range = 0..block.code_language_text().len();
        block.replace_code_language_text_in_range(range, "unknown-lang", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.code_language_text(), "unknown-lang");
        assert!(block.code_highlight_result().is_none());
    });
}

#[gpui::test]
async fn code_language_input_uses_ime_path_without_touching_code_content(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("fn main() {}"),
            ),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.code_language_focus_handle.focus(window);
            block.code_language_selected_range = 0..block.code_language_text().len();
            block.selected_range = 3..3;
            <Block as EntityInputHandler>::replace_text_in_range(
                block, None, "python", window, block_cx,
            );
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.code_language_text(), "python");
        assert_eq!(block.display_text(), "fn main() {}");
        assert_eq!(block.selected_range, 3..3);
        assert_eq!(block.code_language_selected_range, 6..6);
    });
}

#[gpui::test]
async fn code_language_input_handles_utf16_ranges(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("zh😀kana".into()),
                },
                InlineTextTree::plain("body"),
            ),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.code_language_focus_handle.focus(window);
            <Block as EntityInputHandler>::replace_text_in_range(
                block,
                Some(2..4),
                "py",
                window,
                block_cx,
            );
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.code_language_text(), "zhpykana");
        assert_eq!(block.display_text(), "body");
    });
}

#[gpui::test]
async fn code_language_input_clears_language_when_empty(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("rust".into()),
                },
                InlineTextTree::plain("body"),
            ),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.code_language_focus_handle.focus(window);
            block.code_language_selected_range = 0..block.code_language_text().len();
            <Block as EntityInputHandler>::replace_text_in_range(block, None, "", window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.code_language_text(), "");
        assert!(matches!(
            block.kind(),
            BlockKind::CodeBlock { language: None }
        ));
        assert!(block.code_highlight_result().is_none());
    });
}

#[gpui::test]
async fn code_block_without_language_keeps_plain_rendering(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock { language: None },
                InlineTextTree::plain("no highlighting here"),
            ),
        )
    });

    assert!(block.read_with(cx, |block, _cx| block.code_highlight_result().is_none()));
}
