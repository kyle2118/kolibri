//! Import a request from a pasted `curl` command.
//!
//! The command is split into words the way a POSIX shell would (quotes,
//! `$'...'`, backslash escapes and line continuations), but nothing is ever
//! expanded: `$TOKEN` and `{{token}}` stay as the literal text written.

use crate::model::{BodyKind, KeyValue, Method, Request};

/// The parts of a request that a curl command describes.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub method: Method,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub body_kind: BodyKind,
    pub body: String,
}

impl Imported {
    /// Overwrite method, URL, params, headers and body of `req`
    /// (id and name are kept).
    pub fn apply_to(self, req: &mut Request) {
        req.method = self.method;
        req.url = self.url;
        req.params.clear();
        crate::model::params_from_url(req);
        req.headers = self.headers;
        req.body_kind = self.body_kind;
        req.body = self.body;
    }
}

/// True if `text` looks like a curl command (`curl ` at the start).
pub fn is_curl(text: &str) -> bool {
    let t = text.trim_start();
    t.strip_prefix("curl").is_some_and(|rest| rest.starts_with(|c: char| c.is_whitespace()))
}

/// Split a shell command line into words. Stops at an unquoted `|`, `;`,
/// `&&`, `||` or `>` that starts a word (e.g. `curl ... | jq .`).
pub fn split_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            '|' | ';' | '>' if !in_word => break,
            '&' if !in_word && chars.peek() == Some(&'&') => break,
            '\\' => match chars.next() {
                // Line continuation: backslash-newline (or CRLF) disappears.
                Some('\n') => {}
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some(n) => {
                    cur.push(n);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err("Unterminated ' quote".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(n @ ('$' | '`' | '"' | '\\')) => cur.push(n),
                            Some('\n') => {}
                            Some('\r') if chars.peek() == Some(&'\n') => {
                                chars.next();
                            }
                            Some(n) => {
                                cur.push('\\');
                                cur.push(n);
                            }
                            None => return Err("Unterminated \" quote".into()),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err("Unterminated \" quote".into()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                ansi_c_quote(&mut chars, &mut cur)?;
            }
            _ => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// Body of a `$'...'` string (the opening `$'` is already consumed).
fn ansi_c_quote(chars: &mut std::iter::Peekable<std::str::Chars>, out: &mut String) -> Result<(), String> {
    const ERR: &str = "Unterminated $' quote";
    loop {
        let c = chars.next().ok_or(ERR)?;
        if c == '\'' {
            return Ok(());
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let e = chars.next().ok_or(ERR)?;
        match e {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'e' | 'E' => out.push('\x1b'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            '\\' | '\'' | '"' | '?' => out.push(e),
            '0'..='7' => {
                let mut v = e.to_digit(8).unwrap();
                for _ in 0..2 {
                    match chars.peek().and_then(|c| c.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(v & 0xff).unwrap_or('\u{fffd}'));
            }
            'x' | 'u' | 'U' => {
                let max = match e {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let mut v = 0u32;
                let mut n = 0;
                while n < max {
                    match chars.peek().and_then(|c| c.to_digit(16)) {
                        Some(d) => {
                            v = v * 16 + d;
                            chars.next();
                            n += 1;
                        }
                        None => break,
                    }
                }
                if n == 0 {
                    out.push('\\');
                    out.push(e);
                } else {
                    out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
}

/// Options that are accepted and ignored.
const IGNORED_SHORT: &[char] = &['s', 'S', 'L', 'k', 'i', 'v'];
const IGNORED_LONG: &[&str] = &[
    "--silent",
    "--show-error",
    "--location",
    "--insecure",
    "--include",
    "--verbose",
    "--compressed",
];

/// Canonical long name for options that take a value.
fn long_with_value(name: &str) -> Option<&'static str> {
    Some(match name {
        "--request" => "X",
        "--header" => "H",
        "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => "d",
        "--cookie" => "b",
        "--user" => "u",
        "--user-agent" => "A",
        "--referer" => "e",
        "--url" => "url",
        _ => return None,
    })
}

fn short_with_value(c: char) -> Option<&'static str> {
    Some(match c {
        'X' => "X",
        'H' => "H",
        'd' => "d",
        'b' => "b",
        'u' => "u",
        'A' => "A",
        'e' => "e",
        _ => return None,
    })
}

/// Parse a curl command into a request. Values are kept exactly as written.
pub fn parse(cmd: &str) -> Result<Imported, String> {
    let words = split_words(cmd)?;
    let mut it = words.into_iter();
    match it.next().as_deref() {
        Some("curl") => {}
        _ => return Err("Not a curl command".into()),
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut cookies: Vec<String> = Vec::new();
    let mut cookie_at: Option<usize> = None;
    let mut get = false;
    let mut ct_removed = false;
    let mut only_positional = false;

    let set_url = |u: String, url: &mut Option<String>| -> Result<(), String> {
        if url.is_some() {
            return Err("More than one URL in the curl command".into());
        }
        *url = Some(u);
        Ok(())
    };

    while let Some(w) = it.next() {
        // Collect (option, value) pairs from this word.
        let mut opts: Vec<(&'static str, String)> = Vec::new();
        if only_positional || !w.starts_with('-') || w == "-" {
            set_url(w, &mut url)?;
            continue;
        } else if w == "--" {
            only_positional = true;
            continue;
        } else if w.starts_with("--") {
            if IGNORED_LONG.contains(&w.as_str()) {
                continue;
            }
            if w == "--get" {
                get = true;
                continue;
            }
            let Some(name) = long_with_value(&w) else {
                return Err(format!("Unsupported curl option: {w}"));
            };
            let v = it.next().ok_or_else(|| format!("{w} needs a value"))?;
            opts.push((name, v));
        } else {
            // Short options, possibly combined (-sSL, -sX POST, -XPOST).
            let body = &w[1..];
            for (i, c) in body.char_indices() {
                if IGNORED_SHORT.contains(&c) {
                    continue;
                }
                if c == 'G' {
                    get = true;
                    continue;
                }
                let Some(name) = short_with_value(c) else {
                    return Err(format!("Unsupported curl option: -{c}"));
                };
                let rest = &body[i + c.len_utf8()..];
                let v = if rest.is_empty() {
                    it.next().ok_or_else(|| format!("-{c} needs a value"))?
                } else {
                    rest.to_owned()
                };
                opts.push((name, v));
                break;
            }
        }

        for (name, v) in opts {
            match name {
                "X" => method = Some(v),
                "H" => {
                    if let Some((k, val)) = v.split_once(':') {
                        let val = val.trim_start_matches([' ', '\t']);
                        if val.is_empty() {
                            // `-H 'Name:'` removes a header in curl.
                            ct_removed |= k.trim().eq_ignore_ascii_case("content-type");
                            continue;
                        }
                        headers.push(KeyValue::new(k.trim(), val));
                    } else if let Some(k) = v.strip_suffix(';') {
                        // `-H 'Name;'` sends the header with an empty value.
                        headers.push(KeyValue::new(k.trim(), ""));
                    } else {
                        return Err(format!("Invalid header: {v}"));
                    }
                }
                "d" => data.push(v),
                "b" => {
                    if !v.contains('=') {
                        return Err(format!("Cookie files are not supported (-b {v})"));
                    }
                    cookie_at.get_or_insert(headers.len());
                    cookies.push(v);
                }
                "u" => {
                    let creds = if v.contains(':') { v } else { format!("{v}:") };
                    headers.push(KeyValue::new("Authorization", &format!("Basic {}", base64(creds.as_bytes()))));
                }
                "A" => headers.push(KeyValue::new("User-Agent", &v)),
                "e" => headers.push(KeyValue::new("Referer", &v)),
                "url" => set_url(v, &mut url)?,
                _ => unreachable!(),
            }
        }
    }

    let mut url = url.ok_or("No URL in the curl command")?;
    if let Some(at) = cookie_at {
        headers.insert(at, KeyValue::new("Cookie", &cookies.join("; ")));
    }

    let has_data = !data.is_empty();
    let joined = data.join("&");
    let mut body = String::new();
    if get {
        if has_data {
            let (before, frag) = match url.find('#') {
                Some(i) => (url[..i].to_owned(), url[i..].to_owned()),
                None => (url.clone(), String::new()),
            };
            let sep = if !before.contains('?') {
                "?"
            } else if before.ends_with('?') || before.ends_with('&') {
                ""
            } else {
                "&"
            };
            url = format!("{before}{sep}{joined}{frag}");
        }
    } else if has_data {
        body = joined;
    }

    let method = match method {
        Some(m) => Method::ALL
            .into_iter()
            .find(|x| x.as_str().eq_ignore_ascii_case(&m))
            .ok_or_else(|| format!("Unsupported method: {m}"))?,
        None if has_data && !get => Method::POST,
        None => Method::GET,
    };

    let content_type = headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase());
    let body_kind = if body.is_empty() && !(has_data && !get) {
        BodyKind::None
    } else if content_type.as_deref().is_some_and(|ct| ct.contains("json")) {
        BodyKind::Json
    } else {
        BodyKind::Text
    };
    // curl sends -d data as a form unless told otherwise.
    if has_data && !get && content_type.is_none() && !ct_removed {
        headers.push(KeyValue::new("Content-Type", "application/x-www-form-urlencoded"));
    }

    Ok(Imported { method, url, headers, body_kind, body })
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(k: &str, v: &str) -> KeyValue {
        KeyValue::new(k, v)
    }

    #[test]
    fn detects_curl() {
        assert!(is_curl("curl https://x"));
        assert!(is_curl("  curl\n  https://x"));
        assert!(!is_curl("curly"));
        assert!(!is_curl("https://curl.se"));
    }

    #[test]
    fn simple_get() {
        let r = parse("curl https://api.example.com/users?page=2").unwrap();
        assert_eq!(r.method, Method::GET);
        assert_eq!(r.url, "https://api.example.com/users?page=2");
        assert!(r.headers.is_empty());
        assert_eq!(r.body_kind, BodyKind::None);
    }

    #[test]
    fn quotes_and_no_substitution() {
        let r = parse(r#"curl 'https://x/a?t={{token}}&b=$HOME' -H "Authorization: Bearer $TOKEN" -H 'X-A: "q"'"#).unwrap();
        assert_eq!(r.url, "https://x/a?t={{token}}&b=$HOME");
        assert_eq!(r.headers, vec![kv("Authorization", "Bearer $TOKEN"), kv("X-A", "\"q\"")]);
    }

    #[test]
    fn double_quote_escapes() {
        let w = split_words(r#"curl "a\"b\\c\$d\e" "#).unwrap();
        assert_eq!(w, vec!["curl", r#"a"b\c$d\e"#]);
    }

    #[test]
    fn unquoted_backslash_and_concat() {
        let w = split_words(r#"curl a\ b 'x'"y"z"#).unwrap();
        assert_eq!(w, vec!["curl", "a b", "xyz"]);
    }

    #[test]
    fn ansi_c_quotes() {
        let r = parse(r#"curl -d $'{"a":"it\'s\n\x41é"}' -H 'Content-Type: application/json' https://x"#).unwrap();
        assert_eq!(r.body, "{\"a\":\"it's\n\u{41}\u{e9}\"}");
        assert_eq!(r.body_kind, BodyKind::Json);
        assert_eq!(r.method, Method::POST);
    }

    #[test]
    fn line_continuations() {
        let cmd = "curl -X PUT \\\n  'https://x/y' \\\r\n  -H 'A: 1' \\\n  --data-raw '{\n  \"k\": 1\n}'";
        let r = parse(cmd).unwrap();
        assert_eq!(r.method, Method::PUT);
        assert_eq!(r.url, "https://x/y");
        assert_eq!(r.headers[0], kv("A", "1"));
        assert_eq!(r.body, "{\n  \"k\": 1\n}");
        assert_eq!(r.body_kind, BodyKind::Text);
    }

    #[test]
    fn data_defaults_to_post_form() {
        let r = parse("curl https://x -d a=1 --data b=2 --data-binary 'c=3'").unwrap();
        assert_eq!(r.method, Method::POST);
        assert_eq!(r.body, "a=1&b=2&c=3");
        assert_eq!(r.headers, vec![kv("Content-Type", "application/x-www-form-urlencoded")]);
    }

    #[test]
    fn data_at_sign_is_literal() {
        let r = parse("curl https://x --data-raw @file.json").unwrap();
        assert_eq!(r.body, "@file.json");
    }

    #[test]
    fn explicit_method_wins() {
        let r = parse("curl -XPATCH https://x -d a").unwrap();
        assert_eq!(r.method, Method::PATCH);
        let r = parse("curl --request delete https://x").unwrap();
        assert_eq!(r.method, Method::DELETE);
        assert!(parse("curl -X TRACE https://x").is_err());
    }

    #[test]
    fn get_moves_data_to_query() {
        let r = parse("curl -G https://x/s?q=1 -d 'a=b c' --data z").unwrap();
        assert_eq!(r.method, Method::GET);
        assert_eq!(r.url, "https://x/s?q=1&a=b c&z");
        assert_eq!(r.body_kind, BodyKind::None);
        assert!(r.headers.is_empty());
        let r = parse("curl --get --url https://x/s#f -d a=1").unwrap();
        assert_eq!(r.url, "https://x/s?a=1#f");
    }

    #[test]
    fn cookies_user_agent_referer_auth() {
        let r = parse("curl -b 'a=1' -A 'Agent/1' -e https://ref -u user:p@ss -b b=2 https://x").unwrap();
        assert_eq!(
            r.headers,
            vec![
                kv("Cookie", "a=1; b=2"),
                kv("User-Agent", "Agent/1"),
                kv("Referer", "https://ref"),
                kv("Authorization", "Basic dXNlcjpwQHNz"),
            ]
        );
        assert!(parse("curl -b cookies.txt https://x").is_err());
    }

    #[test]
    fn ignored_flags_and_combined_shorts() {
        let r = parse("curl -sSLkiv --compressed --silent --location --insecure -sX POST https://x").unwrap();
        assert_eq!(r.method, Method::POST);
        assert_eq!(r.url, "https://x");
        assert!(parse("curl --max-time 5 https://x").is_err());
    }

    #[test]
    fn header_forms() {
        let r = parse("curl https://x -H 'Empty;' -H 'Content-Type:' -H 'A:b:c' -d x").unwrap();
        assert_eq!(r.headers, vec![kv("Empty", ""), kv("A", "b:c")]);
        assert_eq!(r.body_kind, BodyKind::Text);
    }

    #[test]
    fn stops_at_pipe() {
        let r = parse("curl https://x | jq .").unwrap();
        assert_eq!(r.url, "https://x");
        let r = parse("curl 'https://x?a=1|2'").unwrap();
        assert_eq!(r.url, "https://x?a=1|2");
    }

    #[test]
    fn errors() {
        assert!(parse("curl 'https://x").is_err());
        assert!(parse("curl -H").is_err());
        assert!(parse("curl -s").is_err());
        assert!(parse("curl https://a https://b").is_err());
    }

    #[test]
    fn chrome_copy_as_curl() {
        let cmd = r#"curl 'https://api.example.com/v1/items?id=5' \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  -b 'session=abc; theme=dark' \
  --data-raw '{"name":"x"}' \
  --compressed"#;
        let r = parse(cmd).unwrap();
        assert_eq!(r.method, Method::POST);
        assert_eq!(r.body_kind, BodyKind::Json);
        assert_eq!(r.body, r#"{"name":"x"}"#);
        assert_eq!(
            r.headers,
            vec![
                kv("accept", "application/json"),
                kv("content-type", "application/json"),
                kv("Cookie", "session=abc; theme=dark"),
            ]
        );
    }

    #[test]
    fn apply_fills_params() {
        let mut req = Request { id: 7, name: "n".into(), params: vec![KeyValue::new("old", "1")], ..Default::default() };
        parse("curl 'https://x/a?k={{v}}&z'").unwrap().apply_to(&mut req);
        assert_eq!(req.id, 7);
        assert_eq!(req.name, "n");
        assert_eq!(req.params, vec![kv("k", "{{v}}"), kv("z", "")]);
    }

    #[test]
    fn base64_works() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
}
