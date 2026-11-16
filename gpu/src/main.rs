mod chat;
mod host;
mod map;

use eframe::egui::{self, Color32, FontFamily, FontId, Key, RichText, Stroke};
use lucide_icons::Icon;
use serde::Deserialize;
use host::run as revui;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Evidence {
    provenance: String,
    caption: String,
    path: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct System {
    text: String,
    touched: std::collections::BTreeMap<String, usize>,
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
    at: Option<At>,
    evidence: Vec<Evidence>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct At {
    file: String,
    line: u32,
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
    map: Option<Evidence>,
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
    preview: String,
    finish: Vec<String>,
    system: Option<System>,
    claude: String,
    threads: Vec<Thread>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Thread {
    id: String,
    file: Option<String>,
    line: Option<u32>,
    resolved: bool,
    notes: Vec<(String, String, String)>,
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
    cwd: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Card {
    Summary,
    Part(usize),
    Item(usize),
    Note(usize),
    Thread(usize),
    Loose,
}

type Painted = Vec<Vec<(Color32, String)>>;

/// A hunk's lines coloured by what the code is, in the theme the terminal viewer uses.
fn paint(path: &str, lines: &[String]) -> Painted {
    static LOADED: std::sync::OnceLock<(syntect::parsing::SyntaxSet, syntect::highlighting::Theme)> = std::sync::OnceLock::new();
    let (syntaxes, theme) = LOADED.get_or_init(|| (two_face::syntax::extra_newlines(), two_face::theme::extra().get(two_face::theme::EmbeddedThemeName::TwoDark).clone()));
    let kind = std::path::Path::new(path).extension().and_then(|kind| kind.to_str()).unwrap_or("");
    let syntax = syntaxes.find_syntax_by_extension(kind).unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let mut state = syntect::easy::HighlightLines::new(syntax, theme);
    let plain = |code: &str| vec![(Color32::from_white_alpha(200), code.to_string())];
    lines
        .iter()
        .map(|line| {
            let code = format!("{}\n", line.get(1..).unwrap_or(""));
            match state.highlight_line(&code, syntaxes) {
                Ok(spans) => spans.into_iter().map(|(style, text)| (Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b), text.trim_end_matches('\n').to_string())).collect(),
                Err(_) => plain(code.trim_end_matches('\n')),
            }
        })
        .collect()
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

struct Open {
    dir: String,
    data: Data,
    cards: Vec<Card>,
    at: usize,
    diff: bool,
    contents: Option<usize>,
    in_diff: bool,
    line: usize,
    moved: bool,
    proof: usize,
    draft: String,
    compose: bool,
    publish: Option<(usize, String)>,
    note: String,
    painted: std::collections::HashMap<(usize, usize), Painted>,
    chat: chat::Chat,
    agent: bool,
    ask: bool,
    stamp: String,
    looked: f64,
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
        cards.extend((0..data.threads.len()).map(Card::Thread));
        cards.extend(data.state["notes"].as_array().into_iter().flatten().enumerate().filter(|(_, note)| note["posted"] != true).map(|(n, _)| Card::Note(n)));
        if data.groups.len() > data.review.sections.len() {
            cards.push(Card::Loose);
        }
        let at = std::env::var("REVUI_GPU_AT").ok().and_then(|at| at.parse::<usize>().ok()).unwrap_or(0).min(cards.len() - 1);
        Ok(Self { dir: dir.into(), data, cards, at, diff: true, contents: None, in_diff: false, line: 0, moved: false, proof: 0, draft: String::new(), compose: false, publish: None, note: String::new(), painted: Default::default(), chat: Default::default(), agent: false, ask: false, stamp: host::stamp(dir), looked: 0.0 })
    }

    /// Picks up what the agent, or the terminal viewer, wrote to the bundle since it was last read.
    fn watch(&mut self, now: f64) {
        if now - self.looked < 0.7 {
            return;
        }
        self.looked = now;
        let stamp = host::stamp(&self.dir);
        if stamp != self.stamp {
            let _ = self.refresh();
        }
    }

    fn group(&self) -> Option<usize> {
        match self.cards[self.at] {
            Card::Summary => None,
            Card::Part(part) => Some(part),
            Card::Item(n) => self.data.review.items[n].section,
            Card::Note(n) => {
                let note = &self.data.state["notes"][n];
                let file = note["file"].as_str().or_else(|| self.data.review.items.iter().find(|item| Some(item.id.as_str()) == note["item"].as_str()).and_then(|item| item.at.as_ref().map(|at| at.file.as_str())));
                file.and_then(|file| self.data.groups.iter().position(|group| group.iter().any(|&(f, _)| self.data.files[f].path == file)))
            }
            Card::Thread(n) => self.data.threads[n].file.as_ref().and_then(|file| self.data.groups.iter().position(|group| group.iter().any(|&(f, _)| &self.data.files[f].path == file))),
            Card::Loose => Some(self.data.review.sections.len()),
        }
        .filter(|group| *group < self.data.groups.len())
    }

    fn label(&self, card: Card) -> String {
        match card {
            Card::Summary => "Summary".into(),
            Card::Part(part) => format!("{}. {}", part + 1, self.data.review.sections[part].title),
            Card::Item(n) => format!("{} {}", self.data.review.items[n].id, self.data.review.items[n].title),
            Card::Note(n) => self.about(n),
            Card::Thread(n) => self.said(n),
            Card::Loose => "Code in no part".into(),
        }
    }

    /// What a message to the agent is about: the finding shown, or the part.
    fn on(&self) -> Option<String> {
        match self.cards[self.at] {
            Card::Item(n) => Some(self.data.review.items[n].id.clone()),
            Card::Part(part) => Some((part + 1).to_string()),
            _ => None,
        }
    }

    /// Where a new agent starts: where another review of the same repo ran its agent, or the folder the person picks.
    fn folder(&self) -> Option<String> {
        let all: Vec<Past> = revui(&["list"], None).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        let repo = all.iter().find(|past| past.dir == self.dir).map(|past| past.repo.clone()).unwrap_or_default();
        let known = all.iter().find(|past| !repo.is_empty() && past.repo == repo && std::path::Path::new(&past.cwd).is_dir() && !past.cwd.is_empty()).map(|past| past.cwd.clone());
        #[cfg(target_arch = "wasm32")]
        return known.or(Some(String::new()));
        #[cfg(not(target_arch = "wasm32"))]
        known.or_else(|| rfd::FileDialog::new().set_title(format!("Where is your checkout of {}? The agent starts there.", if repo.is_empty() { "this code" } else { &repo })).pick_folder().map(|folder| folder.to_string_lossy().into_owned()))
    }

    /// Tells the agent: a question typed in ask mode, or the verify or artifact request for the card shown.
    fn tell(&mut self, ctx: &egui::Context, mode: &str, text: &str) {
        let on = self.on();
        let fresh = chat::session(&self.dir).is_none();
        let args: Vec<&str> = ["prompt", self.dir.as_str(), mode].into_iter().chain(on.as_deref()).chain(fresh.then_some("--fresh")).collect();
        match revui(&args, None) {
            Ok(prompt) => {
                let prompt = prompt.trim_end_matches('\n').to_string();
                let shown = if text.is_empty() { format!("{mode} {}", on.clone().unwrap_or_default()) } else { text.to_string() };
                let (claude, Some(folder)) = (self.data.claude.clone(), self.folder()) else { return self.note = "The agent needs the folder of the code under review.".into() };
                self.chat.send(ctx, &self.dir.clone(), &claude, &folder, (mode, on.as_deref()), format!("{prompt}{text}"), shown);
            }
            Err(problem) => self.note = problem,
        }
    }

    fn said(&self, n: usize) -> String {
        let thread = &self.data.threads[n];
        let who = thread.notes.first().map_or("", |note| note.0.as_str());
        match (&thread.file, thread.line) {
            (Some(file), Some(line)) => format!("@{who} on {file}:{line}"),
            _ => format!("@{who} on the whole merge request"),
        }
    }

    fn about(&self, n: usize) -> String {
        let note = &self.data.state["notes"][n];
        match (note["file"].as_str(), note["line"].as_u64(), note["item"].as_str()) {
            (Some(file), Some(line), _) => format!("Your comment on {file}:{line}"),
            (_, _, Some(item)) => format!("Your comment on {item}"),
            _ => "Your comment on the whole merge request".into(),
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
        self.save(state)
    }

    fn save(&mut self, state: serde_json::Value) -> Result<(), String> {
        revui(&["state", &self.dir], Some(&state.to_string()))?;
        self.refresh()
    }

    fn refresh(&mut self) -> Result<(), String> {
        let fresh = Self::load(&self.dir)?;
        (self.data, self.cards, self.stamp) = (fresh.data, fresh.cards, fresh.stamp);
        self.painted.clear();
        self.at = self.at.min(self.cards.len() - 1);
        Ok(())
    }

    /// The lines of the diff a cursor can sit on, in the order they are drawn: the file, the line number, and whether it is a removed line.
    fn lines(&self) -> Vec<(String, u32, bool)> {
        let mut all = Vec::new();
        for &(file, hunk) in self.group().map(|group| self.data.groups[group].as_slice()).unwrap_or_default() {
            let (path, hunk) = (&self.data.files[file].path, &self.data.files[file].hunks[hunk]);
            let (mut old, mut new) = (hunk.old_start, hunk.new_start);
            for line in &hunk.lines {
                match line.chars().next().unwrap_or(' ') {
                    '-' => (all.push((path.clone(), old, true)), old += 1).1,
                    '+' => (all.push((path.clone(), new, false)), new += 1).1,
                    '\\' => {}
                    _ => (all.push((path.clone(), new, false)), old += 1, new += 1).2,
                }
            }
        }
        all
    }

    /// Adds the draft as a comment: on the diff line under the cursor, on the finding shown, or on the whole merge request.
    fn comment(&mut self) -> Result<(), String> {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return Ok(());
        }
        let picked = self.lines().get(self.line).cloned().filter(|_| self.in_diff);
        let item = match self.cards[self.at] {
            Card::Item(n) => Some(self.data.review.items[n].id.clone()),
            _ => None,
        };
        let reply = match self.cards[self.at] {
            Card::Thread(n) => Some(self.data.threads[n].id.clone()),
            _ => None,
        };
        let note = match picked {
            Some((file, line, old)) => serde_json::json!({ "text": text, "file": file, "line": line, "start": null, "old": old, "item": null, "reply_to": null, "posted": false }),
            None => serde_json::json!({ "text": text, "file": null, "line": null, "start": null, "old": false, "item": item, "reply_to": reply, "posted": false }),
        };
        let mut state = self.data.state.clone();
        match state["notes"].as_array_mut() {
            Some(notes) => notes.push(note),
            None => state["notes"] = serde_json::json!([note]),
        }
        state["finished"] = serde_json::Value::Null;
        self.draft.clear();
        self.save(state)
    }
}

struct App {
    reviews: Vec<Past>,
    open: Option<Open>,
    problem: String,
    link: String,
    frames: u32,
    script: Vec<String>,
    tab: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        for (name, bytes) in [("geist", include_bytes!("../fonts/Geist-Regular.ttf").as_slice()), ("geist-bold", include_bytes!("../fonts/Geist-SemiBold.ttf")), ("geist-mono", include_bytes!("../fonts/GeistMono-Regular.ttf")), ("geist-serif", include_bytes!("../fonts/GeistSerif-Regular.ttf"))] {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        }
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "geist".into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "geist-mono".into());
        fonts.families.insert(FontFamily::Name("bold".into()), vec!["geist-bold".into(), "geist".into()]);
        fonts.families.insert(FontFamily::Name("serif".into()), vec!["geist-serif".into(), "geist".into()]);
        fonts.font_data.insert("lucide".into(), std::sync::Arc::new(egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES)));
        fonts.families.values_mut().for_each(|family| family.push("lucide".into()));
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        let named = host::named();
        let mut app = Self { reviews: Vec::new(), open: None, problem: String::new(), link: String::new(), frames: 0, script: std::env::var("REVUI_GPU_KEYS").map(|keys| keys.split(',').map(String::from).collect()).unwrap_or_default(), tab: false };
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

fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

/// A picture from the bundle shown in place; anything else it holds (a recording, a file) as a link that opens it.
fn artifact(ui: &mut egui::Ui, dir: &str, path: &str) {
    let shown = matches!(std::path::Path::new(path).extension().and_then(|kind| kind.to_str()), Some("png" | "jpg" | "jpeg" | "gif" | "webp"));
    match shown {
        true => drop(ui.add(egui::Image::new(host::uri(dir, path)).fit_to_exact_size(egui::vec2(ui.available_width(), 560.0)).corner_radius(0.0))),
        false => drop(ui.hyperlink_to(format!("Open {path}"), host::uri(dir, path))),
    }
}

/// The revui mark: a sheet with a check, drawn in the colours of the appearance.
fn logo(ui: &mut egui::Ui, size: f32) {
    let dark = ui.visuals().dark_mode;
    let (back, body, check) = if dark { (Color32::from_rgb(169, 172, 247), Color32::from_rgb(243, 244, 248), Color32::from_rgb(8, 9, 12)) } else { (Color32::from_rgb(91, 110, 225), Color32::from_rgb(8, 9, 12), Color32::from_rgb(243, 244, 248)) };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (size / 111.0);
    let painter = ui.painter();
    painter.line(vec![at(11.0, 3.25), at(74.65, 3.25), at(107.75, 36.35), at(107.75, 100.0)], Stroke::new(6.5 * size / 111.0, back));
    painter.add(egui::Shape::convex_polygon(vec![at(0.0, 11.0), at(65.0, 11.0), at(100.0, 46.0), at(100.0, 111.0), at(0.0, 111.0)], body, Stroke::NONE));
    painter.line(vec![at(24.0, 64.0), at(42.0, 82.0), at(76.0, 48.0)], Stroke::new(11.0 * size / 111.0, check));
}

fn icon(icon: Icon, label: &str) -> String {
    if label.is_empty() { icon.unicode().to_string() } else { format!("{}  {label}", icon.unicode()) }
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
    /// Plays REVUI_GPU_KEYS, one key or `text:` entry every few frames: how the interface is checked and recorded with no one at the keyboard.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let entry = self.frames.checked_sub(6).filter(|frame| frame % 8 == 0).and_then(|frame| self.script.get((frame / 8) as usize)).cloned().unwrap_or_default();
        match (entry.strip_prefix("text:"), Key::from_name(&entry)) {
            (Some(text), _) => raw.events.push(egui::Event::Text(text.into())),
            (None, Some(key)) => {
                for pressed in [true, false] {
                    raw.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() });
                }
                if let (1, Some(letter)) = (entry.len(), entry.chars().next()) {
                    raw.events.push(egui::Event::Text(letter.to_lowercase().to_string()));
                }
            }
            _ => {}
        }
        // Tab moves between the card and the diff here, as in the terminal, so it never reaches egui's own walk through the widgets.
        if self.open.is_some() {
            let before = raw.events.len();
            raw.events.retain(|event| !matches!(event, egui::Event::Key { key: Key::Tab, modifiers, .. } if !modifiers.shift));
            self.tab |= raw.events.len() != before;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let tones = tones(ui.visuals().dark_mode);
        let ctx = ui.ctx().clone();
        self.frames += 1;
        #[cfg(not(target_arch = "wasm32"))]
        if let (true, Ok(path)) = (self.frames == 12 + 8 * self.script.len() as u32, std::env::var("REVUI_GPU_SHOT")) {
            let quit = ctx.clone();
            ctx.request_screenshot(move |image| {
                let file = std::fs::File::create(&path).expect("screenshot file");
                let mut encoder = png::Encoder::new(file, image.width() as u32, image.height() as u32);
                encoder.set_color(png::ColorType::Rgba);
                let _ = encoder.write_header().and_then(|mut writer| writer.write_image_data(image.as_raw()));
                quit.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        if self.frames < 16 + 8 * self.script.len() as u32 {
            ctx.request_repaint();
        }
        let frame = egui::Frame::new().fill(tones.bg);
        if self.open.is_none() {
            let mut pick = None;
            egui::CentralPanel::default().frame(frame.inner_margin(32.0)).show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(680.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            logo(ui, 26.0);
                            ui.label(RichText::new("revui").font(serif(24.0)).color(tones.strong));
                        });
                        ui.add_space(14.0);
                        ui.label(RichText::new("Merge request or pull request link").font(bold(13.0)).color(tones.strong));
                        ui.horizontal(|ui| {
                            let field = ui.add(egui::TextEdit::singleline(&mut self.link).desired_width(560.0).hint_text("https://gitlab.com/group/project/-/merge_requests/123"));
                            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                            if (button(ui, &tones, "Open", true).clicked() || entered) && !self.link.trim().is_empty() {
                                match revui(&["fetch", self.link.trim()], None) {
                                    Ok(dir) => pick = Some(dir.trim().to_string()),
                                    Err(problem) => self.problem = problem,
                                }
                            }
                        });
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
                self.link.clear();
                self.show(&dir);
            }
            return;
        }

        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |key: Key| !typing && ctx.input(|input| input.key_pressed(key) && !input.modifiers.command && !input.modifiers.ctrl);
        if pressed(Key::Q) && self.open.as_ref().is_some_and(|open| open.contents.is_none() && open.publish.is_none()) {
            return self.list();
        }
        let open = self.open.as_mut().expect("a review is open");
        let modal = open.contents.is_some() || open.publish.is_some();
        let pressed = |key: Key| !modal && pressed(key);
        let step = (pressed(Key::L) || pressed(Key::ArrowRight)) as isize - (pressed(Key::H) || pressed(Key::ArrowLeft)) as isize;
        if step != 0 {
            open.at = (open.at as isize + step).clamp(0, open.cards.len() as isize - 1) as usize;
            (open.in_diff, open.line, open.proof) = (false, 0, 0);
        }
        open.diff ^= pressed(Key::D);
        if pressed(Key::B) {
            open.contents = Some(open.at);
        }
        if std::mem::take(&mut self.tab) && !modal && open.diff && open.group().is_some() {
            (open.in_diff, open.moved) = (!open.in_diff, true);
        }
        let down = (pressed(Key::J) || pressed(Key::ArrowDown)) as isize - (pressed(Key::K) || pressed(Key::ArrowUp)) as isize;
        if down != 0 && open.in_diff {
            (open.line, open.moved) = ((open.line as isize + down).clamp(0, open.lines().len().saturating_sub(1) as isize) as usize, true);
        } else if let (true, Card::Item(n)) = (down != 0, open.cards[open.at]) {
            open.proof = (open.proof as isize + down).clamp(0, open.data.review.items[n].evidence.len().saturating_sub(1) as isize) as usize;
        }
        open.compose |= pressed(Key::C);
        if pressed(Key::T) {
            (open.agent, open.ask) = (true, true);
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::J)) {
            open.agent = !open.agent;
        }
        if pressed(Key::V) {
            open.agent = true;
            open.tell(&ctx, "verify", "");
        }
        if pressed(Key::I) {
            open.agent = true;
            open.tell(&ctx, "artifact", "");
        }
        open.chat.take();
        open.watch(ctx.input(|input| input.time));
        ctx.request_repaint_after(std::time::Duration::from_millis(800));
        if pressed(Key::P) {
            open.publish = Some((0, open.data.preview.clone()));
        }
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
                logo(ui, 20.0);
                ui.label(RichText::new(&open.data.review.title).font(serif(16.0)).color(tones.strong));
                ui.label(RichText::new(format!("{} of {}", open.at + 1, open.cards.len())).font(mono(11.5)).color(tones.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let count = if open.data.outgoing > 0 { open.data.outgoing.to_string() } else { String::new() };
                    if button(ui, &tones, &icon(Icon::Send, &count), true).on_hover_text("Publish (p)").clicked() {
                        open.publish = Some((0, open.data.preview.clone()));
                    }
                    open.agent ^= button(ui, &tones, &icon(Icon::Bot, if open.chat.busy { "working" } else { "" }), open.agent).on_hover_text("Agent (t)").clicked();
                    open.diff ^= button(ui, &tones, &icon(Icon::FileDiff, ""), open.diff).on_hover_text("Diff (d)").clicked();
                    if button(ui, &tones, &icon(Icon::List, ""), false).on_hover_text("Contents (b)").clicked() {
                        open.contents = Some(open.at);
                    }
                });
            });
        });
        egui::Panel::bottom("keys").frame(frame.inner_margin(egui::Margin::symmetric(14, 5))).show_inside(ui, |ui| {
            let here = if open.in_diff { "j k line · c comment on the line · tab card" } else { "j k evidence · a agree · x dismiss · c comment · tab diff" };
            let said = if open.note.is_empty() { String::new() } else { format!("   {}", open.note) };
            ui.label(RichText::new(format!("l next · h back · {here} · t ask · v verify · i diagram · d diff · b contents · p publish · q reviews{said}")).font(mono(11.5)).color(tones.dim));
        });

        if open.agent {
            egui::Panel::right("agent").default_size(380.0).frame(frame.inner_margin(14.0).stroke(Stroke::new(1.0, tones.line))).show_inside(ui, |ui| {
                ui.label(RichText::new("Agent").font(mono(13.0)).color(tones.strong).strong());
                ui.add_space(6.0);
                egui::Panel::bottom("say").frame(egui::Frame::new()).show_inside(ui, |ui| {
                    let about = open.on().map_or("Ask about this review".to_string(), |on| format!("Ask about {on}"));
                    let field = ui.add(egui::TextEdit::multiline(&mut open.chat.draft).desired_width(f32::INFINITY).desired_rows(3).hint_text(about));
                    if std::mem::take(&mut open.ask) {
                        field.request_focus();
                    }
                    let entered = field.has_focus() && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift);
                    ui.horizontal(|ui| {
                        let verify = button(ui, &tones, &icon(Icon::ShieldCheck, "Verify"), false).clicked();
                        let draw = button(ui, &tones, &icon(Icon::Workflow, "Diagram"), false).clicked();
                        let stop = open.chat.busy && button(ui, &tones, &icon(Icon::Square, "Stop"), false).clicked();
                        let send = !open.chat.busy && (button(ui, &tones, &icon(Icon::ArrowUp, "Send"), true).clicked() || entered);
                        if stop {
                            open.chat.stop();
                        }
                        if verify {
                            open.tell(ui.ctx(), "verify", "");
                        }
                        if draw {
                            open.tell(ui.ctx(), "artifact", "");
                        }
                        if send && !open.chat.draft.trim().is_empty() {
                            let text = std::mem::take(&mut open.chat.draft);
                            open.tell(ui.ctx(), "ask", text.trim());
                        }
                    });
                });
                egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
                    if open.chat.log.is_empty() {
                        ui.label(RichText::new("This review's agent. It wrote the guide, so it already knows the change. Ask it about the card you are on.").size(13.0).color(tones.dim));
                    }
                    for said in &open.chat.log {
                        match said {
                            chat::Said::Me(text) => drop(egui::Frame::new().fill(tones.accent).inner_margin(8.0).show(ui, |ui| ui.label(RichText::new(text).size(13.5).color(tones.on_accent)))),
                            chat::Said::Agent(text) => drop(ui.add(egui::Label::new(RichText::new(text).size(13.5).color(tones.text)).selectable(true))),
                            chat::Said::Tool(text) => drop(ui.label(RichText::new(text).font(mono(11.5)).color(tones.dim))),
                            chat::Said::Problem(text) => drop(ui.label(RichText::new(text).size(13.0).color(tones.rust))),
                        }
                        ui.add_space(6.0);
                    }
                    if open.chat.busy {
                        ui.label(RichText::new("working").font(mono(11.5)).color(tones.dim));
                    }
                });
            });
        }

        if let (true, Some(group)) = (open.diff, open.group()) {
            egui::Panel::right("diff").default_size(620.0).frame(egui::Frame::new().fill(INK)).show_inside(ui, |ui| {
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut last = usize::MAX;
                    let mut row = 0;
                    for &(file, hunk_at) in &open.data.groups[group] {
                        let (shown, hunk) = (&open.data.files[file], &open.data.files[file].hunks[hunk_at]);
                        let painted = open.painted.entry((file, hunk_at)).or_insert_with(|| paint(&shown.path, &hunk.lines));
                        if file != last {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 9)).show(ui, |ui| ui.label(RichText::new(&shown.path).font(mono(13.0)).color(Color32::WHITE).strong()));
                            last = file;
                        }
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| ui.label(RichText::new(&hunk.header).font(mono(11.0)).color(Color32::from_white_alpha(120))));
                        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
                        for (nth, line) in hunk.lines.iter().enumerate() {
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
                            let under = open.in_diff && sign != '\\' && row == open.line;
                            row += (sign != '\\') as usize;
                            let drawn = egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, if under { Color32::WHITE } else { Color32::TRANSPARENT })).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.label(RichText::new(format!("{number:>5} ")).font(mono(11.0)).color(Color32::from_white_alpha(120)));
                                    ui.label(RichText::new(format!("{sign} ")).font(mono(12.5)).color(match sign {
                                        '+' => Color32::from_rgb(95, 211, 138),
                                        '-' => Color32::from_rgb(240, 120, 125),
                                        _ => ink,
                                    }));
                                    let mut code = egui::text::LayoutJob::default();
                                    for (color, text) in &painted[nth] {
                                        let color = if sign == '-' { color.gamma_multiply(0.6) } else if sign == '+' { *color } else { color.gamma_multiply(0.75) };
                                        code.append(text, 0.0, egui::TextFormat { font_id: mono(12.5), color, ..Default::default() });
                                    }
                                    let _ = ink;
                                    ui.add(egui::Label::new(code).extend());
                                });
                            });
                            if under && open.moved {
                                drawn.response.scroll_to_me(None);
                            }
                        }
                    }
                });
            });
        }

        egui::CentralPanel::default().frame(frame.inner_margin(egui::Margin::symmetric(30, 26))).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(760.0_f32.min(ui.available_width()));
                match open.cards[open.at] {
                    Card::Summary => {
                        let review = &open.data.review;
                        egui::Frame::new().fill(INK).inner_margin(egui::Margin::symmetric(30, 24)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&review.title).font(serif(25.0)).color(Color32::WHITE));
                            ui.add_space(14.0);
                            let lines = |sign: char| open.data.files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(sign)).count();
                            let stats = [(review.sections.len().to_string(), "parts"), (review.items.iter().filter(|item| item.kind == "finding").count().to_string(), "findings"), (review.items.iter().filter(|item| item.kind == "decision").count().to_string(), "decisions"), (open.data.files.len().to_string(), "files"), (format!("+{} −{}", lines('+'), lines('-')), "lines")];
                            ui.columns(stats.len(), |columns| {
                                for (column, (value, name)) in columns.iter_mut().zip(&stats) {
                                    column.label(RichText::new(value).font(serif(26.0)).color(Color32::WHITE));
                                    column.label(RichText::new(*name).size(12.5).color(ON_INK.gamma_multiply(0.6)));
                                }
                            });
                        });
                        ui.add_space(18.0);
                        prose(ui, &tones, &review.summary);
                        ui.separator();
                        ui.label(RichText::new("Where it sits").font(bold(14.0)).color(tones.strong));
                        ui.add_space(6.0);
                        let own = review.map.as_ref().and_then(|map| map.path.as_ref()).and_then(|path| host::text(&open.dir, &std::path::Path::new(path).with_extension("mmd").to_string_lossy()));
                        let (source, touched) = match (&own, &open.data.system) {
                            (Some(text), _) => (text.as_str(), None),
                            (None, Some(system)) => (system.text.as_str(), Some(&system.touched)),
                            _ => ("", None),
                        };
                        let (mut nodes, edges) = map::parse(source);
                        if let Some(touched) = touched {
                            nodes.iter_mut().for_each(|node| node.state = touched.contains_key(&node.id).then_some("changed"));
                        }
                        let caption = match (&review.map, touched) {
                            (Some(map), None) => map.caption.clone(),
                            _ => {
                                let hit: Vec<&str> = nodes.iter().filter(|node| node.state.is_some()).map(|node| node.label.as_str()).collect();
                                format!("Touches {} of {} parts of the system: {}", hit.len(), nodes.len(), hit.join(", "))
                            }
                        };
                        egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.line)).inner_margin(14.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if nodes.len() >= 2 {
                                let look = map::Look { raised: tones.raised, top: tones.line.gamma_multiply(0.8), side: tones.line, edge: tones.dim, text: tones.strong };
                                map::draw(ui, &look, &nodes, &edges);
                                ui.label(RichText::new(caption).font(bold(13.5)).color(tones.strong));
                                if let Some(text) = review.map.as_ref().and_then(|map| map.text.as_ref()).filter(|_| touched.is_none()) {
                                    ui.label(RichText::new(text).size(13.0).color(tones.text));
                                }
                            } else if let Some(path) = review.map.as_ref().and_then(|map| map.path.as_ref()) {
                                artifact(ui, &open.dir, path);
                            } else {
                                let mut folders: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
                                for file in &open.data.files {
                                    let row = folders.entry(file.path.split_once('/').map_or(".", |(folder, _)| folder)).or_default();
                                    let count = |sign: char| file.hunks.iter().flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(sign)).count();
                                    *row = (row.0 + count('+'), row.1 + count('-'));
                                }
                                let most = folders.values().map(|(added, removed)| added + removed).max().unwrap_or(1).max(1) as f32;
                                for (folder, (added, removed)) in &folders {
                                    ui.horizontal(|ui| {
                                        ui.add_sized([120.0, 16.0], egui::Label::new(RichText::new(*folder).font(mono(12.0)).color(tones.strong)).halign(egui::Align::Min));
                                        let wide = (ui.available_width() - 130.0).max(40.0);
                                        let (bar, _) = ui.allocate_exact_size(egui::vec2(wide, 10.0), egui::Sense::hover());
                                        let green = bar.width() * *added as f32 / most;
                                        ui.painter().rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(green, 10.0)), 0.0, Color32::from_rgb(47, 143, 85));
                                        ui.painter().rect_filled(egui::Rect::from_min_size(bar.min + egui::vec2(green, 0.0), egui::vec2(bar.width() * *removed as f32 / most, 10.0)), 0.0, Color32::from_rgb(194, 71, 77));
                                        ui.label(RichText::new(format!("+{added} −{removed}")).size(12.0).color(tones.dim));
                                    });
                                }
                            }
                        });
                        ui.add_space(14.0);
                        ui.label(RichText::new("Read in this order").font(bold(14.0)).color(tones.strong));
                        for (part, section) in review.sections.iter().enumerate() {
                            ui.label(RichText::new(format!("{:02}  {}", part + 1, section.title)).font(bold(14.5)).color(tones.strong));
                        }
                    }
                    Card::Part(part) => {
                        let section = &open.data.review.sections[part];
                        ui.label(RichText::new(&section.title).font(serif(25.0)).color(tones.strong));
                        ui.label(RichText::new(&section.tier).font(mono(11.5)).color(tones.dim));
                        ui.add_space(10.0);
                        prose(ui, &tones, &section.why);
                    }
                    Card::Thread(n) => {
                        let thread = open.data.threads[n].clone();
                        ui.label(RichText::new(open.said(n)).font(serif(25.0)).color(tones.strong));
                        ui.label(RichText::new(if thread.resolved { "resolved · already on the merge request" } else { "already on the merge request" }).font(mono(11.5)).color(tones.dim));
                        for (who, body, when) in &thread.notes {
                            ui.add_space(10.0);
                            ui.label(RichText::new(format!("@{who} · {when}")).font(bold(13.0)).color(tones.strong));
                            prose(ui, &tones, body);
                        }
                    }
                    Card::Note(n) => {
                        let text = open.data.state["notes"][n]["text"].as_str().unwrap_or("").to_string();
                        ui.label(RichText::new(open.about(n)).font(serif(25.0)).color(tones.strong));
                        ui.add_space(10.0);
                        prose(ui, &tones, &text);
                        if button(ui, &tones, &icon(Icon::Trash2, "Delete this comment"), false).clicked() {
                            let mut state = open.data.state.clone();
                            if let Some(notes) = state["notes"].as_array_mut() {
                                notes.remove(n);
                            }
                            if let Err(problem) = open.save(state) {
                                open.note = problem;
                            }
                        }
                    }
                    Card::Loose => {
                        ui.label(RichText::new("Code in no part").font(serif(25.0)).color(tones.strong));
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
                        ui.label(RichText::new(&item.title).font(serif(25.0)).color(tones.strong));
                        ui.add_space(8.0);
                        if item.kind != "good" {
                            ui.horizontal(|ui| {
                                if button(ui, &tones, &icon(Icon::Check, "Agree"), verdict.as_deref() == Some("agree")).clicked() {
                                    marked = Some((item.id.clone(), "agree"));
                                }
                                if button(ui, &tones, &icon(Icon::X, "Dismiss"), verdict.as_deref() == Some("dismiss")).clicked() {
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
                            let current = !open.in_diff && n == open.proof;
                            egui::Frame::new().fill(INK).stroke(Stroke::new(if current { 2.0 } else { 0.0 }, tones.rust)).inner_margin(14.0).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("evidence.{}", n + 1)).font(mono(11.5)).color(ON_INK.gamma_multiply(0.6)));
                                    ui.label(RichText::new(&evidence.provenance).font(mono(12.0)).color(ON_INK));
                                });
                                ui.label(RichText::new(&evidence.caption).font(bold(14.0)).color(Color32::WHITE));
                                if let Some(path) = &evidence.path {
                                    artifact(ui, &open.dir, path);
                                }
                                if let Some(text) = evidence.text.as_ref().filter(|text| !text.trim().is_empty()) {
                                    ui.label(RichText::new(text).font(mono(12.0)).color(ON_INK));
                                }
                            });
                        }
                    }
                }
                ui.add_space(18.0);
                let about = match (open.lines().get(open.line).filter(|_| open.in_diff), open.cards[open.at]) {
                    (Some((file, line, _)), _) => format!("Comment on {file}:{line}"),
                    (None, Card::Item(n)) => format!("Comment on {}", open.data.review.items[n].id),
                    (None, Card::Thread(_)) => "Reply in this thread".into(),
                    _ => "Comment on the whole merge request".into(),
                };
                ui.label(RichText::new(about).font(bold(14.0)).color(tones.strong));
                let field = ui.add(egui::TextEdit::multiline(&mut open.draft).desired_width(f32::INFINITY).desired_rows(3));
                if std::mem::take(&mut open.compose) {
                    field.request_focus();
                    field.scroll_to_me(None);
                }
                if button(ui, &tones, "Save comment", false).clicked() {
                    if let Err(problem) = open.comment() {
                        open.note = problem;
                    }
                }
            });
        });
        open.moved = false;

        if let Some((mut choice, mut text)) = open.publish.take() {
            let (mut keep, mut send) = (true, false);
            egui::Window::new("Publish your review").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.set_width(680.0);
                ui.label(RichText::new("This is exactly what goes to the merge request. Nothing is posted until you confirm.").color(tones.text));
                for (n, how) in open.data.finish.iter().enumerate() {
                    ui.radio_value(&mut choice, n, how);
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| ui.add(egui::TextEdit::multiline(&mut text).font(mono(12.0)).desired_width(f32::INFINITY)));
                ui.horizontal(|ui| {
                    keep = !button(ui, &tones, "Cancel", false).clicked() && !ui.input(|input| input.key_pressed(Key::Escape));
                    send = button(ui, &tones, &icon(Icon::Send, "Publish"), true).clicked();
                });
            });
            if send {
                open.note = match revui(&["publish", &open.dir, &(choice + 1).to_string()], Some(&text)) {
                    Ok(_) => "Published.".into(),
                    Err(problem) => problem,
                };
                let _ = open.refresh();
            } else if keep {
                open.publish = Some((choice, text));
            }
        }

        if let Some(mut on) = open.contents {
            let mut go = None;
            let (down, enter, leave) = ctx.input(|input| ((input.key_pressed(Key::J) || input.key_pressed(Key::ArrowDown)) as isize - (input.key_pressed(Key::K) || input.key_pressed(Key::ArrowUp)) as isize, input.key_pressed(Key::Enter), input.key_pressed(Key::Escape)));
            on = (on as isize + down).clamp(0, open.cards.len() as isize - 1) as usize;
            egui::Window::new("Contents").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.set_min_width(520.0);
                for (n, card) in open.cards.iter().enumerate() {
                    let label = RichText::new(open.label(*card)).font(if matches!(card, Card::Item(_)) { mono(12.5) } else { bold(13.5) });
                    if ui.selectable_label(n == on, label).clicked() {
                        go = Some(n);
                    }
                }
            });
            open.contents = if leave { None } else { Some(on) };
            if let Some(n) = go.or(enter.then_some(on)) {
                (open.at, open.contents, open.in_diff, open.line, open.proof) = (n, None, false, 0, 0);
            }
        }
        if let Some((id, verdict)) = marked {
            if let Err(problem) = open.mark(&id, verdict) {
                self.problem = problem;
            }
        }
    }
}

/// Started from the Dock, an app gets a bare PATH and no revui: take the PATH a login shell builds, and the revui that ships beside this binary.
#[cfg(not(target_arch = "wasm32"))]
fn settle() {
    use std::process::{Command, Stdio};
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("revui"))).filter(|bin| bin.exists());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let asked = Command::new(shell).args(["-ilc", "printf '\\n%s' \"$PATH\""]).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let path = asked.ok().and_then(|out| String::from_utf8_lossy(&out.stdout).lines().last().map(str::to_string)).filter(|path| path.contains('/'));
    // SAFETY: nothing else runs yet, so no other thread reads the environment while it changes.
    unsafe {
        if let (Some(bin), Err(_)) = (beside, std::env::var("REVUI_BIN")) {
            std::env::set_var("REVUI_BIN", bin);
        }
        if let (Some(path), false) = (path, std::env::var("TERM").is_ok()) {
            std::env::set_var("PATH", path);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    settle();
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 900.0]).with_title("revui"), ..Default::default() };
    eframe::run_native("revui", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window().and_then(|window| window.document()).and_then(|document| document.get_element_by_id("revui")).and_then(|found| found.dyn_into::<web_sys::HtmlCanvasElement>().ok()).expect("the page has a canvas");
        eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(App::new(cc))))).await.expect("revui could not start");
    });
}
