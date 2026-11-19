//! Visual QA tour (development only, `--features qa`).
//!
//! `KOLIBRI_SHOTS=<dir> cargo run --features qa` starts a local mock API,
//! loads a demo collection (the user's data file is never touched), walks
//! through every major screen in dark and light themes and at several window
//! sizes, saves a PNG screenshot of each, then quits.

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------- mock server

fn spawn_mock_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
    });
    format!("http://{addr}")
}

fn serve(stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let l = line.trim_end();
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                if k.eq_ignore_ascii_case("content-length") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
        }
        let mut body = vec![0; content_length];
        let _ = reader.read_exact(&mut body);
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_owned();
        let method = request_line
            .split_whitespace()
            .next()
            .unwrap_or("GET")
            .to_owned();
        if path.starts_with("/slow") {
            std::thread::sleep(Duration::from_secs(60));
        }
        let (status, json) = if path.starts_with("/users") && method == "POST" {
            ("201 Created", r#"{"id":1043,"name":"Ada Lovelace","email":"ada@example.com","role":"admin","createdAt":"2026-11-18T19:42:11Z"}"#.to_owned())
        } else if path.starts_with("/users") {
            ("200 OK", USERS.to_owned())
        } else if path.starts_with("/missing") {
            (
                "404 Not Found",
                r#"{"error":"not_found","message":"No invoice with id inv_404"}"#.to_owned(),
            )
        } else {
            (
                "200 OK",
                r#"{"status":"ok","uptime":86400.5,"version":"2.14.1"}"#.to_owned(),
            )
        };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nX-Request-Id: 7f3c9a2e-41d8-4b9e-a0c2-5d1e8f6b2a90\r\n\
             X-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 4987\r\nServer: kolibri-mock\r\n\
             Connection: keep-alive\r\n\r\n",
            json.len()
        );
        if out
            .write_all(head.as_bytes())
            .and_then(|_| out.write_all(json.as_bytes()))
            .is_err()
        {
            return;
        }
    }
}

const USERS: &str = r#"{"page":2,"limit":20,"total":143,"data":[{"id":21,"name":"Grace Hopper","email":"grace@example.com","role":"admin","active":true,"lastLogin":"2026-11-17T08:12:44Z","tags":["compilers","navy"]},{"id":22,"name":"Alan Turing","email":"alan@example.com","role":"member","active":true,"lastLogin":null,"tags":[]},{"id":23,"name":"Katherine Johnson","email":"katherine@example.com","role":"member","active":false,"lastLogin":"2026-10-02T15:30:00Z","tags":["orbital-mechanics"]},{"id":24,"name":"Linus Torvalds","email":"linus@example.com","role":"owner","active":true,"lastLogin":"2026-11-18T06:01:13Z","tags":["kernel","git"]}]}"#;

// ---------------------------------------------------------------- demo data

fn kv(k: &str, v: &str) -> KeyValue {
    KeyValue::new(k, v)
}

fn demo_store(base: &str) -> Store {
    let mut s = Store {
        version: 1,
        ..Default::default()
    };
    let req = |s: &mut Store, name: &str, method: Method, url: String| {
        let id = s.new_id();
        let mut r = Request {
            id,
            name: name.into(),
            method,
            url,
            ..Default::default()
        };
        model::params_from_url(&mut r);
        r
    };
    let mut users = Vec::new();
    let mut list = req(
        &mut s,
        "List users",
        Method::GET,
        format!("{base}/users?page=2&limit=20&sort=-createdAt"),
    );
    list.headers = vec![
        kv("Accept", "application/json"),
        kv("Authorization", "Bearer {{token}}"),
    ];
    list.params.push(KeyValue {
        enabled: false,
        key: "include".into(),
        value: "teams".into(),
    });
    users.push(list);
    let mut create = req(&mut s, "Create user", Method::POST, format!("{base}/users"));
    create.headers = vec![
        kv("Content-Type", "application/json"),
        kv("Authorization", "Bearer {{token}}"),
        KeyValue {
            enabled: false,
            key: "X-Debug".into(),
            value: "1".into(),
        },
        kv("Idempotency-Key", "c0a8012e-7f3b-4d1a-9e2f-6b5c4d3e2f1a"),
    ];
    create.body_kind = BodyKind::Json;
    create.body = "{\n  \"name\": \"Ada Lovelace\",\n  \"email\": \"ada@example.com\",\n  \"role\": \"admin\",\n  \"notify\": true,\n  \"quota\": 2048\n}".into();
    users.push(create);
    users.push(req(
        &mut s,
        "Update user",
        Method::PATCH,
        format!("{base}/users/21"),
    ));
    users.push(req(
        &mut s,
        "Delete user",
        Method::DELETE,
        format!("{base}/users/21"),
    ));
    let fid = s.new_id();
    s.folders.push(Folder {
        id: fid,
        name: "Users API".into(),
        expanded: true,
        requests: users,
    });

    let payments = vec![
        req(
            &mut s,
            "Get invoice",
            Method::GET,
            format!("{base}/missing/invoices/inv_404"),
        ),
        req(
            &mut s,
            "Refund payment (slow)",
            Method::POST,
            format!("{base}/slow/refunds"),
        ),
        req(
            &mut s,
            "Unreachable service",
            Method::PUT,
            "http://127.0.0.1:9/settle".into(),
        ),
    ];
    let fid = s.new_id();
    s.folders.push(Folder {
        id: fid,
        name: "Payments".into(),
        expanded: true,
        requests: payments,
    });

    let health = vec![
        req(&mut s, "Ping", Method::GET, format!("{base}/health")),
        req(
            &mut s,
            "Status headers",
            Method::HEAD,
            format!("{base}/health"),
        ),
        req(
            &mut s,
            "CORS preflight",
            Method::OPTIONS,
            format!("{base}/health"),
        ),
    ];
    let fid = s.new_id();
    s.folders.push(Folder {
        id: fid,
        name: "Health checks".into(),
        expanded: false,
        requests: health,
    });
    let fid = s.new_id();
    s.folders.push(Folder {
        id: fid,
        name: "Drafts".into(),
        expanded: true,
        requests: vec![],
    });
    s.selected = Some(s.folders[0].requests[0].id);
    s
}

fn request_id(app: &App, name: &str) -> u64 {
    app.store
        .folders
        .iter()
        .flat_map(|f| &f.requests)
        .find(|r| r.name == name)
        .map(|r| r.id)
        .expect(name)
}

fn select(app: &mut App, name: &str) {
    let id = request_id(app, name);
    app.store.selected = Some(id);
}

fn select_and_send(app: &mut App, ctx: &egui::Context, name: &str) {
    select(app, name);
    let id = request_id(app, name);
    app.responses.remove(&id);
    app.send(ctx, id);
}

// ---------------------------------------------------------------- tour

type Setup = fn(&mut App, &egui::Context);
type Ready = fn(&App) -> bool;

struct Step {
    name: &'static str,
    size: [f32; 2],
    light: bool,
    setup: Setup,
    ready: Ready,
    /// Pointer clicks to inject: (position as fraction of the window or absolute, secondary?)
    click: Option<ClickAt>,
}

#[derive(Clone, Copy)]
enum ClickAt {
    /// Secondary click on the first request row of the sidebar.
    FirstRequestRow,
    /// Primary click on the method picker.
    MethodPicker,
}

fn idle(app: &App) -> bool {
    app.inflight.is_empty()
}

fn always(_: &App) -> bool {
    true
}

fn steps() -> Vec<Step> {
    const BIG: [f32; 2] = [1280.0, 800.0];
    vec![
        Step {
            name: "01-response-body",
            size: BIG,
            light: false,
            setup: |a, c| {
                a.req_tab = ReqTab::Params;
                a.resp_tab = RespTab::Body;
                select_and_send(a, c, "List users")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "02-response-headers",
            size: BIG,
            light: false,
            setup: |a, _| a.resp_tab = RespTab::Headers,
            ready: always,
            click: None,
        },
        Step {
            name: "03-request-headers",
            size: BIG,
            light: false,
            setup: |a, c| {
                a.req_tab = ReqTab::Headers;
                a.resp_tab = RespTab::Body;
                select_and_send(a, c, "Create user")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "04-request-body",
            size: BIG,
            light: false,
            setup: |a, _| a.req_tab = ReqTab::Body,
            ready: always,
            click: None,
        },
        Step {
            name: "05-no-body",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.req_tab = ReqTab::Body;
                select(a, "Update user")
            },
            ready: always,
            click: None,
        },
        Step {
            name: "06-404",
            size: BIG,
            light: false,
            setup: |a, c| {
                a.req_tab = ReqTab::Params;
                select_and_send(a, c, "Get invoice")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "07-sending",
            size: BIG,
            light: false,
            setup: |a, c| select_and_send(a, c, "Refund payment (slow)"),
            ready: always,
            click: None,
        },
        Step {
            name: "08-network-error",
            size: BIG,
            light: false,
            setup: |a, c| {
                let id = request_id(a, "Refund payment (slow)");
                a.inflight.remove(&id);
                select_and_send(a, c, "Unreachable service")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "09-load-test",
            size: BIG,
            light: false,
            setup: |a, c| {
                select(a, "List users");
                a.resp_tab = RespTab::LoadTest;
                a.load_cfg = loadtest::Config {
                    mode: loadtest::Mode::Count,
                    total: 3000,
                    seconds: 10,
                    concurrency: 16,
                };
                let id = request_id(a, "List users");
                let p = http::prepare(a.store.find_request(id).unwrap()).unwrap();
                a.loads
                    .insert(id, loadtest::Run::start(p, a.load_cfg.clone(), c.clone()));
            },
            ready: |a| a.loads.values().all(|r| !r.is_running()),
            click: None,
        },
        Step {
            name: "10-load-test-empty",
            size: BIG,
            light: false,
            setup: |a, _| {
                select(a, "Ping");
                a.resp_tab = RespTab::LoadTest
            },
            ready: always,
            click: None,
        },
        Step {
            name: "11-context-menu",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.resp_tab = RespTab::Body;
                select(a, "List users")
            },
            ready: always,
            click: Some(ClickAt::FirstRequestRow),
        },
        Step {
            name: "12-method-menu",
            size: BIG,
            light: false,
            setup: |_, _| {},
            ready: always,
            click: Some(ClickAt::MethodPicker),
        },
        Step {
            name: "13-delete-dialog",
            size: BIG,
            light: false,
            setup: |a, _| {
                let id = request_id(a, "Delete user");
                a.confirm_delete = Some(Item::Request(id))
            },
            ready: always,
            click: None,
        },
        Step {
            name: "14-delete-folder-dialog",
            size: BIG,
            light: false,
            setup: |a, _| {
                let fid = a.store.folders[0].id;
                a.confirm_delete = Some(Item::Folder(fid))
            },
            ready: always,
            click: None,
        },
        Step {
            name: "15-notice-and-curl-error",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.confirm_delete = None;
                a.notice = Some("Could not save /Users/me/Library/Application Support/kolibri/collections.json: Permission denied (os error 13)".into());
                a.curl_err = Some("cURL import failed: Unterminated ' quote".into())
            },
            ready: always,
            click: None,
        },
        Step {
            name: "16-rename",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.notice = None;
                a.curl_err = None;
                let id = request_id(a, "Update user");
                a.apply(Action::StartRename(Item::Request(id)))
            },
            ready: always,
            click: None,
        },
        Step {
            name: "17-no-selection",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.rename = None;
                a.store.selected = None
            },
            ready: always,
            click: None,
        },
        Step {
            name: "18-light-response",
            size: BIG,
            light: true,
            setup: |a, c| {
                a.resp_tab = RespTab::Body;
                a.req_tab = ReqTab::Params;
                select_and_send(a, c, "List users")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "19-light-body",
            size: BIG,
            light: true,
            setup: |a, c| {
                a.req_tab = ReqTab::Body;
                select_and_send(a, c, "Create user")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "20-light-load-test",
            size: BIG,
            light: true,
            setup: |a, _| {
                select(a, "List users");
                a.resp_tab = RespTab::LoadTest
            },
            ready: always,
            click: None,
        },
        Step {
            name: "21-light-dialog",
            size: BIG,
            light: true,
            setup: |a, _| {
                let id = request_id(a, "Delete user");
                a.confirm_delete = Some(Item::Request(id))
            },
            ready: always,
            click: None,
        },
        Step {
            name: "22-small-window",
            size: [760.0, 480.0],
            light: false,
            setup: |a, c| {
                a.confirm_delete = None;
                a.resp_tab = RespTab::Body;
                a.req_tab = ReqTab::Headers;
                select_and_send(a, c, "Create user")
            },
            ready: idle,
            click: None,
        },
        Step {
            name: "23-small-load-test",
            size: [760.0, 480.0],
            light: false,
            setup: |a, _| {
                select(a, "List users");
                a.resp_tab = RespTab::LoadTest
            },
            ready: always,
            click: None,
        },
        Step {
            name: "24-medium-window",
            size: [1024.0, 640.0],
            light: false,
            setup: |a, _| {
                a.resp_tab = RespTab::Body;
                a.req_tab = ReqTab::Params
            },
            ready: always,
            click: None,
        },
        Step {
            name: "25-large-window",
            size: [1600.0, 1000.0],
            light: false,
            setup: |_, _| {},
            ready: always,
            click: None,
        },
        Step {
            name: "26-empty-collection",
            size: BIG,
            light: false,
            setup: |a, _| {
                a.store.folders.clear();
                a.store.selected = None
            },
            ready: always,
            click: None,
        },
    ]
}

enum Phase {
    Start,
    /// Waiting for the Escape that closes menus left open by the previous step.
    Closing(u32),
    Settle {
        frames: u32,
        started: Instant,
    },
    Capture(Arc<AtomicBool>),
}

pub struct Tour {
    dir: PathBuf,
    steps: Vec<Step>,
    idx: usize,
    phase: Phase,
    pending_clicks: Vec<egui::Event>,
}

impl Tour {
    /// Enabled by `KOLIBRI_SHOTS=<dir>`; replaces the store with demo data
    /// and points saving at a throwaway file.
    pub fn from_env(app: &mut App) -> Option<Tour> {
        let dir = PathBuf::from(std::env::var_os("KOLIBRI_SHOTS")?);
        std::fs::create_dir_all(&dir).ok()?;
        let base = spawn_mock_server();
        app.store = demo_store(&base);
        app.path = dir.join("qa-collections.json");
        app.notice = None;
        Some(Tour {
            dir,
            steps: steps(),
            idx: 0,
            phase: Phase::Start,
            pending_clicks: Vec::new(),
        })
    }
}

/// Inject queued pointer events (called from `raw_input_hook`).
pub fn raw_input(app: &mut App, raw: &mut egui::RawInput) {
    if let Some(t) = &mut app.qa {
        if !t.pending_clicks.is_empty() {
            // One event batch per frame: move, then press, then release.
            let e = t.pending_clicks.remove(0);
            raw.events.push(e);
        }
    }
}

fn click_events(ctx: &egui::Context, at: ClickAt) -> Vec<egui::Event> {
    let key = match at {
        ClickAt::FirstRequestRow => "qa_first_request_row",
        ClickAt::MethodPicker => "qa_method_picker",
    };
    let rect: Option<Rect> = ctx.data(|d| d.get_temp(Id::unique(key)));
    let Some(rect) = rect else { return Vec::new() };
    let pos = rect.center();
    let button = match at {
        ClickAt::FirstRequestRow => egui::PointerButton::Secondary,
        ClickAt::MethodPicker => egui::PointerButton::Primary,
    };
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]
}

/// Drive the tour; called at the start of every frame.
pub fn tick(app: &mut App, ctx: &egui::Context) {
    let Some(mut t) = app.qa.take() else { return };
    ctx.request_repaint();
    if t.idx >= t.steps.len() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        app.qa = Some(t);
        return;
    }
    let step = &t.steps[t.idx];
    match &t.phase {
        Phase::Start => {
            // Close any menu left open by the previous step, then set up.
            let had_click = t.idx > 0 && t.steps[t.idx - 1].click.is_some();
            if had_click {
                t.pending_clicks = vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }];
                t.phase = Phase::Closing(0);
            } else {
                t.phase = Phase::Closing(u32::MAX);
            }
        }
        Phase::Closing(n) if *n != u32::MAX && (*n < 3 || !t.pending_clicks.is_empty()) => {
            t.phase = Phase::Closing(n + 1);
        }
        Phase::Closing(_) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(
                step.size[0],
                step.size[1],
            )));
            ctx.set_theme(if step.light {
                egui::Theme::Light
            } else {
                egui::Theme::Dark
            });
            (step.setup)(app, ctx);
            t.phase = Phase::Settle {
                frames: 0,
                started: Instant::now(),
            };
        }
        Phase::Settle { frames, started } => {
            let frames = frames + 1;
            if frames == 8 {
                if let Some(at) = step.click {
                    t.pending_clicks = click_events(ctx, at);
                }
            }
            let waited = started.elapsed();
            let ready = (step.ready)(app) || waited > Duration::from_secs(20);
            if frames >= 24
                && t.pending_clicks.is_empty()
                && ready
                && waited > Duration::from_millis(500)
            {
                let done = Arc::new(AtomicBool::new(false));
                let flag = done.clone();
                let path = t.dir.join(format!("{}.png", step.name));
                ctx.request_screenshot(move |img| {
                    std::thread::spawn(move || {
                        if let Err(e) = write_png(&path, &img) {
                            eprintln!("qa: could not write {}: {e}", path.display());
                        }
                        flag.store(true, Ordering::SeqCst);
                    });
                });
                t.phase = Phase::Capture(done);
            } else {
                t.phase = Phase::Settle {
                    frames,
                    started: *started,
                };
            }
        }
        Phase::Capture(done) => {
            if done.load(Ordering::SeqCst) {
                eprintln!("qa: captured {}", step.name);
                t.idx += 1;
                t.phase = Phase::Start;
            }
        }
    }
    app.qa = Some(t);
}

// ---------------------------------------------------------------- png

/// Minimal PNG writer (uncompressed deflate blocks); no extra dependencies.
fn write_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for px in &img.pixels[y * w..(y + 1) * w] {
            let [r, g, b, a] = px.to_srgba_unmultiplied();
            raw.extend_from_slice(&[r, g, b, a]);
        }
    }
    let mut z = vec![0x78, 0x01];
    for (i, chunk) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(u8::from(last));
        let len = chunk.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &x in data {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}
