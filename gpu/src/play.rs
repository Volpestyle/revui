//! Plays a script with no one at the keyboard, and can save every frame: how the interface is checked, and how its
//! demos are recorded without a screen recorder. `REVUI_GPU_KEYS` is the script, entries split by commas:
//! a key name (`L`, `Tab`, `Enter`, `Cmd+Enter`), `text:…` typed at once, `type:…` typed a letter at a time, `wait:N` frames,
//! `move:x;y` to glide the drawn cursor, `click`, and for the recording `step:Title|detail`, `focus:x;y;w;h`, `unfocus`.

use eframe::egui::{self, Color32, Key, Pos2, Stroke, pos2, vec2};

#[derive(Default)]
pub struct Play {
    entries: Vec<String>,
    next: usize,
    hold: u32,
    pub frame: u32,
    ended: Option<u32>,
    cursor: Option<Pos2>,
    glide: Option<(Pos2, Pos2, u32, u32)>,
    release: bool,
    unhold: bool,
    marks: Vec<serde_json::Value>,
}

impl Play {
    pub fn load() -> Self {
        let script = std::env::var("REVUI_GPU_KEYS").unwrap_or_default();
        let entries = script.split(',').filter(|entry| !entry.is_empty()).flat_map(|entry| match entry.strip_prefix("type:") {
            Some(text) => text.chars().map(|letter| format!("letter:{letter}")).collect(),
            None => vec![entry.to_string()],
        });
        Self { entries: entries.collect(), ..Default::default() }
    }

    /// Whether the script is still running, or has only just ended: the window keeps drawing until then.
    pub fn running(&self) -> bool {
        self.ended.is_none_or(|ended| self.frame < ended + 24)
    }

    /// The frame, counted from the end of the script, at which a single screenshot is due.
    pub fn settled(&self) -> bool {
        self.ended.is_some_and(|ended| self.frame == ended + 6)
    }

    pub fn finished(&self) -> bool {
        self.ended.is_some_and(|ended| self.frame == ended + 20)
    }

    pub fn marks(&self) -> String {
        let meta = serde_json::json!({ "t": 0, "type": "meta", "width": 1360, "height": 900, "scale": 2 });
        serde_json::Value::Array(std::iter::once(meta).chain(self.marks.iter().cloned()).collect()).to_string()
    }

    pub fn step(&mut self, raw: &mut egui::RawInput) {
        self.frame += 1;
        if let Some((from, to, start, length)) = self.glide {
            let k = ((self.frame - start) as f32 / length as f32).min(1.0);
            let eased = k * k * k * (10.0 - 15.0 * k + 6.0 * k * k);
            self.cursor = Some(from + (to - from) * eased);
            if k >= 1.0 {
                self.glide = None;
            }
        }
        if let (true, Some(at)) = (std::mem::take(&mut self.release), self.cursor) {
            raw.events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        }
        if std::mem::take(&mut self.unhold) {
            raw.events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        }
        if self.frame < 6 || self.ended.is_some() {
            return;
        }
        if self.hold > 0 {
            self.hold -= 1;
            return;
        }
        let Some(entry) = self.entries.get(self.next).cloned() else { return self.ended = Some(self.frame) };
        self.next += 1;
        let (name, rest) = entry.split_once(':').unwrap_or((&entry, ""));
        let numbers: Vec<f32> = rest.split([';', ' ']).filter_map(|number| number.trim().parse().ok()).collect();
        let t = self.frame as f64 / 30.0;
        self.hold = 8;
        match (name, numbers.as_slice()) {
            ("wait", [frames]) => {
                self.marks.push(serde_json::json!({ "t": t + *frames as f64 / 30.0, "start": t, "type": "hold" }));
                self.hold = *frames as u32;
            }
            ("text", _) => raw.events.push(egui::Event::Text(rest.into())),
            ("letter", _) => (raw.events.push(egui::Event::Text(rest.into())), self.hold = 1).1,
            ("step", _) => {
                let (title, detail) = rest.split_once('|').unwrap_or((rest, ""));
                self.marks.push(serde_json::json!({ "t": t, "type": "step", "title": title, "detail": detail }));
                self.hold = 0;
            }
            ("focus", [x, y, w, h]) => (self.marks.push(serde_json::json!({ "t": t, "type": "focus", "box": { "x": x, "y": y, "w": w, "h": h } })), self.hold = 0).1,
            ("unfocus", _) => (self.marks.push(serde_json::json!({ "t": t, "type": "unfocus" })), self.hold = 0).1,
            ("move", [x, y]) => {
                let (from, to) = (self.cursor.unwrap_or(pos2(680.0, 450.0)), pos2(*x, *y));
                let length = (8.0 + 5.0 * ((to - from).length() / 40.0 + 1.0).log2()).round() as u32;
                self.glide = Some((from, to, self.frame, length));
                self.hold = length + 3;
                raw.events.push(egui::Event::PointerMoved(to));
            }
            ("click", _) => {
                if let Some(at) = self.cursor {
                    raw.events.push(egui::Event::PointerMoved(at));
                    raw.events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    self.release = true;
                }
            }
            _ => {
                let (held, named) = match (entry.strip_prefix("Cmd+"), entry.strip_prefix("Shift+")) {
                    (Some(named), _) => (egui::Modifiers::COMMAND | egui::Modifiers::MAC_CMD, named),
                    (_, Some(named)) => (egui::Modifiers::SHIFT, named),
                    _ => (egui::Modifiers::NONE, entry.as_str()),
                };
                if !held.is_none() {
                    raw.events.push(egui::Event::ModifiersChanged(held));
                    self.unhold = true;
                }
                if let Some(key) = Key::from_name(named) {
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: held });
                    }
                    if let (1, Some(letter), true) = (named.len(), named.chars().next(), held.is_none()) {
                        raw.events.push(egui::Event::Text(letter.to_lowercase().to_string()));
                    }
                }
            }
        }
    }

    /// The cursor a recording shows: the real one is not in the frames the window saves.
    pub fn draw(&self, ctx: &egui::Context) {
        let Some(at) = self.cursor else { return };
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::new("cursor")));
        let point = |x: f32, y: f32| at + vec2(x - 5.0, y - 3.0) * 0.85;
        let outline = vec![point(5.0, 3.0), point(5.0, 23.0), point(10.5, 17.5), point(14.5, 26.0), point(18.0, 24.4), point(14.0, 16.0), point(21.5, 16.0)];
        let ink = Color32::from_rgb(17, 17, 17);
        painter.add(egui::Shape::convex_polygon(vec![point(5.0, 3.0), point(5.0, 23.0), point(21.5, 16.0)], ink, Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(vec![point(10.5, 17.5), point(14.5, 26.0), point(18.0, 24.4), point(14.0, 16.0)], ink, Stroke::NONE));
        painter.add(egui::Shape::closed_line(outline, Stroke::new(1.6, Color32::WHITE)));
    }
}
