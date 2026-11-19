use crate::curl;
use crate::highlight::{json_job, mono};
use crate::http::{self, HttpResult};
use crate::loadtest;
use crate::model::{self, BodyKind, Folder, KeyValue, Method, Request, Store};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CursorIcon, Galley, Id, Key, KeyboardShortcut, Label,
    Layout, Modifiers, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ROW_H: f32 = 24.0;

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
}

pub fn method_color(m: Method) -> Color32 {
    match m {
        Method::GET => Color32::from_rgb(0x3F, 0xB9, 0x50),
        Method::POST => Color32::from_rgb(0xE8, 0xA3, 0x17),
        Method::PUT => Color32::from_rgb(0x3B, 0x8E, 0xEA),
        Method::PATCH => Color32::from_rgb(0xA3, 0x71, 0xF7),
        Method::DELETE => Color32::from_rgb(0xF0, 0x52, 0x52),
        Method::HEAD | Method::OPTIONS => Color32::from_rgb(0x8B, 0x94, 0x9E),
    }
}

fn status_color(code: u16) -> Color32 {
    match code {
        200..=299 => Color32::from_rgb(0x3F, 0xB9, 0x50),
        300..=399 => Color32::from_rgb(0x3B, 0x8E, 0xEA),
        400..=499 => Color32::from_rgb(0xE8, 0xA3, 0x17),
        _ => Color32::from_rgb(0xF0, 0x52, 0x52),
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

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(8.0, 4.0);
        cc.egui_ctx.set_global_style(style);

        let path = model::data_path();
        let (store, notice) = model::load(&path);
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
        }
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
        let Some(req) = self.store.find_request(id).cloned() else { return };
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
            self.responses.insert(id, ResponseView { result, galley: None });
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
                    Item::Folder(id) => self.store.folder_index(id).map(|i| self.store.folders[i].name.clone()),
                    Item::Request(id) => self.store.find_request(id).map(|r| r.name.clone()),
                };
                if let Some(text) = text {
                    self.rename = Some(Rename { item, text, focus: true });
                }
            }
            Action::NewFolder => {
                let id = self.store.new_id();
                self.store.folders.push(Folder { id, name: "New folder".into(), ..Default::default() });
                self.rename = Some(Rename { item: Item::Folder(id), text: "New folder".into(), focus: true });
                self.touch();
            }
            Action::NewRequest(fid) => {
                let Some(fi) = self.store.folder_index(fid) else { return };
                let id = self.store.new_id();
                let f = &mut self.store.folders[fi];
                f.expanded = true;
                f.requests.push(Request { id, name: "New request".into(), ..Default::default() });
                self.store.selected = Some(id);
                self.rename = Some(Rename { item: Item::Request(id), text: "New request".into(), focus: true });
                self.touch();
            }
            Action::Duplicate(rid) => {
                let Some((fi, ri)) = self.store.locate_request(rid) else { return };
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
                let Some(from) = self.store.folder_index(id) else { return };
                if to > from {
                    to -= 1;
                }
                if to != from {
                    let f = self.store.folders.remove(from);
                    self.store.folders.insert(to.min(self.store.folders.len()), f);
                    self.touch();
                }
            }
            (Item::Request(id), DropTarget::RequestAt { folder, mut index }) => {
                let Some((sf, si)) = self.store.locate_request(id) else { return };
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
                let Some((sf, si)) = self.store.locate_request(id) else { return };
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
        let mut actions = Vec::new();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Kolibri").strong().size(16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("+ Folder").on_hover_text("New folder").clicked() {
                    actions.push(Action::NewFolder);
                }
            });
        });
        ui.separator();

        let mut rows: Vec<(Rect, RowRef)> = Vec::new();
        let mut commit = false;
        let mut cancel = false;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if self.store.folders.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No folders yet").weak());
                    if ui.button("Create a folder").clicked() {
                        actions.push(Action::NewFolder);
                    }
                });
            }
            for fi in 0..self.store.folders.len() {
                let folder = &self.store.folders[fi];
                let fid = folder.id;
                let renaming = matches!(&self.rename, Some(r) if r.item == Item::Folder(fid));
                if renaming {
                    let (rect, c, x) = rename_row(ui, self.rename.as_mut().unwrap(), 4.0);
                    commit |= c;
                    cancel |= x;
                    rows.push((rect, RowRef::Folder(fi)));
                } else {
                    let dragging_this = self.drag == Some(Item::Folder(fid));
                    let resp = folder_row(ui, folder, dragging_this);
                    rows.push((resp.rect, RowRef::Folder(fi)));
                    let plus = Rect::from_min_size(
                        pos2(resp.rect.right() - 26.0, resp.rect.top()),
                        vec2(26.0, resp.rect.height()),
                    );
                    if resp.clicked() {
                        let on_plus = resp.interact_pointer_pos().is_some_and(|p| plus.contains(p));
                        actions.push(if on_plus { Action::NewRequest(fid) } else { Action::Toggle(fid) });
                    }
                    if resp.double_clicked() {
                        actions.push(Action::StartRename(Item::Folder(fid)));
                    }
                    if resp.drag_started() {
                        self.drag = Some(Item::Folder(fid));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("New request").clicked() {
                            actions.push(Action::NewRequest(fid));
                            ui.close();
                        }
                        if ui.button("Rename").clicked() {
                            actions.push(Action::StartRename(Item::Folder(fid)));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Delete folder").clicked() {
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
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
                        ui.label(RichText::new("Empty. Click + to add a request.").weak().small());
                    });
                }
                for ri in 0..folder.requests.len() {
                    let req = &self.store.folders[fi].requests[ri];
                    let rid = req.id;
                    let renaming = matches!(&self.rename, Some(r) if r.item == Item::Request(rid));
                    if renaming {
                        let (rect, c, x) = rename_row(ui, self.rename.as_mut().unwrap(), 26.0);
                        commit |= c;
                        cancel |= x;
                        rows.push((rect, RowRef::Request(fi, ri)));
                        continue;
                    }
                    let selected = self.store.selected == Some(rid);
                    let busy = self.inflight.contains_key(&rid)
                        || self.loads.get(&rid).is_some_and(|l| l.is_running());
                    let resp = request_row(ui, req, selected, busy, self.drag == Some(Item::Request(rid)));
                    rows.push((resp.rect, RowRef::Request(fi, ri)));
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
                        if ui.button("Rename").clicked() {
                            actions.push(Action::StartRename(Item::Request(rid)));
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            actions.push(Action::Duplicate(rid));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Delete request").clicked() {
                            actions.push(Action::AskDelete(Item::Request(rid)));
                            ui.close();
                        }
                    });
                }
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
        let ctx = ui.ctx().clone();
        let (pointer, released, down) =
            ctx.input(|i| (i.pointer.interact_pos(), i.pointer.any_released(), i.pointer.primary_down()));
        let Some(p) = pointer else {
            self.drag = None;
            return;
        };
        ctx.set_cursor_icon(CursorIcon::Grabbing);

        let accent = ui.visuals().selection.stroke.color;
        let painter = ui.painter().clone();
        let line = |y: f32, x0: f32, x1: f32| {
            painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.0, accent));
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
                    let idx = blocks.iter().filter(|b| b.center().y < p.y).count();
                    let y = if idx < blocks.len() { blocks[idx].top() - 1.0 } else { blocks[idx - 1].bottom() + 1.0 };
                    line(y, x0 + 2.0, x1 - 2.0);
                    target = Some(DropTarget::FolderAt(idx));
                }
            }
            Item::Request(_) => {
                let hit = rows.iter().find(|(r, _)| p.y >= r.top() - 1.0 && p.y <= r.bottom() + 1.0);
                match hit {
                    Some((rect, RowRef::Request(fi, ri))) => {
                        let before = p.y < rect.center().y;
                        let y = if before { rect.top() - 1.0 } else { rect.bottom() + 1.0 };
                        line(y, rect.left() + 20.0, rect.right() - 2.0);
                        target = Some(DropTarget::RequestAt { folder: *fi, index: if before { *ri } else { ri + 1 } });
                    }
                    Some((rect, RowRef::Folder(fi))) => {
                        painter.rect_stroke(*rect, 4.0, Stroke::new(1.5, accent), egui::StrokeKind::Inside);
                        target = Some(DropTarget::IntoFolder(*fi));
                    }
                    None => {}
                }
            }
        }

        // Floating label under the cursor.
        let label = match item {
            Item::Folder(id) => self.store.folder_index(id).map(|i| self.store.folders[i].name.clone()),
            Item::Request(id) => self.store.find_request(id).map(|r| r.name.clone()),
        };
        if let Some(label) = label {
            let lp = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::unique("kolibri-drag")));
            let galley = lp.layout_no_wrap(label, egui::FontId::proportional(13.0), ui.visuals().strong_text_color());
            let rect = Rect::from_min_size(p + vec2(14.0, 6.0), galley.size() + vec2(16.0, 8.0));
            lp.rect_filled(rect, 4.0, ui.visuals().window_fill);
            lp.rect_stroke(rect, 4.0, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
            lp.galley(rect.min + vec2(8.0, 4.0), galley, Color32::WHITE);
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
        let ctx = ui.ctx().clone();
        let Some(id) = self.store.selected.filter(|id| self.store.find_request(*id).is_some()) else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Select a request on the left, or create a folder and press + to add one.").weak());
            });
            return;
        };

        let mut send_now = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter)));
        let beautify_key = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)));
        let mut cancel_send = false;
        let mut changed = false;
        let busy = self.inflight.contains_key(&id);

        let (fi, _) = self.store.locate_request(id).unwrap();
        let folder_name = self.store.folders[fi].name.clone();
        let req = self.store.find_request_mut(id).unwrap();

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&folder_name).weak());
            ui.label(RichText::new("/").weak());
            ui.label(RichText::new(&req.name).strong());
        });
        ui.add_space(2.0);

        // URL bar
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("method")
                .width(92.0)
                .selected_text(RichText::new(req.method.as_str()).color(method_color(req.method)).strong().monospace())
                .show_ui(ui, |ui| {
                    for m in Method::ALL {
                        let t = RichText::new(m.as_str()).color(method_color(m)).strong().monospace();
                        changed |= ui.selectable_value(&mut req.method, m, t).changed();
                    }
                });
            let send_w = 84.0;
            let w = (ui.available_width() - send_w - ui.spacing().item_spacing.x).max(100.0);
            // A pasted curl command replaces the whole request. Take the paste
            // event before the single-line editor sees it (it would turn
            // newlines into spaces and break line continuations).
            let url_id = Id::unique("kolibri-url-bar");
            let mut pasted_curl = None;
            if ui.memory(|m| m.has_focus(url_id)) {
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
            let url = ui.add(
                TextEdit::singleline(&mut req.url)
                    .id(url_id)
                    .hint_text("https://api.example.com/resource  (or paste a cURL command)")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(w)
                    .min_size(vec2(0.0, 26.0))
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
                if ui.add_sized([send_w, 26.0], egui::Button::new("Cancel")).clicked() {
                    cancel_send = true;
                }
            } else {
                let b = egui::Button::new(RichText::new("Send").strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(0x25, 0x63, 0xEB));
                if ui.add_sized([send_w, 26.0], b).on_hover_text("Ctrl+Enter").clicked() {
                    send_now = true;
                }
            }
        });
        if let Some(e) = &self.curl_err {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color).small());
        }
        ui.add_space(4.0);

        // Response at the bottom (resizable), request editor fills the rest.
        let avail_h = ui.available_height();
        let mut start_load = false;
        let mut stop_load = false;
        egui::Panel::bottom("response_panel")
            .resizable(true)
            .default_size(avail_h * 0.55)
            .min_size(120.0)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { top: 6, ..Default::default() }))
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
                );
            });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                let np = req.params.iter().filter(|p| p.enabled && !p.key.is_empty()).count();
                let nh = req.headers.iter().filter(|p| p.enabled && !p.key.is_empty()).count();
                tab(ui, &mut self.req_tab, ReqTab::Params, "Params", np);
                tab(ui, &mut self.req_tab, ReqTab::Headers, "Headers", nh);
                let nb = usize::from(req.body_kind != BodyKind::None && !req.body.is_empty());
                tab(ui, &mut self.req_tab, ReqTab::Body, "Body", nb);
            });
            ui.separator();
            match self.req_tab {
                ReqTab::Params => {
                    if kv_table(ui, "params", &mut req.params, "key", "value") {
                        model::url_from_params(req);
                        changed = true;
                    }
                }
                ReqTab::Headers => {
                    changed |= kv_table(ui, "headers", &mut req.headers, "Header", "Value");
                }
                ReqTab::Body => {
                    let mut do_beautify = beautify_key;
                    ui.horizontal(|ui| {
                        changed |= ui.radio_value(&mut req.body_kind, BodyKind::None, "None").changed();
                        changed |= ui.radio_value(&mut req.body_kind, BodyKind::Json, "JSON").changed();
                        changed |= ui.radio_value(&mut req.body_kind, BodyKind::Text, "Text").changed();
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let enabled = req.body_kind != BodyKind::None && !req.body.trim().is_empty();
                            if ui.add_enabled(enabled, egui::Button::new("Beautify")).on_hover_text("Format JSON (Ctrl+B)").clicked() {
                                do_beautify = true;
                            }
                            if let Some(e) = &self.beautify_err {
                                ui.label(RichText::new(e).color(ui.visuals().error_fg_color).small());
                            }
                        });
                    });
                    if do_beautify && req.body_kind != BodyKind::None && !req.body.trim().is_empty() {
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
                    if req.body_kind == BodyKind::None {
                        ui.add_space(8.0);
                        ui.label(RichText::new("This request has no body.").weak());
                    } else {
                        let json = req.body_kind == BodyKind::Json;
                        let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                            let mut job = json_job(text.as_str(), mono(ui), ui.visuals().dark_mode, json);
                            job.wrap.max_width = wrap;
                            ui.fonts_mut(|f| f.layout_job(job))
                        };
                        ScrollArea::vertical().id_salt("body_scroll").auto_shrink([false, false]).show(ui, |ui| {
                            let r = ui.add(
                                TextEdit::multiline(&mut req.body)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(14)
                                    .hint_text(if json { "{\n  \"key\": \"value\"\n}" } else { "Request body" })
                                    .layouter(&mut layouter),
                            );
                            if r.changed() {
                                changed = true;
                                self.beautify_err = None;
                            }
                        });
                    }
                }
            }
        });

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
                    self.loads.insert(id, loadtest::Run::start(p, self.load_cfg.clone(), ctx.clone()));
                    self.load_cache = None;
                }
                Err(e) => {
                    self.responses.insert(id, ResponseView { result: Err(e), galley: None });
                    self.resp_tab = RespTab::Body;
                }
            }
        }
    }

    fn delete_modal(&mut self, ctx: &egui::Context) {
        let Some(item) = self.confirm_delete else { return };
        let (title, detail) = match item {
            Item::Folder(id) => match self.store.folder_index(id) {
                Some(i) => {
                    let f = &self.store.folders[i];
                    (format!("Delete folder \"{}\"?", f.name), format!("This also deletes its {} request(s).", f.requests.len()))
                }
                None => {
                    self.confirm_delete = None;
                    return;
                }
            },
            Item::Request(id) => match self.store.find_request(id) {
                Some(r) => (format!("Delete request \"{}\"?", r.name), String::new()),
                None => {
                    self.confirm_delete = None;
                    return;
                }
            },
        };
        let mut decision = None;
        let resp = egui::Modal::new(Id::unique("confirm_delete")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(RichText::new(title).strong());
            if !detail.is_empty() {
                ui.label(detail);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let del = egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(Color32::from_rgb(0xC0, 0x39, 0x2B));
                if ui.add(del).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
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
        self.poll_inflight();
        self.shortcuts(&ctx);

        if let Some(n) = self.notice.clone() {
            egui::Panel::top("notice").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(n).color(ui.visuals().warn_fg_color));
                    if ui.small_button("Dismiss").clicked() {
                        self.notice = None;
                    }
                });
            });
        }

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(250.0)
            .min_size(160.0)
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default().show(ui, |ui| self.main_area(ui));

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

fn tab<T: PartialEq + Copy>(ui: &mut Ui, cur: &mut T, val: T, label: &str, count: usize) {
    let text = if count > 0 { format!("{label} ({count})") } else { label.to_owned() };
    let selected = *cur == val;
    let rt = if selected { RichText::new(text).strong() } else { RichText::new(text) };
    if ui.selectable_label(selected, rt).clicked() {
        *cur = val;
    }
}

fn x_button(ui: &mut Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let v = ui.visuals();
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, v.widgets.hovered.weak_bg_fill);
    }
    let c = rect.center();
    let d = 4.0;
    let color = if resp.hovered() { v.strong_text_color() } else { v.weak_text_color() };
    let st = Stroke::new(1.5, color);
    ui.painter().line_segment([c + vec2(-d, -d), c + vec2(d, d)], st);
    ui.painter().line_segment([c + vec2(-d, d), c + vec2(d, -d)], st);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn paint_row_bg(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let v = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 4.0, v.selection.bg_fill.gamma_multiply(0.6));
    } else if hovered {
        ui.painter().rect_filled(rect, 4.0, v.widgets.hovered.weak_bg_fill);
    }
}

fn folder_row(ui: &mut Ui, folder: &Folder, dragging: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    let hovered = resp.hovered() && !dragging;
    paint_row_bg(ui, rect, false, hovered);
    let v = ui.visuals();
    let color = if dragging { v.weak_text_color() } else { v.strong_text_color() };
    let painter = ui.painter_at(rect);

    // Disclosure triangle
    let c = pos2(rect.left() + 12.0, rect.center().y);
    let s = 4.0;
    let pts = if folder.expanded {
        vec![c + vec2(-s, -s * 0.6), c + vec2(s, -s * 0.6), c + vec2(0.0, s * 0.8)]
    } else {
        vec![c + vec2(-s * 0.6, -s), c + vec2(s * 0.8, 0.0), c + vec2(-s * 0.6, s)]
    };
    painter.add(egui::Shape::convex_polygon(pts, v.text_color(), Stroke::NONE));

    let font = egui::FontId::proportional(14.0);
    let name_clip = Rect::from_min_max(rect.min, pos2(rect.right() - 54.0, rect.bottom()));
    ui.painter_at(name_clip).text(pos2(rect.left() + 24.0, rect.center().y), Align2::LEFT_CENTER, &folder.name, font, color);
    painter.text(
        pos2(rect.right() - 30.0, rect.center().y),
        Align2::RIGHT_CENTER,
        folder.requests.len().to_string(),
        egui::FontId::proportional(11.0),
        v.weak_text_color(),
    );
    if resp.hovered() {
        painter.text(pos2(rect.right() - 13.0, rect.center().y), Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), v.strong_text_color());
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn request_row(ui: &mut Ui, req: &Request, selected: bool, busy: bool, dragging: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    paint_row_bg(ui, rect, selected, resp.hovered() && !dragging);
    let v = ui.visuals();
    let painter = ui.painter_at(rect);
    let mfont = egui::FontId::monospace(10.5);
    let mc = if dragging { v.weak_text_color() } else { method_color(req.method) };
    painter.text(pos2(rect.left() + 26.0, rect.center().y), Align2::LEFT_CENTER, req.method.as_str(), mfont, mc);
    let tc = if dragging { v.weak_text_color() } else if selected { v.strong_text_color() } else { v.text_color() };
    let right = if busy { rect.right() - 18.0 } else { rect.right() - 4.0 };
    ui.painter_at(Rect::from_min_max(rect.min, pos2(right, rect.bottom()))).text(
        pos2(rect.left() + 78.0, rect.center().y),
        Align2::LEFT_CENTER,
        &req.name,
        egui::FontId::proportional(13.5),
        tc,
    );
    if busy {
        painter.circle_filled(pos2(rect.right() - 9.0, rect.center().y), 3.5, Color32::from_rgb(0x25, 0x63, 0xEB));
    }
    resp
}

/// Returns (rect, commit, cancel).
fn rename_row(ui: &mut Ui, r: &mut Rename, indent: f32) -> (Rect, bool, bool) {
    let mut commit = false;
    let mut cancel = false;
    let inner = ui.horizontal(|ui| {
        ui.add_space(indent);
        let te = ui.add(TextEdit::singleline(&mut r.text).desired_width(ui.available_width() - 4.0));
        if r.focus {
            te.request_focus();
            if let Some(mut st) = TextEdit::load_state(ui.ctx(), te.id) {
                let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(r.text.chars().count()));
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
    });
    (inner.response.rect, commit, cancel)
}

/// Editable key/value table with an always-present blank row at the end.
fn kv_table(ui: &mut Ui, id: &str, rows: &mut Vec<KeyValue>, key_hint: &str, val_hint: &str) -> bool {
    if rows.last().is_none_or(|r| !r.is_blank()) {
        rows.push(KeyValue::default());
    }
    let mut changed = false;
    let mut remove = None;
    let last = rows.len() - 1;
    ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        let col = ((ui.available_width() - 64.0) / 2.0).max(80.0);
        for (i, row) in rows.iter_mut().enumerate() {
            let ghost = i == last;
            ui.horizontal(|ui| {
                if ghost {
                    let mut dummy = true;
                    ui.add_visible(false, egui::Checkbox::new(&mut dummy, ""));
                } else {
                    changed |= ui.checkbox(&mut row.enabled, "").on_hover_text("Include").changed();
                }
                let k = ui.add_sized(
                    [col, 24.0],
                    TextEdit::singleline(&mut row.key).hint_text(key_hint).font(egui::TextStyle::Monospace),
                );
                let v = ui.add_sized(
                    [col, 24.0],
                    TextEdit::singleline(&mut row.value).hint_text(val_hint).font(egui::TextStyle::Monospace),
                );
                changed |= k.changed() || v.changed();
                if !ghost && x_button(ui).on_hover_text("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
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
) {
    let view = responses.get_mut(&id);
    let ok = view.as_ref().and_then(|v| v.result.as_ref().ok());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Response").strong());
        ui.add_space(8.0);
        tab(ui, tab_state, RespTab::Body, "Body", 0);
        tab(ui, tab_state, RespTab::Headers, "Headers", ok.map_or(0, |r| r.headers.len()));
        tab(ui, tab_state, RespTab::LoadTest, "Load test", 0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if *tab_state != RespTab::LoadTest {
                if let Some(r) = ok {
                    ui.label(RichText::new(fmt_size(r.size as u64)).weak());
                    ui.label(RichText::new(fmt_dur(r.elapsed)).weak());
                    ui.label(RichText::new(format!("{} {}", r.status, r.reason)).color(status_color(r.status)).strong());
                }
            }
        });
    });
    ui.separator();

    if *tab_state == RespTab::LoadTest {
        load_test_ui(ui, id, loads.get(&id), cfg, cache, start_load, stop_load);
        return;
    }

    if let Some((started, _)) = inflight.get(&id) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Sending… {:.1}s", started.elapsed().as_secs_f32()));
        });
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        return;
    }
    let Some(view) = view else {
        ui.add_space(10.0);
        ui.label(RichText::new("Press Send to see the response here.").weak());
        return;
    };
    let r = match &view.result {
        Ok(r) => r,
        Err(e) => {
            ui.add_space(6.0);
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
            return;
        }
    };

    match *tab_state {
        RespTab::Body => {
            if r.display.is_empty() {
                ui.label(RichText::new("(empty body)").weak());
                return;
            }
            ui.horizontal(|ui| {
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(r.display.clone());
                }
                if r.truncated {
                    ui.label(RichText::new("Body is large; showing the first 4 MB.").weak().small());
                }
            });
            let dark = ui.visuals().dark_mode;
            let width = ui.available_width() - 16.0;
            let stale = !matches!(&view.galley, Some((w, d, _)) if (*w - width).abs() < 0.5 && *d == dark);
            if stale {
                let highlight = r.is_json && r.display.len() < 2 * 1024 * 1024;
                let mut job = json_job(&r.display, mono(ui), dark, highlight);
                job.wrap.max_width = width;
                let g = ui.fonts_mut(|f| f.layout_job(job));
                view.galley = Some((width, dark, g));
            }
            let galley = view.galley.as_ref().unwrap().2.clone();
            ScrollArea::vertical().id_salt("resp_body").auto_shrink([false, false]).show(ui, |ui| {
                ui.add(Label::new(galley).selectable(true));
            });
        }
        RespTab::Headers => {
            ScrollArea::vertical().id_salt("resp_headers").auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("resp_headers_grid").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (k, v) in &r.headers {
                        ui.add(Label::new(RichText::new(k).monospace().strong()).selectable(true));
                        ui.add(Label::new(RichText::new(v).monospace()).selectable(true).wrap());
                        ui.end_row();
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
    let running = run.is_some_and(|r| r.is_running());
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut cfg.mode, loadtest::Mode::Count, "Requests");
            ui.add_enabled(cfg.mode == loadtest::Mode::Count, egui::DragValue::new(&mut cfg.total).range(1..=10_000_000).speed(5));
            ui.add_space(8.0);
            ui.radio_value(&mut cfg.mode, loadtest::Mode::Duration, "Duration");
            ui.add_enabled(
                cfg.mode == loadtest::Mode::Duration,
                egui::DragValue::new(&mut cfg.seconds).range(1..=3600).suffix(" s"),
            );
            ui.add_space(16.0);
            ui.label("Concurrency");
            ui.add(egui::DragValue::new(&mut cfg.concurrency).range(1..=1000));
        });
    });
    ui.horizontal(|ui| {
        if running {
            if ui.button("Stop").clicked() {
                *stop = true;
            }
        } else if ui
            .add(egui::Button::new(RichText::new("Start load test").color(Color32::WHITE)).fill(Color32::from_rgb(0x25, 0x63, 0xEB)))
            .clicked()
        {
            *start = true;
        }
        ui.label(RichText::new("Sends this exact request, as configured above, from parallel workers.").weak().small());
    });
    ui.add_space(6.0);

    let Some(run) = run else { return };
    let fresh = matches!(cache, Some((t, rid, _)) if *rid == id && t.elapsed() < Duration::from_millis(200));
    if !fresh {
        *cache = Some((Instant::now(), id, run.summary()));
    }
    let s = cache.as_ref().unwrap().2.clone();
    if s.running {
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    ui.add(egui::ProgressBar::new(s.progress).show_percentage().desired_height(14.0));
    ui.add_space(4.0);
    ScrollArea::vertical().id_salt("load_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            egui::Grid::new("load_stats").num_columns(2).spacing([18.0, 3.0]).striped(true).show(ui, |ui| {
                let row = |ui: &mut Ui, k: &str, v: String| {
                    ui.label(RichText::new(k).weak());
                    ui.label(RichText::new(v).monospace());
                    ui.end_row();
                };
                row(ui, "Completed", s.completed.to_string());
                row(ui, "Failed", s.failed.to_string());
                row(ui, "Elapsed", format!("{:.2} s", s.elapsed.as_secs_f64()));
                row(ui, "Requests/sec", format!("{:.1}", s.rps));
                row(ui, "Data received", fmt_size(s.bytes));
            });
            ui.add_space(24.0);
            egui::Grid::new("load_lat").num_columns(2).spacing([18.0, 3.0]).striped(true).show(ui, |ui| {
                for (k, v) in [("Min", s.min), ("Average", s.avg), ("p50", s.p50), ("p90", s.p90), ("p95", s.p95), ("p99", s.p99), ("Max", s.max)] {
                    ui.label(RichText::new(k).weak());
                    ui.label(RichText::new(fmt_dur(v)).monospace());
                    ui.end_row();
                }
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Status codes").weak());
                if s.statuses.is_empty() {
                    ui.label(RichText::new("none yet").weak().small());
                }
                for (code, n) in &s.statuses {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(code.to_string()).color(status_color(*code)).strong().monospace());
                        ui.label(RichText::new(format!("× {n}")).monospace());
                    });
                }
                if !s.errors.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Errors").weak());
                    for (e, n) in &s.errors {
                        ui.label(RichText::new(format!("{n} × {e}")).color(ui.visuals().error_fg_color).small());
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let path = std::env::temp_dir().join(format!("kolibri-test-{}.json", std::process::id()));
        App {
            store: Store::starter(),
            path,
            dirty: false,
            last_edit: Instant::now(),
            notice: None,
            agent: http::new_agent(1),
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
        }
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
}
