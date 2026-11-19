//! Data model and on-disk persistence.
//!
//! Everything lives in one JSON file in the user's config directory.
//! Values are stored and sent exactly as typed: there is no variable or
//! template substitution anywhere in Kolibri.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[allow(clippy::upper_case_acronyms)]
pub enum Method {
    #[default]
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::HEAD,
        Method::OPTIONS,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
            Method::HEAD => "HEAD",
            Method::OPTIONS => "OPTIONS",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct KeyValue {
    pub enabled: bool,
    pub key: String,
    pub value: String,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self { enabled: true, key: String::new(), value: String::new() }
    }
}

impl KeyValue {
    pub fn new(key: &str, value: &str) -> Self {
        Self { enabled: true, key: key.to_owned(), value: value.to_owned() }
    }
    pub fn is_blank(&self) -> bool {
        self.key.is_empty() && self.value.is_empty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum BodyKind {
    #[default]
    None,
    Json,
    Text,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Request {
    pub id: u64,
    pub name: String,
    pub method: Method,
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body_kind: BodyKind,
    pub body: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Folder {
    pub id: u64,
    pub name: String,
    pub expanded: bool,
    pub requests: Vec<Request>,
}

impl Default for Folder {
    fn default() -> Self {
        Self { id: 0, name: String::new(), expanded: true, requests: Vec::new() }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Store {
    pub version: u32,
    pub next_id: u64,
    pub selected: Option<u64>,
    /// Light theme instead of the default dark one.
    pub light_mode: bool,
    pub folders: Vec<Folder>,
}

impl Store {
    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn starter() -> Self {
        let mut s = Store { version: 1, ..Default::default() };
        let fid = s.new_id();
        let rid = s.new_id();
        s.folders.push(Folder {
            id: fid,
            name: "My requests".into(),
            expanded: true,
            requests: vec![Request {
                id: rid,
                name: "Example GET".into(),
                url: "https://httpbin.org/get?hello=world".into(),
                params: vec![KeyValue::new("hello", "world")],
                ..Default::default()
            }],
        });
        s.selected = Some(rid);
        s
    }

    pub fn find_request(&self, id: u64) -> Option<&Request> {
        self.folders.iter().flat_map(|f| f.requests.iter()).find(|r| r.id == id)
    }

    pub fn find_request_mut(&mut self, id: u64) -> Option<&mut Request> {
        self.folders.iter_mut().flat_map(|f| f.requests.iter_mut()).find(|r| r.id == id)
    }

    /// Returns (folder index, request index).
    pub fn locate_request(&self, id: u64) -> Option<(usize, usize)> {
        self.folders.iter().enumerate().find_map(|(fi, f)| {
            f.requests.iter().position(|r| r.id == id).map(|ri| (fi, ri))
        })
    }

    pub fn folder_index(&self, id: u64) -> Option<usize> {
        self.folders.iter().position(|f| f.id == id)
    }

    /// Make sure ids are unique and `next_id` is ahead of all of them
    /// (protects against hand-edited files).
    fn normalize(&mut self) {
        let mut max = 0;
        let mut seen = std::collections::HashSet::new();
        let mut needs_new = Vec::new();
        for (fi, f) in self.folders.iter().enumerate() {
            if !seen.insert(f.id) || f.id == 0 {
                needs_new.push((fi, None));
            }
            max = max.max(f.id);
            for (ri, r) in f.requests.iter().enumerate() {
                if !seen.insert(r.id) || r.id == 0 {
                    needs_new.push((fi, Some(ri)));
                }
                max = max.max(r.id);
            }
        }
        self.next_id = self.next_id.max(max);
        for (fi, ri) in needs_new {
            let id = self.new_id();
            match ri {
                None => self.folders[fi].id = id,
                Some(ri) => self.folders[fi].requests[ri].id = id,
            }
        }
    }
}

pub fn data_path() -> PathBuf {
    if let Some(p) = std::env::var_os("KOLIBRI_DATA") {
        return PathBuf::from(p);
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("kolibri").join("collections.json")
}

/// Load the store. A missing file gives a starter collection; an unreadable
/// one is moved aside (never silently overwritten) and a fresh store is used.
pub fn load(path: &Path) -> (Store, Option<String>) {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Store>(&bytes) {
            Ok(mut s) => {
                s.normalize();
                (s, None)
            }
            Err(e) => {
                let backup = path.with_extension("json.broken");
                let _ = std::fs::rename(path, &backup);
                (
                    Store::starter(),
                    Some(format!(
                        "Could not read {} ({e}). It was moved to {}.",
                        path.display(),
                        backup.display()
                    )),
                )
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Store::starter(), None),
        Err(e) => (Store::starter(), Some(format!("Could not read {}: {e}", path.display()))),
    }
}

/// Atomic save: write a temp file, then rename over the real one.
pub fn save(path: &Path, store: &Store) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(store).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

// ---------------------------------------------------------------------------
// URL <-> params syncing. Plain text only: nothing is decoded or substituted.

/// Split a URL into the part before `?` and its query pairs (raw, undecoded).
pub fn split_query(url: &str) -> (&str, Vec<(String, String)>) {
    let (no_frag, _) = url.split_once('#').unwrap_or((url, ""));
    match no_frag.split_once('?') {
        None => (no_frag, Vec::new()),
        Some((base, q)) => {
            let pairs = q
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| match p.split_once('=') {
                    Some((k, v)) => (k.to_owned(), v.to_owned()),
                    None => (p.to_owned(), String::new()),
                })
                .collect();
            (base, pairs)
        }
    }
}

/// After the URL text changed: enabled params mirror the URL query,
/// disabled params are kept as they were.
pub fn params_from_url(req: &mut Request) {
    let (_, pairs) = split_query(&req.url);
    let mut params: Vec<KeyValue> =
        pairs.iter().map(|(k, v)| KeyValue::new(k, v)).collect();
    params.extend(req.params.iter().filter(|p| !p.enabled && !p.is_blank()).cloned());
    req.params = params;
}

/// After the params table changed: rewrite the URL query from enabled params.
pub fn url_from_params(req: &mut Request) {
    let frag = req.url.find('#').map(|i| req.url[i..].to_owned()).unwrap_or_default();
    let (base, _) = split_query(&req.url);
    let mut url = base.to_owned();
    let mut first = true;
    for p in req.params.iter().filter(|p| p.enabled && !p.key.is_empty()) {
        url.push(if first { '?' } else { '&' });
        first = false;
        url.push_str(&p.key);
        if !p.value.is_empty() {
            url.push('=');
            url.push_str(&p.value);
        }
    }
    url.push_str(&frag);
    req.url = url;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_roundtrip_keeps_text_verbatim() {
        let mut r = Request { url: "http://x/a?token={{abc}}&b=1%202&c".into(), ..Default::default() };
        params_from_url(&mut r);
        assert_eq!(r.params[0], KeyValue::new("token", "{{abc}}"));
        assert_eq!(r.params[1], KeyValue::new("b", "1%202"));
        assert_eq!(r.params[2], KeyValue::new("c", ""));
        url_from_params(&mut r);
        assert_eq!(r.url, "http://x/a?token={{abc}}&b=1%202&c");
    }

    #[test]
    fn disabled_params_survive_url_edits() {
        let mut r = Request { url: "http://x/?a=1".into(), ..Default::default() };
        params_from_url(&mut r);
        r.params.push(KeyValue { enabled: false, key: "off".into(), value: "2".into() });
        r.url = "http://x/?a=1&b=3".into();
        params_from_url(&mut r);
        assert_eq!(r.params.len(), 3);
        assert!(!r.params[2].enabled);
    }

    #[test]
    fn dark_mode_is_the_default() {
        assert!(!Store::starter().light_mode);
        // Files saved before the setting existed also open in dark mode.
        let s: Store = serde_json::from_str(r#"{"version":1,"folders":[]}"#).unwrap();
        assert!(!s.light_mode);
    }

    #[test]
    fn normalize_fixes_duplicate_ids() {
        let mut s = Store::default();
        s.folders.push(Folder { id: 5, ..Default::default() });
        s.folders.push(Folder { id: 5, ..Default::default() });
        s.normalize();
        assert_ne!(s.folders[0].id, s.folders[1].id);
        assert!(s.next_id >= 6);
    }
}
