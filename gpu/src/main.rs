use eframe::egui::{self, Color32, FontFamily, FontId, Key, RichText, Stroke};
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Evidence {
    provenance: String,
    caption: String,
    text: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Item {
    id: String,
    kind: String,
    severity: String,
    title: String,
    body: String,
    section: Option<usize>,
    evidence: Vec<Evidence>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Section {
    title: String,
    tier: String,
    why: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Review {
    title: String,
    summary: String,
    sections: Vec<Section>,
    items: Vec<Item>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Hunk {
    header: String,
    old_start: u32,
    new_start: u32,
    lines: Vec<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct FileDiff {
    path: String,
    hunks: Vec<Hunk>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Data {
    review: Review,
    files: Vec<FileDiff>,
    groups: Vec<Vec<(usize, usize)>>,
    state: serde_json::Value,
    outgoing: usize,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Past {
    dir: String,
    repo: String,
    reference: String,
    title: String,
    author: String,
    items: usize,
    marked: usize,
    done: Option<String>,
    working: String,
    when: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Card {
    Summary,
    Part(usize),
    Item(usize),
    Loose,
}

/// The colours of the page, for the appearance the system is in.
struct Tones {
    bg: Color32,
    raised: Color32,
    line: Color32,
    dim: Color32,
    text: Color32,
    strong: Color32,
    accent: Color32,
    on_accent: Color32,
    rust: Color32,
}

const INK: Color32 = Color32::from_rgb(11, 11, 11);
const ON_INK: Color32 = Color32::from_rgb(238, 237, 231);
const ADDED: Color32 = Color32::from_rgb(20, 46, 28);
const REMOVED: Color32 = Color32::from_rgb(58, 22, 26);

fn tones(dark: bool) -> Tones {
    let rgb = Color32::from_rgb;
    match dark {
        true => Tones { bg: rgb(22, 22, 20), raised: rgb(31, 31, 28), line: rgb(58, 58, 53), dim: rgb(156, 156, 146), text: rgb(205, 204, 196), strong: rgb(241, 240, 234), accent: rgb(241, 240, 234), on_accent: INK, rust: rgb(196, 122, 90) },
        false => Tones { bg: rgb(246, 245, 240), raised: Color32::WHITE, line: rgb(214, 212, 202), dim: rgb(108, 109, 101), text: rgb(58, 59, 54), strong: rgb(22, 22, 20), accent: INK, on_accent: Color32::WHITE, rust: rgb(111, 63, 43) },
    }
}

fn revui(args: &[&str], input: Option<&str>) -> Result<String, String> {
    let bin = std::env::var("REVUI_BIN").unwrap_or_else(|_| "revui".into());
    let mut child = Command::new(bin).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("revui: {e}"))?;
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    match out.status.success() {
        true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        false => Err(String::from_utf8_lossy(&out.stderr).lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("revui failed").trim().to_string()),
    }
}

struct Open {
    dir: String,
    data: Data,
    cards: Vec<Card>,
    at: usize,
    diff: bool,
    contents: bool,
}

impl Open {
    fn load(dir: &str) -> Result<Self, String> {
        let data: Data = serde_json::from_str(&revui(&["json", dir], None)?).map_err(|e| format!("review: {e}"))?;
        let mut cards = vec![Card::Summary];
        for part in 0..data.review.sections.len() {
            cards.push(Card::Part(part));
            cards.extend(data.review.items.iter().enumerate().filter(|(_, item)| item.section == Some(part)).map(|(n, _)| Card::Item(n)));
        }
        cards.extend(data.review.items.iter().enumerate().filter(|(_, item)| item.section.is_none_or(|part| part >= data.review.sections.len())).map(|(n, _)| Card::Item(n)));
        if data.groups.len() > data.review.sections.len() {
            cards.push(Card::Loose);
        }
        let at = std::env::var("REVUI_GPU_AT").ok().and_then(|at| at.parse::<usize>().ok()).unwrap_or(0).min(cards.len() - 1);
        Ok(Self { dir: dir.into(), data, cards, at, diff: true, contents: false })
    }

    fn group(&self) -> Option<usize> {
        match self.cards[self.at] {
            Card::Summary => None,
            Card::Part(part) => Some(part),
            Card::Item(n) => self.data.review.items[n].section,
            Card::Loose => Some(self.data.review.sections.len()),
        }
        .filter(|group| *group < self.data.groups.len())
    }

    fn label(&self, card: Card) -> String {
        match card {
            Card::Summary => "Summary".into(),
            Card::Part(part) => format!("{}. {}", part + 1, self.data.review.sections[part].title),
            Card::Item(n) => format!("{} {}", self.data.review.items[n].id, self.data.review.items[n].title),
            Card::Loose => "Code in no part".into(),
        }
    }

    fn verdict(&self, id: &str) -> Option<&str> {
        self.data.state["verdicts"][id].as_str()
    }

    /// Marks a finding, or clears the mark when it is the one already there, and saves through revui as the page does.
    fn mark(&mut self, id: &str, verdict: &str) -> Result<(), String> {
        let mut state = self.data.state.clone();
        let cleared = state["verdicts"][id].as_str() == Some(verdict);
        if let Some(verdicts) = state["verdicts"].as_object_mut() {
            match cleared {
                true => drop(verdicts.remove(id)),
                false => drop(verdicts.insert(id.into(), verdict.into())),
            }
        }
        state["finished"] = serde_json::Value::Null;
        revui(&["state", &self.dir], Some(&state.to_string()))?;
        let at = self.at;
        *self = Self { at, diff: self.diff, ..Self::load(&self.dir)? };
        Ok(())
    }
}

struct App {
    reviews: Vec<Past>,
    open: Option<Open>,
    problem: String,
    frames: u32,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        for (name, bytes) in [("geist", include_bytes!("../fonts/Geist-Regular.ttf").as_slice()), ("geist-bold", include_bytes!("../fonts/Geist-SemiBold.ttf")), ("geist-mono", include_bytes!("../fonts/GeistMono-Regular.ttf"))] {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        }
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "geist".into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "geist-mono".into());
        fonts.families.insert(FontFamily::Name("bold".into()), vec!["geist-bold".into(), "geist".into()]);
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        let named = std::env::args().nth(1);
        let mut app = Self { reviews: Vec::new(), open: None, problem: String::new(), frames: 0 };
        match named {
            Some(dir) => app.show(&dir),
            None => app.list(),
        }
        app
    }

    fn list(&mut self) {
        self.open = None;
        match revui(&["list"], None).and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string())) {
            Ok(reviews) => self.reviews = reviews,
            Err(problem) => self.problem = problem,
        }
    }

    fn show(&mut self, dir: &str) {
        match Open::load(dir) {
            Ok(open) => (self.open, self.problem) = (Some(open), String::new()),
            Err(problem) => self.problem = problem,
        }
    }
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn prose(ui: &mut egui::Ui, tones: &Tones, text: &str) {
    for block in text.trim().split("\n\n").flat_map(|block| block.split("\n# ").enumerate().map(|(n, part)| if n == 0 { part.to_string() } else { format!("# {part}") })) {
        let mut lines = block.lines().peekable();
        if let Some(heading) = lines.peek().and_then(|line| line.strip_prefix("# ")) {
            ui.add_space(6.0);
            ui.label(RichText::new(heading).font(bold(14.0)).color(tones.strong));
            lines.next();
        }
        let body = lines.collect::<Vec<_>>().join("\n");
        if !body.trim().is_empty() {
            ui.label(RichText::new(body).size(14.5).color(tones.text));
        }
        ui.add_space(6.0);
    }
}

fn button(ui: &mut egui::Ui, tones: &Tones, label: &str, on: bool) -> egui::Response {
    let (fill, ink) = if on { (tones.accent, tones.on_accent) } else { (tones.raised, tones.strong) };
    ui.add(egui::Button::new(RichText::new(label).font(bold(13.0)).color(ink)).fill(fill).stroke(Stroke::new(1.0, if on { tones.accent } else { tones.text })).corner_radius(0.0))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let tones = tones(ui.visuals().dark_mode);
        let ctx = ui.ctx().clone();
        self.frames += 1;
        if let (3, Ok(path)) = (self.frames, std::env::var("REVUI_GPU_SHOT")) {
            let quit = ctx.clone();
            ctx.request_screenshot(move |image| {
                let file = std::fs::File::create(&path).expect("screenshot file");
                let mut encoder = png::Encoder::new(file, image.width() as u32, image.height() as u32);
                encoder.set_color(png::ColorType::Rgba);
                let _ = encoder.write_header().and_then(|mut writer| writer.write_image_data(image.as_raw()));
                quit.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        if self.frames < 6 {
            ctx.request_repaint();
        }
        let frame = egui::Frame::new().fill(tones.bg);
        if self.open.is_none() {
            let mut pick = None;
            egui::CentralPanel::default().frame(frame.inner_margin(32.0)).show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(680.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(RichText::new("revui").font(bold(24.0)).color(tones.strong));
                        ui.add_space(14.0);
                        ui.label(RichText::new("YOUR REVIEWS").font(mono(11.5)).color(tones.dim));
                        ui.add_space(4.0);
                        if !self.problem.is_empty() {
                            ui.label(RichText::new(&self.problem).color(tones.rust));
                        }
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for past in &self.reviews {
                                let state = if !past.working.trim().is_empty() { past.working.clone() } else if past.done.is_some() { "published".into() } else if past.items > 0 { format!("{} of {} marked", past.marked, past.items) } else { "no guide yet".into() };
                                let about = [format!("{}{}", past.repo.rsplit('/').next().unwrap_or(""), past.reference), past.author.clone(), state, past.when.clone()].join(" · ");
                                let row = egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.line)).inner_margin(10.0).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(if past.title.is_empty() { &past.dir } else { &past.title }).size(14.5).color(tones.strong));
                                    ui.label(RichText::new(about).size(12.0).color(tones.dim));
                                });
                                if row.response.interact(egui::Sense::click()).clicked() {
                                    pick = Some(past.dir.clone());
                                }
                            }
                        });
                    });
                });
            });
            if let Some(dir) = pick {
                self.show(&dir);
            }
            return;
        }

        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |key: Key| !typing && ctx.input(|input| input.key_pressed(key) && !input.modifiers.command && !input.modifiers.ctrl);
        if pressed(Key::Q) {
            return self.list();
        }
        let open = self.open.as_mut().expect("a review is open");
        let step = (pressed(Key::L) || pressed(Key::ArrowRight)) as isize - (pressed(Key::H) || pressed(Key::ArrowLeft)) as isize;
        open.at = (open.at as isize + step).clamp(0, open.cards.len() as isize - 1) as usize;
        open.diff ^= pressed(Key::D);
        open.contents ^= pressed(Key::B);
        let mut marked: Option<(String, &str)> = None;
        if let Card::Item(n) = open.cards[open.at] {
            let id = open.data.review.items[n].id.clone();
            match (pressed(Key::A), pressed(Key::X)) {
                (true, _) => marked = Some((id, "agree")),
                (_, true) => marked = Some((id, "dismiss")),
                _ => {}
            }
        }

        egui::Panel::top("bar").frame(frame.inner_margin(egui::Margin::symmetric(16, 9))).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("revui").font(bold(14.0)).color(tones.strong));
                ui.label(RichText::new(&open.data.review.title).size(16.0).color(tones.strong));
                ui.label(RichText::new(format!("{} of {}", open.at + 1, open.cards.len())).font(mono(11.5)).color(tones.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    button(ui, &tones, &format!("Publish {}", open.data.outgoing), true);
                    open.diff ^= button(ui, &tones, "Diff", open.diff).clicked();
                    open.contents ^= button(ui, &tones, "Contents", false).clicked();
                });
            });
        });
        egui::Panel::bottom("keys").frame(frame.inner_margin(egui::Margin::symmetric(14, 5))).show_inside(ui, |ui| {
            ui.label(RichText::new("l next · h back · a agree · x dismiss · d diff · b contents · q reviews").font(mono(11.5)).color(tones.dim));
        });

        if let (true, Some(group)) = (open.diff, open.group()) {
            egui::Panel::right("diff").default_size(620.0).frame(egui::Frame::new().fill(INK)).show_inside(ui, |ui| {
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut last = usize::MAX;
                    for &(file, hunk) in &open.data.groups[group] {
                        let (shown, hunk) = (&open.data.files[file], &open.data.files[file].hunks[hunk]);
                        if file != last {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 9)).show(ui, |ui| ui.label(RichText::new(&shown.path).font(mono(13.0)).color(Color32::WHITE).strong()));
                            last = file;
                        }
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| ui.label(RichText::new(&hunk.header).font(mono(11.0)).color(Color32::from_white_alpha(120))));
                        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
                        for line in &hunk.lines {
                            let sign = line.chars().next().unwrap_or(' ');
                            let number = if sign == '-' { old } else { new };
                            match sign {
                                '-' => old += 1,
                                '+' => new += 1,
                                '\\' => {}
                                _ => (old, new) = (old + 1, new + 1),
                            }
                            let (fill, ink) = match sign {
                                '+' => (ADDED, Color32::WHITE),
                                '-' => (REMOVED, Color32::from_white_alpha(170)),
                                _ => (Color32::TRANSPARENT, Color32::from_white_alpha(140)),
                            };
                            egui::Frame::new().fill(fill).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.label(RichText::new(format!("{number:>5} ")).font(mono(11.0)).color(Color32::from_white_alpha(120)));
                                    ui.label(RichText::new(format!("{sign} ")).font(mono(12.5)).color(match sign {
                                        '+' => Color32::from_rgb(95, 211, 138),
                                        '-' => Color32::from_rgb(240, 120, 125),
                                        _ => ink,
                                    }));
                                    ui.label(RichText::new(line.get(1..).unwrap_or("")).font(mono(12.5)).color(ink));
                                });
                            });
                        }
                    }
                });
            });
        }

        egui::CentralPanel::default().frame(frame.inner_margin(egui::Margin::symmetric(30, 26))).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(760.0);
                match open.cards[open.at] {
                    Card::Summary => {
                        let review = &open.data.review;
                        egui::Frame::new().fill(INK).inner_margin(egui::Margin::symmetric(30, 24)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&review.title).size(25.0).color(Color32::WHITE));
                            ui.add_space(14.0);
                            let lines = |sign: char| open.data.files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(sign)).count();
                            let stats = [(review.sections.len().to_string(), "parts"), (review.items.iter().filter(|item| item.kind == "finding").count().to_string(), "findings"), (review.items.iter().filter(|item| item.kind == "decision").count().to_string(), "decisions"), (open.data.files.len().to_string(), "files"), (format!("+{} −{}", lines('+'), lines('-')), "lines")];
                            ui.columns(stats.len(), |columns| {
                                for (column, (value, name)) in columns.iter_mut().zip(&stats) {
                                    column.label(RichText::new(value).size(26.0).color(Color32::WHITE));
                                    column.label(RichText::new(*name).size(12.5).color(ON_INK.gamma_multiply(0.6)));
                                }
                            });
                        });
                        ui.add_space(18.0);
                        prose(ui, &tones, &review.summary);
                        ui.separator();
                        ui.label(RichText::new("Read in this order").font(bold(14.0)).color(tones.strong));
                        for (part, section) in review.sections.iter().enumerate() {
                            ui.label(RichText::new(format!("{:02}  {}", part + 1, section.title)).font(bold(14.5)).color(tones.strong));
                        }
                    }
                    Card::Part(part) => {
                        let section = &open.data.review.sections[part];
                        ui.label(RichText::new(&section.title).size(25.0).color(tones.strong));
                        ui.label(RichText::new(&section.tier).font(mono(11.5)).color(tones.dim));
                        ui.add_space(10.0);
                        prose(ui, &tones, &section.why);
                    }
                    Card::Loose => {
                        ui.label(RichText::new("Code in no part").size(25.0).color(tones.strong));
                        prose(ui, &tones, "These hunks are in the diff, but the guide put them in no section. Nothing vouches for them: read them.");
                    }
                    Card::Item(n) => {
                        let item = open.data.review.items[n].clone();
                        let verdict = open.verdict(&item.id).map(String::from);
                        ui.horizontal(|ui| {
                            let kind = if item.kind == "finding" && !item.severity.is_empty() { &item.severity } else { &item.kind };
                            egui::Frame::new().fill(if item.severity == "blocker" { tones.rust } else { tones.raised }).stroke(Stroke::new(1.0, tones.rust)).inner_margin(egui::Margin::symmetric(7, 2)).show(ui, |ui| {
                                ui.label(RichText::new(kind).font(bold(12.0)).color(if item.severity == "blocker" { Color32::WHITE } else { tones.rust }))
                            });
                            ui.label(RichText::new(&item.id).font(mono(11.5)).color(tones.dim));
                        });
                        ui.label(RichText::new(&item.title).size(25.0).color(tones.strong));
                        ui.add_space(8.0);
                        if item.kind != "good" {
                            ui.horizontal(|ui| {
                                if button(ui, &tones, "✓ Agree", verdict.as_deref() == Some("agree")).clicked() {
                                    marked = Some((item.id.clone(), "agree"));
                                }
                                if button(ui, &tones, "✕ Dismiss", verdict.as_deref() == Some("dismiss")).clicked() {
                                    marked = Some((item.id.clone(), "dismiss"));
                                }
                            });
                        }
                        ui.add_space(10.0);
                        prose(ui, &tones, &item.body);
                        ui.separator();
                        ui.label(RichText::new(format!("Evidence ({})", if item.evidence.is_empty() { "none".into() } else { item.evidence.len().to_string() })).font(bold(14.0)).color(tones.strong));
                        for (n, evidence) in item.evidence.iter().enumerate() {
                            ui.add_space(8.0);
                            egui::Frame::new().fill(INK).inner_margin(14.0).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("evidence.{}", n + 1)).font(mono(11.5)).color(ON_INK.gamma_multiply(0.6)));
                                    ui.label(RichText::new(&evidence.provenance).font(mono(12.0)).color(ON_INK));
                                });
                                ui.label(RichText::new(&evidence.caption).font(bold(14.0)).color(Color32::WHITE));
                                if let Some(text) = evidence.text.as_ref().filter(|text| !text.trim().is_empty()) {
                                    ui.label(RichText::new(text).font(mono(12.0)).color(ON_INK));
                                }
                            });
                        }
                    }
                }
            });
        });

        if open.contents {
            let mut go = None;
            egui::Window::new("Contents").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.set_min_width(520.0);
                for (n, card) in open.cards.iter().enumerate() {
                    let label = RichText::new(open.label(*card)).font(if matches!(card, Card::Item(_)) { mono(12.5) } else { bold(13.5) });
                    if ui.selectable_label(n == open.at, label).clicked() {
                        go = Some(n);
                    }
                }
            });
            if let Some(n) = go {
                (open.at, open.contents) = (n, false);
            }
        }
        if let Some((id, verdict)) = marked {
            if let Err(problem) = open.mark(&id, verdict) {
                self.problem = problem;
            }
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 900.0]).with_title("revui"), ..Default::default() };
    eframe::run_native("revui", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
