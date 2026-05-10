//! Online update checks against the public project manifest.
//!
//! The app compares its compiled package version with the version published in
//! the main branch Cargo manifest. Gitee is used only as a timeout fallback for
//! GitHub so ordinary HTTP, parsing, or version errors stay visible.

use std::fmt;
use std::time::Duration;

use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use semver::Version;

pub(crate) const GITHUB_CARGO_TOML_URL: &str =
    "https://raw.githubusercontent.com/manyougz/velotype/refs/heads/main/Cargo.toml";
pub(crate) const GITEE_CARGO_TOML_URL: &str =
    "https://raw.giteeusercontent.com/manyougz/velotype/raw/main/Cargo.toml";
pub(crate) const RELEASES_URL: &str = "https://github.com/manyougz/velotype/releases";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const UPDATE_ACCEPT: &str = "text/plain,application/toml,*/*;q=0.8";
const UPDATE_USER_AGENT: &str = concat!(
    "Velotype/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/manyougz/velotype)"
);

/// Remote endpoint used to retrieve the published Velotype manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateSource {
    /// GitHub raw content endpoint.
    GitHub,
    /// Gitee mirror endpoint, used only when GitHub times out.
    Gitee,
}

impl UpdateSource {
    fn url(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_CARGO_TOML_URL,
            Self::Gitee => GITEE_CARGO_TOML_URL,
        }
    }
}

impl fmt::Display for UpdateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => f.write_str("GitHub"),
            Self::Gitee => f.write_str("Gitee"),
        }
    }
}

/// Coarse failure reason for a manifest fetch attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteFetchFailureKind {
    /// Request exceeded the configured timeout.
    Timeout,
    /// The server returned a non-success HTTP status.
    HttpStatus,
    /// Request setup or transport failed before a response was usable.
    Network,
    /// The response body could not be read as text.
    Body,
}

/// Error produced while fetching one remote manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteFetchFailure {
    pub(crate) source: UpdateSource,
    pub(crate) kind: RemoteFetchFailureKind,
    detail: String,
}

impl RemoteFetchFailure {
    fn new(source: UpdateSource, kind: RemoteFetchFailureKind, detail: impl Into<String>) -> Self {
        Self {
            source,
            kind,
            detail: detail.into(),
        }
    }

    fn timeout(source: UpdateSource, detail: impl Into<String>) -> Self {
        Self::new(source, RemoteFetchFailureKind::Timeout, detail)
    }

    fn is_timeout(&self) -> bool {
        self.kind == RemoteFetchFailureKind::Timeout
    }
}

impl fmt::Display for RemoteFetchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} update manifest fetch failed: {}",
            self.source, self.detail
        )
    }
}

impl std::error::Error for RemoteFetchFailure {}

/// Error returned by the full update-check pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckError {
    /// No usable remote manifest could be fetched.
    Fetch(RemoteFetchFailure),
    /// The manifest was fetched but could not produce a valid package version.
    ParseVersion(String),
}

impl fmt::Display for UpdateCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fetch(error) => write!(f, "{error}"),
            Self::ParseVersion(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for UpdateCheckError {}

/// Version comparison result used by the editor UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckResult {
    /// The remote version is newer than the running build.
    UpdateAvailable(UpdateVersionInfo),
    /// The running build is at least as new as the remote version.
    UpToDate(UpdateVersionInfo),
}

/// Version data shown in localized update prompts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateVersionInfo {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) source: UpdateSource,
}

pub(crate) fn check_latest_version(
    current_version: &str,
) -> Result<UpdateCheckResult, UpdateCheckError> {
    check_latest_version_with(current_version, |source| fetch_remote_cargo_toml(source))
}

fn check_latest_version_with<F>(
    current_version: &str,
    mut fetch: F,
) -> Result<UpdateCheckResult, UpdateCheckError>
where
    F: FnMut(UpdateSource) -> Result<String, RemoteFetchFailure>,
{
    match fetch(UpdateSource::GitHub) {
        Ok(manifest) => compare_manifest_version(current_version, &manifest, UpdateSource::GitHub),
        Err(error) if error.is_timeout() => {
            let manifest = fetch(UpdateSource::Gitee).map_err(UpdateCheckError::Fetch)?;
            compare_manifest_version(current_version, &manifest, UpdateSource::Gitee)
        }
        Err(error) => Err(UpdateCheckError::Fetch(error)),
    }
}

fn compare_manifest_version(
    current_version: &str,
    manifest: &str,
    source: UpdateSource,
) -> Result<UpdateCheckResult, UpdateCheckError> {
    let current = parse_semver(current_version, "current app version")?;
    let latest_text = extract_package_version(manifest)?;
    let latest = parse_semver(&latest_text, "remote Cargo.toml version")?;
    let info = UpdateVersionInfo {
        current_version: current_version.to_string(),
        latest_version: latest_text,
        source,
    };

    if latest > current {
        Ok(UpdateCheckResult::UpdateAvailable(info))
    } else {
        Ok(UpdateCheckResult::UpToDate(info))
    }
}

fn parse_semver(version: &str, label: &str) -> Result<Version, UpdateCheckError> {
    Version::parse(version).map_err(|err| {
        UpdateCheckError::ParseVersion(format!("{label} '{version}' is not valid SemVer: {err}"))
    })
}

pub(crate) fn extract_package_version(manifest: &str) -> Result<String, UpdateCheckError> {
    let parsed: toml::Value = toml::from_str(manifest).map_err(|err| {
        UpdateCheckError::ParseVersion(format!("failed to parse remote Cargo.toml: {err}"))
    })?;
    parsed
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            UpdateCheckError::ParseVersion(
                "remote Cargo.toml does not contain [package].version".to_string(),
            )
        })
}

fn fetch_remote_cargo_toml(source: UpdateSource) -> Result<String, RemoteFetchFailure> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .default_headers(update_request_headers())
        .build()
        .map_err(|err| {
            RemoteFetchFailure::new(
                source,
                RemoteFetchFailureKind::Network,
                format!("failed to build HTTP client: {err}"),
            )
        })?;

    let response = client.get(source.url()).send().map_err(|err| {
        if err.is_timeout() {
            RemoteFetchFailure::timeout(source, format!("request timed out after 5 seconds"))
        } else {
            RemoteFetchFailure::new(source, RemoteFetchFailureKind::Network, err.to_string())
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(RemoteFetchFailure::new(
            source,
            RemoteFetchFailureKind::HttpStatus,
            format!("server returned HTTP {status}"),
        ));
    }

    response.text().map_err(|err| {
        RemoteFetchFailure::new(source, RemoteFetchFailureKind::Body, err.to_string())
    })
}

fn update_request_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UPDATE_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static(UPDATE_ACCEPT));
    headers
}

#[cfg(test)]
mod tests {
    use super::{
        RemoteFetchFailure, RemoteFetchFailureKind, UpdateCheckError, UpdateCheckResult,
        UpdateSource, check_latest_version_with, extract_package_version,
    };
    use std::cell::RefCell;

    #[test]
    fn extracts_package_version_from_cargo_toml() {
        let manifest = r#"
            [package]
            name = "velotype"
            version = "0.2.2"
        "#;

        assert_eq!(extract_package_version(manifest).unwrap(), "0.2.2");
    }

    #[test]
    fn reports_update_when_remote_version_is_newer() {
        let result =
            check_latest_version_with(
                "0.2.1",
                |_| Ok("[package]\nversion = \"0.2.2\"".to_string()),
            )
            .unwrap();

        match result {
            UpdateCheckResult::UpdateAvailable(info) => {
                assert_eq!(info.current_version, "0.2.1");
                assert_eq!(info.latest_version, "0.2.2");
                assert_eq!(info.source, UpdateSource::GitHub);
            }
            _ => panic!("expected an available update"),
        }
    }

    #[test]
    fn reports_up_to_date_when_remote_version_is_same_or_older() {
        let same =
            check_latest_version_with(
                "0.2.1",
                |_| Ok("[package]\nversion = \"0.2.1\"".to_string()),
            )
            .unwrap();
        assert!(matches!(same, UpdateCheckResult::UpToDate(_)));

        let older =
            check_latest_version_with(
                "0.2.1",
                |_| Ok("[package]\nversion = \"0.2.0\"".to_string()),
            )
            .unwrap();
        assert!(matches!(older, UpdateCheckResult::UpToDate(_)));
    }

    #[test]
    fn falls_back_to_gitee_only_after_github_timeout() {
        let calls = RefCell::new(Vec::new());
        let result = check_latest_version_with("0.2.1", |source| {
            calls.borrow_mut().push(source);
            match source {
                UpdateSource::GitHub => Err(RemoteFetchFailure::timeout(
                    UpdateSource::GitHub,
                    "timed out",
                )),
                UpdateSource::Gitee => Ok("[package]\nversion = \"0.2.2\"".to_string()),
            }
        })
        .unwrap();

        assert_eq!(
            calls.into_inner(),
            vec![UpdateSource::GitHub, UpdateSource::Gitee]
        );
        match result {
            UpdateCheckResult::UpdateAvailable(info) => {
                assert_eq!(info.source, UpdateSource::Gitee);
            }
            _ => panic!("expected an available update from Gitee"),
        }
    }

    #[test]
    fn does_not_fall_back_after_non_timeout_github_error() {
        let calls = RefCell::new(Vec::new());
        let error = check_latest_version_with("0.2.1", |source| {
            calls.borrow_mut().push(source);
            Err(RemoteFetchFailure::new(
                source,
                RemoteFetchFailureKind::HttpStatus,
                "server returned HTTP 404",
            ))
        })
        .expect_err("non-timeout GitHub errors should stop the check");

        assert_eq!(calls.into_inner(), vec![UpdateSource::GitHub]);
        assert!(matches!(error, UpdateCheckError::Fetch(_)));
    }

    #[test]
    fn rejects_invalid_or_missing_versions() {
        assert!(extract_package_version("not toml").is_err());
        assert!(extract_package_version("[package]\nname = \"velotype\"").is_err());

        let error = check_latest_version_with("0.2.1", |_| {
            Ok("[package]\nversion = \"not-a-version\"".to_string())
        })
        .expect_err("invalid SemVer should fail");
        assert!(matches!(error, UpdateCheckError::ParseVersion(_)));
    }
}
