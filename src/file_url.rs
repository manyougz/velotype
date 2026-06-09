//! File URL parsing for platform open-file events.

use std::path::PathBuf;

/// Parses a local file URL into a path.
///
/// GPUI's macOS `on_open_urls` callback provides `file://` URLs, while tests
/// and other callers may pass plain paths. Only local `file://` URLs are
/// accepted; non-file schemes and remote file authorities are rejected.
pub(crate) fn parse_file_url(value: &str) -> Option<PathBuf> {
    if let Some(rest) = value.strip_prefix("file://") {
        let path = if let Some(path) = rest.strip_prefix("localhost/") {
            format!("/{path}")
        } else if rest.starts_with('/') {
            rest.to_string()
        } else {
            return None;
        };

        return percent_decode(&path).map(PathBuf::from);
    }

    if value.contains("://") {
        return None;
    }

    Some(PathBuf::from(value))
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes.get(i + 1).copied().and_then(hex_value)?;
            let lo = bytes.get(i + 2).copied().and_then(hex_value)?;
            decoded.push((hi << 4) | lo);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_file_url;
    use std::path::PathBuf;

    #[test]
    fn parses_file_url_with_spaces() {
        assert_eq!(
            parse_file_url("file:///Users/example/My%20Notes/test%20file.md"),
            Some(PathBuf::from("/Users/example/My Notes/test file.md"))
        );
    }

    #[test]
    fn parses_file_url_with_unicode() {
        assert_eq!(
            parse_file_url("file:///Users/example/Notes/%E2%9C%93-%E6%96%87.md"),
            Some(PathBuf::from("/Users/example/Notes/✓-文.md"))
        );
    }

    #[test]
    fn parses_localhost_authority() {
        assert_eq!(
            parse_file_url("file://localhost/Users/example/test.md"),
            Some(PathBuf::from("/Users/example/test.md"))
        );
    }

    #[test]
    fn rejects_non_file_scheme() {
        assert_eq!(parse_file_url("https://example.com/test.md"), None);
    }

    #[test]
    fn passes_plain_path_through() {
        assert_eq!(
            parse_file_url("notes/100% literal.md"),
            Some(PathBuf::from("notes/100% literal.md"))
        );
    }

    #[test]
    fn rejects_remote_file_authority() {
        assert_eq!(parse_file_url("file://example.com/share/test.md"), None);
    }
}
