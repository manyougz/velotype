//! Classification helpers for rendered-mode external paste.
//!
//! File import keeps CommonMark paragraph semantics, but paste should match
//! the user's visual expectation more closely. Plain physical lines become
//! separate blocks, while structural Markdown and block/risky HTML are left to
//! the document block builder.

use super::html::is_inline_tag;

pub(crate) fn should_split_plain_multiline_paste(lines: &[String]) -> bool {
    lines.iter().filter(|line| !line.trim().is_empty()).count() >= 2
        && lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .all(|line| is_plain_inline_paste_line(line))
}

fn is_plain_inline_paste_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return true;
    }

    if trimmed.starts_with('<') {
        return is_closed_inline_html_line(trimmed);
    }

    !(trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || trimmed.starts_with('>')
        || trimmed.starts_with('|')
        || trimmed.starts_with("$$")
        || trimmed.starts_with('\t')
        || line.starts_with("    ")
        || is_simple_list_marker(trimmed)
        || is_simple_atx_heading(trimmed)
        || is_simple_separator(trimmed)
        || is_simple_reference_definition(trimmed))
}

fn is_closed_inline_html_line(trimmed: &str) -> bool {
    let Some(name) = leading_html_tag_name(trimmed) else {
        return false;
    };

    // A closed safe inline tag at column 0 is still paragraph content. Block
    // HTML, risky children, and unclosed tags must keep the conservative path.
    let lower = trimmed.to_ascii_lowercase();
    is_inline_tag(&name)
        && !lower.contains("<script")
        && !lower.contains("<style")
        && lower.contains(&format!("</{name}>"))
}

fn leading_html_tag_name(trimmed: &str) -> Option<String> {
    let tagged = trimmed.strip_prefix('<')?;
    if tagged.starts_with('/') || tagged.starts_with('!') || tagged.starts_with('?') {
        return None;
    }

    let name_len = tagged
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .count();
    if name_len == 0 {
        return None;
    }

    let name = &tagged[..name_len];
    let suffix = &tagged[name_len..];
    let next = suffix.chars().next()?;
    matches!(next, '>' | ' ' | '\t' | '/').then(|| name.to_ascii_lowercase())
}

fn is_simple_list_marker(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1].is_ascii_whitespace()
    {
        return true;
    }

    let Some(marker_end) = trimmed.find(['.', ')']) else {
        return false;
    };
    marker_end > 0
        && marker_end <= 9
        && trimmed[..marker_end]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        && trimmed
            .as_bytes()
            .get(marker_end + 1)
            .is_some_and(|byte| byte.is_ascii_whitespace())
}

fn is_simple_atx_heading(trimmed: &str) -> bool {
    let marker_count = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&marker_count)
        && trimmed
            .as_bytes()
            .get(marker_count)
            .is_some_and(|byte| byte.is_ascii_whitespace())
}

fn is_simple_separator(trimmed: &str) -> bool {
    let without_spaces = trimmed
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    without_spaces.len() >= 3
        && without_spaces.iter().all(|byte| *byte == without_spaces[0])
        && matches!(without_spaces[0], b'-' | b'*' | b'_')
}

fn is_simple_reference_definition(trimmed: &str) -> bool {
    trimmed.starts_with('[') && trimmed.contains("]:")
}

#[cfg(test)]
mod tests {
    use super::should_split_plain_multiline_paste;

    #[test]
    fn accepts_plain_lines_with_script_syntax() {
        let lines = vec![
            "H~2~O".to_string(),
            "CO<sub>2</sub>".to_string(),
            "x<sup>n</sup>".to_string(),
        ];

        assert!(should_split_plain_multiline_paste(&lines));
    }

    #[test]
    fn accepts_closed_safe_inline_html_at_line_start() {
        let lines = vec![
            "<sub>2</sub>".to_string(),
            "<sup>n</sup>".to_string(),
            "<span style=\"color:red\">x</span>".to_string(),
            "<strong>y</strong>".to_string(),
        ];

        assert!(should_split_plain_multiline_paste(&lines));
    }

    #[test]
    fn rejects_block_or_unclosed_html_at_line_start() {
        let lines = vec!["<div>x</div>".to_string(), "<p>y</p>".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["<script>x</script>".to_string(), "<sup>n</sup>".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["<style>x</style>".to_string(), "<sup>n</sup>".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["<span>x".to_string(), "<sup>n</sup>".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));
    }

    #[test]
    fn rejects_structural_markdown() {
        let lines = vec!["```mermaid".to_string(), "flowchart LR".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["- item".to_string(), "- next".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["| A |".to_string(), "| --- |".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec![
            "```rust".to_string(),
            "fn main() {}".to_string(),
            "```".to_string(),
        ];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["> quote".to_string(), "> more".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));

        let lines = vec!["# Title".to_string(), "body".to_string()];
        assert!(!should_split_plain_multiline_paste(&lines));
    }
}
