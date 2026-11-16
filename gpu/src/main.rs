mod chat;
mod fx;
mod host;
mod map;
mod play;

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
    plain: String,
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
    plain: String,
    sections: Vec<Section>,
    items: Vec<Item>,
    map: Option<Evidence>,
}

/// Where references in prose open: the change's own URL (its neighbours differ by the number) and the ticket tracker.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Links {
    change: Option<String>,
    tickets: Option<String>,
}

impl Links {
    fn of(&self, word: &str) -> Option<String> {
        if word.starts_with("http") {
            return Some(word.into());
        }
        match word.strip_prefix(['!', '#']) {
            Some(number) => {
                let (base, _) = self.change.as_ref().filter(|url| url.contains("/pull/") == word.starts_with('#'))?.rsplit_once('/')?;
                Some(format!("{base}/{number}"))
            }
            None => {
                let jargon = ["UTF", "SHA", "AES", "TLS", "ISO", "HTTP", "MD", "RSA", "GPT", "ES", "UTC"].contains(&word.split('-').next()?);
                self.tickets.as_ref().filter(|_| !jargon).map(|url| url.replace("{}", word))
            }
        }
    }
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
    product: bool,
    #[serde(rename = "refs")]
    links: Links,
    quoted: std::collections::BTreeMap<String, bool>,
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
    /// The row where a selection made with V began; it runs from there to the cursor.
    range: Option<usize>,
    floating: bool,
    minimized: bool,
    asking: bool,
    leave: bool,
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
        let mut open = Self { dir: dir.into(), cards, at, diff: !data.product, data, contents: None, in_diff: false, line: 0, moved: false, proof: 0, draft: String::new(), compose: false, publish: None, note: String::new(), painted: Default::default(), chat: Default::default(), agent: false, ask: false, range: None, floating: false, minimized: false, asking: false, leave: false, stamp: host::stamp(dir), looked: 0.0 };
        open.chat.sync(dir);
        Ok(open)
    }

    /// Picks up what the agent, or the terminal viewer, wrote to the bundle since it was last read.
    fn watch(&mut self, now: f64) {
        if now - self.looked < 0.7 {
            return;
        }
        self.looked = now;
        self.chat.sync(&self.dir);
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

    /// Where a new agent starts: where another review of the same repo ran its agent, else the repo as `revui checkout`
    /// finds it under the folder the person keeps their code in, which they are asked for once.
    fn folder(&self) -> Option<String> {
        let all: Vec<Past> = revui(&["list"], None).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        let repo = all.iter().find(|past| past.dir == self.dir).map(|past| past.repo.clone()).unwrap_or_default();
        let known = all.iter().find(|past| !repo.is_empty() && past.repo == repo && std::path::Path::new(&past.cwd).is_dir() && !past.cwd.is_empty()).map(|past| past.cwd.clone());
        #[cfg(target_arch = "wasm32")]
        return known.or(Some(String::new()));
        #[cfg(not(target_arch = "wasm32"))]
        known.or_else(|| revui(&["checkout", &self.dir], None).ok()).or_else(|| self.root()).map(|found| found.trim().to_string())
    }

    /// Asks where the person keeps their code, has revui remember it, and gives back where this review's repo is under it.
    #[cfg(not(target_arch = "wasm32"))]
    fn root(&self) -> Option<String> {
        let root = rfd::FileDialog::new().set_title("Where do you keep your code? Pick the folder your checkouts are under: revui finds each repo there.").pick_folder()?;
        revui(&["checkout", &self.dir, &root.to_string_lossy()], None).ok()
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
        let who = thread.notes.first().map_or("", |note| note.0.trim_start_matches('@'));
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

    /// What a comment made from the diff lands on: the file, its last line, the side, and the first line when V selected several.
    /// A selection is cut to the stretch that shares the last row's file and side, since one comment cannot span more.
    fn picked(&self) -> Option<(String, u32, bool, Option<u32>)> {
        let rows = self.lines();
        let (low, high) = self.range.map_or((self.line, self.line), |from| (from.min(self.line), from.max(self.line)));
        let (file, line, old) = rows.get(high).filter(|_| self.in_diff)?.clone();
        let first = (low..high).rev().take_while(|&row| rows[row].0 == file && rows[row].2 == old).last().map(|row| rows[row].1);
        Some((file, line, old, first))
    }

    /// Adds the draft as a comment: on the diff line under the cursor, on the finding shown, or on the whole merge request.
    fn comment(&mut self) -> Result<(), String> {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return Ok(());
        }
        let picked = self.picked();
        self.range = None;
        let item = match self.cards[self.at] {
            Card::Item(n) => Some(self.data.review.items[n].id.clone()),
            _ => None,
        };
        let reply = match self.cards[self.at] {
            Card::Thread(n) => Some(self.data.threads[n].id.clone()),
            _ => None,
        };
        let note = match picked {
            Some((file, line, old, start)) => serde_json::json!({ "text": text, "file": file, "line": line, "start": start, "old": old, "item": null, "reply_to": null, "posted": false }),
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
    play: play::Play,
    dressed: Option<bool>,
    captured: std::sync::Arc<std::sync::atomic::AtomicU32>,
    tab: bool,
    row: usize,
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
        fx::install(cc);
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        let named = host::named();
        let mut app = Self { reviews: Vec::new(), open: None, problem: String::new(), link: String::new(), play: play::Play::load(), dressed: None, captured: Default::default(), tab: false, row: 0 };
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

/// A picture from the bundle shown in place; anything else it holds (a recording, a file) as a link. Either opens in the machine's own viewer or player.
fn artifact(ui: &mut egui::Ui, dir: &str, path: &str) {
    let shown = matches!(std::path::Path::new(path).extension().and_then(|kind| kind.to_str()), Some("png" | "jpg" | "jpeg" | "gif" | "webp"));
    let hit = match shown {
        true => ui.add(egui::Image::new(host::uri(dir, path)).fit_to_exact_size(egui::vec2(ui.available_width(), 560.0)).corner_radius(0.0).sense(egui::Sense::click())),
        false => ui.link(format!("Open {path}")),
    };
    if hit.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open it (Enter)").clicked() {
        host::open(dir, path);
    }
}

/// The revui mark: a sheet with a check, drawn in the colours of the appearance.
fn logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let (back, body, check) = if dark { (Color32::from_rgb(169, 172, 247), Color32::from_rgb(243, 244, 248), Color32::from_rgb(8, 9, 12)) } else { (Color32::from_rgb(91, 110, 225), Color32::from_rgb(8, 9, 12), Color32::from_rgb(243, 244, 248)) };
    let (rect, mark) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (size / 111.0);
    let painter = ui.painter();
    painter.line(vec![at(11.0, 3.25), at(74.65, 3.25), at(107.75, 36.35), at(107.75, 100.0)], Stroke::new(6.5 * size / 111.0, back));
    painter.add(egui::Shape::convex_polygon(vec![at(0.0, 11.0), at(65.0, 11.0), at(100.0, 46.0), at(100.0, 111.0), at(0.0, 111.0)], body, Stroke::NONE));
    painter.line(vec![at(24.0, 64.0), at(42.0, 82.0), at(76.0, 48.0)], Stroke::new(11.0 * size / 111.0, check));
    mark
}

fn icon(icon: Icon, label: &str) -> String {
    if label.is_empty() { icon.unicode().to_string() } else { format!("{}  {label}", icon.unicode()) }
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// One run of prose as laid-out text: file paths, routes, calls and constants are set in mono on a tint, the way backticks would, and `**this**` in bold.
fn inline(tones: &Tones, text: &str, size: f32) -> egui::text::LayoutJob {
    static CODE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\*\*[^*\n]+\*\*|`[^`]+`|[\w.-]+/[\w./-]*\.\w+(?::\d+)?|\B/[a-z][\w/:-]+|\b\w[\w-]+\.[a-z]{2,4}(?::\d+)?\b|\b[A-Za-z_][\w.]*\([^()\s]*\)|\b[A-Z][A-Z0-9]*_[A-Z0-9_]+\b|\b[a-z]+_[a-z_]+\b").expect("a fixed pattern"));
    let plain = egui::TextFormat { font_id: FontId::proportional(size), color: tones.text, line_height: Some((size * 1.53).round()), ..Default::default() };
    let code = egui::TextFormat { font_id: mono(size - 2.0), color: tones.strong, background: tones.raised, ..plain.clone() };
    let strong = egui::TextFormat { font_id: bold(size), color: tones.strong, ..plain.clone() };
    let mut job = egui::text::LayoutJob::default();
    let mut from = 0;
    for found in CODE.find_iter(text) {
        job.append(&text[from..found.start()], 0.0, plain.clone());
        match found.as_str().strip_prefix("**") {
            Some(said) => job.append(said.trim_end_matches('*'), 0.0, strong.clone()),
            None => job.append(found.as_str().trim_matches('`'), 0.0, code.clone()),
        }
        from = found.end();
    }
    job.append(&text[from..], 0.0, plain);
    job
}

/// A line of prose that wraps, with merge request numbers, ticket ids and URLs as links.
fn line(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str, size: f32) {
    static LINK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"https?://[^\s)>\]]+|\B[!#]\d+\b|\b[A-Z]{2,}-\d+\b").expect("a fixed pattern"));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut from = 0;
        for found in LINK.find_iter(text) {
            let Some(url) = links.of(found.as_str()) else { continue };
            ui.label(inline(tones, &text[from..found.start()], size));
            let mut job = inline(tones, "", size);
            job.append(found.as_str(), 0.0, egui::TextFormat { font_id: FontId::proportional(size), color: tones.accent, line_height: Some((size * 1.53).round()), underline: Stroke::new(1.0, tones.accent.gamma_multiply(0.5)), ..Default::default() });
            if ui.add(egui::Label::new(job).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&url).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            from = found.end();
        }
        ui.label(inline(tones, &text[from..], size));
    });
}

fn prose(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str) {
    prose_at(ui, tones, links, text, 15.0);
}

fn prose_at(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str, size: f32) {
    ui.scope(|ui| {
        ui.set_max_width(680.0_f32.min(ui.available_width()));
        for block in text.trim().split("\n\n").flat_map(|block| block.split("\n# ").enumerate().map(|(n, part)| if n == 0 { part.to_string() } else { format!("# {part}") })) {
            let mut lines = block.lines().peekable();
            if let Some(heading) = lines.peek().and_then(|line| line.strip_prefix("# ")) {
                ui.add_space(12.0);
                ui.label(RichText::new(heading).font(bold(size)).color(tones.strong));
                ui.add_space(2.0);
                lines.next();
            }
            for line in lines.filter(|line| !line.trim().is_empty()) {
                let (mark, rest) = line.trim_start().split_once(' ').unwrap_or(("", line));
                let listed = mark == "-" || mark == "*" || mark.strip_suffix('.').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                if listed {
                    ui.horizontal_top(|ui| {
                        ui.add_sized([22.0, (size * 1.53).round()], egui::Label::new(RichText::new(if mark.ends_with('.') { mark } else { "•" }).font(mono(size - 2.0)).color(tones.dim)));
                        self::line(ui, tones, links, rest, size);
                    });
                } else {
                    self::line(ui, tones, links, line, size);
                }
            }
            ui.add_space(8.0);
        }
    });
}

fn button(ui: &mut egui::Ui, tones: &Tones, label: &str, on: bool) -> egui::Response {
    let (fill, ink) = if on { (tones.accent, tones.on_accent) } else { (tones.raised, tones.strong) };
    let tight = std::mem::replace(&mut ui.spacing_mut().button_padding, egui::vec2(12.0, 7.0));
    let made = ui.add(egui::Button::new(RichText::new(label).font(bold(14.0)).color(ink)).fill(fill).stroke(Stroke::new(1.0, if on { tones.accent } else { tones.text })).corner_radius(0.0));
    ui.spacing_mut().button_padding = tight;
    fx::touch(ui, made, ink)
}

/// What the agent said: prose as the cards set it, and what it fenced as code on a dark slab.
fn reply(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str) {
    for (n, part) in text.split("```").enumerate() {
        if n % 2 == 1 {
            let code = part.split_once('\n').map_or(part, |(_, code)| code).trim_end();
            egui::Frame::new().fill(INK).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(code).font(mono(12.0)).color(ON_INK)).selectable(true));
            });
        } else if !part.trim().is_empty() {
            prose_at(ui, tones, links, part, 13.5);
        }
    }
}

/// A run of the agent's tool calls rolled up into one line. It opens to the calls, and each call opens to what it
/// ran and what came back.
fn steps(ui: &mut egui::Ui, tones: &Tones, steps: &[&chat::Step], live: bool) {
    let flip = |ui: &mut egui::Ui, key: egui::Id, text: egui::WidgetText| {
        let shown = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
        let row = ui.add(egui::Label::new(text).truncate().selectable(false).sense(egui::Sense::click()));
        let pressed = fx::touch(ui, row, tones.strong).clicked();
        if pressed {
            ui.data_mut(|data| data.insert_temp(key, !shown));
        }
        shown != pressed
    };
    let mut kinds: Vec<(&str, usize)> = Vec::new();
    for step in steps {
        match kinds.iter_mut().find(|(name, _)| *name == step.name) {
            Some((_, times)) => *times += 1,
            None => kinds.push((&step.name, 1)),
        }
    }
    let kinds: Vec<String> = kinds.iter().map(|(name, times)| if *times > 1 { format!("{name} ×{times}") } else { name.to_string() }).collect();
    let failed = steps.iter().filter(|step| step.result.as_ref().is_some_and(|(_, failed)| *failed)).count();
    let key = egui::Id::new(("steps", &steps[0].id));
    let shown = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    let head = format!("{}  {} step{}{}   {}", if shown { Icon::ChevronDown } else { Icon::ChevronRight }.unicode(), steps.len(), if steps.len() == 1 { "" } else { "s" }, if failed > 0 { format!(", {failed} failed") } else { String::new() }, kinds.join(" · "));
    if !flip(ui, key, RichText::new(head).font(mono(11.5)).color(tones.dim).into()) {
        if let (true, Some(last)) = (live, steps.last()) {
            ui.add(egui::Label::new(RichText::new(format!("     {} {}", last.name, last.what)).font(mono(11.5)).color(tones.dim)).truncate());
        }
        return;
    }
    for step in steps {
        let (mark, ink) = match &step.result {
            Some((_, true)) => (Icon::X, tones.rust),
            Some(_) => (Icon::Check, tones.dim),
            None => (Icon::Ellipsis, tones.dim),
        };
        let mut row = egui::text::LayoutJob::default();
        row.append(&format!("     {}  ", mark.unicode()), 0.0, egui::TextFormat { font_id: mono(11.5), color: ink, ..Default::default() });
        row.append(&step.name, 0.0, egui::TextFormat { font_id: mono(11.5), color: tones.strong, ..Default::default() });
        row.append(&format!("  {}", step.what), 0.0, egui::TextFormat { font_id: mono(11.5), color: tones.dim, ..Default::default() });
        if !flip(ui, egui::Id::new(("step", &step.id)), row.into()) {
            continue;
        }
        egui::Frame::new().fill(INK).inner_margin(9.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(&step.input).font(mono(11.5)).color(ON_INK)).selectable(true));
            if let Some((said, failed)) = &step.result {
                let lines: Vec<&str> = said.lines().collect();
                let more = lines.len().saturating_sub(16);
                ui.add_space(6.0);
                ui.add(egui::Label::new(RichText::new(lines[..lines.len() - more].join("\n")).font(mono(11.0)).color(if *failed { tones.rust } else { ON_INK.gamma_multiply(0.6) })).selectable(true));
                if more > 0 {
                    ui.label(RichText::new(format!("… {more} more lines")).font(mono(11.0)).color(ON_INK.gamma_multiply(0.45)));
                }
            }
        });
    }
}

/// The review's agent: what was said, the box to ask in, and the controls that detach, minimize and close it.
fn agent(ui: &mut egui::Ui, open: &mut Open, tones: &Tones) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Agent").font(mono(13.0)).color(tones.strong).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let small = |ui: &mut egui::Ui, icon: Icon, hint: &str| {
                let made = ui.add(egui::Button::new(RichText::new(icon.unicode().to_string()).size(15.0).color(tones.text)).frame(false));
                fx::touch(ui, made, tones.text).on_hover_text(hint).clicked()
            };
            open.agent = !small(ui, Icon::X, "Close (Cmd-J)");
            open.minimized = small(ui, Icon::Minus, "Minimize to the bottom bar (Cmd-Shift-M)");
            let (other, hint) = if open.floating { (Icon::PanelRight, "Dock to the side (Cmd-Shift-D)") } else { (Icon::PictureInPicture2, "Detach: a panel you can move and resize (Cmd-Shift-D)") };
            open.floating ^= small(ui, other, hint);
            #[cfg(not(target_arch = "wasm32"))]
            if small(ui, Icon::SquareTerminal, "Carry this conversation on in a terminal: the same Claude Code session, as `revui resume` starts it anywhere") {
                if let Err(problem) = revui(&["resume", &open.dir], None) {
                    open.note = problem;
                }
            }
        });
    });
    let links = open.data.links.clone();
    ui.add_space(6.0);
    egui::ScrollArea::vertical().auto_shrink(false).max_height((ui.available_height() - 124.0).max(80.0)).stick_to_bottom(true).show(ui, |ui| {
        if open.chat.log.is_empty() {
            ui.label(RichText::new("This review's agent. It wrote the guide, so it already knows the change. Ask it about the card you are on.\n\nCmd-J opens and closes it, Cmd-Shift-M minimizes it to the bottom bar, Cmd-Shift-D pulls it out or docks it.").size(13.0).color(tones.dim));
        }
        let log = &open.chat.log;
        let mut n = 0;
        while n < log.len() {
            match &log[n] {
                chat::Said::Me(text) => {
                    ui.add_space(6.0);
                    let said = egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.line)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(text).size(13.5).color(tones.strong)).selectable(true));
                    });
                    ui.painter().rect_filled(egui::Rect::from_min_size(said.response.rect.min, egui::vec2(2.0, said.response.rect.height())), 0.0, tones.accent);
                }
                chat::Said::Agent(text) => reply(ui, tones, &links, text),
                chat::Said::Tool(_) => {
                    let run: Vec<&chat::Step> = log[n..].iter().map_while(|said| if let chat::Said::Tool(step) = said { Some(step) } else { None }).collect();
                    n += run.len() - 1;
                    steps(ui, tones, &run, open.chat.busy && n + 1 == log.len());
                }
                chat::Said::Problem(text) => drop(ui.label(RichText::new(text).size(13.0).color(tones.rust))),
            }
            ui.add_space(8.0);
            n += 1;
        }
        if open.chat.busy {
            ui.horizontal(|ui| {
                let (bar, _) = ui.allocate_exact_size(egui::vec2(44.0, 2.0), egui::Sense::hover());
                fx::shimmer(ui, bar, tones.strong);
                ui.label(RichText::new("working").font(mono(11.5)).color(tones.dim));
            });
        }
    });
    ui.add_space(8.0);
    let about = open.on().map_or("Ask about this review".to_string(), |on| format!("Ask about {on}"));
    let field = ui.add(egui::TextEdit::multiline(&mut open.chat.draft).desired_width(f32::INFINITY).desired_rows(3).margin(egui::Margin::symmetric(10, 9)).hint_text(about));
    ui.add_space(6.0);
    if std::mem::take(&mut open.ask) {
        field.request_focus();
    } else if open.leave {
        field.surrender_focus();
    }
    open.asking = field.has_focus();
    let entered = field.has_focus() && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift);
    ui.horizontal(|ui| {
        let verify = button(ui, tones, &icon(Icon::ShieldCheck, "Verify"), false).clicked();
        let draw = button(ui, tones, &icon(Icon::Workflow, "Diagram"), false).clicked();
        let stop = open.chat.busy && button(ui, tones, &icon(Icon::Square, "Stop"), false).clicked();
        let send = !open.chat.busy && (button(ui, tones, &icon(Icon::ArrowUp, "Send"), true).clicked() || entered);
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
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.play.step(raw);
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
        fx::begin();
        self.play.draw(&ctx);
        let middle = ui.max_rect().center();
        if self.dressed != Some(ui.visuals().dark_mode) {
            self.dressed = Some(ui.visuals().dark_mode);
            let mut visuals = ui.visuals().clone();
            (visuals.window_corner_radius, visuals.menu_corner_radius) = (0.into(), 0.into());
            (visuals.window_fill, visuals.panel_fill, visuals.extreme_bg_color) = (tones.raised, tones.bg, if ui.visuals().dark_mode { INK } else { Color32::WHITE });
            (visuals.window_stroke, visuals.window_shadow) = (Stroke::new(1.0, tones.strong), egui::Shadow::NONE);
            (visuals.selection.bg_fill, visuals.selection.stroke) = (tones.accent, Stroke::new(1.0, tones.on_accent));
            for look in [&mut visuals.widgets.noninteractive, &mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active, &mut visuals.widgets.open] {
                look.corner_radius = 0.into();
            }
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, tones.line);
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, tones.text);
            visuals.override_text_color = Some(tones.text);
            visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
            ctx.set_visuals(visuals);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let save = |path: String, quit: Option<egui::Context>| {
                move |image: std::sync::Arc<egui::ColorImage>| {
                    let Ok(file) = std::fs::File::create(&path) else { return };
                    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width() as u32, image.height() as u32);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_compression(png::Compression::Fast);
                    let _ = encoder.write_header().and_then(|mut writer| writer.write_image_data(image.as_raw()));
                    if let Some(quit) = quit {
                        quit.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            };
            if let Ok(dir) = std::env::var("REVUI_GPU_FRAMES") {
                let n = self.captured.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ctx.request_screenshot(save(format!("{dir}/{n:06}.png"), None));
                if self.play.finished() {
                    let _ = std::fs::write(format!("{dir}/events.json"), self.play.marks());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            } else if let (true, Ok(path)) = (self.play.settled(), std::env::var("REVUI_GPU_SHOT")) {
                ctx.request_screenshot(save(path, Some(ctx.clone())));
            }
        }
        if self.play.running() {
            ctx.request_repaint();
        }
        let frame = egui::Frame::new().fill(tones.bg);
        if self.open.is_none() {
            let mut pick = None;
            let typing = ctx.egui_wants_keyboard_input();
            let key = |key: Key| !typing && ctx.input(|input| input.key_pressed(key) && !input.modifiers.command && !input.modifiers.ctrl);
            let (last, before) = (self.reviews.len().saturating_sub(1), self.row);
            if key(Key::G) {
                self.row = if ctx.input(|input| input.modifiers.shift) { last } else { 0 };
            }
            let down = (key(Key::J) || key(Key::ArrowDown)) as isize - (key(Key::K) || key(Key::ArrowUp)) as isize;
            self.row = (self.row as isize + down).clamp(0, last as isize) as usize;
            let (moved, insert) = (self.row != before, key(Key::I));
            if key(Key::Enter) || key(Key::L) {
                pick = self.reviews.get(self.row).map(|past| past.dir.clone());
            }
            egui::CentralPanel::default().frame(frame.inner_margin(32.0)).show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(680.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            let _ = logo(ui, 26.0);
                            ui.label(RichText::new("revui").font(serif(24.0)).color(tones.strong));
                        });
                        ui.add_space(14.0);
                        ui.label(RichText::new("Merge request or pull request link").font(bold(13.0)).color(tones.strong));
                        ui.horizontal(|ui| {
                            let field = ui.add(egui::TextEdit::singleline(&mut self.link).desired_width(560.0).margin(egui::Margin::symmetric(10, 8)).hint_text("https://gitlab.com/group/project/-/merge_requests/123"));
                            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                            if insert {
                                field.request_focus();
                            }
                            if (button(ui, &tones, "Open", true).clicked() || entered) && !self.link.trim().is_empty() {
                                match revui(&["fetch", self.link.trim()], None) {
                                    Ok(dir) => pick = Some(dir.trim().to_string()),
                                    Err(problem) => self.problem = problem,
                                }
                            }
                        });
                        ui.add_space(14.0);
                        ui.label(RichText::new("YOUR REVIEWS     j k move · enter open · g G ends · i paste a link · esc back to the list").font(mono(11.5)).color(tones.dim));
                        ui.add_space(4.0);
                        if !self.problem.is_empty() {
                            ui.label(RichText::new(&self.problem).color(tones.rust));
                        }
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (n, past) in self.reviews.iter().enumerate() {
                                let (fill, ink, faint) = if n == self.row { (tones.accent, tones.on_accent, tones.on_accent.gamma_multiply(0.7)) } else { (tones.raised, tones.strong, tones.dim) };
                                let state = if !past.working.trim().is_empty() { past.working.clone() } else if past.done.is_some() { "published".into() } else if past.items > 0 { format!("{} of {} marked", past.marked, past.items) } else { "no guide yet".into() };
                                let about = [format!("{}{}", past.repo.rsplit('/').next().unwrap_or(""), past.reference), past.author.clone(), state, past.when.clone()].join(" · ");
                                let row = egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, tones.line)).inner_margin(10.0).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(if past.title.is_empty() { &past.dir } else { &past.title }).size(14.5).color(ink));
                                    ui.label(RichText::new(about).size(12.0).color(faint));
                                });
                                if n == self.row && moved {
                                    row.response.scroll_to_me(None);
                                }
                                if fx::touch(ui, row.response.interact(egui::Sense::click()), ink).clicked() {
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
        let links = open.data.links.clone();
        let modal = open.contents.is_some() || open.publish.is_some();
        let pressed = |key: Key| !modal && pressed(key);
        let step = (pressed(Key::L) || pressed(Key::ArrowRight)) as isize - (pressed(Key::H) || pressed(Key::ArrowLeft)) as isize;
        if step != 0 {
            open.at = (open.at as isize + step).clamp(0, open.cards.len() as isize - 1) as usize;
            (open.in_diff, open.line, open.proof, open.range) = (false, 0, 0, None);
        }
        open.diff ^= pressed(Key::D);
        if pressed(Key::B) {
            open.contents = Some(open.at);
        }
        let shift = ctx.input(|input| input.modifiers.shift);
        let held = |key: Key| !typing && !modal && ctx.input(|input| input.modifiers.ctrl && input.key_pressed(key));
        let asking = std::mem::take(&mut open.asking);
        if std::mem::take(&mut self.tab) && !modal {
            let chat = open.agent && !open.minimized;
            open.leave = true;
            if asking {
            } else if open.in_diff {
                (open.in_diff, open.range, open.ask) = (false, None, chat);
            } else if open.diff && open.group().is_some() {
                (open.in_diff, open.moved) = (true, true);
            } else {
                open.ask = chat;
            }
        }
        let down = (pressed(Key::J) || pressed(Key::ArrowDown)) as isize - (pressed(Key::K) || pressed(Key::ArrowUp)) as isize;
        let half = (pressed(Key::U) && !shift || pressed(Key::PageDown) || held(Key::D)) as isize - (pressed(Key::U) && shift || pressed(Key::PageUp) || held(Key::U)) as isize;
        let end = pressed(Key::G).then_some(shift);
        let mut glide = (0.0, None);
        if open.in_diff {
            let last = open.lines().len().saturating_sub(1) as isize;
            let to = match end {
                Some(bottom) => bottom as isize * last,
                None => open.line as isize + down + half * 15,
            };
            if down != 0 || half != 0 || end.is_some() {
                (open.line, open.moved) = (to.clamp(0, last) as usize, true);
            }
            if pressed(Key::V) && shift {
                open.range = if open.range.is_some() { None } else { Some(open.line) };
            }
            if ctx.input(|input| input.key_pressed(Key::Escape)) {
                open.range = None;
            }
        } else {
            glide = ((down * 60 + half * 420) as f32, end);
            if let (true, Card::Item(n)) = (pressed(Key::E), open.cards[open.at]) {
                let count = open.data.review.items[n].evidence.len().max(1);
                (open.proof, open.moved) = ((open.proof + if shift { count - 1 } else { 1 }) % count, true);
            }
            if let (true, Card::Item(n)) = (pressed(Key::Enter) || pressed(Key::O), open.cards[open.at]) {
                if let Some(path) = open.data.review.items[n].evidence.get(open.proof).and_then(|evidence| evidence.path.as_ref()) {
                    host::open(&open.dir, path);
                }
            }
        }
        open.compose |= pressed(Key::C);
        if pressed(Key::T) {
            (open.agent, open.minimized, open.ask) = (true, false, true);
        }
        let chord = |key: Key| ctx.input(|input| input.modifiers.command && input.modifiers.shift && input.key_pressed(key));
        if ctx.input(|input| input.modifiers.command && !input.modifiers.shift && input.key_pressed(Key::J)) {
            (open.agent, open.minimized) = (!open.agent || open.minimized, false);
        }
        if chord(Key::M) && open.agent {
            open.minimized ^= true;
        }
        if chord(Key::D) {
            (open.agent, open.minimized, open.floating) = (true, false, !open.floating || !open.agent || open.minimized);
        }
        if pressed(Key::V) && !shift {
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
        let mut home = false;
        let (mut mapping, mut rooting, mut rootless) = (false, false, false);
        let mut turn = 0;
        let mut opening: Option<String> = None;
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
                ui.set_min_height(34.0);
                home = logo(ui, 20.0).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Your reviews (q)").clicked();
                ui.label(RichText::new(&open.data.review.title).font(serif(16.0)).color(tones.strong));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let count = if open.data.outgoing > 0 { open.data.outgoing.to_string() } else { String::new() };
                    if button(ui, &tones, &icon(Icon::Send, &count), true).on_hover_text("Publish (p)").clicked() {
                        open.publish = Some((0, open.data.preview.clone()));
                    }
                    if button(ui, &tones, &icon(Icon::Bot, if open.chat.busy { "working" } else { "" }), open.agent && !open.minimized).on_hover_text("Agent (t)").clicked() {
                        (open.agent, open.minimized) = (!open.agent || open.minimized, false);
                    }
                    open.diff ^= button(ui, &tones, &icon(Icon::FileDiff, ""), open.diff).on_hover_text("Diff (d)").clicked();
                    if button(ui, &tones, &icon(Icon::List, ""), false).on_hover_text("Contents (b)").clicked() {
                        open.contents = Some(open.at);
                    }
                    ui.add_space(10.0);
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let bare = |ui: &mut egui::Ui, icon: Icon, hint: &str, on: bool| {
                        let made = ui.add_enabled(on, egui::Button::new(RichText::new(icon.unicode().to_string()).size(17.0).color(tones.strong)).frame(false));
                        fx::touch(ui, made, tones.strong).on_hover_text(hint).clicked()
                    };
                    turn += bare(ui, Icon::ChevronRight, "Next (l)", open.at + 1 < open.cards.len()) as isize;
                    ui.label(RichText::new(format!("{} of {}", open.at + 1, open.cards.len())).font(mono(11.5)).color(tones.dim));
                    turn -= bare(ui, Icon::ChevronLeft, "Back (h)", open.at > 0) as isize;
                });
            });
            if open.chat.busy {
                let edge = ui.clip_rect();
                fx::shimmer(ui, egui::Rect::from_min_max(egui::pos2(edge.left(), edge.bottom() - 2.0), edge.max), tones.accent);
            }
        });
        if turn != 0 {
            open.at = (open.at as isize + turn).clamp(0, open.cards.len() as isize - 1) as usize;
            (open.in_diff, open.line, open.proof, open.range) = (false, 0, 0, None);
        }
        egui::Panel::bottom("keys").frame(frame.inner_margin(egui::Margin::symmetric(14, 5))).show_inside(ui, |ui| {
            let here = if open.in_diff { "j k line · V select lines · c comment on them · tab next panel" } else { "j k scroll · e evidence · enter open it · a agree · x dismiss · c comment · tab next panel" };
            let said = if open.note.is_empty() { String::new() } else { format!("   {}", open.note) };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if open.agent && open.minimized {
                    let name = format!("{}  Agent{}", Icon::Bot.unicode(), if open.chat.busy { " · working" } else { "" });
                    let chip = ui.add(egui::Button::new(RichText::new(name).font(bold(12.5)).color(tones.on_accent)).fill(tones.accent).corner_radius(0.0));
                    if fx::touch(ui, chip, tones.on_accent).on_hover_text("Restore the agent (Cmd-J or Cmd-Shift-M)").clicked() {
                        open.minimized = false;
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(format!("l next · h back · {here} · g G ends · u U page · t ask · v verify · i diagram · d diff · b contents · p publish · q reviews{said}")).font(mono(11.5)).color(tones.dim)).truncate());
                });
            });
        });

        let bar = |ui: &egui::Ui, on: bool| {
            if on {
                ui.painter().hline(ui.clip_rect().x_range(), ui.clip_rect().top() + 1.0, Stroke::new(2.0, tones.accent));
            }
        };
        if open.agent && !open.floating && !open.minimized {
            egui::Panel::right("agent").default_size(380.0).frame(frame.inner_margin(14.0).stroke(Stroke::new(1.0, tones.line))).show_inside(ui, |ui| {
                agent(ui, open, &tones);
                bar(ui, open.asking);
            });
        }

        if let (true, Some(group)) = (open.diff, open.group()) {
            let mut clicked = None;
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
                            let this = row;
                            let under = open.in_diff && sign != '\\' && row == open.line;
                            let chosen = open.in_diff && sign != '\\' && open.range.is_some_and(|from| (from.min(open.line)..=from.max(open.line)).contains(&row));
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
                            if chosen {
                                ui.painter().rect_filled(drawn.response.rect, 0.0, Color32::from_white_alpha(28));
                                ui.painter().rect_filled(egui::Rect::from_min_size(drawn.response.rect.min, egui::vec2(3.0, drawn.response.rect.height())), 0.0, Color32::WHITE);
                            }
                            if under && open.moved {
                                drawn.response.scroll_to_me(None);
                            }
                            if sign != '\\' && drawn.response.interact(egui::Sense::click()).clicked() {
                                clicked = Some(this);
                            }
                        }
                    }
                });
                bar(ui, open.in_diff);
            });
            if let Some(row) = clicked {
                (open.in_diff, open.line, open.leave) = (true, row, true);
            }
        }

        egui::CentralPanel::default().frame(frame.inner_margin(egui::Margin::symmetric(30, 26))).show_inside(ui, |ui| {
            let area = egui::ScrollArea::vertical().auto_shrink(false);
            let area = match glide.1 {
                Some(bottom) => area.vertical_scroll_offset(if bottom { f32::MAX } else { 0.0 }),
                None => area,
            };
            area.show(ui, |ui| {
                ui.scroll_with_delta(egui::vec2(0.0, -glide.0));
                let mut column = ui.available_rect_before_wrap();
                column.min.x += ((column.width() - 760.0) / 2.0).max(0.0);
                column.max.x = column.max.x.min(column.min.x + 760.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                    let arrived = fx::arrive(ui.ctx(), open.at);
                    ui.set_opacity(arrived);
                    ui.add_space((1.0 - arrived) * 10.0);
                    match open.cards[open.at] {
                        Card::Summary => {
                            let review = &open.data.review;
                            let slab = egui::Frame::new().fill(INK).inner_margin(egui::Margin::symmetric(30, 24)).show(ui, |ui| {
                                let under = ui.painter().add(egui::Shape::Noop);
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(&review.title).font(serif(25.0)).color(Color32::WHITE));
                                ui.add_space(14.0);
                                let lines = |sign: char| open.data.files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(sign)).count();
                                let stats = [(review.sections.len().to_string(), "parts"), (review.items.iter().filter(|item| item.kind == "finding").count().to_string(), "findings"), (review.items.iter().filter(|item| item.kind == "decision").count().to_string(), "decisions"), (open.data.files.len().to_string(), "files"), (format!("+{} −{}", lines('+'), lines('-')), "lines")];
                                ui.columns(stats.len(), |columns| {
                                    for (column, (value, name)) in columns.iter_mut().zip(&stats) {
                                        column.add(egui::Label::new(RichText::new(value).font(serif(26.0)).color(Color32::WHITE)).extend());
                                        column.label(RichText::new(*name).size(12.5).color(ON_INK.gamma_multiply(0.6)));
                                    }
                                });
                                under
                            });
                            fx::flow(ui, slab.inner, slab.response.rect, Color32::from_rgb(154, 158, 242));
                            ui.add_space(18.0);
                            if open.data.product && !review.plain.trim().is_empty() {
                                prose(ui, &tones, &links, &review.plain);
                                ui.label(RichText::new("In detail").font(bold(14.0)).color(tones.strong));
                            }
                            prose(ui, &tones, &links, &review.summary);
                            ui.separator();
                            ui.label(RichText::new("Where it sits").font(bold(14.0)).color(tones.strong));
                            ui.add_space(6.0);
                            let drawn = review.map.as_ref().and_then(|map| map.path.as_ref()).and_then(|path| host::text(&open.dir, &std::path::Path::new(path).with_extension("mmd").to_string_lossy()));
                            let sights: Vec<&'static str> = [review.map.is_some().then_some("This change"), open.data.system.is_some().then_some("Whole system"), Some("Folders")].into_iter().flatten().collect();
                            let kept = egui::Id::new("map shows");
                            let (mut sight, mut calls, mut named) = ui.data(|data| data.get_temp::<(&'static str, bool, Option<bool>)>(kept)).unwrap_or((sights[0], true, None));
                            if !sights.contains(&sight) {
                                sight = sights[0];
                            }
                            let changed: Vec<&str> = open.data.files.iter().map(|file| file.path.as_str()).collect();
                            let (mut edges, mut text) = (Vec::new(), String::new());
                            let (mut nodes, caption);
                            match (sight, &review.map, &open.data.system) {
                                ("This change", Some(map), _) => {
                                    (nodes, edges) = map::parse(drawn.as_deref().unwrap_or(""));
                                    caption = map.caption.clone();
                                    text = map.text.clone().unwrap_or_default();
                                }
                                ("Whole system", _, Some(system)) => {
                                    (nodes, edges) = map::parse(&system.text);
                                    nodes.iter_mut().for_each(|node| node.state = system.touched.contains_key(&node.id).then_some("changed"));
                                    let hit: Vec<&str> = nodes.iter().filter(|node| node.state.is_some()).map(|node| node.label.as_str()).collect();
                                    caption = format!("Touches {} of {} parts of the system: {}", hit.len(), nodes.len(), hit.join(", "));
                                }
                                _ => {
                                    // ponytail: the repo's folders are asked for once a run; a checkout that appears later shows after the review is opened again.
                                    let asked = egui::Id::new(("tree", &open.dir));
                                    let tree = ui.data(|data| data.get_temp::<Result<std::sync::Arc<std::collections::BTreeMap<String, usize>>, String>>(asked)).unwrap_or_else(|| {
                                        let read = revui(&["tree", &open.dir], None).and_then(|listed| serde_json::from_str(&listed).map(std::sync::Arc::new).map_err(|e| e.to_string()));
                                        ui.data_mut(|data| data.insert_temp(asked, read.clone()));
                                        read
                                    });
                                    match tree {
                                        Ok(tree) => {
                                            let more;
                                            (nodes, more) = map::folders(&tree, &changed);
                                            caption = format!("The repo's folders, each as tall as it is full. This change touches the {} lit{}.", nodes.iter().filter(|node| node.state.is_some()).count(), if more == 0 { String::new() } else { format!(" ({more} smaller folders are left out)") });
                                        }
                                        Err(why) => {
                                            let paths: Vec<Vec<&str>> = changed.iter().map(|path| path.split('/').collect()).collect();
                                            let shared = (0..).take_while(|&n| !paths.is_empty() && paths.iter().all(|parts| parts.len() > n + 1 && parts[n] == paths[0][n])).count();
                                            let mut places: std::collections::BTreeMap<&str, usize> = Default::default();
                                            for (file, parts) in open.data.files.iter().zip(&paths) {
                                                *places.entry(parts[shared]).or_default() += file.hunks.iter().flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(['+', '-'])).count();
                                            }
                                            nodes = places.iter().map(|(name, lines)| map::Node { id: name.to_string(), label: name.to_string(), form: map::Form::Block, state: Some("changed"), tall: 10.0 + 9.0 * (1.0 + *lines as f32).ln(), note: format!("{lines} lines changed"), opens: paths.iter().zip(&changed).find(|(parts, _)| parts[shared] == *name).map(|(_, path)| path.to_string()) }).collect();
                                            nodes.sort_by_key(|node| std::cmp::Reverse(places[node.id.as_str()]));
                                            nodes.truncate(36);
                                            caption = format!("Only where this change lands{}, each as tall as its lines changed.", if shared == 0 { String::new() } else { format!(" in {}", paths[0][..shared].join("/")) });
                                            rootless = why.contains("not known yet");
                                            text = if rootless { "To see the rest of the repo around it, tell revui where your code is kept.".into() } else { format!("The rest of the repo is not shown: {}.", why.trim().trim_start_matches("Error: ").trim_matches('"')) };
                                        }
                                    }
                                }
                            }
                            if let (true, Some(system)) = (sight != "Folders", &open.data.system) {
                                let owns: Vec<(&str, &str)> = system.text.lines().filter_map(|line| line.trim().strip_prefix("%%")?.split_once(':')).flat_map(|(id, folders)| folders.split(',').map(move |folder| (id.trim(), folder.trim()))).filter(|(_, folder)| !folder.is_empty()).collect();
                                for node in &mut nodes {
                                    node.opens = changed.iter().find(|path| owns.iter().any(|(id, folder)| *id == node.id && path.starts_with(folder))).map(|path| path.to_string());
                                }
                            }
                            let labels = named.unwrap_or(nodes.len() <= 14);
                            egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.line)).inner_margin(14.0).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    for name in sights.iter().filter(|_| sights.len() > 1) {
                                        if button(ui, &tones, name, sight == *name).clicked() {
                                            sight = name;
                                        }
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if button(ui, &tones, &icon(Icon::Tag, "Names"), labels).on_hover_text("Name every block, or only the lit ones and the one under the pointer").clicked() {
                                            named = Some(!labels);
                                        }
                                        if !edges.is_empty() {
                                            calls ^= button(ui, &tones, &icon(Icon::Waypoints, "Calls"), calls).on_hover_text("Who calls whom").clicked();
                                        }
                                    });
                                });
                                let look = map::Look { raised: tones.raised, top: tones.line.gamma_multiply(0.8), side: tones.line, edge: tones.dim, text: tones.strong };
                                match (nodes.is_empty(), review.map.as_ref().and_then(|map| map.path.as_ref())) {
                                    (false, _) => opening = map::draw(ui, &look, &nodes, &edges, map::Show { calls, labels }),
                                    (true, Some(path)) => artifact(ui, &open.dir, path),
                                    _ => {}
                                }
                                ui.label(RichText::new(&caption).font(bold(13.5)).color(tones.strong));
                                if !text.is_empty() {
                                    ui.label(RichText::new(&text).size(13.0).color(tones.text));
                                }
                                if review.map.is_none() || rootless {
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        #[cfg(not(target_arch = "wasm32"))]
                                        if rootless {
                                            rooting = button(ui, &tones, &icon(Icon::FolderSearch, "Show the whole repo"), false).clicked();
                                        }
                                        if review.map.is_none() {
                                            mapping = button(ui, &tones, &icon(Icon::Workflow, "Have the agent draw the map"), false).on_hover_text("This review has no map of the system around it yet").clicked();
                                        }
                                    });
                                }
                            });
                            ui.data_mut(|data| data.insert_temp(kept, (sight, calls, named)));
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
                            prose(ui, &tones, &links, &section.why);
                        }
                        Card::Thread(n) => {
                            let thread = open.data.threads[n].clone();
                            ui.label(RichText::new(open.said(n)).font(serif(25.0)).color(tones.strong));
                            ui.label(RichText::new(if thread.resolved { "resolved · already on the merge request" } else { "already on the merge request" }).font(mono(11.5)).color(tones.dim));
                            for (who, body, when) in &thread.notes {
                                ui.add_space(10.0);
                                ui.label(RichText::new(format!("@{} · {when}", who.trim_start_matches('@'))).font(bold(13.0)).color(tones.strong));
                                prose(ui, &tones, &links, body);
                            }
                        }
                        Card::Note(n) => {
                            let text = open.data.state["notes"][n]["text"].as_str().unwrap_or("").to_string();
                            ui.label(RichText::new(open.about(n)).font(serif(25.0)).color(tones.strong));
                            ui.add_space(10.0);
                            prose(ui, &tones, &links, &text);
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
                            prose(ui, &tones, &links, "These hunks are in the diff, but the guide put them in no section. Nothing vouches for them: read them.");
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
                                    if verdict.is_some() {
                                        let quoted = open.data.quoted.get(&item.id).copied().unwrap_or(false);
                                        ui.label(RichText::new(if quoted { "goes out quoted" } else { "only your comment goes out" }).size(12.0).color(tones.dim));
                                        if button(ui, &tones, &icon(Icon::Quote, "Quote it"), quoted).clicked() {
                                            let mut state = open.data.state.clone();
                                            state["quotes"][&item.id] = (!quoted).into();
                                            if let Err(problem) = open.save(state) {
                                                open.note = problem;
                                            }
                                        }
                                    }
                                });
                            }
                            ui.add_space(10.0);
                            if open.data.product && !item.plain.trim().is_empty() {
                                prose(ui, &tones, &links, &item.plain);
                                ui.label(RichText::new("In detail").font(bold(14.0)).color(tones.strong));
                            }
                            prose(ui, &tones, &links, &item.body);
                            ui.separator();
                            ui.label(RichText::new(format!("Evidence ({})", if item.evidence.is_empty() { "none".into() } else { item.evidence.len().to_string() })).font(bold(14.0)).color(tones.strong));
                            for (n, evidence) in item.evidence.iter().enumerate() {
                                ui.add_space(8.0);
                                let current = !open.in_diff && n == open.proof;
                                let proof = egui::Frame::new().fill(INK).stroke(Stroke::new(if current { 2.0 } else { 0.0 }, tones.rust)).inner_margin(14.0).show(ui, |ui| {
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
                                if current && open.moved {
                                    proof.response.scroll_to_me(None);
                                }
                            }
                        }
                    }
                    ui.add_space(18.0);
                    let about = match (open.picked(), open.cards[open.at]) {
                        (Some((file, line, _, Some(first))), _) => format!("Comment on {file}:{first}-{line}"),
                        (Some((file, line, _, None)), _) => format!("Comment on {file}:{line}"),
                        (None, Card::Item(n)) => format!("Comment on {}", open.data.review.items[n].id),
                        (None, Card::Thread(_)) => "Reply in this thread".into(),
                        _ => "Comment on the whole merge request".into(),
                    };
                    ui.label(RichText::new(about).font(bold(14.0)).color(tones.strong));
                    let field = ui.add(egui::TextEdit::multiline(&mut open.draft).desired_width(f32::INFINITY).desired_rows(3).margin(egui::Margin::symmetric(10, 9)));
                    ui.add_space(6.0);
                    if std::mem::take(&mut open.compose) {
                        field.request_focus();
                        field.scroll_to_me(None);
                    } else if open.leave {
                        field.surrender_focus();
                    }
                    let sent = field.has_focus() && ui.input(|input| input.modifiers.command && input.key_pressed(Key::Enter));
                    if sent {
                        field.surrender_focus();
                    }
                    if button(ui, &tones, "Save comment", false).on_hover_text("Cmd-Enter").clicked() || sent {
                        if let Err(problem) = open.comment() {
                            open.note = problem;
                        }
                    }
                });
            });
            bar(ui, !open.in_diff && !open.asking);
        });
        if open.agent && open.floating && !open.minimized {
            let at = ctx.content_rect().right_bottom() - egui::vec2(460.0, 620.0);
            egui::Window::new("agent").title_bar(false).resizable(true).default_pos(at).default_size([420.0, 540.0]).min_size([320.0, 320.0]).frame(egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.strong)).inner_margin(14.0)).show(&ctx, |ui| agent(ui, open, &tones));
        }
        (open.moved, open.leave) = (false, false);

        if let Some((mut choice, mut text)) = open.publish.take() {
            let (mut keep, mut send) = (true, false);
            egui::Window::new(RichText::new("Publish your review").font(serif(20.0)).color(tones.strong)).collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).fixed_pos(middle).show(&ctx, |ui| {
                ui.set_width(680.0);
                ui.label(RichText::new("This is exactly what goes to the merge request. Nothing is posted until you confirm.").color(tones.text));
                for (n, how) in open.data.finish.iter().enumerate() {
                    ui.radio_value(&mut choice, n, how).on_hover_cursor(egui::CursorIcon::PointingHand);
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
            let seen = egui::Id::new("contents row");
            let moved = ctx.data(|data| data.get_temp::<usize>(seen)) != Some(on);
            ctx.data_mut(|data| data.insert_temp(seen, on));
            let frame = egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.strong)).inner_margin(egui::Margin::symmetric(0, 16));
            let shown = egui::Modal::new(egui::Id::new("contents")).frame(frame).show(&ctx, |ui| {
                ui.set_width(600.0_f32.min(ui.ctx().content_rect().width() - 80.0));
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("Contents").font(serif(22.0)).color(tones.strong));
                });
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.6).show(ui, |ui| {
                    for (n, card) in open.cards.iter().enumerate() {
                        let nested = matches!(card, Card::Item(_) | Card::Thread(_) | Card::Note(_));
                        let (row, hit) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
                        let ink = if n == on { tones.on_accent } else if hit.hovered() { tones.accent } else if nested { tones.text } else { tones.strong };
                        if n == on {
                            ui.painter().rect_filled(row, 0.0, tones.accent);
                            if moved {
                                hit.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                        ui.painter().hline(row.x_range(), row.bottom(), Stroke::new(1.0, tones.line));
                        if let Card::Item(item) = card {
                            let mark = match open.verdict(&open.data.review.items[*item].id) {
                                Some("dismiss") => Some(tones.rust),
                                Some(_) => Some(if n == on { tones.on_accent } else { tones.accent }),
                                None => None,
                            };
                            if let Some(mark) = mark {
                                ui.painter().rect_filled(egui::Rect::from_center_size(egui::pos2(row.left() + 24.0, row.center().y), egui::vec2(7.0, 7.0)), 0.0, mark);
                            }
                        }
                        let inner = egui::Rect::from_min_max(row.min + egui::vec2(if nested { 38.0 } else { 20.0 }, 0.0), row.max - egui::vec2(20.0, 0.0));
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                            ui.add(egui::Label::new(RichText::new(open.label(*card)).font(if nested { mono(12.5) } else { bold(14.0) }).color(ink)).truncate().selectable(false));
                        });
                        if hit.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            go = Some(n);
                        }
                    }
                });
            });
            let leave = leave || shown.should_close();
            if leave || go.is_some() || enter {
                ctx.data_mut(|data| data.remove::<usize>(seen));
            }
            open.contents = if leave { None } else { Some(on) };
            if let Some(n) = go.or(enter.then_some(on)) {
                (open.at, open.contents, open.in_diff, open.line, open.proof) = (n, None, false, 0, 0);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if rooting && open.root().is_some() {
            ctx.data_mut(|data| data.remove_by_type::<Result<std::sync::Arc<std::collections::BTreeMap<String, usize>>, String>>());
        }
        if let Some(file) = opening {
            // ponytail: the window waits while revui fetches the file for an editor that asks for it; move it to a thread if that shows.
            let line = open.data.files.iter().find(|known| known.path == file).and_then(|known| known.hunks.first()).map_or(1, |hunk| hunk.new_start.max(1));
            if let Err(problem) = revui(&["open", &open.dir, &file, &line.to_string()], None) {
                open.note = problem;
            }
        }
        if mapping {
            open.agent = true;
            open.tell(&ctx, "ask", "Draw the map for this review, as `revui skill guide` says under \"The map\": the system map of the repo first if `revui map` shows none, then this review's `assets/map.mmd` and `map` in review.json.");
        }
        if let Some((id, verdict)) = marked {
            if let Err(problem) = open.mark(&id, verdict) {
                self.problem = problem;
            }
        }
        if home {
            self.list();
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
        std::env::set_var("REVUI_SOCKET", std::env::temp_dir().join(format!("revui-app-{}.sock", std::process::id())));
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
