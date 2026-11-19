//! Import of Postman Collection files (format v2.0 / v2.1).
//!
//! Kolibri folders are flat, so the collection's top-level requests go into a
//! folder named after the collection, and every nested Postman folder becomes
//! its own Kolibri folder named by its path ("Collection / Users / Admin").
//!
//! Kolibri never substitutes variables, so collection variables
//! (`{{baseUrl}}` etc.) are resolved once, at import time. Unknown variables
//! are left as typed. Inherited auth (collection / folder level) is turned
//! into plain headers or query params on each request.

use crate::model::{self, BodyKind, Folder, KeyValue, Method, Request, Store};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Imported {
    pub folders: Vec<Folder>,
    pub requests: usize,
    /// Things that could not be carried over exactly.
    pub warnings: Vec<String>,
}

/// Parse a Postman collection and build Kolibri folders, taking ids from `store`.
pub fn import(json: &str, store: &mut Store) -> Result<Imported, String> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| format!("Not a valid JSON file: {e}"))?;
    // Postman API exports wrap the collection: {"collection": {...}}.
    let root = match root.get("collection") {
        Some(c) if c.get("item").is_some() => c,
        _ => &root,
    };
    let Some(items) = root.get("item").and_then(Value::as_array) else {
        if root.get("requests").is_some() {
            return Err("This is a Postman v1 collection. Re-export it from Postman as \
                        Collection v2.1."
                .into());
        }
        return Err("Not a Postman collection (no \"item\" list found).".into());
    };

    let name = str_at(root, &["info", "name"]).unwrap_or("Imported collection").to_owned();
    let mut cx = Cx {
        vars: variables(root.get("variable")),
        warnings: Vec::new(),
        requests: 0,
        folders: Vec::new(),
    };
    let auth = root.get("auth").cloned();
    cx.walk(store, &name, items, auth.as_ref());

    if cx.requests == 0 {
        return Err(format!("\"{name}\" contains no requests."));
    }
    // Several requests with the same unsupported feature: say it once.
    cx.warnings.dedup();
    Ok(Imported { folders: cx.folders, requests: cx.requests, warnings: cx.warnings })
}

struct Cx {
    vars: HashMap<String, String>,
    warnings: Vec<String>,
    requests: usize,
    folders: Vec<Folder>,
}

impl Cx {
    fn walk(&mut self, store: &mut Store, path: &str, items: &[Value], auth: Option<&Value>) {
        let id = store.new_id();
        let slot = self.folders.len();
        self.folders.push(Folder { id, name: path.to_owned(), expanded: true, requests: vec![] });

        for item in items {
            let name = item.get("name").and_then(Value::as_str).unwrap_or("Untitled");
            let own_auth = item.get("auth").filter(|a| !a.is_null());
            let auth = own_auth.or(auth);
            if let Some(children) = item.get("item").and_then(Value::as_array) {
                self.walk(store, &format!("{path} / {name}"), children, auth);
            } else if let Some(req) = item.get("request") {
                let r = self.request(store.new_id(), name, req, auth);
                self.folders[slot].requests.push(r);
                self.requests += 1;
            }
        }
        // Drop folders that only held sub-folders.
        if self.folders[slot].requests.is_empty() {
            self.folders.remove(slot);
        }
    }

    fn sub(&self, s: &str) -> String {
        substitute(s, &self.vars)
    }

    fn request(&mut self, id: u64, name: &str, req: &Value, inherited: Option<&Value>) -> Request {
        let mut r = Request { id, name: name.to_owned(), ..Default::default() };

        // `request` may be just a URL string.
        if let Some(url) = req.as_str() {
            r.url = self.sub(url);
            model::params_from_url(&mut r);
            return r;
        }

        let method = req.get("method").and_then(Value::as_str).unwrap_or("GET");
        r.method = match Method::ALL.iter().find(|m| m.as_str().eq_ignore_ascii_case(method)) {
            Some(&m) => m,
            None => {
                self.warnings.push(format!("Method {method} is not supported; \"{name}\" uses GET."));
                Method::GET
            }
        };

        // URL and query params.
        let url = req.get("url");
        r.url = self.sub(&url_text(url));
        model::params_from_url(&mut r);
        if let Some(query) = url.and_then(|u| u.get("query")).and_then(Value::as_array) {
            for q in query.iter().filter(|q| is_disabled(q)) {
                r.params.push(KeyValue {
                    enabled: false,
                    key: self.sub(str_of(q, "key")),
                    value: self.sub(str_of(q, "value")),
                });
            }
        }

        // Headers.
        if let Some(headers) = req.get("header").and_then(Value::as_array) {
            for h in headers {
                r.headers.push(KeyValue {
                    enabled: !is_disabled(h),
                    key: self.sub(str_of(h, "key")),
                    value: self.sub(str_of(h, "value")),
                });
            }
        }

        let auth = req.get("auth").filter(|a| !a.is_null()).or(inherited);
        if let Some(auth) = auth {
            self.auth(&mut r, auth);
        }
        if let Some(body) = req.get("body") {
            self.body(&mut r, body);
        }
        r
    }

    fn auth(&mut self, r: &mut Request, auth: &Value) {
        let kind = auth.get("type").and_then(Value::as_str).unwrap_or("noauth");
        let field = |k: &str| self.sub(&auth_field(auth, kind, k));
        let header = |r: &mut Request, key: &str, value: String| {
            if !r.headers.iter().any(|h| h.enabled && h.key.eq_ignore_ascii_case(key)) {
                r.headers.push(KeyValue::new(key, &value));
            }
        };
        match kind {
            "noauth" | "inherit" => {}
            "bearer" => header(r, "Authorization", format!("Bearer {}", field("token"))),
            "basic" => {
                let creds = format!("{}:{}", field("username"), field("password"));
                header(r, "Authorization", format!("Basic {}", base64(creds.as_bytes())));
            }
            "apikey" => {
                let (key, value) = (field("key"), field("value"));
                if field("in") == "query" {
                    r.params.push(KeyValue::new(&key, &value));
                    model::url_from_params(r);
                } else {
                    header(r, &key, value);
                }
            }
            other => self.warnings.push(format!(
                "Auth type \"{other}\" can't be imported; add the header by hand (\"{}\").",
                r.name
            )),
        }
    }

    fn body(&mut self, r: &mut Request, body: &Value) {
        if body.get("disabled").and_then(Value::as_bool) == Some(true) {
            return;
        }
        match body.get("mode").and_then(Value::as_str).unwrap_or("") {
            "raw" => {
                let raw = self.sub(str_of(body, "raw"));
                if raw.is_empty() {
                    return;
                }
                let lang = str_at(body, &["options", "raw", "language"]).unwrap_or("");
                let looks_json = serde_json::from_str::<Value>(&raw).is_ok();
                r.body_kind = if lang == "json" || (lang.is_empty() && looks_json) {
                    BodyKind::Json
                } else {
                    BodyKind::Text
                };
                r.body = raw;
            }
            "urlencoded" => self.form(r, body.get("urlencoded"), false),
            "formdata" => self.form(r, body.get("formdata"), true),
            "graphql" => {
                let g = body.get("graphql").cloned().unwrap_or(Value::Null);
                let vars = g
                    .get("variables")
                    .and_then(Value::as_str)
                    .and_then(|v| serde_json::from_str::<Value>(v).ok())
                    .unwrap_or(Value::Object(Map::new()));
                let doc = serde_json::json!({ "query": str_of(&g, "query"), "variables": vars });
                r.body_kind = BodyKind::Json;
                r.body = self.sub(&serde_json::to_string_pretty(&doc).unwrap_or_default());
                ensure_content_type(r, "application/json");
            }
            "file" => self.warnings.push(format!(
                "File bodies can't be imported (\"{}\").",
                r.name
            )),
            _ => {}
        }
    }

    /// urlencoded / form-data bodies become an `a=1&b=2` text body.
    fn form(&mut self, r: &mut Request, fields: Option<&Value>, multipart: bool) {
        let Some(fields) = fields.and_then(Value::as_array) else { return };
        let mut parts = Vec::new();
        for f in fields.iter().filter(|f| !is_disabled(f)) {
            if f.get("type").and_then(Value::as_str) == Some("file") {
                self.warnings.push(format!("File form fields were skipped (\"{}\").", r.name));
                continue;
            }
            parts.push(format!(
                "{}={}",
                self.sub(str_of(f, "key")),
                self.sub(str_of(f, "value"))
            ));
        }
        if parts.is_empty() {
            return;
        }
        if multipart {
            self.warnings.push(format!(
                "Multipart form data is sent as URL-encoded (\"{}\").",
                r.name
            ));
        }
        r.body_kind = BodyKind::Text;
        r.body = parts.join("&");
        ensure_content_type(r, "application/x-www-form-urlencoded");
    }
}

fn ensure_content_type(r: &mut Request, value: &str) {
    if !r.headers.iter().any(|h| h.key.eq_ignore_ascii_case("content-type")) {
        r.headers.push(KeyValue::new("Content-Type", value));
    }
}

fn is_disabled(v: &Value) -> bool {
    v.get("disabled").and_then(Value::as_bool) == Some(true)
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(v, |v, k| v.get(*k))?.as_str()
}

/// Collection variables: `[{"key": "baseUrl", "value": "https://..."}]`.
fn variables(v: Option<&Value>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for var in v.and_then(Value::as_array).into_iter().flatten() {
        if is_disabled(var) {
            continue;
        }
        let Some(key) = var.get("key").and_then(Value::as_str) else { continue };
        let value = match var.get("value") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => continue,
            Some(other) => other.to_string(),
        };
        out.insert(key.to_owned(), value);
    }
    out
}

/// Replace `{{name}}` with known variables (one pass, so values that
/// themselves contain `{{...}}` are not expanded again).
fn substitute(s: &str, vars: &HashMap<String, String>) -> String {
    if vars.is_empty() || !s.contains("{{") {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = &after[..end];
                match vars.get(name.trim()) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("{{");
                        out.push_str(name);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// A Postman URL is either a string or an object with `raw` (or its parts).
fn url_text(url: Option<&Value>) -> String {
    let Some(url) = url else { return String::new() };
    if let Some(s) = url.as_str() {
        return s.to_owned();
    }
    if let Some(raw) = url.get("raw").and_then(Value::as_str) {
        return raw.to_owned();
    }
    let join = |v: Option<&Value>, sep: &str| match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => {
            a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(sep)
        }
        _ => String::new(),
    };
    let mut s = String::new();
    if let Some(proto) = url.get("protocol").and_then(Value::as_str) {
        s.push_str(proto);
        s.push_str("://");
    }
    s.push_str(&join(url.get("host"), "."));
    if let Some(port) = url.get("port").and_then(Value::as_str) {
        s.push(':');
        s.push_str(port);
    }
    let path = join(url.get("path"), "/");
    if !path.is_empty() {
        s.push('/');
        s.push_str(path.trim_start_matches('/'));
    }
    let query: Vec<String> = url
        .get("query")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|q| !is_disabled(q))
        .map(|q| match q.get("value").and_then(Value::as_str) {
            Some(v) if !v.is_empty() => format!("{}={v}", str_of(q, "key")),
            _ => str_of(q, "key").to_owned(),
        })
        .collect();
    if !query.is_empty() {
        s.push('?');
        s.push_str(&query.join("&"));
    }
    s
}

/// Auth params: v2.1 uses `{"bearer": [{"key": "token", "value": "..."}]}`,
/// v2.0 uses `{"bearer": {"token": "..."}}`.
fn auth_field(auth: &Value, kind: &str, key: &str) -> String {
    let value = match auth.get(kind) {
        Some(Value::Array(list)) => {
            list.iter().find(|e| str_of(e, "key") == key).and_then(|e| e.get("value"))
        }
        Some(obj @ Value::Object(_)) => obj.get(key),
        _ => None,
    };
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "info": { "name": "Demo API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
      "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}" }] },
      "variable": [
        { "key": "baseUrl", "value": "https://api.example.com" },
        { "key": "token", "value": "secret" }
      ],
      "item": [
        {
          "name": "Health",
          "request": { "method": "GET", "url": { "raw": "{{baseUrl}}/health?verbose=1",
            "query": [{ "key": "verbose", "value": "1" }, { "key": "off", "value": "2", "disabled": true }] } }
        },
        {
          "name": "Users",
          "item": [
            {
              "name": "Create user",
              "request": {
                "method": "post",
                "header": [{ "key": "X-Trace", "value": "1", "disabled": true }],
                "url": "{{baseUrl}}/users",
                "body": { "mode": "raw", "raw": "{\"name\": \"{{unknown}}\"}", "options": { "raw": { "language": "json" } } }
              }
            },
            {
              "name": "Admin",
              "item": [{
                "name": "Login",
                "request": {
                  "method": "POST",
                  "auth": { "type": "basic", "basic": [{ "key": "username", "value": "a" }, { "key": "password", "value": "b" }] },
                  "url": { "protocol": "https", "host": ["example", "com"], "path": ["login"] },
                  "body": { "mode": "urlencoded", "urlencoded": [{ "key": "x", "value": "1" }] }
                }
              }]
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn imports_nested_collection() {
        let mut store = Store::default();
        let out = import(SAMPLE, &mut store).unwrap();
        assert_eq!(out.requests, 3);
        let names: Vec<_> = out.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Demo API", "Demo API / Users", "Demo API / Users / Admin"]);

        let health = &out.folders[0].requests[0];
        assert_eq!(health.url, "https://api.example.com/health?verbose=1");
        assert_eq!(health.params.len(), 2);
        assert!(!health.params[1].enabled);
        assert_eq!(health.headers[0], KeyValue::new("Authorization", "Bearer secret"));

        let create = &out.folders[1].requests[0];
        assert_eq!(create.method, Method::POST);
        assert_eq!(create.body_kind, BodyKind::Json);
        assert_eq!(create.body, "{\"name\": \"{{unknown}}\"}");
        assert!(!create.headers[0].enabled);

        let login = &out.folders[2].requests[0];
        assert_eq!(login.url, "https://example.com/login");
        assert_eq!(login.headers[0], KeyValue::new("Authorization", "Basic YTpi"));
        assert_eq!(login.body, "x=1");

        let mut ids: Vec<u64> = out.folders.iter().map(|f| f.id).collect();
        ids.extend(out.folders.iter().flat_map(|f| f.requests.iter().map(|r| r.id)));
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "ids must be unique");
    }

    #[test]
    fn rejects_non_collections() {
        let mut store = Store::default();
        assert!(import("{}", &mut store).is_err());
        assert!(import("not json", &mut store).is_err());
        assert!(import(r#"{"requests": []}"#, &mut store).unwrap_err().contains("v1"));
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    }
}
