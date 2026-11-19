//! Reusable UI components built on the design system in `theme.rs`:
//! vector icons, buttons, segmented tabs, badges, checkboxes, menu items,
//! empty states, callouts, keycaps and metric cards.

use crate::theme::{self, font, mix, radius, size, space, Palette};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Galley, Margin,
    Painter, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, Widget,
    WidgetInfo,
};
use std::sync::Arc;

// ---------------------------------------------------------------- icons

/// Icons are drawn as strokes on a 16×16 grid, so they stay crisp at any
/// size and always match the text color they sit next to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plus,
    ChevronRight,
    ChevronDown,
    Folder,
    FolderPlus,
    Send,
    Copy,
    Close,
    Alert,
    ErrorCircle,
    Bolt,
    Braces,
    Inbox,
    Trash,
    Pencil,
    Check,
    Stop,
    /// ⌘
    Command,
    /// ↵
    Return,
    Moon,
    Sun,
}

pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0 * s, 8.0 * s);
    let pt = |x: f32, y: f32| pos2(o.x + x * s, o.y + y * s);
    let st = Stroke::new((1.35 * s).max(1.0), color);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(Shape::closed_line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ));
    };
    match icon {
        Icon::Plus => {
            line(&[(8.0, 3.0), (8.0, 13.0)]);
            line(&[(3.0, 8.0), (13.0, 8.0)]);
        }
        Icon::ChevronRight => line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]),
        Icon::ChevronDown => line(&[(3.5, 6.0), (8.0, 10.5), (12.5, 6.0)]),
        Icon::Folder => closed(&[
            (2.0, 3.5),
            (6.2, 3.5),
            (7.7, 5.2),
            (14.0, 5.2),
            (14.0, 12.8),
            (2.0, 12.8),
        ]),
        Icon::FolderPlus => {
            closed(&[
                (2.0, 3.5),
                (6.2, 3.5),
                (7.7, 5.2),
                (14.0, 5.2),
                (14.0, 12.8),
                (2.0, 12.8),
            ]);
            line(&[(8.0, 7.2), (8.0, 11.0)]);
            line(&[(6.1, 9.1), (9.9, 9.1)]);
        }
        Icon::Send => {
            closed(&[(2.5, 7.6), (13.8, 2.6), (9.4, 13.6), (7.6, 9.0)]);
            line(&[(7.6, 9.0), (13.8, 2.6)]);
        }
        Icon::Copy => {
            p.rect_stroke(
                Rect::from_min_max(pt(5.5, 5.5), pt(13.5, 13.5)),
                2.0 * s,
                st,
                StrokeKind::Middle,
            );
            line(&[
                (3.0, 10.5),
                (2.5, 10.5),
                (2.5, 2.5),
                (10.5, 2.5),
                (10.5, 3.0),
            ]);
        }
        Icon::Close => {
            line(&[(4.0, 4.0), (12.0, 12.0)]);
            line(&[(12.0, 4.0), (4.0, 12.0)]);
        }
        Icon::Alert => {
            closed(&[(8.0, 2.2), (14.4, 13.4), (1.6, 13.4)]);
            line(&[(8.0, 6.2), (8.0, 9.4)]);
            p.circle_filled(pt(8.0, 11.4), 0.85 * s, color);
        }
        Icon::ErrorCircle => {
            p.circle_stroke(pt(8.0, 8.0), 6.2 * s, st);
            line(&[(8.0, 4.8), (8.0, 8.8)]);
            p.circle_filled(pt(8.0, 11.0), 0.85 * s, color);
        }
        Icon::Bolt => closed(&[
            (9.2, 1.8),
            (3.4, 9.2),
            (7.8, 9.2),
            (6.8, 14.2),
            (12.6, 6.8),
            (8.2, 6.8),
        ]),
        Icon::Braces => {
            line(&[
                (6.0, 2.5),
                (4.8, 2.9),
                (4.6, 6.6),
                (3.0, 8.0),
                (4.6, 9.4),
                (4.8, 13.1),
                (6.0, 13.5),
            ]);
            line(&[
                (10.0, 2.5),
                (11.2, 2.9),
                (11.4, 6.6),
                (13.0, 8.0),
                (11.4, 9.4),
                (11.2, 13.1),
                (10.0, 13.5),
            ]);
        }
        Icon::Inbox => {
            closed(&[
                (2.0, 9.0),
                (4.4, 3.0),
                (11.6, 3.0),
                (14.0, 9.0),
                (14.0, 13.0),
                (2.0, 13.0),
            ]);
            line(&[
                (2.0, 9.0),
                (5.4, 9.0),
                (6.4, 10.6),
                (9.6, 10.6),
                (10.6, 9.0),
                (14.0, 9.0),
            ]);
        }
        Icon::Trash => {
            line(&[(2.8, 4.4), (13.2, 4.4)]);
            line(&[(6.2, 4.4), (6.2, 2.6), (9.8, 2.6), (9.8, 4.4)]);
            line(&[(4.2, 4.4), (5.0, 13.4), (11.0, 13.4), (11.8, 4.4)]);
        }
        Icon::Pencil => {
            closed(&[
                (3.0, 13.0),
                (3.4, 10.4),
                (10.8, 3.0),
                (13.0, 5.2),
                (5.6, 12.6),
            ]);
            line(&[(9.4, 4.4), (11.6, 6.6)]);
        }
        Icon::Check => line(&[(3.4, 8.4), (6.6, 11.4), (12.6, 4.6)]),
        Icon::Command => {
            p.rect_stroke(
                Rect::from_min_max(pt(5.6, 5.6), pt(10.4, 10.4)),
                0.0,
                st,
                StrokeKind::Middle,
            );
            for (cx, cy) in [(4.2, 4.2), (11.8, 4.2), (4.2, 11.8), (11.8, 11.8)] {
                p.circle_stroke(pt(cx, cy), 1.6 * s, st);
            }
        }
        Icon::Return => {
            line(&[(12.6, 3.6), (12.6, 10.0), (3.6, 10.0)]);
            line(&[(6.4, 7.2), (3.6, 10.0), (6.4, 12.8)]);
        }
        Icon::Moon => {
            // Crescent: the part of the big circle outside the small one.
            let (c1, r1) = (pos2(7.6, 8.4), 5.6);
            let (c2, r2) = (pos2(10.4, 5.6), 4.4);
            let on = |c: Pos2, r: f32, a: f32| pos2(c.x + r * a.cos(), c.y + r * a.sin());
            let n = 72;
            let step = std::f32::consts::TAU / n as f32;
            // Start each sweep where the arc is hidden, so the kept points are one run.
            let a1 = (c2.y - c1.y).atan2(c2.x - c1.x);
            let outer: Vec<Pos2> = (0..=n)
                .map(|i| on(c1, r1, a1 + i as f32 * step))
                .filter(|q| q.distance(c2) > r2)
                .collect();
            let a2 = (c2.y - c1.y).atan2(c2.x - c1.x);
            let mut inner: Vec<Pos2> = (0..=n)
                .map(|i| on(c2, r2, a2 + i as f32 * step))
                .filter(|q| q.distance(c1) < r1)
                .collect();
            if let (Some(last), Some(first), Some(end)) = (outer.last(), inner.first(), inner.last()) {
                if last.distance(*first) > last.distance(*end) {
                    inner.reverse();
                }
            }
            let pts: Vec<Pos2> = outer.into_iter().chain(inner).map(|q| pt(q.x, q.y)).collect();
            p.add(Shape::closed_line(pts, st));
        }
        Icon::Sun => {
            p.circle_stroke(pt(8.0, 8.0), 2.8 * s, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (dx, dy) = (a.cos(), a.sin());
                line(&[(8.0 + 4.6 * dx, 8.0 + 4.6 * dy), (8.0 + 6.2 * dx, 8.0 + 6.2 * dy)]);
            }
        }
        Icon::Stop => {
            p.rect_stroke(
                Rect::from_min_max(pt(4.0, 4.0), pt(12.0, 12.0)),
                1.5 * s,
                st,
                StrokeKind::Middle,
            );
        }
    }
}

/// An icon as a standalone, non-interactive widget.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, icon, color);
    resp
}

// ---------------------------------------------------------------- text helpers

pub fn galley(ui: &Ui, text: impl Into<String>, font: FontId, color: Color32) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.into(), font, color)
}

/// Paint text clipped to `max_x`, with an ellipsis when it does not fit.
pub fn paint_text_clipped(
    ui: &Ui,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(0.0));
    let g = ui.fonts_mut(|f| f.layout_job(job));
    let rect = align.anchor_size(pos, g.size());
    ui.painter().galley(rect.min, g, color);
}

/// Width of a shortcut label; `⌘` and `↵` are drawn as icons so they
/// render with any font.
pub fn keys_width(ui: &Ui, keys: &str, font: FontId) -> f32 {
    let icon = font.size;
    let mut w = 0.0;
    let mut text = String::new();
    let flush = |text: &mut String, w: &mut f32| {
        if !text.is_empty() {
            *w += galley(ui, std::mem::take(text), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x;
        }
    };
    for c in keys.chars() {
        if matches!(c, '⌘' | '↵') {
            flush(&mut text, &mut w);
            w += icon + 1.0;
        } else {
            text.push(c);
        }
    }
    flush(&mut text, &mut w);
    w
}

/// Paint a shortcut label starting at `left` (vertically centered on `cy`).
pub fn paint_keys(ui: &Ui, left: f32, cy: f32, keys: &str, font: FontId, color: Color32) {
    let icon = font.size;
    let mut x = left;
    let mut text = String::new();
    let flush = |text: &mut String, x: &mut f32| {
        if !text.is_empty() {
            let g = galley(ui, std::mem::take(text), font.clone(), color);
            let w = g.size().x;
            ui.painter()
                .galley(pos2(*x, cy - g.size().y / 2.0), g, color);
            *x += w;
        }
    };
    for c in keys.chars() {
        let kind = match c {
            '⌘' => Some(Icon::Command),
            '↵' => Some(Icon::Return),
            _ => None,
        };
        if let Some(kind) = kind {
            flush(&mut text, &mut x);
            let r = Rect::from_center_size(pos2(x + icon / 2.0, cy), Vec2::splat(icon - 1.0));
            paint_icon(ui.painter(), r, kind, color);
            x += icon + 1.0;
        } else {
            text.push(c);
        }
    }
    flush(&mut text, &mut x);
}

fn focus_ring(ui: &Ui, rect: Rect, cr: impl Into<CornerRadius>, p: &Palette) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        cr,
        Stroke::new(2.0, p.accent.gamma_multiply(0.55)),
        StrokeKind::Outside,
    );
}

// ---------------------------------------------------------------- buttons

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The one main action of a screen (Send, Start).
    Primary,
    /// Regular actions.
    Secondary,
    /// Low-emphasis actions in toolbars.
    Ghost,
    /// Destructive confirmation.
    Danger,
}

/// A button from the design system: label, optional leading icon and an
/// optional keyboard-shortcut hint.
pub struct Button<'a> {
    label: &'a str,
    icon: Option<Icon>,
    kind: Kind,
    shortcut: Option<&'a str>,
    height: f32,
    min_width: f32,
    enabled: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str, kind: Kind) -> Self {
        Self {
            label,
            icon: None,
            kind,
            shortcut: None,
            height: size::CONTROL,
            min_width: 0.0,
            enabled: true,
        }
    }
    pub fn primary(label: &'a str) -> Self {
        Self::new(label, Kind::Primary)
    }
    pub fn secondary(label: &'a str) -> Self {
        Self::new(label, Kind::Secondary)
    }
    pub fn ghost(label: &'a str) -> Self {
        Self::new(label, Kind::Ghost)
    }
    pub fn danger(label: &'a str) -> Self {
        Self::new(label, Kind::Danger)
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn shortcut(mut self, s: &'a str) -> Self {
        self.shortcut = Some(s);
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
    pub fn small(self) -> Self {
        self.height(size::CONTROL_SM)
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}

struct ButtonColors {
    fill: Color32,
    stroke: Color32,
    text: Color32,
}

fn button_colors(p: &Palette, kind: Kind, hovered: bool, pressed: bool) -> ButtonColors {
    match kind {
        Kind::Primary => ButtonColors {
            fill: if pressed {
                mix(p.accent, Color32::BLACK, 0.12)
            } else if hovered {
                p.accent_hover
            } else {
                p.accent
            },
            stroke: Color32::TRANSPARENT,
            text: p.on_accent,
        },
        Kind::Danger => ButtonColors {
            fill: if pressed {
                mix(p.danger, Color32::BLACK, 0.12)
            } else if hovered {
                mix(p.danger, Color32::WHITE, 0.10)
            } else {
                p.danger
            },
            stroke: Color32::TRANSPARENT,
            text: p.on_accent,
        },
        Kind::Secondary => ButtonColors {
            fill: if pressed {
                p.bg_active
            } else if hovered {
                p.bg_hover
            } else {
                p.bg_surface
            },
            stroke: if hovered {
                mix(p.border_strong, p.text_faint, 0.3)
            } else {
                p.border_strong
            },
            text: p.text,
        },
        Kind::Ghost => ButtonColors {
            fill: if pressed {
                p.bg_active
            } else if hovered {
                p.bg_hover
            } else {
                Color32::TRANSPARENT
            },
            stroke: Color32::TRANSPARENT,
            text: if hovered { p.text } else { p.text_muted },
        },
    }
}

impl Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = theme::palette(ui);
        let font = if self.height <= size::CONTROL_SM {
            font::small_medium()
        } else {
            font::label()
        };
        let label = galley(ui, self.label, font, Color32::PLACEHOLDER);
        let hint_font = font::small();
        let pad = if self.height <= size::CONTROL_SM {
            8.0
        } else {
            12.0
        };
        let icon_w = if self.icon.is_some() {
            size::ICON + if self.label.is_empty() { 0.0 } else { 6.0 }
        } else {
            0.0
        };
        let hint_w = self
            .shortcut
            .map_or(0.0, |s| keys_width(ui, s, hint_font.clone()) + 8.0);
        let width = (pad * 2.0 + icon_w + label.size().x + hint_w).max(self.min_width);

        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, self.height), sense);
        resp.widget_info(|| WidgetInfo::labeled(Role::Button, self.enabled, self.label));
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let hovered = self.enabled && resp.hovered();
        let pressed = self.enabled && resp.is_pointer_button_down_on();
        let mut c = button_colors(p, self.kind, hovered, pressed);
        if !self.enabled {
            let a = 0.45;
            c.fill = c.fill.gamma_multiply(a);
            c.text = c.text.gamma_multiply(a);
            c.stroke = c.stroke.gamma_multiply(a);
        }
        let cr = CornerRadius::same(radius::MD);
        let painter = ui.painter();
        painter.rect(
            rect,
            cr,
            c.fill,
            Stroke::new(1.0, c.stroke),
            StrokeKind::Inside,
        );
        if resp.has_focus() {
            focus_ring(ui, rect, cr, p);
        }

        let content_w = icon_w + label.size().x + hint_w;
        let mut x = rect.center().x - content_w / 2.0;
        if let Some(icon) = self.icon {
            let r = Rect::from_center_size(
                pos2(x + size::ICON / 2.0, rect.center().y),
                Vec2::splat(size::ICON),
            );
            paint_icon(painter, r, icon, c.text);
            x += icon_w;
        }
        painter.galley(
            pos2(x, rect.center().y - label.size().y / 2.0),
            label.clone(),
            c.text,
        );
        x += label.size().x + 8.0;
        if let Some(keys) = self.shortcut {
            let hc = if matches!(self.kind, Kind::Primary | Kind::Danger) {
                c.text.gamma_multiply(0.7)
            } else {
                p.text_faint
            };
            paint_keys(ui, x, rect.center().y, keys, hint_font, hc);
        }
        if self.enabled {
            resp.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            resp
        }
    }
}

/// Square, frameless icon button for toolbars and rows.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_sized(ui, icon, tooltip, size::CONTROL_SM)
}

pub fn icon_button_sized(ui: &mut Ui, icon: Icon, tooltip: &str, side: f32) -> Response {
    let p = theme::palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, true, tooltip));
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    let cr = CornerRadius::same(radius::MD);
    if pressed {
        ui.painter().rect_filled(rect, cr, p.bg_active);
    } else if hovered {
        ui.painter().rect_filled(rect, cr, p.bg_hover);
    }
    if resp.has_focus() {
        focus_ring(ui, rect, cr, p);
    }
    let c = if hovered { p.text } else { p.text_muted };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(size::ICON)),
        icon,
        c,
    );
    resp.on_hover_text(tooltip)
        .on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- tabs

/// One item of a segmented control.
pub struct Segment<'a, T> {
    pub value: T,
    pub label: &'a str,
    /// Shown as a small count after the label when non-zero.
    pub count: usize,
}

pub fn seg<T>(value: T, label: &str, count: usize) -> Segment<'_, T> {
    Segment {
        value,
        label,
        count,
    }
}

/// Pill-style segmented control (tabs). Returns true if the selection changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id_salt: &str,
    current: &mut T,
    items: &[Segment<'_, T>],
) -> bool {
    let p = theme::palette(ui);
    let h = size::CONTROL;
    let pad_x = 10.0;
    let inset = 2.0;
    let label_font = font::label();
    let count_font = font::small_medium();

    struct Item {
        label: Arc<Galley>,
        count: Option<Arc<Galley>>,
        w: f32,
    }
    let measured: Vec<Item> = items
        .iter()
        .map(|it| {
            let label = galley(ui, it.label, label_font.clone(), Color32::PLACEHOLDER);
            let count = (it.count > 0).then(|| {
                galley(
                    ui,
                    it.count.to_string(),
                    count_font.clone(),
                    Color32::PLACEHOLDER,
                )
            });
            let w = pad_x * 2.0 + label.size().x + count.as_ref().map_or(0.0, |c| c.size().x + 6.0);
            Item { label, count, w }
        })
        .collect();
    let total: f32 = measured.iter().map(|m| m.w).sum::<f32>() + inset * 2.0;
    let (outer, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        outer,
        CornerRadius::same(radius::LG),
        mix(p.bg_app, p.text, if p.dark { 0.035 } else { 0.04 }),
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );

    // Slide the selection pill between items.
    let mut x = outer.left() + inset;
    let mut sel = None;
    let mut rects = Vec::with_capacity(items.len());
    for (it, m) in items.iter().zip(&measured) {
        let r = Rect::from_min_size(pos2(x, outer.top() + inset), vec2(m.w, h - inset * 2.0));
        if it.value == *current {
            sel = Some(r);
        }
        rects.push(r);
        x += m.w;
    }
    let base_id = ui.scope_id().with(("segmented", id_salt));
    if let Some(r) = sel {
        let ctx = ui.ctx();
        let x0 = ctx.animate_value_with_time(base_id.with("x"), r.left(), 0.12);
        let w = ctx.animate_value_with_time(base_id.with("w"), r.width(), 0.12);
        let pill = Rect::from_min_size(pos2(x0, r.top()), vec2(w, r.height()));
        painter.rect(
            pill,
            CornerRadius::same(radius::MD),
            if p.dark { p.bg_active } else { p.bg_app },
            Stroke::new(1.0, p.border_strong),
            StrokeKind::Inside,
        );
    }

    let mut changed = false;
    for (i, (it, m)) in items.iter().zip(measured).enumerate() {
        let r = rects[i];
        let resp = ui.interact(r, base_id.with(i), Sense::click());
        resp.widget_info(|| WidgetInfo::selected(Role::Tab, true, it.value == *current, it.label));
        let selected = it.value == *current;
        if resp.clicked() && !selected {
            *current = it.value;
            changed = true;
        }
        if resp.has_focus() {
            focus_ring(ui, r, CornerRadius::same(radius::MD), p);
        }
        let color = if selected || resp.hovered() {
            p.text
        } else {
            p.text_muted
        };
        let lx = r.left() + pad_x;
        let painter = ui.painter();
        painter.galley(
            pos2(lx, r.center().y - m.label.size().y / 2.0),
            m.label.clone(),
            color,
        );
        if let Some(c) = m.count {
            let cx = lx + m.label.size().x + 6.0;
            painter.galley(
                pos2(cx, r.center().y - c.size().y / 2.0),
                c,
                if selected { p.accent } else { p.text_faint },
            );
        }
        resp.on_hover_cursor(CursorIcon::PointingHand);
    }
    changed
}

// ---------------------------------------------------------------- small elements

/// Small tinted pill with colored text (status codes, labels).
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let p = theme::palette(ui);
    let g = galley(ui, text, font::small_medium(), color);
    let size = vec2(g.size().x + 14.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::SM + 6),
        p.tint(color),
        Stroke::new(1.0, p.tint_border(color)),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, color);
    resp
}

/// Status pill: colored dot + text.
pub fn status_badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let p = theme::palette(ui);
    let g = galley(ui, text, font::small_medium(), color);
    let size = vec2(g.size().x + 26.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(11),
        p.tint(color),
        Stroke::new(1.0, p.tint_border(color)),
        StrokeKind::Inside,
    );
    ui.painter()
        .circle_filled(pos2(rect.left() + 11.0, rect.center().y), 3.0, color);
    ui.painter().galley(
        pos2(rect.left() + 19.0, rect.center().y - g.size().y / 2.0),
        g,
        color,
    );
    resp
}

/// Muted "label value" metadata pair, e.g. "Time 120 ms".
pub fn meta(ui: &mut Ui, label: &str, value: &str) -> Response {
    let p = theme::palette(ui);
    let l = galley(ui, label, font::small(), p.text_faint);
    let v = galley(ui, value, font::small_medium(), p.text_muted);
    let gap = 5.0;
    let size = vec2(l.size().x + gap + v.size().x, l.size().y.max(v.size().y));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let cy = rect.center().y;
    let lw = l.size().x;
    ui.painter()
        .galley(pos2(rect.left(), cy - l.size().y / 2.0), l, p.text_faint);
    ui.painter().galley(
        pos2(rect.left() + lw + gap, cy - v.size().y / 2.0),
        v,
        p.text_muted,
    );
    resp
}

/// Keyboard key cap, e.g. "⌘ ↵".
pub fn kbd(ui: &mut Ui, keys: &str) -> Response {
    let p = theme::palette(ui);
    let w = keys_width(ui, keys, font::small_medium());
    let size = vec2((w + 12.0).max(22.0), 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::SM),
        p.bg_surface,
        Stroke::new(1.0, p.border_strong),
        StrokeKind::Inside,
    );
    paint_keys(
        ui,
        rect.center().x - w / 2.0,
        rect.center().y,
        keys,
        font::small_medium(),
        p.text_muted,
    );
    resp
}

/// Uppercase section header.
pub fn overline(ui: &mut Ui, text: &str) -> Response {
    let p = theme::palette(ui);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(font::overline())
            .color(p.text_faint)
            .extra_letter_spacing(0.6),
    )
}

/// Thin horizontal divider spanning the available width.
pub fn divider(ui: &mut Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.border));
}

/// Compact checkbox (16×16) centered in `rect`. Returns its response.
pub fn checkbox_at(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    checked: &mut bool,
    tooltip: &str,
) -> Response {
    let p = theme::palette(ui);
    let mut resp = ui.interact(rect, id, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, *checked, tooltip));
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let b = Rect::from_center_size(rect.center(), Vec2::splat(15.0));
    let cr = CornerRadius::same(radius::SM);
    if *checked {
        let fill = if resp.hovered() {
            p.accent_hover
        } else {
            p.accent
        };
        ui.painter().rect_filled(b, cr, fill);
        paint_icon(ui.painter(), b.shrink(1.5), Icon::Check, p.on_accent);
    } else {
        let stroke = if resp.hovered() {
            p.text_faint
        } else {
            p.border_strong
        };
        ui.painter().rect(
            b,
            cr,
            p.bg_surface,
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        );
    }
    if resp.has_focus() {
        focus_ring(ui, b, cr, p);
    }
    resp.on_hover_text(tooltip)
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// Spinner in the accent color.
pub fn spinner(ui: &mut Ui, size: f32) -> Response {
    let p = theme::palette(ui);
    ui.add(egui::Spinner::new().size(size).color(p.accent))
}

/// Thin progress bar with rounded ends.
pub fn progress(ui: &mut Ui, fraction: f32, color: Color32) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let cr = CornerRadius::same(3);
    ui.painter().rect_filled(rect, cr, p.bg_active);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let filled =
            Rect::from_min_size(rect.min, vec2((rect.width() * f).max(6.0), rect.height()));
        ui.painter().rect_filled(filled, cr, color);
    }
}

/// Theme switch as a single icon button. It shows the mode a click switches
/// to: a sun while dark, a moon while light. `dark` is toggled on click.
pub fn theme_toggle(ui: &mut Ui, dark: &mut bool) -> Response {
    let (icon_kind, tip) = if *dark {
        (Icon::Sun, "Switch to light mode")
    } else {
        (Icon::Moon, "Switch to dark mode")
    };
    let mut resp = icon_button_sized(ui, icon_kind, tip, size::CONTROL);
    if resp.clicked() {
        *dark = !*dark;
        resp.mark_changed();
    }
    resp
}

// ---------------------------------------------------------------- menus

/// Full-width menu row with an icon. Use inside context menus and popups.
pub fn menu_item(ui: &mut Ui, icon: Option<Icon>, label: &str, danger: bool) -> Response {
    let p = theme::palette(ui);
    let width = ui.spacing().menu_width;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, size::CONTROL), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let hovered = resp.hovered() || resp.has_focus();
    if hovered {
        let fill = if danger { p.tint(p.danger) } else { p.bg_hover };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::MD), fill);
    }
    let color = if danger {
        p.danger
    } else if hovered {
        p.text
    } else {
        mix(p.text, p.text_muted, 0.4)
    };
    let mut x = rect.left() + space::SM;
    if let Some(icon) = icon {
        let r = Rect::from_center_size(
            pos2(x + size::ICON / 2.0, rect.center().y),
            Vec2::splat(size::ICON),
        );
        paint_icon(
            ui.painter(),
            r,
            icon,
            if danger { p.danger } else { p.text_muted },
        );
        x += size::ICON + space::SM;
    }
    ui.painter().text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font::body(),
        color,
    );
    resp
}

pub fn menu_separator(ui: &mut Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.spacing().menu_width, 7.0), Sense::hover());
    ui.painter().hline(
        rect.x_range().shrink(4.0),
        rect.center().y,
        Stroke::new(1.0, p.border),
    );
}

// ---------------------------------------------------------------- containers

/// Bordered card on the surface color.
pub fn card(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.bg_surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::LG))
        .inner_margin(Margin::same(space::MD as i8))
}

/// Tinted message box for errors and warnings.
pub fn callout(ui: &mut Ui, color: Color32, icon_kind: Icon, title: &str, message: &str) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.tint(color))
        .stroke(Stroke::new(1.0, p.tint_border(color)))
        .corner_radius(CornerRadius::same(radius::LG))
        .inner_margin(Margin::symmetric(space::MD as i8, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.add_space(1.0);
                icon(ui, icon_kind, 16.0, color);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = space::XS;
                    ui.label(egui::RichText::new(title).font(font::label()).color(p.text));
                    if !message.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(message)
                                    .font(font::body())
                                    .color(p.text_muted),
                            )
                            .selectable(true)
                            .wrap(),
                        );
                    }
                });
            });
        });
}

/// Centered empty state: icon tile, title, body and optional shortcut hints.
pub fn empty_state(ui: &mut Ui, icon_kind: Icon, title: &str, body: &str, hints: &[(&str, &str)]) {
    let p = theme::palette(ui);
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = space::SM;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(radius::XL),
            p.bg_surface,
            Stroke::new(1.0, p.border_strong),
            StrokeKind::Inside,
        );
        paint_icon(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
            icon_kind,
            p.text_muted,
        );
        ui.add_space(space::XS);
        ui.label(
            egui::RichText::new(title)
                .font(font::heading())
                .color(p.text),
        );
        if !body.is_empty() {
            ui.label(
                egui::RichText::new(body)
                    .font(font::body())
                    .color(p.text_muted),
            );
        }
        if !hints.is_empty() {
            ui.add_space(space::XS);
            for (keys, what) in hints {
                // Center each hint row by measuring it first.
                let label_w = galley(ui, *what, font::small(), p.text_faint).size().x;
                let keys_w = (keys_width(ui, keys, font::small_medium()) + 12.0).max(22.0);
                let row_w = keys_w + space::SM + label_w;
                ui.allocate_ui(vec2(row_w, 22.0), |ui| {
                    ui.horizontal(|ui| {
                        kbd(ui, keys);
                        ui.label(
                            egui::RichText::new(*what)
                                .font(font::small())
                                .color(p.text_faint),
                        );
                    });
                });
            }
        }
    });
}

/// Metric card: small label above a large value.
pub fn metric(ui: &mut Ui, label: &str, value: &str, color: Option<Color32>, width: f32) {
    let p = theme::palette(ui);
    card(p)
        .inner_margin(Margin::symmetric(space::MD as i8, 10))
        .show(ui, |ui| {
            // The 1 px border on each side is outside the content width.
            ui.set_width(width - 2.0 * space::MD - 2.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = space::XXS;
                ui.label(
                    egui::RichText::new(label)
                        .font(font::small())
                        .color(p.text_faint),
                );
                ui.label(
                    egui::RichText::new(value)
                        .font(font::metric())
                        .color(color.unwrap_or(p.text)),
                );
            });
        });
}
