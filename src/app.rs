use crate::curl;
use crate::highlight::json_job;
use crate::http::{self, HttpResult};
use crate::kit::{self, seg, Icon};
use crate::loadtest;
use crate::postman;
use crate::model::{self, BodyKind, Folder, KeyValue, Method, Request, Store};
use crate::theme::{self, font, mix, radius, size, space, Palette};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Galley, Id, Key,
    KeyboardShortcut, Label, Layout, Margin, Modifiers, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, TextEdit, Ui,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "qa")]
#[path = "qa.rs"]
mod qa;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Folder(u64),
    Request(u64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReqTab {
    Params,
    Headers,
    Body,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RespTab {
    Body,
    Headers,
    LoadTest,
}

#[derive(Clone, Copy, Debug)]
enum DropTarget {
    /// Insert the dragged folder before folder index `.0` (== len: at end).
    FolderAt(usize),
    /// Insert the dragged request into folder `folder` before `index`.
    RequestAt { folder: usize, index: usize },
    /// Append the dragged request to the end of folder `.0`.
    IntoFolder(usize),
}

#[derive(Clone, Copy)]
enum RowRef {
    Folder(usize),
    Request(usize, usize),
}

struct Rename {
    item: Item,
    text: String,
    focus: bool,
}

struct ResponseView {
    result: HttpResult,
    galley: Option<(f32, bool, Arc<Galley>)>,
}

enum Action {
    Select(u64),
    Toggle(u64),
    StartRename(Item),
    NewRequest(u64),
    NewFolder,
    ImportPostman,
    Duplicate(u64),
    AskDelete(Item),
}

pub struct App {
    store: Store,
    path: PathBuf,
    dirty: bool,
    last_edit: Instant,
    notice: Option<String>,

    agent: ureq::Agent,
    inflight: HashMap<u64, (Instant, Receiver<HttpResult>)>,
    responses: HashMap<u64, ResponseView>,
    loads: HashMap<u64, loadtest::Run>,
    load_cfg: loadtest::Config,
    load_cache: Option<(Instant, u64, loadtest::Summary)>,

    req_tab: ReqTab,
    resp_tab: RespTab,
    rename: Option<Rename>,
    drag: Option<Item>,
    confirm_delete: Option<Item>,
    beautify_err: Option<String>,
    curl_err: Option<String>,
    logo: Option<egui::TextureHandle>,

    #[cfg(feature = "qa")]
    qa: Option<qa::Tour>,
}

/// Minimum width of the Send button.
const SEND_W: f32 = 116.0;

/// Platform-appropriate shortcut label, e.g. "⌘↵" on macOS, "Ctrl+↵" elsewhere.
fn shortcut(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

/// Short method label for the narrow sidebar column.
fn method_short(m: Method) -> &'static str {
    match m {
        Method::DELETE => "DEL",
        Method::OPTIONS => "OPT",
        other => other.as_str(),
    }
}

fn fmt_size(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

fn fmt_dur(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 10.0 {
        format!("{ms:.2} ms")
    } else if ms < 10_000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.1} s", ms / 1000.0)
    }
}

fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-64.png")).ok()?;
    let img = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    Some(ctx.load_texture("kolibri-logo", img, egui::TextureOptions::LINEAR))
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let path = model::data_path();
        let (store, notice) = model::load(&path);
        cc.egui_ctx.set_theme(if store.light_mode { egui::Theme::Light } else { egui::Theme::Dark });
        #[allow(unused_mut)]
        let mut app = Self::with_store(store, path, notice);
        app.logo = load_logo(&cc.egui_ctx);
        #[cfg(feature = "qa")]
        {
            app.qa = qa::Tour::from_env(&mut app);
        }
        app
    }

    fn with_store(store: Store, path: PathBuf, notice: Option<String>) -> Self {
        Self {
            store,
            path,
            dirty: false,
            last_edit: Instant::now(),
            notice,
            agent: http::new_agent(4),
            inflight: HashMap::new(),
            responses: HashMap::new(),
            loads: HashMap::new(),
            load_cfg: loadtest::Config::default(),
            load_cache: None,
            req_tab: ReqTab::Params,
            resp_tab: RespTab::Body,
            rename: None,
            drag: None,
            confirm_delete: None,
            beautify_err: None,
            curl_err: None,
            logo: None,
            #[cfg(feature = "qa")]
            qa: None,
        }
    }

    /// Ask for a Postman collection file and add its folders and requests.
    fn import_postman(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import Postman collection")
            .add_filter("Postman collection", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.notice = Some(format!("Could not read {}: {e}", path.display()));
                return;
            }
        };
        let result = postman::import(&text, &mut self.store);
        self.apply_import(result);
    }

    fn apply_import(&mut self, result: Result<postman::Imported, String>) {
        match result {
            Ok(imported) => {
                let first = imported.folders.first().and_then(|f| f.requests.first()).map(|r| r.id);
                self.store.folders.extend(imported.folders);
                if first.is_some() {
                    self.store.selected = first;
                }
                self.notice = match imported.warnings.as_slice() {
                    [] => None,
                    [one] => Some(format!("Imported {} requests. {one}", imported.requests)),
                    [one, rest @ ..] => Some(format!(
                        "Imported {} requests. {one} (+{} more notes)",
                        imported.requests,
                        rest.len()
                    )),
                };
                self.touch();
            }
            Err(e) => self.notice = Some(format!("Postman import failed: {e}")),
        }
    }

    /// Switch between dark and light mode and remember the choice.
    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.store.light_mode = !dark;
        ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
        self.touch();
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.last_edit = Instant::now();
    }

    fn save_now(&mut self) {
        match model::save(&self.path, &self.store) {
            Ok(()) => self.dirty = false,
            Err(e) => {
                self.notice = Some(format!("Could not save {}: {e}", self.path.display()));
                self.dirty = false; // don't spin; next edit retries
            }
        }
    }

    // ------------------------------------------------------------------ sending

    fn send(&mut self, ctx: &egui::Context, id: u64) {
        let Some(req) = self.store.find_request(id).cloned() else {
            return;
        };
        let (tx, rx) = channel();
        let agent = self.agent.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = http::send(&agent, &req);
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.inflight.insert(id, (Instant::now(), rx));
        if self.resp_tab == RespTab::LoadTest {
            self.resp_tab = RespTab::Body;
        }
    }

    fn poll_inflight(&mut self) {
        let mut done = Vec::new();
        for (id, (_, rx)) in &self.inflight {
            match rx.try_recv() {
                Ok(r) => done.push((*id, Some(r))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => done.push((*id, None)),
                Err(_) => {}
            }
        }
        for (id, r) in done {
            self.inflight.remove(&id);
            let result = r.unwrap_or_else(|| Err("Request thread stopped unexpectedly".into()));
            self.responses.insert(
                id,
                ResponseView {
                    result,
                    galley: None,
                },
            );
        }
    }

    // ------------------------------------------------------------------ actions

    fn apply(&mut self, a: Action) {
        match a {
            Action::Select(id) => {
                self.store.selected = Some(id);
                self.beautify_err = None;
                self.curl_err = None;
                self.touch();
            }
            Action::Toggle(fid) => {
                if let Some(fi) = self.store.folder_index(fid) {
                    self.store.folders[fi].expanded ^= true;
                    self.touch();
                }
            }
            Action::StartRename(item) => {
                let text = match item {
                    Item::Folder(id) => self
                        .store
                        .folder_index(id)
                        .map(|i| self.store.folders[i].name.clone()),
                    Item::Request(id) => self.store.find_request(id).map(|r| r.name.clone()),
                };
                if let Some(text) = text {
                    self.rename = Some(Rename {
                        item,
                        text,
                        focus: true,
                    });
                }
            }
            Action::NewFolder => {
                let id = self.store.new_id();
                self.store.folders.push(Folder {
                    id,
                    name: "New folder".into(),
                    ..Default::default()
                });
                self.rename = Some(Rename {
                    item: Item::Folder(id),
                    text: "New folder".into(),
                    focus: true,
                });
                self.touch();
            }
            Action::ImportPostman => self.import_postman(),
            Action::NewRequest(fid) => {
                let Some(fi) = self.store.folder_index(fid) else {
                    return;
                };
                let id = self.store.new_id();
                let f = &mut self.store.folders[fi];
                f.expanded = true;
                f.requests.push(Request {
                    id,
                    name: "New request".into(),
                    ..Default::default()
                });
                self.store.selected = Some(id);
                self.rename = Some(Rename {
                    item: Item::Request(id),
                    text: "New request".into(),
                    focus: true,
                });
                self.touch();
            }
            Action::Duplicate(rid) => {
                let Some((fi, ri)) = self.store.locate_request(rid) else {
                    return;
                };
                let id = self.store.new_id();
                let mut copy = self.store.folders[fi].requests[ri].clone();
                copy.id = id;
                copy.name.push_str(" copy");
                self.store.folders[fi].requests.insert(ri + 1, copy);
                self.store.selected = Some(id);
                self.touch();
            }
            Action::AskDelete(item) => self.confirm_delete = Some(item),
        }
    }

    fn commit_rename(&mut self) {
        let Some(r) = self.rename.take() else { return };
        let name = r.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match r.item {
            Item::Folder(id) => {
                if let Some(i) = self.store.folder_index(id) {
                    self.store.folders[i].name = name;
                }
            }
            Item::Request(id) => {
                if let Some(req) = self.store.find_request_mut(id) {
                    req.name = name;
                }
            }
        }
        self.touch();
    }

    fn delete(&mut self, item: Item) {
        match item {
            Item::Folder(id) => {
                if let Some(i) = self.store.folder_index(id) {
                    let f = self.store.folders.remove(i);
                    for r in &f.requests {
                        self.responses.remove(&r.id);
                        self.loads.remove(&r.id);
                        if self.store.selected == Some(r.id) {
                            self.store.selected = None;
                        }
                    }
                }
            }
            Item::Request(id) => {
                if let Some((fi, ri)) = self.store.locate_request(id) {
                    self.store.folders[fi].requests.remove(ri);
                    self.responses.remove(&id);
                    self.loads.remove(&id);
                    if self.store.selected == Some(id) {
                        self.store.selected = None;
                    }
                }
            }
        }
        self.touch();
    }

    fn apply_drop(&mut self, item: Item, target: DropTarget) {
        match (item, target) {
            (Item::Folder(id), DropTarget::FolderAt(mut to)) => {
                let Some(from) = self.store.folder_index(id) else {
                    return;
                };
                if to > from {
                    to -= 1;
                }
                if to != from {
                    let f = self.store.folders.remove(from);
                    self.store
                        .folders
                        .insert(to.min(self.store.folders.len()), f);
                    self.touch();
                }
            }
            (Item::Request(id), DropTarget::RequestAt { folder, mut index }) => {
                let Some((sf, si)) = self.store.locate_request(id) else {
                    return;
                };
                if sf == folder && (index == si || index == si + 1) {
                    return;
                }
                let r = self.store.folders[sf].requests.remove(si);
                if sf == folder && index > si {
                    index -= 1;
                }
                let dest = &mut self.store.folders[folder];
                dest.expanded = true;
                dest.requests.insert(index.min(dest.requests.len()), r);
                self.touch();
            }
            (Item::Request(id), DropTarget::IntoFolder(folder)) => {
                let Some((sf, si)) = self.store.locate_request(id) else {
                    return;
                };
                if sf == folder {
                    return;
                }
                let r = self.store.folders[sf].requests.remove(si);
                let dest = &mut self.store.folders[folder];
                dest.expanded = true;
                dest.requests.push(r);
                self.touch();
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        let mut actions = Vec::new();

        // Brand + primary sidebar action.
        ui.horizontal(|ui| {
            ui.set_height(size::CONTROL);
            ui.spacing_mut().item_spacing.x = space::SM;
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new((logo.id(), vec2(20.0, 20.0))));
            }
            ui.label(RichText::new("Kolibri").font(font::title()).color(p.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::icon_button(ui, Icon::FolderPlus, "New folder").clicked() {
                    actions.push(Action::NewFolder);
                }
                if kit::icon_button(ui, Icon::Import, "Import Postman collection").clicked() {
                    actions.push(Action::ImportPostman);
                }
            });
        });
        ui.add_space(space::MD);
        ui.horizontal(|ui| {
            ui.add_space(space::SM);
            kit::overline(ui, "Collections");
        });
        ui.add_space(space::XS);

        let mut rows: Vec<(Rect, RowRef)> = Vec::new();
        let mut commit = false;
        let mut cancel = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if self.store.folders.is_empty() {
                    ui.add_space(space::XL);
                    kit::empty_state(
                        ui,
                        Icon::Folder,
                        "No collections",
                        "Create a folder to organize requests.",
                        &[],
                    );
                    ui.add_space(space::MD);
                    ui.vertical_centered(|ui| {
                        if ui
                            .add(kit::Button::secondary("New folder").icon(Icon::FolderPlus))
                            .clicked()
                        {
                            actions.push(Action::NewFolder);
                        }
                    });
                }
                for fi in 0..self.store.folders.len() {
                    let folder = &self.store.folders[fi];
                    let fid = folder.id;
                    let renaming = matches!(&self.rename, Some(r) if r.item == Item::Folder(fid));
                    if renaming {
                        let (rect, c, x) = rename_row(ui, self.rename.as_mut().unwrap(), 26.0);
                        commit |= c;
                        cancel |= x;
                        rows.push((rect, RowRef::Folder(fi)));
                    } else {
                        let dragging_this = self.drag == Some(Item::Folder(fid));
                        let resp = folder_row(ui, folder, dragging_this);
                        rows.push((resp.rect, RowRef::Folder(fi)));
                        let plus = Rect::from_min_size(
                            pos2(resp.rect.right() - 28.0, resp.rect.top()),
                            vec2(28.0, resp.rect.height()),
                        );
                        if resp.clicked() {
                            let on_plus = resp
                                .interact_pointer_pos()
                                .is_some_and(|p| plus.contains(p));
                            actions.push(if on_plus {
                                Action::NewRequest(fid)
                            } else {
                                Action::Toggle(fid)
                            });
                        }
                        if resp.double_clicked() {
                            actions.push(Action::StartRename(Item::Folder(fid)));
                        }
                        if resp.drag_started() {
                            self.drag = Some(Item::Folder(fid));
                        }
                        resp.context_menu(|ui| {
                            if kit::menu_item(ui, Some(Icon::Plus), "New request", false).clicked()
                            {
                                actions.push(Action::NewRequest(fid));
                                ui.close();
                            }
                            if kit::menu_item(ui, Some(Icon::Pencil), "Rename", false).clicked() {
                                actions.push(Action::StartRename(Item::Folder(fid)));
                                ui.close();
                            }
                            kit::menu_separator(ui);
                            if kit::menu_item(ui, Some(Icon::Trash), "Delete folder", true)
                                .clicked()
                            {
                                actions.push(Action::AskDelete(Item::Folder(fid)));
                                ui.close();
                            }
                        });
                    }

                    let folder = &self.store.folders[fi];
                    if !folder.expanded {
                        continue;
                    }
                    if folder.requests.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), size::ROW),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(rect.left() + 34.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            "No requests yet",
                            font::small(),
                            p.text_faint,
                        );
                    }
                    for ri in 0..folder.requests.len() {
                        let req = &self.store.folders[fi].requests[ri];
                        let rid = req.id;
                        let renaming =
                            matches!(&self.rename, Some(r) if r.item == Item::Request(rid));
                        if renaming {
                            let (rect, c, x) = rename_row(ui, self.rename.as_mut().unwrap(), 30.0);
                            commit |= c;
                            cancel |= x;
                            rows.push((rect, RowRef::Request(fi, ri)));
                            continue;
                        }
                        let selected = self.store.selected == Some(rid);
                        let busy = self.inflight.contains_key(&rid)
                            || self.loads.get(&rid).is_some_and(|l| l.is_running());
                        let resp = request_row(
                            ui,
                            req,
                            selected,
                            busy,
                            self.drag == Some(Item::Request(rid)),
                        );
                        rows.push((resp.rect, RowRef::Request(fi, ri)));
                        #[cfg(feature = "qa")]
                        if fi == 0 && ri == 0 {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(Id::unique("qa_first_request_row"), resp.rect)
                            });
                        }
                        if resp.clicked() {
                            actions.push(Action::Select(rid));
                        }
                        if resp.double_clicked() {
                            actions.push(Action::StartRename(Item::Request(rid)));
                        }
                        if resp.drag_started() {
                            self.drag = Some(Item::Request(rid));
                        }
                        resp.context_menu(|ui| {
                            if kit::menu_item(ui, Some(Icon::Pencil), "Rename", false).clicked() {
                                actions.push(Action::StartRename(Item::Request(rid)));
                                ui.close();
                            }
                            if kit::menu_item(ui, Some(Icon::Copy), "Duplicate", false).clicked() {
                                actions.push(Action::Duplicate(rid));
                                ui.close();
                            }
                            kit::menu_separator(ui);
                            if kit::menu_item(ui, Some(Icon::Trash), "Delete request", true)
                                .clicked()
                            {
                                actions.push(Action::AskDelete(Item::Request(rid)));
                                ui.close();
                            }
                        });
                    }
                    ui.add_space(space::XS);
                }
                ui.add_space(40.0);
            });

        if cancel {
            self.rename = None;
        } else if commit {
            self.commit_rename();
        }
        for a in actions {
            self.apply(a);
        }
        self.handle_drag(ui, &rows);
    }

    fn handle_drag(&mut self, ui: &mut Ui, rows: &[(Rect, RowRef)]) {
        let Some(item) = self.drag else { return };
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let (pointer, released, down) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.any_released(),
                i.pointer.primary_down(),
            )
        });
        let Some(pos) = pointer else {
            self.drag = None;
            return;
        };
        ctx.set_cursor_icon(CursorIcon::Grabbing);

        let accent = p.accent;
        let painter = ui.painter().clone();
        let line = |y: f32, x0: f32, x1: f32| {
            painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.0, accent));
            painter.circle_filled(pos2(x0, y), 3.0, accent);
        };
        let x0 = rows.first().map(|r| r.0.left()).unwrap_or(0.0);
        let x1 = rows.first().map(|r| r.0.right()).unwrap_or(0.0);

        let mut target = None;
        match item {
            Item::Folder(_) => {
                // A folder "block" is its header plus its visible requests.
                let mut blocks: Vec<Rect> = Vec::new();
                for (rect, r) in rows {
                    match r {
                        RowRef::Folder(_) => blocks.push(*rect),
                        RowRef::Request(..) => {
                            if let Some(b) = blocks.last_mut() {
                                *b = b.union(*rect);
                            }
                        }
                    }
                }
                if !blocks.is_empty() {
                    let idx = blocks.iter().filter(|b| b.center().y < pos.y).count();
                    let y = if idx < blocks.len() {
                        blocks[idx].top() - 1.0
                    } else {
                        blocks[idx - 1].bottom() + 1.0
                    };
                    line(y, x0 + 4.0, x1 - 4.0);
                    target = Some(DropTarget::FolderAt(idx));
                }
            }
            Item::Request(_) => {
                let hit = rows
                    .iter()
                    .find(|(r, _)| pos.y >= r.top() - 1.0 && pos.y <= r.bottom() + 1.0);
                match hit {
                    Some((rect, RowRef::Request(fi, ri))) => {
                        let before = pos.y < rect.center().y;
                        let y = if before {
                            rect.top() - 1.0
                        } else {
                            rect.bottom() + 1.0
                        };
                        line(y, rect.left() + 28.0, rect.right() - 4.0);
                        target = Some(DropTarget::RequestAt {
                            folder: *fi,
                            index: if before { *ri } else { ri + 1 },
                        });
                    }
                    Some((rect, RowRef::Folder(fi))) => {
                        painter.rect(
                            *rect,
                            CornerRadius::same(radius::MD),
                            p.tint(accent),
                            Stroke::new(1.0, accent),
                            StrokeKind::Inside,
                        );
                        target = Some(DropTarget::IntoFolder(*fi));
                    }
                    None => {}
                }
            }
        }

        // Floating chip under the cursor.
        let label = match item {
            Item::Folder(id) => self
                .store
                .folder_index(id)
                .map(|i| self.store.folders[i].name.clone()),
            Item::Request(id) => self.store.find_request(id).map(|r| r.name.clone()),
        };
        if let Some(label) = label {
            let lp = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                Id::unique("kolibri-drag"),
            ));
            let galley = lp.layout_no_wrap(label, font::label(), p.text);
            let rect = Rect::from_min_size(pos + vec2(14.0, 8.0), galley.size() + vec2(20.0, 12.0));
            lp.add(
                egui::Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: p.shadow,
                }
                .as_shape(rect, CornerRadius::same(radius::MD)),
            );
            lp.rect(
                rect,
                CornerRadius::same(radius::MD),
                p.bg_elevated,
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Inside,
            );
            lp.galley(rect.min + vec2(10.0, 6.0), galley, p.text);
        }

        if released || !down {
            if let Some(t) = target {
                self.apply_drop(item, t);
            }
            self.drag = None;
        }
    }

    // ------------------------------------------------------------------ main area

    fn main_area(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let Some(id) = self
            .store
            .selected
            .filter(|id| self.store.find_request(*id).is_some())
        else {
            self.no_selection(ui);
            return;
        };

        let mut send_now = ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter))
        });
        let beautify_key = ctx
            .input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)));
        let mut cancel_send = false;
        let mut changed = false;
        let busy = self.inflight.contains_key(&id);

        let (fi, _) = self.store.locate_request(id).unwrap();
        let folder_name = self.store.folders[fi].name.clone();
        let mut dark = !self.store.light_mode;
        let mut theme_changed = false;
        let req = self.store.find_request_mut(id).unwrap();

        // Header: breadcrumb on the left, theme icon on the right (above Send).
        ui.horizontal(|ui| {
            ui.set_height(size::CONTROL);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let toggle = kit::theme_toggle(ui, &mut dark);
                theme_changed = toggle.changed();
                #[cfg(feature = "qa")]
                ui.ctx().data_mut(|d| d.insert_temp(Id::unique("qa_theme_toggle"), toggle.rect));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    kit::icon(ui, Icon::Folder, size::ICON, p.text_faint);
                    ui.label(
                        RichText::new(&folder_name)
                            .font(font::body())
                            .color(p.text_muted),
                    );
                    kit::icon(ui, Icon::ChevronRight, size::ICON_SM, p.text_faint);
                    ui.add(
                        Label::new(RichText::new(&req.name).font(font::title()).color(p.text))
                            .truncate(),
                    );
                });
            });
        });
        ui.add_space(space::SM);

        // URL bar: [method ▾ | url .................] [Send ⌘↵]
        let url_id = Id::unique("kolibri-url-bar");
        ui.horizontal(|ui| {
            let h = size::CONTROL_LG;
            let send_w = SEND_W;
            let w = (ui.available_width() - send_w - space::SM).max(220.0);
            let (bar, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            let focused = ui.memory(|m| m.has_focus(url_id));
            ui.painter().rect(
                bar,
                CornerRadius::same(radius::LG),
                p.bg_surface,
                Stroke::new(1.0, if focused { p.accent } else { p.border_strong }),
                StrokeKind::Inside,
            );
            if focused {
                ui.painter().rect_stroke(
                    bar.expand(2.0),
                    CornerRadius::same(radius::LG + 2),
                    Stroke::new(2.0, p.accent.gamma_multiply(0.25)),
                    StrokeKind::Outside,
                );
            }

            // Method picker.
            let mrect = Rect::from_min_size(bar.min, vec2(100.0, h));
            let mresp = ui.interact(mrect, ui.scope_id().with("method_picker"), Sense::click());
            #[cfg(feature = "qa")]
            ui.ctx()
                .data_mut(|d| d.insert_temp(Id::unique("qa_method_picker"), mrect));
            if mresp.hovered() {
                ui.painter().rect_filled(
                    mrect.shrink(3.0),
                    CornerRadius::same(radius::MD),
                    p.bg_hover,
                );
            }
            ui.painter().text(
                pos2(mrect.left() + 14.0, mrect.center().y),
                Align2::LEFT_CENTER,
                req.method.as_str(),
                font::label(),
                p.method(req.method),
            );
            kit::paint_icon(
                ui.painter(),
                Rect::from_center_size(
                    pos2(mrect.right() - 16.0, mrect.center().y),
                    vec2(12.0, 12.0),
                ),
                Icon::ChevronDown,
                p.text_faint,
            );
            ui.painter().vline(
                mrect.right(),
                bar.y_range().shrink(8.0),
                Stroke::new(1.0, p.border_strong),
            );
            let mresp = mresp
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("HTTP method");
            egui::Popup::menu(&mresp).width(160.0).show(|ui| {
                for m in Method::ALL {
                    let (r, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), size::CONTROL),
                        Sense::click(),
                    );
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(r, CornerRadius::same(radius::MD), p.bg_hover);
                    }
                    ui.painter().text(
                        pos2(r.left() + 10.0, r.center().y),
                        Align2::LEFT_CENTER,
                        m.as_str(),
                        font::label(),
                        p.method(m),
                    );
                    if m == req.method {
                        kit::paint_icon(
                            ui.painter(),
                            Rect::from_center_size(
                                pos2(r.right() - 16.0, r.center().y),
                                vec2(14.0, 14.0),
                            ),
                            Icon::Check,
                            p.text_muted,
                        );
                    }
                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && req.method != m {
                        req.method = m;
                        changed = true;
                    }
                }
            });

            // A pasted curl command replaces the whole request. Take the paste
            // event before the single-line editor sees it (it would turn
            // newlines into spaces and break line continuations).
            let mut pasted_curl = None;
            if focused {
                ui.input_mut(|i| {
                    if let Some(pos) = i
                        .events
                        .iter()
                        .position(|e| matches!(e, egui::Event::Paste(t) if curl::is_curl(t)))
                    {
                        if let egui::Event::Paste(t) = i.events.remove(pos) {
                            pasted_curl = Some(t);
                        }
                    }
                });
            }
            if let Some(cmd) = pasted_curl {
                match curl::parse(&cmd) {
                    Ok(imported) => {
                        imported.apply_to(req);
                        self.curl_err = None;
                        changed = true;
                    }
                    Err(e) => self.curl_err = Some(format!("cURL import failed: {e}")),
                }
            }
            let trect = Rect::from_min_max(
                pos2(mrect.right() + 1.0, bar.top()),
                pos2(bar.right() - 4.0, bar.bottom()),
            );
            let url = ui.put(
                trect,
                TextEdit::singleline(&mut req.url)
                    .id(url_id)
                    .frame(Frame::NONE.inner_margin(Margin::symmetric(10, 0)))
                    .hint_text("https://api.example.com/resource   or paste a cURL command")
                    .font(font::mono())
                    .desired_width(f32::INFINITY)
                    .min_size(trect.size())
                    .align(Align2::LEFT_CENTER),
            );
            if url.changed() {
                model::params_from_url(req);
                self.curl_err = None;
                changed = true;
            }
            if url.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                send_now = true;
            }

            if busy {
                let b = kit::Button::secondary("Cancel")
                    .icon(Icon::Stop)
                    .height(h)
                    .min_width(ui.available_width().max(send_w));
                if ui.add(b).clicked() {
                    cancel_send = true;
                }
            } else {
                let hint = shortcut("↵");
                let b = kit::Button::primary("Send")
                    .icon(Icon::Send)
                    .shortcut(&hint)
                    .height(h)
                    .min_width(ui.available_width().max(send_w));
                if ui
                    .add(b)
                    .on_hover_text(format!("Send request ({hint})"))
                    .clicked()
                {
                    send_now = true;
                }
            }
        });
        if let Some(e) = &self.curl_err {
            ui.add_space(space::SM);
            kit::callout(
                ui,
                p.danger,
                Icon::ErrorCircle,
                "Could not import cURL command",
                e.trim_start_matches("cURL import failed: "),
            );
        }
        ui.add_space(space::LG);

        // Response at the bottom (resizable), request editor fills the rest.
        let avail_h = ui.available_height();
        let mut start_load = false;
        let mut stop_load = false;
        egui::Panel::bottom("response_panel")
            .resizable(true)
            .default_size(avail_h * 0.55)
            .min_size(160.0)
            .max_size((avail_h - 150.0).max(160.0))
            .frame(Frame::NONE.inner_margin(Margin {
                top: space::MD as i8,
                bottom: space::MD as i8,
                ..Default::default()
            }))
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                response_area(
                    ui,
                    id,
                    &mut self.resp_tab,
                    &mut self.responses,
                    &self.inflight,
                    &self.loads,
                    &mut self.load_cfg,
                    &mut self.load_cache,
                    &mut start_load,
                    &mut stop_load,
                    &mut cancel_send,
                );
            });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let np = req
                .params
                .iter()
                .filter(|p| p.enabled && !p.key.is_empty())
                .count();
            let nh = req
                .headers
                .iter()
                .filter(|p| p.enabled && !p.key.is_empty())
                .count();
            let nb = usize::from(req.body_kind != BodyKind::None && !req.body.is_empty());
            let mut do_beautify = beautify_key;
            ui.horizontal(|ui| {
                kit::segmented(
                    ui,
                    "req_tabs",
                    &mut self.req_tab,
                    &[
                        seg(ReqTab::Params, "Params", np),
                        seg(ReqTab::Headers, "Headers", nh),
                        seg(ReqTab::Body, "Body", nb),
                    ],
                );
                if self.req_tab == ReqTab::Body {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enabled =
                            req.body_kind != BodyKind::None && !req.body.trim().is_empty();
                        let hint = shortcut("B");
                        let b = kit::Button::ghost("Beautify")
                            .icon(Icon::Braces)
                            .shortcut(&hint)
                            .enabled(enabled);
                        if ui.add(b).on_hover_text("Format the JSON body").clicked() {
                            do_beautify = true;
                        }
                        ui.add_space(space::XS);
                        changed |= kit::segmented(
                            ui,
                            "body_kind",
                            &mut req.body_kind,
                            &[
                                seg(BodyKind::None, "None", 0),
                                seg(BodyKind::Json, "JSON", 0),
                                seg(BodyKind::Text, "Text", 0),
                            ],
                        );
                    });
                }
            });
            ui.add_space(space::MD);
            match self.req_tab {
                ReqTab::Params => {
                    if kv_table(ui, "params", &mut req.params, "Key", "Value") {
                        model::url_from_params(req);
                        changed = true;
                    }
                }
                ReqTab::Headers => {
                    changed |= kv_table(ui, "headers", &mut req.headers, "Header", "Value");
                }
                ReqTab::Body => {
                    if do_beautify && req.body_kind != BodyKind::None && !req.body.trim().is_empty()
                    {
                        match http::beautify_json(&req.body) {
                            Ok(s) => {
                                if s != req.body {
                                    req.body = s;
                                    changed = true;
                                }
                                self.beautify_err = None;
                            }
                            Err(e) => self.beautify_err = Some(e),
                        }
                    }
                    if let Some(e) = &self.beautify_err {
                        kit::callout(ui, p.warning, Icon::Alert, "Could not beautify the body", e);
                        ui.add_space(space::SM);
                    }
                    if req.body_kind == BodyKind::None {
                        ui.add_space(space::XL);
                        kit::empty_state(
                            ui,
                            Icon::Braces,
                            "No body",
                            "This request is sent without a body. Choose JSON or Text to add one.",
                            &[],
                        );
                    } else {
                        let json = req.body_kind == BodyKind::Json;
                        let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                            let mut job =
                                json_job(text.as_str(), font::mono(), ui.visuals().dark_mode, json);
                            job.wrap.max_width = wrap;
                            ui.fonts_mut(|f| f.layout_job(job))
                        };
                        let h = (ui.available_height() - space::MD).max(60.0);
                        kit::card(p).inner_margin(Margin::same(0)).show(ui, |ui| {
                            ScrollArea::vertical()
                                .id_salt("body_scroll")
                                .max_height(h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let r = ui.add(
                                        TextEdit::multiline(&mut req.body)
                                            .code_editor()
                                            .frame(
                                                Frame::NONE
                                                    .inner_margin(Margin::same(space::MD as i8)),
                                            )
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(12)
                                            .min_size(vec2(0.0, h))
                                            .hint_text(if json {
                                                "{\n  \"key\": \"value\"\n}"
                                            } else {
                                                "Request body"
                                            })
                                            .layouter(&mut layouter),
                                    );
                                    if r.changed() {
                                        changed = true;
                                        self.beautify_err = None;
                                    }
                                });
                        });
                    }
                }
            }
        });

        if theme_changed {
            self.set_dark(&ctx, dark);
        }
        if changed {
            self.touch();
        }
        if cancel_send {
            self.inflight.remove(&id);
        }
        if send_now && !busy {
            self.send(&ctx, id);
        }
        if stop_load {
            if let Some(r) = self.loads.get(&id) {
                r.stop();
            }
        }
        if start_load {
            let req = self.store.find_request(id).unwrap();
            match http::prepare(req) {
                Ok(p) => {
                    self.loads.insert(
                        id,
                        loadtest::Run::start(p, self.load_cfg.clone(), ctx.clone()),
                    );
                    self.load_cache = None;
                }
                Err(e) => {
                    self.responses.insert(
                        id,
                        ResponseView {
                            result: Err(e),
                            galley: None,
                        },
                    );
                    self.resp_tab = RespTab::Body;
                }
            }
        }
    }

    /// Main area when no request is selected.
    fn no_selection(&mut self, ui: &mut Ui) {
        let first_folder = self.store.folders.first().map(|f| f.id);
        let mut action = None;
        // Same header position as on the request screen.
        let mut dark = !self.store.light_mode;
        let toggled = ui
            .horizontal(|ui| {
                ui.set_height(size::CONTROL);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    kit::theme_toggle(ui, &mut dark).changed()
                })
                .inner
            })
            .inner;
        if toggled {
            let ctx = ui.ctx().clone();
            self.set_dark(&ctx, dark);
        }
        ui.add_space((ui.available_height() * 0.28).max(space::XL));
        let (title, body) = if first_folder.is_some() {
            (
                "No request selected",
                "Pick a request in the sidebar, or create a new one.",
            )
        } else {
            (
                "Welcome to Kolibri",
                "Create a folder to start collecting requests.",
            )
        };
        kit::empty_state(ui, Icon::Send, title, body, &[]);
        ui.add_space(space::LG);
        ui.vertical_centered(|ui| {
            // Center the button row by measuring it.
            let w = if first_folder.is_some() { 260.0 } else { 120.0 };
            ui.allocate_ui(vec2(w, size::CONTROL), |ui| {
                ui.horizontal(|ui| {
                    if let Some(fid) = first_folder {
                        if ui
                            .add(kit::Button::primary("New request").icon(Icon::Plus))
                            .clicked()
                        {
                            action = Some(Action::NewRequest(fid));
                        }
                    }
                    if ui
                        .add(kit::Button::secondary("New folder").icon(Icon::FolderPlus))
                        .clicked()
                    {
                        action = Some(Action::NewFolder);
                    }
                });
            });
        });
        if let Some(a) = action {
            self.apply(a);
        }
    }

    fn delete_modal(&mut self, ctx: &egui::Context) {
        let Some(item) = self.confirm_delete else {
            return;
        };
        let p = theme::palette_for(ctx.global_style().visuals.dark_mode);
        let (title, detail) = match item {
            Item::Folder(id) => match self.store.folder_index(id) {
                Some(i) => {
                    let f = &self.store.folders[i];
                    let n = f.requests.len();
                    (
                        format!("Delete folder “{}”?", f.name),
                        if n == 0 {
                            "The folder is empty. This can't be undone.".to_owned()
                        } else {
                            format!(
                                "This also deletes its {n} request{}. This can't be undone.",
                                if n == 1 { "" } else { "s" }
                            )
                        },
                    )
                }
                None => {
                    self.confirm_delete = None;
                    return;
                }
            },
            Item::Request(id) => match self.store.find_request(id) {
                Some(r) => (
                    format!("Delete request “{}”?", r.name),
                    "This can't be undone.".to_owned(),
                ),
                None => {
                    self.confirm_delete = None;
                    return;
                }
            },
        };
        let mut decision = None;
        let frame = Frame::new()
            .fill(p.bg_elevated)
            .stroke(Stroke::new(1.0, p.border_strong))
            .corner_radius(CornerRadius::same(radius::XL))
            .inner_margin(Margin::same(space::XL as i8))
            .shadow(egui::Shadow {
                offset: [0, 12],
                blur: 40,
                spread: 0,
                color: p.shadow,
            });
        let resp = egui::Modal::new(Id::unique("confirm_delete"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(if p.dark { 150 } else { 70 }))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                    ui.painter().rect(
                        r,
                        CornerRadius::same(radius::LG),
                        p.tint(p.danger),
                        Stroke::new(1.0, p.tint_border(p.danger)),
                        StrokeKind::Inside,
                    );
                    kit::paint_icon(
                        ui.painter(),
                        Rect::from_center_size(r.center(), vec2(18.0, 18.0)),
                        Icon::Trash,
                        p.danger,
                    );
                    ui.add_space(space::XS);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = space::XS;
                        ui.add(
                            Label::new(RichText::new(&title).font(font::heading()).color(p.text))
                                .wrap(),
                        );
                        ui.add(
                            Label::new(
                                RichText::new(&detail)
                                    .font(font::body())
                                    .color(p.text_muted),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.add_space(space::XL);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(kit::Button::danger("Delete").min_width(84.0))
                        .clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        decision = Some(true);
                    }
                    if ui
                        .add(kit::Button::secondary("Cancel").min_width(84.0))
                        .clicked()
                    {
                        decision = Some(false);
                    }
                });
            });
        if resp.should_close() && decision.is_none() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                self.confirm_delete = None;
                self.delete(item);
            }
            Some(false) => self.confirm_delete = None,
            None => {}
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let no_focus = ctx.memory(|m| m.focused().is_none());
        if !no_focus || self.rename.is_some() || self.confirm_delete.is_some() {
            return;
        }
        let (f2, del) = ctx.input(|i| (i.key_pressed(Key::F2), i.key_pressed(Key::Delete)));
        if let Some(id) = self.store.selected {
            if f2 {
                self.apply(Action::StartRename(Item::Request(id)));
            }
            if del {
                self.apply(Action::AskDelete(Item::Request(id)));
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    #[cfg(feature = "qa")]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        qa::raw_input(self, raw);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty {
            self.save_now();
        }
    }
}

impl App {
    /// One frame of the whole window.
    fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        #[cfg(feature = "qa")]
        qa::tick(self, &ctx);
        let want = if self.store.light_mode { egui::Theme::Light } else { egui::Theme::Dark };
        if ctx.theme() != want {
            ctx.set_theme(want);
        }
        let p = theme::palette(ui);
        self.poll_inflight();
        self.shortcuts(&ctx);

        if let Some(n) = self.notice.clone() {
            egui::Panel::top("notice")
                .show_separator_line(false)
                .frame(
                    Frame::new()
                        .fill(p.tint(p.warning))
                        .stroke(Stroke::new(1.0, p.tint_border(p.warning)))
                        .inner_margin(Margin::symmetric(space::LG as i8, 6)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        kit::icon(ui, Icon::Alert, size::ICON, p.warning);
                        ui.add(
                            Label::new(RichText::new(n).font(font::body()).color(p.text))
                                .truncate(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(kit::Button::ghost("Dismiss").small()).clicked() {
                                self.notice = None;
                            }
                        });
                    });
                });
        }

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(size::SIDEBAR_DEFAULT)
            .size_range(size::SIDEBAR_MIN..=size::SIDEBAR_MAX)
            .frame(Frame::new().fill(p.bg_sidebar).inner_margin(Margin {
                left: 10,
                right: 10,
                top: 12,
                bottom: 0,
            }))
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg_app).inner_margin(Margin {
                left: 20,
                right: 20,
                top: 14,
                bottom: 0,
            }))
            .show(ui, |ui| self.main_area(ui));

        self.delete_modal(&ctx);

        if self.dirty {
            if self.last_edit.elapsed() >= Duration::from_millis(400) {
                self.save_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(450));
            }
        }
    }
}

// ---------------------------------------------------------------------- widgets

fn row_bg(ui: &Ui, rect: Rect, p: &Palette, selected: bool, hovered: bool) {
    let cr = CornerRadius::same(radius::MD);
    if selected {
        ui.painter().rect_filled(rect, cr, p.bg_active);
    } else if hovered {
        ui.painter().rect_filled(rect, cr, p.bg_hover);
    }
}

fn folder_row(ui: &mut Ui, folder: &Folder, dragging: bool) -> egui::Response {
    let p = theme::palette(ui);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::ROW),
        Sense::click_and_drag(),
    );
    let hovered = resp.hovered() && !dragging;
    row_bg(ui, rect, p, false, hovered);
    let painter = ui.painter();
    let cy = rect.center().y;
    let chevron = if folder.expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    kit::paint_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 11.0, cy), vec2(11.0, 11.0)),
        chevron,
        p.text_faint,
    );
    kit::paint_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 28.0, cy), vec2(14.0, 14.0)),
        Icon::Folder,
        p.text_muted,
    );

    let color = if dragging { p.text_faint } else { p.text };
    let right_reserved = 34.0;
    kit::paint_text_clipped(
        ui,
        pos2(rect.left() + 42.0, cy),
        Align2::LEFT_CENTER,
        &folder.name,
        font::label(),
        color,
        rect.width() - 42.0 - right_reserved,
    );

    if resp.hovered() && !dragging {
        let r = Rect::from_center_size(pos2(rect.right() - 14.0, cy), vec2(22.0, 22.0));
        let on = resp.hover_pos().is_some_and(|h| r.expand(3.0).contains(h));
        if on {
            ui.painter()
                .rect_filled(r, CornerRadius::same(radius::SM), p.bg_active);
        }
        kit::paint_icon(
            ui.painter(),
            Rect::from_center_size(r.center(), vec2(13.0, 13.0)),
            Icon::Plus,
            if on { p.text } else { p.text_muted },
        );
    } else {
        ui.painter().text(
            pos2(rect.right() - 10.0, cy),
            Align2::RIGHT_CENTER,
            folder.requests.len().to_string(),
            font::small(),
            p.text_faint,
        );
    }
    let resp = if resp.hovered() {
        resp.on_hover_text_at_pointer("Click + to add a request")
    } else {
        resp
    };
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn request_row(
    ui: &mut Ui,
    req: &Request,
    selected: bool,
    busy: bool,
    dragging: bool,
) -> egui::Response {
    let p = theme::palette(ui);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::ROW),
        Sense::click_and_drag(),
    );
    row_bg(ui, rect, p, selected, resp.hovered() && !dragging);
    let cy = rect.center().y;
    let mc = if dragging {
        p.text_faint
    } else {
        p.method(req.method)
    };
    ui.painter().text(
        pos2(rect.left() + 28.0, cy),
        Align2::LEFT_CENTER,
        method_short(req.method),
        font::mono_small(),
        mc,
    );
    let tc = if dragging {
        p.text_faint
    } else if selected {
        p.text
    } else {
        mix(p.text, p.text_muted, 0.45)
    };
    let x = rect.left() + 68.0;
    let right = if busy { 22.0 } else { 8.0 };
    let name = if req.name.is_empty() {
        "Untitled"
    } else {
        &req.name
    };
    kit::paint_text_clipped(
        ui,
        pos2(x, cy),
        Align2::LEFT_CENTER,
        name,
        font::body(),
        tc,
        rect.right() - right - x,
    );
    if busy {
        ui.painter()
            .circle_filled(pos2(rect.right() - 11.0, cy), 3.5, p.accent);
    }
    resp
}

/// Returns (rect, commit, cancel).
fn rename_row(ui: &mut Ui, r: &mut Rename, indent: f32) -> (Rect, bool, bool) {
    let p = theme::palette(ui);
    let mut commit = false;
    let mut cancel = false;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), size::ROW), Sense::hover());
    let field = Rect::from_min_max(
        pos2(rect.left() + indent, rect.top() + 2.0),
        pos2(rect.right() - 2.0, rect.bottom() - 2.0),
    );
    let te = ui.put(
        field,
        TextEdit::singleline(&mut r.text)
            .frame(
                Frame::new()
                    .fill(p.bg_surface)
                    .stroke(Stroke::new(1.0, p.accent))
                    .corner_radius(CornerRadius::same(radius::MD))
                    .inner_margin(Margin::symmetric(8, 0)),
            )
            .font(font::body())
            .desired_width(f32::INFINITY)
            .min_size(field.size())
            .align(Align2::LEFT_CENTER),
    );
    if r.focus {
        te.request_focus();
        if let Some(mut st) = TextEdit::load_state(ui.ctx(), te.id) {
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(r.text.chars().count()),
            );
            st.cursor.set_char_range(Some(all));
            st.store(ui.ctx(), te.id);
        }
        r.focus = false;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        cancel = true;
    } else if te.lost_focus() {
        commit = true;
    }
    (rect, commit, cancel)
}

/// Editable key/value table with an always-present blank row at the end.
fn kv_table(
    ui: &mut Ui,
    id: &str,
    rows: &mut Vec<KeyValue>,
    key_hint: &str,
    val_hint: &str,
) -> bool {
    let p = theme::palette(ui);
    if rows.last().is_none_or(|r| !r.is_blank()) {
        rows.push(KeyValue::default());
    }
    let mut changed = false;
    let mut remove = None;
    let last = rows.len() - 1;
    ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width();
            let check_w = 36.0;
            let del_w = 36.0;
            let col = ((w - check_w - del_w) / 2.0).floor();
            let head_h = 30.0;
            let row_h = size::TABLE_ROW;
            let top = ui.cursor().top();
            let table = Rect::from_min_size(
                pos2(ui.cursor().left(), top),
                vec2(w, head_h + row_h * rows.len() as f32),
            );

            // Header.
            let (head, _) = ui.allocate_exact_size(vec2(w, head_h), Sense::hover());
            ui.painter().rect_filled(
                head,
                CornerRadius {
                    nw: radius::LG,
                    ne: radius::LG,
                    sw: 0,
                    se: 0,
                },
                mix(p.bg_app, p.text, if p.dark { 0.03 } else { 0.025 }),
            );
            let hx = head.left() + check_w;
            for (x, label) in [(hx + 10.0, key_hint), (hx + col + 10.0, val_hint)] {
                ui.painter().text(
                    pos2(x, head.center().y),
                    Align2::LEFT_CENTER,
                    label.to_uppercase(),
                    font::overline(),
                    p.text_faint,
                );
            }

            for (i, row) in rows.iter_mut().enumerate() {
                let ghost = i == last;
                let (rr, _) = ui.allocate_exact_size(vec2(w, row_h), Sense::hover());
                ui.painter()
                    .hline(rr.x_range(), rr.top(), Stroke::new(1.0, p.border));
                let hovered = ui.rect_contains_pointer(rr);
                if hovered && !ghost {
                    ui.painter().rect_filled(
                        rr.shrink2(vec2(1.0, 0.0)).with_min_y(rr.top() + 1.0),
                        CornerRadius::ZERO,
                        mix(p.bg_app, p.bg_hover, 0.6),
                    );
                }
                let check = Rect::from_min_size(rr.min, vec2(check_w, row_h));
                let kcell = Rect::from_min_size(pos2(check.right(), rr.top()), vec2(col, row_h));
                let vcell = Rect::from_min_size(pos2(kcell.right(), rr.top()), vec2(col, row_h));
                let dcell = Rect::from_min_max(pos2(vcell.right(), rr.top()), rr.max);
                ui.painter()
                    .vline(kcell.left(), rr.y_range(), Stroke::new(1.0, p.border));
                ui.painter()
                    .vline(vcell.left(), rr.y_range(), Stroke::new(1.0, p.border));

                if !ghost {
                    changed |= kit::checkbox_at(
                        ui,
                        check,
                        ui.scope_id().with((id, "check", i)),
                        &mut row.enabled,
                        "Include",
                    )
                    .changed();
                } else {
                    kit::paint_icon(
                        ui.painter(),
                        Rect::from_center_size(check.center(), vec2(12.0, 12.0)),
                        Icon::Plus,
                        p.text_faint,
                    );
                }
                let text_color = if row.enabled || ghost {
                    p.text
                } else {
                    p.text_faint
                };
                for (cell, text, hint) in [
                    (kcell, &mut row.key, key_hint),
                    (vcell, &mut row.value, val_hint),
                ] {
                    let inner = cell.shrink2(vec2(1.0, 1.0));
                    let r = ui.put(
                        inner,
                        TextEdit::singleline(text)
                            .frame(Frame::NONE.inner_margin(Margin::symmetric(10, 0)))
                            .hint_text(hint)
                            .font(font::mono())
                            .text_color(text_color)
                            .desired_width(f32::INFINITY)
                            .min_size(inner.size())
                            .align(Align2::LEFT_CENTER),
                    );
                    if r.has_focus() {
                        ui.painter().rect_stroke(
                            inner,
                            CornerRadius::same(radius::SM),
                            Stroke::new(1.0, p.accent),
                            StrokeKind::Inside,
                        );
                    }
                    changed |= r.changed();
                }
                if !ghost {
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(dcell)
                            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
                    );
                    child.set_clip_rect(dcell.intersect(ui.clip_rect()));
                    let resp = kit::icon_button_sized(&mut child, Icon::Close, "Remove", 22.0);
                    if resp.clicked() {
                        remove = Some(i);
                    }
                }
            }
            ui.painter().rect_stroke(
                table,
                CornerRadius::same(radius::LG),
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
            ui.add_space(space::MD);
        });
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn response_area(
    ui: &mut Ui,
    id: u64,
    tab_state: &mut RespTab,
    responses: &mut HashMap<u64, ResponseView>,
    inflight: &HashMap<u64, (Instant, Receiver<HttpResult>)>,
    loads: &HashMap<u64, loadtest::Run>,
    cfg: &mut loadtest::Config,
    cache: &mut Option<(Instant, u64, loadtest::Summary)>,
    start_load: &mut bool,
    stop_load: &mut bool,
    cancel_send: &mut bool,
) {
    let p = theme::palette(ui);
    let view = responses.get_mut(&id);
    let ok = view.as_ref().and_then(|v| v.result.as_ref().ok());
    ui.horizontal(|ui| {
        ui.set_height(size::CONTROL);
        if ui.available_width() > 560.0 {
            ui.label(RichText::new("Response").font(font::label()).color(p.text));
            ui.add_space(space::XS);
        }
        kit::segmented(
            ui,
            "resp_tabs",
            tab_state,
            &[
                seg(RespTab::Body, "Body", 0),
                seg(
                    RespTab::Headers,
                    "Headers",
                    ok.map_or(0, |r| r.headers.len()),
                ),
                seg(RespTab::LoadTest, "Load test", 0),
            ],
        );
        if *tab_state != RespTab::LoadTest {
            if let Some(r) = ok {
                let room = ui.available_width();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if room > 250.0 {
                        kit::meta(ui, "Size", &fmt_size(r.size as u64));
                        ui.add_space(space::SM);
                        kit::meta(ui, "Time", &fmt_dur(r.elapsed));
                        ui.add_space(space::SM);
                    }
                    let text = if r.reason.is_empty() {
                        r.status.to_string()
                    } else {
                        format!("{} {}", r.status, r.reason)
                    };
                    kit::status_badge(ui, &text, p.status(r.status));
                });
            }
        }
    });
    ui.add_space(space::MD);

    if *tab_state == RespTab::LoadTest {
        load_test_ui(ui, id, loads.get(&id), cfg, cache, start_load, stop_load);
        return;
    }

    if let Some((started, _)) = inflight.get(&id) {
        ui.add_space((ui.available_height() * 0.25).min(60.0));
        ui.vertical_centered(|ui| {
            kit::spinner(ui, 22.0);
            ui.add_space(space::SM);
            ui.label(
                RichText::new("Sending request…")
                    .font(font::heading())
                    .color(p.text),
            );
            ui.label(
                RichText::new(format!("{:.1} s", started.elapsed().as_secs_f32()))
                    .font(font::mono())
                    .color(p.text_muted),
            );
            ui.add_space(space::SM);
            if ui
                .add(kit::Button::secondary("Cancel").icon(Icon::Stop).small())
                .clicked()
            {
                *cancel_send = true;
            }
        });
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        return;
    }
    let Some(view) = view else {
        ui.add_space((ui.available_height() * 0.22).min(56.0));
        let send = shortcut("↵");
        let beautify = shortcut("B");
        kit::empty_state(
            ui,
            Icon::Send,
            "No response yet",
            "Send the request to see its response here.",
            &[(&send, "Send request"), (&beautify, "Beautify JSON body")],
        );
        return;
    };
    let r = match &view.result {
        Ok(r) => r,
        Err(e) => {
            kit::callout(ui, p.danger, Icon::ErrorCircle, "Request failed", e);
            return;
        }
    };

    match *tab_state {
        RespTab::Body => {
            if r.display.is_empty() {
                ui.add_space(space::LG);
                kit::empty_state(
                    ui,
                    Icon::Inbox,
                    "Empty body",
                    "The server sent no content.",
                    &[],
                );
                return;
            }
            if r.truncated {
                kit::callout(
                    ui,
                    p.info,
                    Icon::Alert,
                    "Large response",
                    "Showing the first 4 MB of the body.",
                );
                ui.add_space(space::SM);
            }
            let dark = ui.visuals().dark_mode;
            let width = ui.available_width() - 2.0 * space::MD - 4.0;
            let stale =
                !matches!(&view.galley, Some((w, d, _)) if (*w - width).abs() < 0.5 && *d == dark);
            if stale {
                let highlight = r.is_json && r.display.len() < 2 * 1024 * 1024;
                let mut job = json_job(&r.display, font::mono(), dark, highlight);
                job.wrap.max_width = width;
                let g = ui.fonts_mut(|f| f.layout_job(job));
                view.galley = Some((width, dark, g));
            }
            let galley = view.galley.as_ref().unwrap().2.clone();
            let frame = kit::card(p).show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("resp_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(Label::new(galley).selectable(true));
                    });
            });
            // Copy button floats in the card's top-right corner.
            let r_btn = Rect::from_min_size(
                frame.response.rect.right_top() + vec2(-34.0, 8.0),
                vec2(26.0, 26.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r_btn));
            let copy = kit::icon_button_sized(&mut child, Icon::Copy, "Copy body", 26.0);
            if copy.clicked() {
                ui.ctx().copy_text(r.display.clone());
            }
        }
        RespTab::Headers => {
            kit::card(p).inner_margin(Margin::same(0)).show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("resp_headers")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let w = ui.available_width();
                        let key_w = (w * 0.32).clamp(140.0, 260.0);
                        for (i, (k, v)) in r.headers.iter().enumerate() {
                            if i > 0 {
                                kit::divider(ui);
                            }
                            Frame::new()
                                .inner_margin(Margin::symmetric(space::MD as i8, 7))
                                .show(ui, |ui| {
                                    ui.horizontal_top(|ui| {
                                        ui.allocate_ui_with_layout(
                                            vec2(key_w, 0.0),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                ui.set_width(key_w);
                                                ui.add(
                                                    Label::new(
                                                        RichText::new(k)
                                                            .font(font::mono())
                                                            .color(p.text_muted),
                                                    )
                                                    .selectable(true)
                                                    .wrap(),
                                                );
                                            },
                                        );
                                        ui.add(
                                            Label::new(
                                                RichText::new(v).font(font::mono()).color(p.text),
                                            )
                                            .selectable(true)
                                            .wrap(),
                                        );
                                    });
                                });
                        }
                    });
            });
        }
        RespTab::LoadTest => {}
    }
}

fn load_test_ui(
    ui: &mut Ui,
    id: u64,
    run: Option<&loadtest::Run>,
    cfg: &mut loadtest::Config,
    cache: &mut Option<(Instant, u64, loadtest::Summary)>,
    start: &mut bool,
    stop: &mut bool,
) {
    // One scroll area for the whole view, so it fits short windows.
    ScrollArea::vertical()
        .id_salt("load_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| load_test_body(ui, id, run, cfg, cache, start, stop));
}

fn load_test_body(
    ui: &mut Ui,
    id: u64,
    run: Option<&loadtest::Run>,
    cfg: &mut loadtest::Config,
    cache: &mut Option<(Instant, u64, loadtest::Summary)>,
    start: &mut bool,
    stop: &mut bool,
) {
    let p = theme::palette(ui);
    let running = run.is_some_and(|r| r.is_running());
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                kit::segmented(
                    ui,
                    "load_mode",
                    &mut cfg.mode,
                    &[
                        seg(loadtest::Mode::Count, "Requests", 0),
                        seg(loadtest::Mode::Duration, "Duration", 0),
                    ],
                );
                match cfg.mode {
                    loadtest::Mode::Count => {
                        ui.add_sized(
                            [96.0, size::CONTROL],
                            egui::DragValue::new(&mut cfg.total)
                                .range(1..=10_000_000)
                                .speed(5),
                        );
                    }
                    loadtest::Mode::Duration => {
                        ui.add_sized(
                            [96.0, size::CONTROL],
                            egui::DragValue::new(&mut cfg.seconds)
                                .range(1..=3600)
                                .suffix(" s"),
                        );
                    }
                }
                ui.add_space(space::SM);
                ui.label(
                    RichText::new("Concurrency")
                        .font(font::body())
                        .color(p.text_muted),
                );
                ui.add_sized(
                    [72.0, size::CONTROL],
                    egui::DragValue::new(&mut cfg.concurrency).range(1..=1000),
                );
            });
        });
        ui.add_space(space::SM);
        if running {
            if ui
                .add(kit::Button::secondary("Stop").icon(Icon::Stop))
                .clicked()
            {
                *stop = true;
            }
        } else if ui
            .add(kit::Button::primary("Start load test").icon(Icon::Bolt))
            .clicked()
        {
            *start = true;
        }
    });
    ui.add_space(space::XS);
    ui.label(
        RichText::new("Sends this exact request, as configured above, from parallel workers.")
            .font(font::small())
            .color(p.text_faint),
    );
    ui.add_space(space::MD);

    let Some(run) = run else {
        ui.add_space(space::LG);
        kit::empty_state(
            ui,
            Icon::Bolt,
            "No load test yet",
            "Pick a request count or duration, then start the test.",
            &[],
        );
        return;
    };
    let fresh = matches!(cache, Some((t, rid, _)) if *rid == id && t.elapsed() < Duration::from_millis(200));
    if !fresh {
        *cache = Some((Instant::now(), id, run.summary()));
    }
    let s = cache.as_ref().unwrap().2.clone();
    if s.running {
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    ui.horizontal(|ui| {
        let pct = format!("{:.0}%", s.progress * 100.0);
        let label_w = 44.0;
        ui.allocate_ui(vec2(ui.available_width() - label_w, 14.0), |ui| {
            ui.add_space(4.0);
            kit::progress(ui, s.progress, if s.running { p.accent } else { p.success });
        });
        ui.label(
            RichText::new(pct)
                .font(font::small_medium())
                .color(p.text_muted),
        );
    });
    ui.add_space(space::MD);

    {
        {
            // Metric cards wrap onto more rows when the window is narrow.
            let gap = space::SM;
            // Leave room for the floating scrollbar.
            let avail = ui.available_width() - 4.0;
            let per_row = ((avail + gap) / (150.0 + gap)).floor().clamp(1.0, 5.0);
            let card_w = ((avail - gap * (per_row - 1.0)) / per_row).floor();
            let failed_color = (s.failed > 0).then_some(p.danger);
            let metrics: [(&str, String, Option<Color32>); 5] = [
                ("Requests / sec", format!("{:.1}", s.rps), None),
                ("Completed", s.completed.to_string(), None),
                ("Failed", s.failed.to_string(), failed_color),
                ("Elapsed", format!("{:.2} s", s.elapsed.as_secs_f64()), None),
                ("Data received", fmt_size(s.bytes), None),
            ];
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, gap);
                for (label, value, color) in &metrics {
                    kit::metric(ui, label, value, *color, card_w);
                }
            });
            ui.add_space(gap);

            let two_col = avail > 560.0;
            let half = if two_col {
                ((avail - gap) / 2.0).floor()
            } else {
                avail
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, gap);
                // Latency distribution with proportional bars.
                kit::card(p).show(ui, |ui| {
                    ui.set_width(half - 2.0 * space::MD - 2.0);
                    ui.vertical(|ui| {
                        kit::overline(ui, "Latency");
                        ui.add_space(space::XS);
                        let max = s.max.as_secs_f64().max(1e-9);
                        for (k, v) in [
                            ("Min", s.min),
                            ("Average", s.avg),
                            ("p50", s.p50),
                            ("p90", s.p90),
                            ("p95", s.p95),
                            ("p99", s.p99),
                            ("Max", s.max),
                        ] {
                            ui.horizontal(|ui| {
                                ui.set_height(22.0);
                                let w = ui.available_width();
                                ui.allocate_ui(vec2(64.0, 22.0), |ui| {
                                    ui.set_width(64.0);
                                    ui.label(
                                        RichText::new(k).font(font::small()).color(p.text_muted),
                                    );
                                });
                                let bar_w = (w - 64.0 - 80.0 - 2.0 * space::SM).max(20.0);
                                let (bar, _) =
                                    ui.allocate_exact_size(vec2(bar_w, 6.0), Sense::hover());
                                ui.painter()
                                    .rect_filled(bar, CornerRadius::same(3), p.bg_active);
                                let f = (v.as_secs_f64() / max) as f32;
                                if f > 0.0 {
                                    let fill = Rect::from_min_size(
                                        bar.min,
                                        vec2((bar.width() * f).max(4.0), bar.height()),
                                    );
                                    ui.painter().rect_filled(
                                        fill,
                                        CornerRadius::same(3),
                                        p.accent.gamma_multiply(0.85),
                                    );
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(fmt_dur(v)).font(font::mono()).color(p.text),
                                    );
                                });
                            });
                        }
                    });
                });
                // Status code breakdown and errors.
                kit::card(p).show(ui, |ui| {
                    ui.set_width(half - 2.0 * space::MD - 2.0);
                    ui.vertical(|ui| {
                        kit::overline(ui, "Status codes");
                        ui.add_space(space::XS);
                        if s.statuses.is_empty() {
                            ui.label(
                                RichText::new("None yet")
                                    .font(font::small())
                                    .color(p.text_faint),
                            );
                        }
                        let total: u64 = s.statuses.iter().map(|(_, n)| *n).sum::<u64>().max(1);
                        for (code, n) in &s.statuses {
                            ui.horizontal(|ui| {
                                ui.set_height(24.0);
                                kit::badge(ui, &code.to_string(), p.status(*code));
                                ui.label(
                                    RichText::new(format!("{n}"))
                                        .font(font::mono())
                                        .color(p.text),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let pct = *n as f64 * 100.0 / total as f64;
                                    ui.label(
                                        RichText::new(format!("{pct:.1}%"))
                                            .font(font::small())
                                            .color(p.text_faint),
                                    );
                                });
                            });
                        }
                        if !s.errors.is_empty() {
                            ui.add_space(space::SM);
                            kit::overline(ui, "Errors");
                            ui.add_space(space::XS);
                            for (e, n) in &s.errors {
                                ui.add(
                                    Label::new(
                                        RichText::new(format!("{n} × {e}"))
                                            .font(font::small())
                                            .color(p.danger),
                                    )
                                    .wrap(),
                                );
                            }
                        }
                    });
                });
            });
            ui.add_space(space::MD);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let path = std::env::temp_dir().join(format!("kolibri-test-{}.json", std::process::id()));
        App::with_store(Store::starter(), path, None)
    }

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 780.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.draw(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn pasting_curl_into_url_bar_imports_it() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = test_app();
        frame(&ctx, &mut app, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::unique("kolibri-url-bar")));
        frame(&ctx, &mut app, vec![]);
        let cmd = "curl --url 'https://x.test/api/v1/items' \\\n  -H 'Accept: */*' \\\n  -b 'a=1; b=x|y$z' \\\n  --data-raw '{\"sortBy\":\"default\"}'\n";
        frame(&ctx, &mut app, vec![egui::Event::Paste(cmd.into())]);
        frame(&ctx, &mut app, vec![]);
        let id = app.store.selected.unwrap();
        let req = app.store.find_request(id).unwrap();
        assert_eq!(app.curl_err, None);
        assert_eq!(req.url, "https://x.test/api/v1/items");
        assert_eq!(req.method, Method::POST);
        assert_eq!(req.body, "{\"sortBy\":\"default\"}");
        assert_eq!(req.headers.len(), 3);
    }

    #[test]
    fn dark_by_default_and_toggle_is_saved() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = test_app();
        frame(&ctx, &mut app, vec![]);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        app.set_dark(&ctx, false);
        frame(&ctx, &mut app, vec![]);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(app.store.light_mode);
        assert!(app.dirty, "the choice is written to the data file");
    }

    #[test]
    fn every_screen_renders_in_both_themes() {
        for dark in [true, false] {
            let ctx = egui::Context::default();
            theme::install(&ctx);
            let mut app = test_app();
            app.store.light_mode = !dark;
            let id = app.store.selected.unwrap();
            for tab in [ReqTab::Params, ReqTab::Headers, ReqTab::Body] {
                app.req_tab = tab;
                frame(&ctx, &mut app, vec![]);
            }
            app.responses.insert(
                id,
                ResponseView {
                    result: Err("boom".into()),
                    galley: None,
                },
            );
            for tab in [RespTab::Body, RespTab::Headers, RespTab::LoadTest] {
                app.resp_tab = tab;
                frame(&ctx, &mut app, vec![]);
            }
            app.confirm_delete = Some(Item::Request(id));
            app.notice = Some("notice".into());
            frame(&ctx, &mut app, vec![]);
            app.confirm_delete = None;
            app.store.selected = None;
            frame(&ctx, &mut app, vec![]);
            app.store.folders.clear();
            frame(&ctx, &mut app, vec![]);
        }
    }
}
