//! Data model and on-disk persistence.
//!
//! Everything lives in one JSON file in the user's config directory.
//! Values are stored and sent exactly as typed: there is no variable or
//! template substitution anywhere in Kolibri.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    /// Subfolders, shown above the folder's own requests.
    pub folders: Vec<Folder>,
}

impl Default for Folder {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            expanded: true,
            requests: Vec::new(),
            folders: Vec::new(),
        }
    }
}

impl Folder {
    /// Requests in this folder and all its subfolders.
    pub fn count_requests(&self) -> usize {
        self.requests.len() + self.folders.iter().map(Folder::count_requests).sum::<usize>()
    }

    /// Number of subfolders at any depth.
    pub fn count_folders(&self) -> usize {
        self.folders.len() + self.folders.iter().map(Folder::count_folders).sum::<usize>()
    }

    /// Ids of all requests in this folder and its subfolders.
    pub fn request_ids(&self) -> Vec<u64> {
        let mut out = Vec::new();
        walk(std::slice::from_ref(self), &mut |f| out.extend(f.requests.iter().map(|r| r.id)));
        out
    }
}

/// Visit every folder, depth first, parents before children.
fn walk<'a>(folders: &'a [Folder], f: &mut impl FnMut(&'a Folder)) {
    for folder in folders {
        f(folder);
        walk(&folder.folders, f);
    }
}

fn walk_mut(folders: &mut [Folder], f: &mut impl FnMut(&mut Folder)) {
    for folder in folders {
        f(folder);
        walk_mut(&mut folder.folders, f);
    }
}

/// Result of [`Store::search`]: ids of the folders and requests to show.
#[derive(Default, Debug)]
pub struct SearchHits {
    pub folders: HashSet<u64>,
    pub requests: HashSet<u64>,
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
    /// Requests not saved in any folder yet. They live only in their tab.
    pub drafts: Vec<Request>,
    /// Open editor tabs, in order. `selected` is the active one.
    pub tabs: Vec<u64>,
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
            folders: Vec::new(),
        });
        s.selected = Some(rid);
        s
    }

    /// A request in any folder, or a draft.
    pub fn find_request(&self, id: u64) -> Option<&Request> {
        if let Some(d) = self.drafts.iter().find(|r| r.id == id) {
            return Some(d);
        }
        let mut found = None;
        walk(&self.folders, &mut |f| {
            if found.is_none() {
                found = f.requests.iter().find(|r| r.id == id);
            }
        });
        found
    }

    pub fn find_request_mut(&mut self, id: u64) -> Option<&mut Request> {
        if let Some(i) = self.drafts.iter().position(|r| r.id == id) {
            return self.drafts.get_mut(i);
        }
        let (fid, ri) = self.locate_request(id)?;
        self.folder_mut(fid)?.requests.get_mut(ri)
    }

    pub fn is_draft(&self, id: u64) -> bool {
        self.drafts.iter().any(|r| r.id == id)
    }

    /// Move draft `id` to the end of folder `folder`. False if either is missing.
    pub fn save_draft(&mut self, id: u64, folder: u64) -> bool {
        let Some(i) = self.drafts.iter().position(|r| r.id == id) else {
            return false;
        };
        if self.folder(folder).is_none() {
            return false;
        }
        let r = self.drafts.remove(i);
        let f = self.folder_mut(folder).unwrap();
        f.expanded = true;
        f.requests.push(r);
        true
    }

    /// Returns (id of the folder holding the request, request index).
    /// Drafts are in no folder.
    pub fn locate_request(&self, id: u64) -> Option<(u64, usize)> {
        let mut found = None;
        walk(&self.folders, &mut |f| {
            if found.is_none() {
                if let Some(ri) = f.requests.iter().position(|r| r.id == id) {
                    found = Some((f.id, ri));
                }
            }
        });
        found
    }

    /// Child indexes leading from the top level to folder `id`.
    pub fn folder_path(&self, id: u64) -> Option<Vec<usize>> {
        fn find(folders: &[Folder], id: u64, path: &mut Vec<usize>) -> bool {
            for (i, f) in folders.iter().enumerate() {
                path.push(i);
                if f.id == id || find(&f.folders, id, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        find(&self.folders, id, &mut path).then_some(path)
    }

    pub fn folder_at(&self, path: &[usize]) -> Option<&Folder> {
        let (first, rest) = path.split_first()?;
        rest.iter().try_fold(self.folders.get(*first)?, |f, &i| f.folders.get(i))
    }

    pub fn folder_at_mut(&mut self, path: &[usize]) -> Option<&mut Folder> {
        let (first, rest) = path.split_first()?;
        rest.iter().try_fold(self.folders.get_mut(*first)?, |f, &i| f.folders.get_mut(i))
    }

    pub fn folder(&self, id: u64) -> Option<&Folder> {
        self.folder_at(&self.folder_path(id)?)
    }

    pub fn folder_mut(&mut self, id: u64) -> Option<&mut Folder> {
        let path = self.folder_path(id)?;
        self.folder_at_mut(&path)
    }

    /// The parent of folder `id` (`None` = top level).
    pub fn parent_of(&self, id: u64) -> Option<Option<u64>> {
        let path = self.folder_path(id)?;
        Some(self.folder_at(&path[..path.len() - 1]).map(|f| f.id))
    }

    /// The list a folder lives in: top level for `None`, else a folder's subfolders.
    pub fn children_mut(&mut self, parent: Option<u64>) -> Option<&mut Vec<Folder>> {
        match parent {
            None => Some(&mut self.folders),
            Some(id) => self.folder_mut(id).map(|f| &mut f.folders),
        }
    }

    /// Detach folder `id`; returns it with its former parent and index.
    pub fn take_folder(&mut self, id: u64) -> Option<(Option<u64>, usize, Folder)> {
        let path = self.folder_path(id)?;
        let (&index, parent_path) = path.split_last()?;
        let parent = self.folder_at(parent_path).map(|f| f.id);
        let list = self.children_mut(parent)?;
        Some((parent, index, list.remove(index)))
    }

    /// Is folder `id` the folder `ancestor` or somewhere inside it?
    pub fn is_within(&self, id: u64, ancestor: u64) -> bool {
        self.folder(ancestor).is_some_and(|a| {
            let mut hit = a.id == id;
            walk(&a.folders, &mut |f| hit |= f.id == id);
            hit
        })
    }

    /// Names from the top-level folder down to folder `id`.
    pub fn folder_trail(&self, id: u64) -> Vec<String> {
        let path = self.folder_path(id).unwrap_or_default();
        (1..=path.len())
            .filter_map(|n| self.folder_at(&path[..n]).map(|f| f.name.clone()))
            .collect()
    }

    /// Folders and requests to show for a sidebar search. A request matches
    /// on its name or URL (case-insensitive). A folder whose name matches
    /// shows everything inside it; other folders show only to reach a match.
    pub fn search(&self, query: &str) -> SearchHits {
        fn visit(f: &Folder, q: &str, inside_match: bool, hits: &mut SearchHits) -> bool {
            let all = inside_match || f.name.to_lowercase().contains(q);
            let mut any = all;
            for sub in &f.folders {
                any |= visit(sub, q, all, hits);
            }
            for r in &f.requests {
                if all || r.name.to_lowercase().contains(q) || r.url.to_lowercase().contains(q) {
                    hits.requests.insert(r.id);
                    any = true;
                }
            }
            if any {
                hits.folders.insert(f.id);
            }
            any
        }
        let q = query.trim().to_lowercase();
        let mut hits = SearchHits::default();
        for f in &self.folders {
            visit(f, &q, false, &mut hits);
        }
        hits
    }

    /// Every folder as (id, name, depth), in sidebar order.
    pub fn folder_list(&self) -> Vec<(u64, String, usize)> {
        fn visit(folders: &[Folder], depth: usize, out: &mut Vec<(u64, String, usize)>) {
            for f in folders {
                out.push((f.id, f.name.clone(), depth));
                visit(&f.folders, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        visit(&self.folders, 0, &mut out);
        out
    }

    /// Open every folder on the way to request `id` (so its row is visible).
    /// Returns true if any folder had to be opened.
    pub fn expand_to_request(&mut self, id: u64) -> bool {
        let Some(path) = self.locate_request(id).and_then(|(fid, _)| self.folder_path(fid)) else {
            return false;
        };
        let mut changed = false;
        for n in 1..=path.len() {
            if let Some(f) = self.folder_at_mut(&path[..n]) {
                changed |= !f.expanded;
                f.expanded = true;
            }
        }
        changed
    }

    /// Collapse every folder at every depth.
    pub fn collapse_all(&mut self) {
        walk_mut(&mut self.folders, &mut |f| f.expanded = false);
    }

    /// Make sure ids are unique and `next_id` is ahead of all of them
    /// (protects against hand-edited files).
    fn normalize(&mut self) {
        // Pass 1: find the highest id, so fresh ids can't collide with any.
        let mut max = self.drafts.iter().map(|r| r.id).max().unwrap_or(0);
        walk(&self.folders, &mut |f| {
            max = max.max(f.id);
            max = f.requests.iter().map(|r| r.id).fold(max, u64::max);
        });
        let mut next = self.next_id.max(max);
        // Pass 2: re-number zero and repeated ids.
        let mut seen = HashSet::new();
        let mut fix = |id: &mut u64| {
            if *id == 0 || !seen.insert(*id) {
                next += 1;
                *id = next;
            }
        };
        walk_mut(&mut self.folders, &mut |f| {
            fix(&mut f.id);
            f.requests.iter_mut().for_each(|r| fix(&mut r.id));
        });
        self.drafts.iter_mut().for_each(|r| fix(&mut r.id));
        self.next_id = next;
        self.fix_tabs();
    }

    /// Keep tabs pointing at real requests, without repeats, and make sure
    /// the selected request has a tab.
    pub fn fix_tabs(&mut self) {
        let mut seen = HashSet::new();
        let tabs = std::mem::take(&mut self.tabs);
        self.tabs = tabs
            .into_iter()
            .filter(|id| seen.insert(*id) && self.find_request(*id).is_some())
            .collect();
        match self.selected {
            Some(id) if self.find_request(id).is_some() => {
                if !self.tabs.contains(&id) {
                    self.tabs.push(id);
                }
            }
            _ => self.selected = self.tabs.last().copied(),
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
    fn old_files_load_and_nested_lookups_work() {
        let mut s: Store = serde_json::from_str(
            r#"{"folders":[{"id":1,"name":"A","requests":[{"id":2}]}]}"#,
        )
        .unwrap();
        assert!(s.folders[0].folders.is_empty());
        let sub = Folder { id: 3, name: "B".into(), requests: vec![Request { id: 4, ..Default::default() }], ..Default::default() };
        s.folders[0].folders.push(sub);
        s.folders[0].folders[0].folders.push(Folder { id: 5, ..Default::default() });

        assert_eq!(s.locate_request(4), Some((3, 0)));
        assert_eq!(s.folder_path(5), Some(vec![0, 0, 0]));
        assert_eq!(s.parent_of(3), Some(Some(1)));
        assert_eq!(s.parent_of(1), Some(None));
        assert_eq!(s.folder_trail(3), ["A", "B"]);
        assert!(s.is_within(5, 1) && s.is_within(3, 3) && !s.is_within(1, 3));
        assert_eq!(s.folders[0].count_requests(), 2);
        assert_eq!(s.folders[0].request_ids(), [2, 4]);

        s.collapse_all();
        assert!(!s.folders[0].expanded && !s.folder(5).unwrap().expanded);

        let (parent, index, f) = s.take_folder(3).unwrap();
        assert_eq!((parent, index, f.id), (Some(1), 0, 3));
        assert!(s.find_request(4).is_none());
    }

    #[test]
    fn search_matches_name_or_url_and_keeps_parents() {
        let req = |id, name: &str, url: &str| Request {
            id,
            name: name.into(),
            url: url.into(),
            ..Default::default()
        };
        let mut s = Store::default();
        s.folders.push(Folder {
            id: 1,
            name: "Shop".into(),
            requests: vec![req(2, "List orders", "https://x/orders")],
            folders: vec![Folder {
                id: 3,
                name: "Users".into(),
                requests: vec![req(4, "Get one", "https://x/users/1")],
                ..Default::default()
            }],
            ..Default::default()
        });
        s.folders.push(Folder { id: 5, name: "Other".into(), ..Default::default() });

        // By name, case-insensitive; parents stay visible, others don't.
        let h = s.search("ORDERS");
        assert_eq!(h.requests, HashSet::from([2]));
        assert_eq!(h.folders, HashSet::from([1]));

        // By URL, through a subfolder.
        let h = s.search(" /users/ ");
        assert_eq!(h.requests, HashSet::from([4]));
        assert_eq!(h.folders, HashSet::from([1, 3]));

        // A matching folder shows all of its contents.
        let h = s.search("users");
        assert!(h.requests.contains(&4) && h.folders.contains(&3));

        assert!(s.search("nothing-like-this").folders.is_empty());
    }

    #[test]
    fn drafts_and_tabs() {
        let mut s = Store::starter();
        let saved = s.selected.unwrap();
        let fid = s.folders[0].id;
        s.drafts.push(Request { id: 99, url: "https://x".into(), ..Default::default() });
        assert!(s.is_draft(99) && s.find_request(99).is_some());
        assert_eq!(s.locate_request(99), None);

        // Old files have no tabs: the selected request gets one.
        s.tabs = vec![99, 99, 12345];
        s.normalize();
        assert_eq!(s.tabs, [99, saved]);

        assert!(s.save_draft(99, fid));
        assert!(!s.is_draft(99));
        assert_eq!(s.locate_request(99), Some((fid, 1)));
        assert!(!s.save_draft(99, fid), "already saved");
    }

    #[test]
    fn expand_to_request_opens_every_parent() {
        let mut s = Store::default();
        s.folders.push(Folder {
            id: 1,
            folders: vec![Folder {
                id: 2,
                requests: vec![Request { id: 3, ..Default::default() }],
                ..Default::default()
            }],
            ..Default::default()
        });
        s.folders.push(Folder { id: 4, ..Default::default() });
        s.collapse_all();
        assert!(s.expand_to_request(3));
        assert!(s.folder(1).unwrap().expanded && s.folder(2).unwrap().expanded);
        assert!(!s.folder(4).unwrap().expanded, "unrelated folders stay closed");
        assert!(!s.expand_to_request(3), "nothing left to open");
        assert!(!s.expand_to_request(99));
    }

    #[test]
    fn normalize_fixes_nested_duplicate_ids() {
        let mut s = Store::default();
        let mut a = Folder { id: 7, ..Default::default() };
        a.folders.push(Folder { id: 7, ..Default::default() });
        s.folders.push(a);
        s.normalize();
        assert_ne!(s.folders[0].id, s.folders[0].folders[0].id);
        assert!(s.next_id >= 8);
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
