//! Sending requests. Everything the user typed is sent verbatim; the only
//! change ever made to the URL is percent-encoding characters that are not
//! legal in a URL at all (spaces, quotes, braces, non-ASCII, ...), because
//! the request could not be sent otherwise.

use crate::model::{BodyKind, Method, Request};
use std::time::{Duration, Instant};
use ureq::http;

pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub elapsed: Duration,
    pub headers: Vec<(String, String)>,
    pub size: usize,
    /// Body as shown to the user (pretty-printed if it is JSON).
    pub display: String,
    pub is_json: bool,
    pub truncated: bool,
}

pub type HttpResult = Result<HttpResponse, String>;

const MAX_BODY: u64 = 200 * 1024 * 1024;
const MAX_DISPLAY: usize = 4 * 1024 * 1024;

pub fn new_agent(pool: usize) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("Kolibri/", env!("CARGO_PKG_VERSION")))
        .max_idle_connections(pool.max(4) * 2)
        .max_idle_connections_per_host(pool.max(4))
        .allow_non_standard_methods(true)
        .tls_config(tls)
        .build()
        .new_agent()
}

fn needs_escape(b: u8) -> bool {
    !(b.is_ascii_graphic()) || matches!(b, b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}')
}

/// Percent-encode only bytes that can't legally appear in a URL.
/// Existing `%XX` sequences and everything else are left untouched.
pub fn escape_illegal(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for &b in url.as_bytes() {
        if needs_escape(b) {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out
}

/// A fully prepared request that can be sent many times (load testing).
#[derive(Clone)]
pub struct Prepared {
    pub method: http::Method,
    pub uri: http::Uri,
    pub headers: Vec<(http::HeaderName, http::HeaderValue)>,
    pub body: Vec<u8>,
}

pub fn prepare(req: &Request) -> Result<Prepared, String> {
    let raw = req.url.trim();
    if raw.is_empty() {
        return Err("Enter a URL".into());
    }
    let with_scheme = if raw.contains("://") { raw.to_owned() } else { format!("http://{raw}") };
    // Drop the #fragment; it is never sent to servers.
    let no_frag = with_scheme.split('#').next().unwrap_or_default();
    let uri: http::Uri = escape_illegal(no_frag).parse().map_err(|e| format!("Invalid URL: {e}"))?;
    if uri.host().is_none() {
        return Err("Invalid URL: missing host".into());
    }

    let method = http::Method::from_bytes(req.method.as_str().as_bytes()).unwrap();
    let mut headers = Vec::new();
    let mut has_ct = false;
    for h in req.headers.iter().filter(|h| h.enabled && !h.key.trim().is_empty()) {
        let name = http::HeaderName::from_bytes(h.key.trim().as_bytes())
            .map_err(|_| format!("Invalid header name: {:?}", h.key))?;
        let value = http::HeaderValue::from_bytes(h.value.as_bytes())
            .map_err(|_| format!("Invalid value for header {:?}", h.key))?;
        has_ct |= name == http::header::CONTENT_TYPE;
        headers.push((name, value));
    }

    let body = match req.body_kind {
        BodyKind::None => Vec::new(),
        BodyKind::Json | BodyKind::Text => req.body.as_bytes().to_vec(),
    };
    if !has_ct && !body.is_empty() {
        let ct = if req.body_kind == BodyKind::Json { "application/json" } else { "text/plain; charset=utf-8" };
        headers.push((http::header::CONTENT_TYPE, http::HeaderValue::from_static(ct)));
    }
    Ok(Prepared { method, uri, headers, body })
}

pub fn build(p: &Prepared) -> Result<http::Request<Vec<u8>>, String> {
    let mut b = http::Request::builder().method(p.method.clone()).uri(p.uri.clone());
    for (k, v) in &p.headers {
        b = b.header(k, v);
    }
    b.body(p.body.clone()).map_err(|e| e.to_string())
}

fn error_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::HostNotFound => "Could not resolve host".into(),
        ureq::Error::ConnectionFailed => "Connection refused or failed".into(),
        ureq::Error::Timeout(t) => format!("Timed out ({t})"),
        ureq::Error::Io(io) => format!("Network error: {io}"),
        other => other.to_string(),
    }
}

/// Send once, returning status and drained body size (used by load tests).
pub fn send_raw(agent: &ureq::Agent, p: &Prepared) -> Result<(u16, usize), String> {
    let req = build(p)?;
    let mut resp = agent.run(req).map_err(error_text)?;
    let status = resp.status().as_u16();
    let n = resp.body_mut().with_config().limit(MAX_BODY).read_to_vec().map(|v| v.len()).unwrap_or(0);
    Ok((status, n))
}

pub fn send(agent: &ureq::Agent, req: &Request) -> HttpResult {
    let p = prepare(req)?;
    let started = Instant::now();
    let mut resp = agent.run(build(&p)?).map_err(error_text)?;
    let status = resp.status();
    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_owned(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let bytes = if req.method == Method::HEAD {
        Vec::new()
    } else {
        resp.body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|e| format!("Failed reading body: {e}"))?
    };
    let elapsed = started.elapsed();
    let (display, is_json, truncated) = render_body(&bytes);
    Ok(HttpResponse {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_owned(),
        elapsed,
        headers,
        size: bytes.len(),
        display,
        is_json,
        truncated,
    })
}

fn render_body(bytes: &[u8]) -> (String, bool, bool) {
    let first = bytes.iter().find(|b| !b.is_ascii_whitespace());
    if matches!(first, Some(b'{') | Some(b'[')) && bytes.len() <= MAX_DISPLAY * 2 {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
            if let Ok(s) = serde_json::to_string_pretty(&v) {
                let t = s.len() > MAX_DISPLAY;
                return (truncate(s, MAX_DISPLAY), true, t);
            }
        }
    }
    let t = bytes.len() > MAX_DISPLAY;
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_DISPLAY)]).into_owned();
    (s, false, t)
}

fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut i = max;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
    }
    s
}

pub fn beautify_json(text: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}"))?;
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_only_illegal() {
        assert_eq!(escape_illegal("http://h/a b?x={{t}}&y=%20"), "http://h/a%20b?x=%7B%7Bt%7D%7D&y=%20");
        assert_eq!(escape_illegal("http://h/ok?a=1&b=2"), "http://h/ok?a=1&b=2");
    }

    #[test]
    fn beautify_keeps_order_and_numbers() {
        let out = beautify_json(r#"{"z":1.10,"a":[1,2],"big":12345678901234567890123}"#).unwrap();
        assert_eq!(out, "{\n  \"z\": 1.10,\n  \"a\": [\n    1,\n    2\n  ],\n  \"big\": 12345678901234567890123\n}");
    }

    #[test]
    fn headers_sent_verbatim() {
        let r = Request {
            url: "example.com/x".into(),
            headers: vec![crate::model::KeyValue::new("Authorization", "Bearer {{token}}")],
            ..Default::default()
        };
        let p = prepare(&r).unwrap();
        assert_eq!(p.uri.to_string(), "http://example.com/x");
        assert_eq!(p.headers[0].1.to_str().unwrap(), "Bearer {{token}}");
    }
}
