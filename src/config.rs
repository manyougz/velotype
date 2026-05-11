//! Shared user-configuration helpers for imported language and theme packs.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use directories::ProjectDirs;
use serde_json::{Map, Value};

pub(crate) const RECENT_FILES_LIMIT: usize = 20;

/// Cross-platform configuration directories owned by Velotype.
#[derive(Debug, Clone)]
pub(crate) struct VelotypeConfigDirs {
    root: PathBuf,
}

impl VelotypeConfigDirs {
    /// Resolves the platform-specific app config directory.
    ///
    /// GPUI does not currently expose an app config path, so user-imported
    /// language and theme packs are stored under the OS location returned by
    /// `directories::ProjectDirs`.
    pub(crate) fn from_system() -> anyhow::Result<Self> {
        let dirs = ProjectDirs::from("com", "manyougz", "Velotype")
            .context("failed to resolve the Velotype config directory")?;
        Ok(Self {
            root: dirs.config_dir().to_path_buf(),
        })
    }

    /// Creates a directory set from a caller-provided root for tests.
    #[cfg(test)]
    pub(crate) fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn languages_dir(&self) -> PathBuf {
        self.root.join("languages")
    }

    pub(crate) fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    pub(crate) fn history_file(&self) -> PathBuf {
        self.root.join(".history")
    }
}

pub(crate) fn read_recent_files() -> anyhow::Result<Vec<PathBuf>> {
    read_recent_files_with_dirs(&VelotypeConfigDirs::from_system()?)
}

pub(crate) fn record_recent_file(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    record_recent_file_with_dirs(path, &VelotypeConfigDirs::from_system()?)
}

pub(crate) fn remove_recent_file(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    remove_recent_file_with_dirs(path, &VelotypeConfigDirs::from_system()?)
}

pub(crate) fn read_recent_files_with_dirs(
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<Vec<PathBuf>> {
    let path = dirs.history_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };

    Ok(normalize_recent_files(text.lines().map(PathBuf::from)))
}

pub(crate) fn record_recent_file_with_dirs(
    path: &Path,
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<Vec<PathBuf>> {
    if path.to_string_lossy().trim().is_empty() {
        bail!("recent file path cannot be empty");
    }

    let mut paths = read_recent_files_with_dirs(dirs)?;
    let path = path.to_path_buf();
    paths.retain(|existing| !same_recent_path(existing, &path));
    paths.insert(0, path);
    paths.truncate(RECENT_FILES_LIMIT);
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

pub(crate) fn remove_recent_file_with_dirs(
    path: &Path,
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = read_recent_files_with_dirs(dirs)?;
    paths.retain(|existing| !same_recent_path(existing, path));
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

fn write_recent_files_with_dirs(
    paths: &[PathBuf],
    dirs: &VelotypeConfigDirs,
) -> anyhow::Result<()> {
    let history_file = dirs.history_file();
    let normalized = normalize_recent_files(paths.iter().cloned());
    if normalized.is_empty() {
        match std::fs::remove_file(&history_file) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to remove '{}'", history_file.display()));
            }
        }
        return Ok(());
    }

    if let Some(parent) = history_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let mut content = String::new();
    for path in normalized {
        content.push_str(&path.to_string_lossy());
        content.push('\n');
    }
    std::fs::write(&history_file, content)
        .with_context(|| format!("failed to write '{}'", history_file.display()))
}

fn normalize_recent_files(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::new();
    for path in paths {
        let text = path.to_string_lossy();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if normalized
            .iter()
            .any(|existing| same_recent_path(existing, &path))
        {
            continue;
        }
        normalized.push(path);
        if normalized.len() == RECENT_FILES_LIMIT {
            break;
        }
    }
    normalized
}

fn same_recent_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub(crate) fn is_supported_config_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("jsonc")
        })
        .unwrap_or(false)
}

pub(crate) fn read_json_or_jsonc(path: &Path) -> anyhow::Result<Value> {
    if !is_supported_config_file(path) {
        bail!("configuration files must use the .json or .jsonc extension");
    }

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    let parsed = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("jsonc"))
        .unwrap_or(false)
    {
        parse_jsonc_value(&text)?
    } else {
        serde_json::from_str(&text)?
    };
    Ok(parsed)
}

pub(crate) fn parse_jsonc_value(text: &str) -> anyhow::Result<Value> {
    let stripped = strip_jsonc_comments(text)?;
    Ok(serde_json::from_str(&stripped)?)
}

pub(crate) fn strip_jsonc_comments(input: &str) -> anyhow::Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut closed = false;
                    let mut previous = '\0';
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                        }
                        if previous == '*' && next == '/' {
                            closed = true;
                            break;
                        }
                        previous = next;
                    }
                    if !closed {
                        bail!("unterminated block comment in JSONC file");
                    }
                    continue;
                }
                _ => {}
            }
        }

        output.push(ch);
    }

    Ok(output)
}

pub(crate) fn sanitize_config_file_stem(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_separator = false;
    for ch in value.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_separator && !output.is_empty() {
                output.push('_');
                last_was_separator = true;
            }
        } else if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
            last_was_separator = false;
        }
    }

    let output = output.trim_matches(['_', '.']).to_string();
    if output.is_empty() {
        "custom".into()
    } else {
        output
    }
}

pub(crate) fn prune_empty_json_values(value: &mut Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => {
            items.retain_mut(|item| !prune_empty_json_values(item));
            items.is_empty()
        }
        Value::Object(object) => {
            object.retain(|_, item| !prune_empty_json_values(item));
            object.is_empty()
        }
        Value::Bool(_) | Value::Number(_) => false,
    }
}

pub(crate) fn merge_non_empty_json_values(base: &mut Value, patch: &Value) {
    if is_empty_json_value(patch) {
        return;
    }

    match (base, patch) {
        (Value::Object(base_object), Value::Object(patch_object)) => {
            for (key, patch_value) in patch_object {
                if is_empty_json_value(patch_value) {
                    continue;
                }
                match base_object.get_mut(key) {
                    Some(base_value) => merge_non_empty_json_values(base_value, patch_value),
                    None => {
                        base_object.insert(key.clone(), patch_value.clone());
                    }
                }
            }
        }
        (base_value, patch_value) => {
            *base_value = patch_value.clone();
        }
    }
}

pub(crate) fn object_without_empty_values(mut object: Map<String, Value>) -> Map<String, Value> {
    object.retain(|_, value| !prune_empty_json_values(value));
    object
}

fn is_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.iter().all(is_empty_json_value),
        Value::Object(object) => object.values().all(is_empty_json_value),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RECENT_FILES_LIMIT, VelotypeConfigDirs, parse_jsonc_value, prune_empty_json_values,
        read_recent_files_with_dirs, record_recent_file_with_dirs, remove_recent_file_with_dirs,
        sanitize_config_file_stem, strip_jsonc_comments,
    };
    use serde_json::json;
    use std::path::{Path, PathBuf};

    #[test]
    fn jsonc_comments_are_stripped_without_touching_strings() {
        let text = r#"
        {
            // line comment
            "url": "https://example.com/a//b",
            "text": "/* not a comment */",
            /* block comment */
            "value": 1
        }
        "#;

        let parsed = parse_jsonc_value(text).expect("jsonc should parse");
        assert_eq!(parsed["url"], "https://example.com/a//b");
        assert_eq!(parsed["text"], "/* not a comment */");
        assert_eq!(parsed["value"], 1);
        assert!(strip_jsonc_comments(text).is_ok());
    }

    #[test]
    fn empty_values_are_pruned_recursively() {
        let mut value = json!({
            "name": "",
            "colors": {
                "text_default": null,
                "selection": "#fff"
            },
            "items": ["", null]
        });

        assert!(!prune_empty_json_values(&mut value));
        assert_eq!(value, json!({ "colors": { "selection": "#fff" } }));
    }

    #[test]
    fn config_file_stems_are_sanitized() {
        assert_eq!(
            sanitize_config_file_stem("My Theme / Blue"),
            "My_Theme_Blue"
        );
        assert_eq!(sanitize_config_file_stem("  ...  "), "custom");
    }

    #[test]
    fn missing_recent_history_file_returns_empty_list() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);

        assert!(read_recent_files_with_dirs(&dirs).unwrap().is_empty());
        assert!(!dirs.history_file().exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_recent_history_write_does_not_create_file() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);

        super::write_recent_files_with_dirs(&[], &dirs).unwrap();

        assert!(!dirs.history_file().exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn blank_recent_file_path_is_rejected() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);

        assert!(record_recent_file_with_dirs(Path::new("   "), &dirs).is_err());
        assert!(!dirs.history_file().exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recent_history_filters_empty_lines_and_deduplicates() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            dirs.history_file(),
            "  \nC:\\one.md\n\nC:\\two.md\nC:\\one.md\n",
        )
        .unwrap();

        let paths = read_recent_files_with_dirs(&dirs).unwrap();
        assert_eq!(
            paths,
            vec![PathBuf::from("C:\\one.md"), PathBuf::from("C:\\two.md")]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recording_recent_file_moves_it_to_front_and_truncates() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);

        for index in 0..(RECENT_FILES_LIMIT + 2) {
            record_recent_file_with_dirs(&PathBuf::from(format!("file-{index}.md")), &dirs)
                .unwrap();
        }
        record_recent_file_with_dirs(&PathBuf::from("file-3.md"), &dirs).unwrap();

        let paths = read_recent_files_with_dirs(&dirs).unwrap();
        assert_eq!(paths.len(), RECENT_FILES_LIMIT);
        assert_eq!(paths[0], PathBuf::from("file-3.md"));
        assert_eq!(
            paths
                .iter()
                .filter(|path| path.as_path() == Path::new("file-3.md"))
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn removing_recent_file_persists_history_without_it() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);
        record_recent_file_with_dirs(&PathBuf::from("one.md"), &dirs).unwrap();
        record_recent_file_with_dirs(&PathBuf::from("two.md"), &dirs).unwrap();

        let paths = remove_recent_file_with_dirs(&PathBuf::from("one.md"), &dirs).unwrap();

        assert_eq!(paths, vec![PathBuf::from("two.md")]);
        assert_eq!(
            read_recent_files_with_dirs(&dirs).unwrap(),
            vec![PathBuf::from("two.md")]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn removing_last_recent_file_deletes_history_file() {
        let root = std::env::temp_dir().join(format!("velotype-config-{}", uuid::Uuid::new_v4()));
        let dirs = VelotypeConfigDirs::from_root(&root);
        let path = PathBuf::from("only.md");
        record_recent_file_with_dirs(&path, &dirs).unwrap();
        assert!(dirs.history_file().exists());

        let paths = remove_recent_file_with_dirs(&path, &dirs).unwrap();

        assert!(paths.is_empty());
        assert!(!dirs.history_file().exists());
        assert!(read_recent_files_with_dirs(&dirs).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(root);
    }
}
