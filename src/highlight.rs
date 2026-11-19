//! Tiny, tolerant JSON syntax highlighter producing an egui LayoutJob.

use eframe::egui::{self, text::ByteIndex, text::LayoutJob, text::LayoutSection, Color32, FontId, TextFormat};

struct Palette {
    key: Color32,
    string: Color32,
    number: Color32,
    keyword: Color32,
    punct: Color32,
    plain: Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            key: Color32::from_rgb(0x9C, 0xDC, 0xFE),
            string: Color32::from_rgb(0xCE, 0x91, 0x78),
            number: Color32::from_rgb(0xB5, 0xCE, 0xA8),
            keyword: Color32::from_rgb(0x56, 0x9C, 0xD6),
            punct: Color32::from_gray(170),
            plain: Color32::from_gray(220),
        }
    } else {
        Palette {
            key: Color32::from_rgb(0x00, 0x45, 0x9E),
            string: Color32::from_rgb(0xA3, 0x15, 0x15),
            number: Color32::from_rgb(0x09, 0x86, 0x58),
            keyword: Color32::from_rgb(0x00, 0x00, 0xFF),
            punct: Color32::from_gray(90),
            plain: Color32::from_gray(30),
        }
    }
}

pub fn json_job(text: &str, font: FontId, dark: bool, highlight: bool) -> LayoutJob {
    let p = palette(dark);
    let mut job = LayoutJob { text: text.to_owned(), ..Default::default() };
    let mut push = |range: std::ops::Range<usize>, color: Color32| {
        if range.is_empty() {
            return;
        }
        if let Some(last) = job.sections.last_mut() {
            if last.format.color == color && last.byte_range.end == ByteIndex(range.start) {
                last.byte_range.end = ByteIndex(range.end);
                return;
            }
        }
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(range.start)..ByteIndex(range.end),
            format: TextFormat { font_id: font.clone(), color, ..Default::default() },
        });
    };
    if !highlight {
        push(0..text.len(), p.plain);
        return job;
    }
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        match c {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                let mut j = i;
                while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                    j += 1;
                }
                let color = if j < b.len() && b[j] == b':' { p.key } else { p.string };
                push(start..i, color);
            }
            b'-' | b'0'..=b'9' => {
                i += 1;
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
                    i += 1;
                }
                push(start..i, p.number);
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                i += 1;
                push(start..i, p.punct);
            }
            c if c.is_ascii_alphabetic() => {
                while i < b.len() && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let w = &text[start..i];
                let color = if matches!(w, "true" | "false" | "null") { p.keyword } else { p.plain };
                push(start..i, color);
            }
            _ => {
                // advance one UTF-8 char
                i += 1;
                while i < b.len() && (b[i] & 0xC0) == 0x80 {
                    i += 1;
                }
                push(start..i, p.plain);
            }
        }
    }
    job
}

pub fn mono(ui: &egui::Ui) -> FontId {
    egui::TextStyle::Monospace.resolve(ui.style())
}
