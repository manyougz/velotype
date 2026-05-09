//! HTTP client integration used by remote image loading.

use std::io;
use std::str::FromStr;
use std::sync::Arc;

use futures::AsyncReadExt;
use futures::FutureExt;
use futures::channel::oneshot;
use gpui::App;
use gpui::http_client::{self, AsyncBody, HttpClient};
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, HeaderMap, HeaderValue, PRAGMA, USER_AGENT,
};

const DEFAULT_IMAGE_ACCEPT: &str =
    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const DEFAULT_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7";
const DEFAULT_CACHE_CONTROL: &str = "no-cache";

pub(crate) fn install_http_client(cx: &mut App) {
    match ReqwestTransportHttpClient::new(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36",
    ) {
        Ok(client) => cx.set_http_client(Arc::new(client)),
        Err(error) => eprintln!("failed to install HTTP client for image loading: {error}"),
    }
}

pub(crate) fn is_remote_image_source(source: &str) -> bool {
    http_client::Uri::from_str(source)
        .ok()
        .and_then(|uri| uri.scheme_str().map(str::to_owned))
        .is_some_and(|scheme| scheme == "http" || scheme == "https")
}

/// GPUI `HttpClient` bridge backed by reqwest's blocking transport.
///
/// GPUI expects an async client interface, while image loading in this app only
/// needs simple HTTP(S) fetches. Requests are executed on a short-lived thread
/// and returned as `AsyncBody` values to match GPUI's contract.
struct ReqwestTransportHttpClient {
    client: reqwest::blocking::Client,
    user_agent: HeaderValue,
    default_headers: HeaderMap,
}

impl ReqwestTransportHttpClient {
    fn new(user_agent: &str) -> anyhow::Result<Self> {
        let default_headers = default_image_request_headers(user_agent)?;
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(user_agent)
            .default_headers(default_headers.clone())
            .build()?;
        Ok(Self {
            client,
            user_agent: HeaderValue::from_str(user_agent)?,
            default_headers,
        })
    }

    fn execute_request(
        client: reqwest::blocking::Client,
        default_headers: HeaderMap,
        request: http_client::Request<AsyncBody>,
    ) -> anyhow::Result<http_client::Response<AsyncBody>> {
        let (parts, mut body) = request.into_parts();
        let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())?;
        let url = parts.uri.to_string();
        let body_bytes = futures::executor::block_on(async move {
            let mut bytes = Vec::new();
            body.read_to_end(&mut bytes).await?;
            Ok::<Vec<u8>, io::Error>(bytes)
        })?;

        let mut builder = apply_missing_default_headers(
            client.request(method, url),
            &parts.headers,
            &default_headers,
        );
        if !body_bytes.is_empty() {
            builder = builder.body(body_bytes);
        }

        let response = builder.send()?;
        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let bytes = response.bytes()?;

        let mut response_builder = http_client::Response::builder()
            .status(status)
            .version(version);
        for (name, value) in &headers {
            response_builder = response_builder.header(name, value);
        }
        Ok(response_builder.body(AsyncBody::from(bytes.to_vec()))?)
    }
}

fn default_image_request_headers(user_agent: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static(DEFAULT_IMAGE_ACCEPT));
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static(DEFAULT_ACCEPT_LANGUAGE),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(DEFAULT_CACHE_CONTROL),
    );
    headers.insert(PRAGMA, HeaderValue::from_static(DEFAULT_CACHE_CONTROL));
    Ok(headers)
}

fn apply_missing_default_headers(
    builder: reqwest::blocking::RequestBuilder,
    request_headers: &HeaderMap,
    default_headers: &HeaderMap,
) -> reqwest::blocking::RequestBuilder {
    let mut headers = request_headers.clone();
    for (name, value) in default_headers {
        if !headers.contains_key(name) {
            headers.insert(name.clone(), value.clone());
        }
    }
    builder.headers(headers)
}

impl HttpClient for ReqwestTransportHttpClient {
    fn type_name(&self) -> &'static str {
        "velotype_reqwest_transport_http_client"
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn send(
        &self,
        request: http_client::Request<AsyncBody>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<http_client::Response<AsyncBody>>> {
        let client = self.client.clone();
        let default_headers = self.default_headers.clone();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::execute_request(client, default_headers, request));
        });
        async move {
            rx.await
                .map_err(|_| anyhow::anyhow!("image HTTP worker dropped before responding"))?
        }
        .boxed()
    }

    fn proxy(&self) -> Option<&http_client::Url> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_ACCEPT_LANGUAGE, DEFAULT_CACHE_CONTROL, DEFAULT_IMAGE_ACCEPT,
        apply_missing_default_headers, default_image_request_headers, is_remote_image_source,
    };
    use reqwest::header::{
        ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, CONTENT_LENGTH, HOST,
        HeaderMap, HeaderValue, PRAGMA, USER_AGENT,
    };

    const TEST_USER_AGENT: &str = "VelotypeTest/1.0";

    #[test]
    fn default_image_headers_include_browser_like_fetch_context() {
        let headers = default_image_request_headers(TEST_USER_AGENT).expect("headers");

        assert_eq!(headers.get(USER_AGENT).unwrap(), TEST_USER_AGENT);
        assert_eq!(headers.get(ACCEPT).unwrap(), DEFAULT_IMAGE_ACCEPT);
        assert_eq!(
            headers.get(ACCEPT_LANGUAGE).unwrap(),
            DEFAULT_ACCEPT_LANGUAGE
        );
        assert_eq!(headers.get(CACHE_CONTROL).unwrap(), DEFAULT_CACHE_CONTROL);
        assert_eq!(headers.get(PRAGMA).unwrap(), DEFAULT_CACHE_CONTROL);
    }

    #[test]
    fn default_image_headers_leave_transport_managed_headers_unset() {
        let headers = default_image_request_headers(TEST_USER_AGENT).expect("headers");

        assert!(!headers.contains_key(ACCEPT_ENCODING));
        assert!(!headers.contains_key(CONNECTION));
        assert!(!headers.contains_key(CONTENT_LENGTH));
        assert!(!headers.contains_key(HOST));
    }

    #[test]
    fn explicit_request_headers_override_default_image_headers() {
        let defaults = default_image_request_headers(TEST_USER_AGENT).expect("headers");
        let mut request_headers = HeaderMap::new();
        request_headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        request_headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-GB"));

        let request = apply_missing_default_headers(
            reqwest::blocking::Client::new().get("https://example.com/image.png"),
            &request_headers,
            &defaults,
        )
        .build()
        .expect("request should build");
        let headers = request.headers();

        assert_eq!(headers.get(ACCEPT).unwrap(), "application/json");
        assert_eq!(headers.get(ACCEPT_LANGUAGE).unwrap(), "en-GB");
        assert_eq!(headers.get(USER_AGENT).unwrap(), TEST_USER_AGENT);
        assert_eq!(headers.get(CACHE_CONTROL).unwrap(), DEFAULT_CACHE_CONTROL);
        assert_eq!(headers.get(PRAGMA).unwrap(), DEFAULT_CACHE_CONTROL);
    }

    #[test]
    fn detects_remote_http_sources() {
        assert!(is_remote_image_source("https://example.com/image.png"));
        assert!(is_remote_image_source("http://example.com/image.gif"));
        assert!(!is_remote_image_source("./image.png"));
        assert!(!is_remote_image_source("images/photo.jpg"));
    }
}
