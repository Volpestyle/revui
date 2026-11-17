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

/// A step a sandbox run took, or a check it made against the running change, with what proves it.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Ran {
    step: String,
    name: String,
    ok: Option<bool>,
    seconds: f64,
    status: serde_json::Value,
    log: String,
    detail: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Verify {
    steps: Vec<Ran>,
    checks: Vec<Ran>,
    api: String,
    scope: String,
    untested: Vec<String>,
    media: Vec<Evidence>,
}

impl Verify {
    fn clean(&self) -> bool {
        self.steps.iter().all(|step| step.ok == Some(true))
    }
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
struct Source {
    url: String,
    reference: String,
    repo: String,
    author: String,
    branch: String,
    target: String,
    head_sha: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Review {
    sources: Vec<Source>,
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
    verify: Option<Verify>,
    claude: String,
    threads: Vec<Thread>,
    product: bool,
    #[serde(rename = "refs")]
    links: Links,
    /// Where each item's line opens on the code host.
    #[serde(rename = "links")]
    hosted: std::collections::BTreeMap<String, String>,
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
    Verify,
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
#[derive(Clone, Copy)]
struct Tones {
    bg: Color32,
    pane: Color32,
    raised: Color32,
    line: Color32,
    faint: Color32,
    dim: Color32,
    text: Color32,
    strong: Color32,
    accent: Color32,
    on_accent: Color32,
    rust: Color32,
    link: Color32,
}

const INK: Color32 = Color32::from_rgb(11, 11, 13);
const ON_INK: Color32 = Color32::from_rgb(237, 237, 240);
const ADDED: Color32 = Color32::from_rgb(15, 29, 21);
const REMOVED: Color32 = Color32::from_rgb(33, 18, 20);

fn tones(dark: bool) -> Tones {
    let rgb = Color32::from_rgb;
    match dark {
        true => Tones { bg: INK, pane: rgb(16, 16, 18), raised: rgb(23, 23, 26), line: rgb(31, 31, 35), faint: rgb(102, 102, 110), dim: rgb(160, 160, 168), text: ON_INK, strong: ON_INK, accent: ON_INK, on_accent: INK, rust: rgb(240, 163, 94), link: rgb(169, 172, 247) },
        false => Tones { bg: Color32::WHITE, pane: rgb(250, 250, 250), raised: rgb(242, 242, 243), line: rgb(232, 232, 234), faint: rgb(142, 142, 150), dim: rgb(85, 85, 92), text: rgb(24, 24, 27), strong: rgb(24, 24, 27), accent: rgb(24, 24, 27), on_accent: Color32::WHITE, rust: rgb(184, 86, 10), link: rgb(52, 71, 170) },
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
    /// The contents column has the keys, as it does when a review opens: j and k move between cards.
    rail: bool,
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
        cards.extend(data.verify.is_some().then_some(Card::Verify));
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
        let mut open = Self { dir: dir.into(), cards, at, diff: !data.product, data, contents: None, in_diff: false, rail: true, line: 0, moved: false, proof: 0, draft: String::new(), compose: false, publish: None, note: String::new(), painted: Default::default(), chat: Default::default(), agent: false, ask: false, range: None, floating: false, minimized: false, asking: false, leave: false, stamp: host::stamp(dir), looked: 0.0 };
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
            Card::Summary | Card::Verify => None,
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
            Card::Verify => "Sandbox verification".into(),
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
        // While the review's agent is open in a pane of the person's terminal, it is told there: one agent, wherever it runs.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let args: Vec<&str> = ["agent", self.dir.as_str(), mode].into_iter().chain(on.as_deref()).chain(["--if-open"]).collect();
            if let Ok(sent) = revui(&args, Some(text)) {
                self.chat.log.push(chat::Said::Me(if text.is_empty() { format!("{mode} {}", on.clone().unwrap_or_default()) } else { text.to_string() }));
                return self.note = sent.trim().to_string();
            }
        }
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
        for (name, bytes) in [("sans", include_bytes!("../fonts/DMSans-Regular.ttf").as_slice()), ("sans-medium", include_bytes!("../fonts/DMSans-Medium.ttf")), ("sans-bold", include_bytes!("../fonts/DMSans-SemiBold.ttf")), ("geist-mono", include_bytes!("../fonts/GeistMono-Regular.ttf"))] {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        }
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "sans".into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "geist-mono".into());
        fonts.families.insert(FontFamily::Name("bold".into()), vec!["sans-bold".into(), "sans".into()]);
        fonts.families.insert(FontFamily::Name("medium".into()), vec!["sans-medium".into(), "sans".into()]);
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

/// The medium weight headings are set in.
fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

/// How tall a line of text of this size is: reading text gets more air than the small text of a panel.
fn leading(size: f32) -> f32 {
    (size * 1.62).round()
}

/// The heading of a card.
fn title(ui: &mut egui::Ui, tones: &Tones, text: impl AsRef<str>) {
    ui.add_space(4.0);
    let set = balanced(ui, text.as_ref(), bold(22.0), 29.0, tones.strong);
    ui.label(set);
    ui.add_space(8.0);
}

/// A heading in as few lines as it needs, and those of about one length, so no word is left alone on the last.
fn balanced(ui: &egui::Ui, text: &str, font: FontId, tall: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let set = |wide: f32| {
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: font.clone(), color, line_height: Some(tall), ..Default::default() });
        job.wrap.max_width = wide;
        ui.painter().layout_job(job)
    };
    let full = ui.available_width();
    let lines = set(full).rows.len();
    let (mut tight, mut loose) = (full / lines.max(1) as f32, full);
    for _ in 0..7 {
        let between = (tight + loose) / 2.0;
        if set(between).rows.len() > lines { tight = between } else { loose = between }
    }
    set(loose)
}

/// The backdrop of the summary's card, as the one-page guide draws it: a field of characters, dithered between
/// neighbouring weights with an ordered pattern, denser towards the far corner so the title stays clear. It drifts slowly.
fn field(ui: &egui::Ui, rect: egui::Rect, ink: Color32) -> egui::Shape {
    const RAMP: [char; 9] = [' ', '.', '·', ':', '-', '=', '+', '*', '#'];
    const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    const CELL: egui::Vec2 = egui::vec2(7.0, 13.0);
    // It moves in steps, eight a second, so a row is set again only when it has changed.
    let time = ((ui.input(|input| input.time % 3600.0) / 0.12).floor() * 120.0) as f32;
    if ui.input(|input| input.focused) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }
    let (columns, rows) = (((rect.width() - 4.0) / CELL.x).floor().max(1.0) as usize, (rect.height() / CELL.y).floor().max(1.0) as usize);
    let look = egui::TextFormat { font_id: mono(11.0), color: ink.gamma_multiply(0.2 * ui.opacity()), ..Default::default() };
    // Each character stands in its own cell, as wide as the page's: the font's own step is a little narrower.
    let narrow = ui.painter().layout_no_wrap("0".into(), mono(11.0), ink).size().x;
    let look = egui::TextFormat { extra_letter_spacing: CELL.x - narrow, ..look };
    let drawn = (0..rows).filter_map(|row| {
        let down = row as f32 / rows as f32;
        let line: String = (0..columns)
            .map(|column| {
                let across = column as f32 / columns as f32;
                let wave = 0.5 + 0.5 * (across * 5.2 + time * 0.00011 + (down * 3.1 + time * 0.00008).sin() * 1.7).sin();
                let level = wave * across * across * (1.0 - 0.45 * down) * (RAMP.len() - 1) as f32;
                let step = level.fract() > (BAYER[(row % 4) * 4 + column % 4] + 0.5) / 16.0;
                RAMP[(level as usize + step as usize).min(RAMP.len() - 1)]
            })
            .collect();
        let line = line.trim_end();
        (!line.trim().is_empty()).then(|| egui::Shape::galley(rect.left_top() + egui::vec2(0.0, row as f32 * CELL.y), ui.painter().layout_job(egui::text::LayoutJob::single_section(line.to_string(), look.clone())), ink))
    });
    egui::Shape::Vec(drawn.collect())
}

/// The colour of what an item is: its id in the contents, its chip, and the word on its card.
fn hue(tones: &Tones, item: &Item) -> Color32 {
    let rgb = |dark: u32, light: u32| {
        let [_, r, g, b] = if tones.bg == INK { dark } else { light }.to_be_bytes();
        Color32::from_rgb(r, g, b)
    };
    match kind(item) {
        "blocker" => rgb(0xff7369, 0xc4281c),
        "fix" => rgb(0xf0a35e, 0xb8560a),
        "suggestion" => rgb(0xd9b95a, 0x8a6d0b),
        "question" => rgb(0x8ab4f8, 0x1f5fbf),
        "decision" => rgb(0xc4a7f5, 0x7a4fc4),
        "good" => rgb(0x5fc48a, 0x1f8a4c),
        _ => tones.faint,
    }
}

fn kind(item: &Item) -> &str {
    if item.kind == "finding" && !item.severity.is_empty() { &item.severity } else { &item.kind }
}

fn tier(tier: &str) -> &str {
    match tier {
        "core" => "the core change",
        "supporting" => "supporting code",
        "low" => "low signal, skim it",
        other => other,
    }
}

/// An item's id in a small box, coloured by what the item is; a press goes to it.
fn chip(ui: &mut egui::Ui, tones: &Tones, item: &Item) -> egui::Response {
    let made = egui::Frame::new().stroke(Stroke::new(1.0, tones.line)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| ui.add(egui::Label::new(RichText::new(&item.id).font(mono(12.0)).color(hue(tones, item))).selectable(false)));
    fx::touch(ui, made.response.interact(egui::Sense::click()), tones.strong)
}

/// A button that is only its icon, lit while what it shows is open.
fn bare(ui: &mut egui::Ui, tones: &Tones, icon: Icon, hint: &str, enabled: bool, on: bool) -> bool {
    let made = ui.add_enabled(enabled, egui::Button::new(RichText::new(icon.unicode().to_string()).size(17.0).color(if on { tones.strong } else { tones.dim })).frame(false).min_size(egui::vec2(30.0, 30.0)));
    fx::touch(ui, made, tones.strong).on_hover_text(hint).clicked()
}

/// The colours of a line added and a line removed, which are also those of a step that passed and one that failed.
fn signs(tones: &Tones) -> (Color32, Color32) {
    if tones.bg == INK { (Color32::from_rgb(95, 196, 138), Color32::from_rgb(255, 115, 105)) } else { (Color32::from_rgb(31, 138, 76), Color32::from_rgb(196, 40, 28)) }
}

/// One line of a sandbox run: whether it passed, what it was and how long it took or what came back. A press
/// opens what proves it, the end of a log or a response; one that failed starts open.
fn ran(ui: &mut egui::Ui, tones: &Tones, ok: bool, name: &str, note: &str, proof: &str) {
    let fold = ui.id().with(("ran", name));
    let mut opened = ui.data(|data| data.get_temp::<bool>(fold)).unwrap_or(!ok);
    let (passed, failed) = signs(tones);
    let row = ui.horizontal(|ui| {
        ui.set_min_height(34.0);
        ui.add_sized([22.0, 20.0], egui::Label::new(RichText::new(if ok { Icon::Check } else { Icon::X }.unicode().to_string()).size(15.0).color(if ok { passed } else { failed })).selectable(false));
        ui.add(egui::Label::new(RichText::new(name).size(14.5).color(if ok { tones.text } else { failed })).selectable(false));
        if !proof.is_empty() {
            ui.add(egui::Label::new(RichText::new(if opened { "hide" } else { "log" }).font(mono(11.0)).color(tones.faint)).selectable(false));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add(egui::Label::new(RichText::new(note).font(mono(12.5)).color(tones.faint)).selectable(false)));
    });
    let hit = ui.interact(row.response.rect, fold, egui::Sense::click());
    if !proof.is_empty() && fx::touch(ui, hit, tones.strong).clicked() {
        opened = !opened;
        ui.data_mut(|data| data.insert_temp(fold, opened));
    }
    if opened && !proof.is_empty() {
        egui::Frame::new().fill(tones.pane).stroke(Stroke::new(1.0, tones.line)).corner_radius(8.0).outer_margin(egui::Margin { left: 30, bottom: 10, ..Default::default() }).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::both().id_salt(fold).max_height(320.0).show(ui, |ui| ui.add(egui::Label::new(RichText::new(proof).font(mono(12.0)).line_height(Some(18.0)).color(tones.text)).extend()));
        });
    }
    ui.painter().hline(ui.available_rect_before_wrap().x_range(), ui.cursor().top(), Stroke::new(1.0, tones.line));
    ui.add_space(1.0);
}

/// The contents: each part with what kind of code it is above its title, and under it its items, their ids in one
/// column and their titles in the next. `folded` is the strip of marks the column narrows to; `pick` is the row
/// the keys are on when the list is laid over the review, where it is set a size larger. Returns a row pressed.
fn contents(ui: &mut egui::Ui, open: &Open, tones: &Tones, folded: bool, pick: Option<usize>, scroll: bool) -> Option<usize> {
    let (mut pressed, over) = (None, pick.is_some());
    let (large, small) = if over { (15.0, 14.0) } else { (14.0, 13.0) };
    ui.spacing_mut().item_spacing.y = 1.0;
    for (n, card) in open.cards.iter().enumerate() {
        let (on, lit) = (n == open.at || pick == Some(n), n == pick.unwrap_or(open.at));
        let item = match card {
            Card::Item(item) => Some(&open.data.review.items[*item]),
            _ => None,
        };
        let nested = matches!(card, Card::Item(_) | Card::Thread(_) | Card::Note(_));
        let fill = if lit { tones.strong.gamma_multiply(0.07) } else { Color32::TRANSPARENT };
        let ink = if on { tones.strong } else if nested { tones.dim } else { tones.text };
        let mark = item.map_or(tones.faint, |item| if n == open.at { tones.strong } else { hue(tones, item) });
        if !nested && n > 0 {
            ui.add_space(if folded { 8.0 } else { 12.0 });
        }
        let row = if folded {
            let short = match card {
                Card::Summary => Icon::List.unicode().to_string(),
                Card::Verify => if open.data.verify.as_ref().is_some_and(Verify::clean) { Icon::Check } else { Icon::X }.unicode().to_string(),
                Card::Part(part) => (part + 1).to_string(),
                Card::Item(_) => item.map(|item| item.id.clone()).unwrap_or_default(),
                Card::Thread(_) | Card::Note(_) => Icon::MessageSquare.unicode().to_string(),
                Card::Loose => "…".into(),
            };
            egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(0, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| ui.add(egui::Label::new(RichText::new(short).font(mono(12.0)).color(if nested { mark } else { ink })).selectable(false)));
            })
        } else if nested {
            let row = egui::Frame::new().fill(fill).outer_margin(egui::Margin { left: 10, ..Default::default() }).inner_margin(egui::Margin { left: 12, right: 10, top: 5, bottom: 5 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if let Some(item) = item {
                        ui.add_sized([30.0, (small * 1.42_f32).round()], egui::Label::new(RichText::new(&item.id).font(mono(11.5)).color(mark)).selectable(false).halign(egui::Align::Min));
                    }
                    let name = item.map_or_else(|| open.label(*card), |item| item.title.clone());
                    ui.add(egui::Label::new(RichText::new(name).size(small).line_height(Some((small * 1.42_f32).round())).color(ink)).wrap().selectable(false));
                });
            });
            let edge = row.response.rect;
            ui.painter().vline(edge.left() + 10.5, edge.y_range().expand(0.5), Stroke::new(1.0, if on { tones.strong } else { tones.line }));
            if let Some(verdict) = item.and_then(|item| open.verdict(&item.id)) {
                ui.painter().circle_filled(egui::pos2(edge.right() - 6.0, edge.top() + 14.0), 2.5, if verdict == "dismiss" { tones.rust } else { tones.strong });
            }
            row
        } else {
            egui::Frame::new().fill(fill).inner_margin(egui::Margin { left: 10, right: 6, top: 8, bottom: 8 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 2.0;
                if let Card::Part(part) = card {
                    let kind = &open.data.review.sections[*part].tier;
                    let over = if kind.is_empty() { (part + 1).to_string() } else { format!("{} · {}", part + 1, kind.to_uppercase()) };
                    ui.add(egui::Label::new(RichText::new(over).font(mono(10.5)).extra_letter_spacing(0.8).color(tones.faint)).selectable(false));
                }
                let name = match card {
                    Card::Part(part) => open.data.review.sections[*part].title.clone(),
                    other => open.label(*other),
                };
                ui.add(egui::Label::new(RichText::new(name).font(display(large)).line_height(Some((large * 1.42_f32).round())).color(if on { tones.strong } else { tones.text })).wrap().selectable(false));
            })
        };
        let hit = row.response.interact(egui::Sense::click());
        if lit && scroll {
            hit.scroll_to_me(None);
        }
        if fx::touch(ui, hit, tones.strong).on_hover_text(if folded { open.label(*card) } else { String::new() }).clicked() {
            pressed = Some(n);
        }
    }
    pressed
}

/// A key as it is printed on one, and what it does.
fn keycap(ui: &mut egui::Ui, tones: &Tones, keys: &str, does: &str) {
    ui.spacing_mut().item_spacing.x = 4.0;
    for key in keys.split(' ') {
        egui::Frame::new().fill(tones.raised).corner_radius(5.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| ui.add(egui::Label::new(RichText::new(key).font(mono(12.0)).color(tones.strong)).selectable(false)));
    }
    ui.add(egui::Label::new(RichText::new(does).font(mono(12.5)).color(tones.faint)).selectable(false));
    ui.add_space(10.0);
}

/// The heading of a part of a card.
fn head(ui: &mut egui::Ui, tones: &Tones, text: impl AsRef<str>) {
    ui.add_space(10.0);
    ui.label(RichText::new(text.as_ref().to_uppercase()).font(bold(11.5)).extra_letter_spacing(0.8).line_height(Some(19.0)).color(tones.faint));
    ui.add_space(4.0);
}

/// A picture from the bundle shown in place; anything else it holds (a recording, a file) as a link. Either opens in the machine's own viewer or player.
fn artifact(ui: &mut egui::Ui, dir: &str, path: &str) {
    let shown = matches!(std::path::Path::new(path).extension().and_then(|kind| kind.to_str()), Some("png" | "jpg" | "jpeg" | "gif" | "webp"));
    let hit = match shown {
        true => ui.add(egui::Image::new(host::uri(dir, path)).fit_to_exact_size(egui::vec2(ui.available_width(), 560.0)).corner_radius(8.0).sense(egui::Sense::click())),
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
    let plain = egui::TextFormat { font_id: FontId::proportional(size), color: tones.text, line_height: Some(leading(size)), ..Default::default() };
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
            job.append(found.as_str(), 0.0, egui::TextFormat { font_id: FontId::proportional(size), color: tones.link, line_height: Some(leading(size)), ..Default::default() });
            if ui.add(egui::Label::new(job).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&url).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            from = found.end();
        }
        ui.label(inline(tones, &text[from..], size));
    });
}

fn prose(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str) {
    prose_at(ui, tones, links, text, 15.5);
}

fn prose_at(ui: &mut egui::Ui, tones: &Tones, links: &Links, text: &str, size: f32) {
    let roomy = size >= 15.0;
    ui.scope(|ui| {
        ui.set_max_width(800.0_f32.min(ui.available_width()));
        ui.spacing_mut().item_spacing.y = 0.0;
        let blocks = text.trim().split("\n\n").flat_map(|block| block.split("\n# ").enumerate().map(|(n, part)| if n == 0 { part.to_string() } else { format!("# {part}") }));
        for (n, block) in blocks.enumerate() {
            let mut lines = block.lines().peekable();
            if let Some(heading) = lines.peek().and_then(|line| line.strip_prefix("# ")) {
                match roomy {
                    true => head(ui, tones, heading),
                    false => {
                        ui.add_space(if n == 0 { 0.0 } else { 6.0 });
                        ui.label(RichText::new(heading).font(display(size + 1.5)).line_height(Some(leading(size))).color(tones.strong));
                        ui.add_space(4.0);
                    }
                }
                lines.next();
            }
            let mut after = 0.0;
            for line in lines.filter(|line| !line.trim().is_empty()) {
                let (mark, rest) = line.trim_start().split_once(' ').unwrap_or(("", line));
                let listed = mark == "-" || mark == "*" || mark.strip_suffix('.').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                ui.add_space(after);
                if listed {
                    ui.horizontal_top(|ui| {
                        ui.add_sized([if roomy { 24.0 } else { 18.0 }, leading(size)], egui::Label::new(RichText::new(if mark.ends_with('.') { mark } else { "•" }).size(size).color(tones.text)));
                        self::line(ui, tones, links, rest, size);
                    });
                } else {
                    self::line(ui, tones, links, line, size);
                }
                after = if listed { 4.0 } else { 0.0 };
            }
            ui.add_space(10.0);
        }
    });
}

fn button(ui: &mut egui::Ui, tones: &Tones, label: &str, on: bool) -> egui::Response {
    let (fill, ink) = if on { (tones.accent, tones.on_accent) } else { (tones.raised, tones.strong) };
    let tight = std::mem::replace(&mut ui.spacing_mut().button_padding, egui::vec2(12.0, 7.0));
    let made = ui.add(egui::Button::new(RichText::new(label).font(bold(14.0)).color(ink)).fill(fill).stroke(Stroke::new(1.0, if on { tones.accent } else { tones.line })).corner_radius(8.0));
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
            if small(ui, Icon::SquareTerminal, "Open this agent in a pane of your terminal: the same conversation, and while it is open there, what you ask here is typed into it") {
                let (on, _) = (open.on(), open.folder());
                let args: Vec<&str> = ["agent", open.dir.as_str(), "ask"].into_iter().chain(on.as_deref()).collect();
                open.note = match revui(&args, None).or_else(|_| revui(&["resume", &open.dir], None)) {
                    Ok(said) => said.trim().to_string(),
                    Err(problem) => problem,
                };
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
            // Only the press counts: the release and the repeats of a held key are swallowed too, or one keystroke would move twice.
            raw.events.retain(|event| match event {
                egui::Event::Key { key: Key::Tab, modifiers, pressed, repeat, .. } if !modifiers.shift => {
                    self.tab |= *pressed && !*repeat;
                    false
                }
                _ => true,
            });
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
            (visuals.window_corner_radius, visuals.menu_corner_radius) = (14.into(), 8.into());
            (visuals.window_fill, visuals.panel_fill, visuals.extreme_bg_color) = (tones.bg, tones.bg, tones.pane);
            (visuals.window_stroke, visuals.window_shadow) = (Stroke::new(1.0, tones.line), egui::Shadow { offset: [0, 16], blur: 48, spread: 0, color: Color32::from_black_alpha(70) });
            (visuals.selection.bg_fill, visuals.selection.stroke) = (tones.accent, Stroke::new(1.0, tones.on_accent));
            for look in [&mut visuals.widgets.noninteractive, &mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active, &mut visuals.widgets.open] {
                look.corner_radius = 8.into();
            }
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, tones.line);
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, tones.line);
            visuals.override_text_color = Some(tones.text);
            // Light text on dark is drawn as the rasterizer covers it, not thickened: the weight a browser gives the same fonts.
            visuals.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Off;
            ctx.all_styles_mut(|style| style.spacing.scroll = egui::style::ScrollStyle::floating());
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
                            ui.label(RichText::new("revui").font(display(24.0)).color(tones.strong));
                        });
                        ui.add_space(14.0);
                        ui.label(RichText::new("Merge request or pull request link").font(display(14.0)).color(tones.strong));
                        ui.add_space(4.0);
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
                                let row = egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, tones.line)).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(if past.title.is_empty() { &past.dir } else { &past.title }).size(16.0).line_height(Some(24.0)).color(ink));
                                    ui.label(RichText::new(about).size(13.0).line_height(Some(20.0)).color(faint));
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
        // A sheet over the review: the keys, or what the review is made of.
        let sheets = egui::Id::new("sheet");
        let mut sheet = ctx.data(|data| data.get_temp::<&'static str>(sheets));
        if sheet.is_some() && ctx.input(|input| input.key_pressed(Key::Escape) || input.key_pressed(Key::Q)) {
            sheet = None;
            ctx.data_mut(|data| data.remove::<&'static str>(sheets));
        } else if pressed(Key::Q) && self.open.as_ref().is_some_and(|open| open.contents.is_none() && open.publish.is_none()) {
            return self.list();
        }
        let open = self.open.as_mut().expect("a review is open");
        let links = open.data.links.clone();
        let listed = open.contents.is_some();
        let modal = listed || open.publish.is_some() || sheet.is_some();
        let wide = ctx.content_rect().width() >= 860.0;
        open.rail &= wide;
        let folds = egui::Id::new("rail folded");
        let mut folded = ctx.data(|data| data.get_temp::<bool>(folds)).unwrap_or(false);
        let pressed = |key: Key| !modal && pressed(key);
        let step = pressed(Key::ArrowRight) as isize - pressed(Key::ArrowLeft) as isize;
        if step != 0 {
            open.at = (open.at as isize + step).clamp(0, open.cards.len() as isize - 1) as usize;
            (open.in_diff, open.line, open.proof, open.range) = (false, 0, 0, None);
        }
        open.diff ^= pressed(Key::D);
        let shift = ctx.input(|input| input.modifiers.shift);
        if pressed(Key::B) && !shift {
            open.contents = Some(open.at);
        }
        folded ^= pressed(Key::B) && shift;
        if pressed(Key::Questionmark) || pressed(Key::Slash) {
            sheet = Some("keys");
        }
        if pressed(Key::S) && shift {
            sheet = Some("stats");
        }
        if pressed(Key::M) {
            if let Some(url) = open.data.review.sources.first().filter(|source| !source.url.is_empty()) {
                ctx.open_url(egui::OpenUrl::new_tab(&url.url));
            }
        }
        let held = |key: Key| !typing && !modal && ctx.input(|input| input.modifiers.ctrl && input.key_pressed(key));
        let asking = std::mem::take(&mut open.asking);
        // The columns take the keys in turn: contents, card, diff, agent. Tab and l go on to the next, h back to the one before.
        let across = std::mem::take(&mut self.tab) as isize + pressed(Key::L) as isize - pressed(Key::H) as isize;
        if across != 0 && !modal {
            open.leave = true;
            let columns: Vec<&str> = [("contents", wide), ("card", true), ("diff", open.diff && open.group().is_some()), ("agent", open.agent && !open.minimized)].into_iter().filter(|(_, there)| *there).map(|(name, _)| name).collect();
            let now = if asking { "agent" } else if open.in_diff { "diff" } else if open.rail { "contents" } else { "card" };
            let at = columns.iter().position(|name| *name == now).unwrap_or(0) as isize;
            let to = columns[(at + across).rem_euclid(columns.len() as isize) as usize];
            (open.rail, open.in_diff, open.ask, open.range) = (to == "contents", to == "diff", to == "agent", None);
            open.moved |= to == "diff";
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
        } else if open.rail {
            let last = open.cards.len() as isize - 1;
            let to = match end {
                Some(bottom) => bottom as isize * last,
                None => open.at as isize + down + half * 5,
            };
            if to.clamp(0, last) as usize != open.at {
                (open.at, open.line, open.proof) = (to.clamp(0, last) as usize, 0, 0);
            }
        } else {
            glide = ((down * 60 + half * 420) as f32, end);
            if let Card::Item(n) = open.cards[open.at] {
                // e goes through the evidence, opening what each one holds; space opens or closes the one it is on.
                let holds: Vec<bool> = open.data.review.items[n].evidence.iter().map(|evidence| evidence.text.as_ref().is_some_and(|text| !text.trim().is_empty())).collect();
                let fold = |at: usize| egui::Id::new(("evidence open", &open.dir, n, at));
                let opened = |at: usize| ctx.data(|data| data.get_temp::<bool>(fold(at))).unwrap_or(false);
                let on = open.proof.min(holds.len().saturating_sub(1));
                if pressed(Key::E) && !holds.is_empty() {
                    let to = if shift { (on + holds.len() - 1) % holds.len() } else if holds[on] && !opened(on) { on } else { (on + 1) % holds.len() };
                    ctx.data_mut(|data| data.insert_temp(fold(to), true));
                    (open.proof, open.moved) = (to, true);
                }
                if pressed(Key::Space) && holds.get(on) == Some(&true) {
                    let now = opened(on);
                    ctx.data_mut(|data| data.insert_temp(fold(on), !now));
                }
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
        let mut jump: Option<usize> = None;
        let mut opening: Option<String> = None;
        if let Card::Item(n) = open.cards[open.at] {
            let id = open.data.review.items[n].id.clone();
            match (pressed(Key::A), pressed(Key::X)) {
                (true, _) => marked = Some((id, "agree")),
                (_, true) => marked = Some((id, "dismiss")),
                _ => {}
            }
        }

        let source = open.data.review.sources.first().cloned().unwrap_or_default();
        let changed = |sign: char| open.data.files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines).filter(|line| line.starts_with(sign)).count();
        let (added, removed) = (changed('+'), changed('-'));
        let (plus, minus) = signs(&tones);
        let under = egui::Panel::top("bar").frame(frame.inner_margin(egui::Margin { left: 18, right: 18, top: 10, bottom: 8 })).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                home = logo(ui, 20.0).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Your reviews (q)").clicked();
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if bare(ui, &tones, if tones.bg == INK { Icon::Sun } else { Icon::Moon }, "Switch between light and dark", true, false) {
                        ctx.set_theme(if tones.bg == INK { egui::Theme::Light } else { egui::Theme::Dark });
                    }
                    if bare(ui, &tones, Icon::ChartNoAxesColumn, "Stats for this review (S)", true, sheet == Some("stats")) {
                        sheet = Some("stats");
                    }
                    ui.add_space(8.0);
                    if open.data.outgoing > 0 {
                        ui.label(RichText::new(open.data.outgoing.to_string()).font(mono(12.0)).color(tones.strong));
                    }
                    if bare(ui, &tones, Icon::Send, "Publish (p)", true, open.data.outgoing > 0) {
                        open.publish = Some((0, open.data.preview.clone()));
                    }
                    if bare(ui, &tones, Icon::Bot, if open.chat.busy { "Agent, working (t)" } else { "Agent (t)" }, true, open.agent && !open.minimized) {
                        (open.agent, open.minimized) = (!open.agent || open.minimized, false);
                    }
                    open.diff ^= bare(ui, &tones, Icon::FileDiff, "Diff (d)", true, open.diff);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let named = format!("{} {}", source.repo.rsplit('/').next().unwrap_or(""), source.reference);
                            if !named.trim().is_empty() {
                                let link = ui.add(egui::Label::new(RichText::new(named.trim()).font(bold(15.0)).line_height(Some(24.0)).color(tones.dim)).selectable(false).sense(egui::Sense::click()));
                                if link.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open the merge request (m)").clicked() && !source.url.is_empty() {
                                    ctx.open_url(egui::OpenUrl::new_tab(&source.url));
                                }
                            }
                            let part = |font: FontId, color| egui::TextFormat { font_id: font, color, line_height: Some(24.0), ..Default::default() };
                            let mut title = egui::text::LayoutJob::default();
                            title.append(&open.data.review.title, 0.0, part(bold(15.0), tones.strong));
                            title.append(&format!("+{added}"), 10.0, part(display(15.0), plus));
                            title.append(&format!("−{removed}"), 5.0, part(display(15.0), minus));
                            ui.add(egui::Label::new(title).truncate());
                        });
                        let about = [format!("{} files", open.data.files.len()), format!("head {:.8}", source.head_sha), format!("{} → {}", source.branch, source.target), source.author.clone()];
                        ui.add(egui::Label::new(RichText::new(about.iter().filter(|part| !part.is_empty() && part.as_str() != " → " && part.as_str() != "head ").cloned().collect::<Vec<_>>().join(" · ")).size(12.5).line_height(Some(20.0)).color(tones.faint)).truncate());
                    });
                });
            });
            if open.chat.busy {
                let edge = ui.clip_rect();
                fx::shimmer(ui, egui::Rect::from_min_max(egui::pos2(edge.left(), edge.bottom() - 2.0), edge.max), tones.accent);
            }
        });
        let under = under.response.rect.bottom();
        // Where you are: the contents column, back and next, and the card shown, which opens the list of them all.
        egui::Panel::top("where").frame(frame.inner_margin(egui::Margin::symmetric(18, 6))).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                folded ^= bare(ui, &tones, Icon::PanelLeft, "Collapse or open the contents column (B)", true, false);
                ui.add_space(6.0);
                turn -= bare(ui, &tones, Icon::ChevronLeft, "Previous card (←)", open.at > 0, false) as isize;
                turn += bare(ui, &tones, Icon::ChevronRight, "Next card (→)", open.at + 1 < open.cards.len(), false) as isize;
                ui.add_space(8.0);
                let name = match open.cards[open.at] {
                    Card::Summary => "summary".into(),
                    Card::Verify => "verification".into(),
                    Card::Part(part) => format!("part {}", part + 1),
                    Card::Item(n) => format!("{} {}", kind(&open.data.review.items[n]), open.data.review.items[n].id),
                    Card::Thread(_) => "thread".into(),
                    Card::Note(_) => "your comment".into(),
                    Card::Loose => "code in no part".into(),
                };
                let now = ui.add(egui::Label::new(RichText::new(format!("{} of {} · {name} {}", open.at + 1, open.cards.len(), Icon::ChevronDown.unicode())).font(mono(13.0)).color(tones.strong)).selectable(false).sense(egui::Sense::click()));
                if fx::touch(ui, now, tones.strong).on_hover_text("Contents (b)").clicked() {
                    open.contents = Some(open.at);
                }
            });
        });
        ctx.data_mut(|data| data.insert_temp(folds, folded));
        if turn != 0 {
            open.at = (open.at as isize + turn).clamp(0, open.cards.len() as isize - 1) as usize;
            (open.in_diff, open.line, open.proof, open.range) = (false, 0, 0, None);
        }
        egui::Panel::bottom("keys").frame(frame.inner_margin(egui::Margin::symmetric(14, 5))).show_inside(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if open.agent && open.minimized {
                    let name = format!("{}  Agent{}", Icon::Bot.unicode(), if open.chat.busy { " · working" } else { "" });
                    let chip = ui.add(egui::Button::new(RichText::new(name).font(bold(12.5)).color(tones.on_accent)).fill(tones.accent).corner_radius(8.0));
                    if fx::touch(ui, chip, tones.on_accent).on_hover_text("Restore the agent (Cmd-J or Cmd-Shift-M)").clicked() {
                        open.minimized = false;
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    let item = matches!(open.cards[open.at], Card::Item(_));
                    let here: &[(&str, &str)] = match (open.in_diff, open.rail, item) {
                        (true, _, _) => &[("j k", "line"), ("V", "select"), ("c", "comment")],
                        (_, true, _) => &[("j k", "card")],
                        (_, _, true) => &[("j k", "scroll"), ("a", "agree"), ("x", "dismiss"), ("c", "comment"), ("e", "evidence")],
                        _ => &[("j k", "scroll"), ("c", "comment")],
                    };
                    for (keys, does) in [("→", "next"), ("←", "back"), ("h l", "column")].iter().chain(here).chain(&[("t", "ask"), ("d", "diff"), ("b", "contents"), ("p", "publish"), ("?", "help")]) {
                        keycap(ui, &tones, keys, does);
                    }
                    if !open.note.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&open.note).font(mono(12.5)).color(tones.dim)).truncate());
                    }
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

        // The contents stand in a column beside every card. Folded, it is a strip of marks.
        if wide {
            let turned = ctx.data(|data| data.get_temp::<usize>(egui::Id::new("rail shows"))) != Some(open.at);
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("rail shows"), open.at));
            egui::Panel::left("rail").exact_size(if folded { 54.0 } else { 300.0 }).resizable(false).frame(frame.inner_margin(egui::Margin { left: if folded { 6 } else { 10 }, right: if folded { 6 } else { 10 }, top: 14, bottom: 0 })).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("rail").auto_shrink(false).show(ui, |ui| {
                    if let Some(pressed) = contents(ui, open, &tones, folded, None, turned) {
                        (jump, open.rail) = (Some(pressed), true);
                    }
                    ui.add_space(28.0);
                });
                bar(ui, open.rail);
            });
        }

        if let (true, Some(group)) = (open.diff, open.group()) {
            let mut clicked = None;
            // Every row of the diff is one height, so only the rows in view are laid out and coloured: a part can run to thousands of lines.
            const TALL: f32 = 19.0;
            enum Shown {
                Gap,
                File(usize),
                Hunk(usize, usize),
                Line { file: usize, hunk: usize, nth: usize, number: u32, sign: char, at: usize },
            }
            let mut rows = Vec::new();
            let (mut last, mut at) = (usize::MAX, 0);
            for &(file, hunk_at) in &open.data.groups[group] {
                let hunk = &open.data.files[file].hunks[hunk_at];
                if file != last {
                    rows.extend((last != usize::MAX).then_some(Shown::Gap));
                    rows.push(Shown::File(file));
                    last = file;
                }
                rows.push(Shown::Hunk(file, hunk_at));
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
                    rows.push(Shown::Line { file, hunk: hunk_at, nth, number, sign, at });
                    at += (sign != '\\') as usize;
                }
            }
            // The line a finding is about is marked, and the diff opens on it; a part opens at its top.
            let cited = match open.cards[open.at] {
                Card::Item(n) => open.data.review.items[n].at.clone(),
                _ => None,
            };
            let marked = cited.as_ref().and_then(|at| open.lines().iter().position(|(file, line, old)| !old && *file == at.file && *line == at.line));
            let turned = ctx.data(|data| data.get_temp::<(usize, usize)>(egui::Id::new("diff shows"))) != Some((open.at, group));
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("diff shows"), (open.at, group)));
            if let (true, false, Some(line)) = (turned, open.in_diff, marked) {
                open.line = line;
            }
            let cursor = rows.iter().position(|row| matches!(row, Shown::Line { at, sign, .. } if *sign != '\\' && *at == open.line));
            let half = (ctx.content_rect().width() - if wide { if folded { 54.0 } else { 300.0 } } else { 0.0 }) / 2.0;
            egui::Panel::right("diff").default_size(half.max(320.0)).frame(egui::Frame::new().fill(INK)).show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let about = match &cited {
                    Some(at) => format!("diff · {}:{}", at.file, at.line),
                    None => format!("diff · {} files in this part", rows.iter().filter(|row| matches!(row, Shown::File(_))).count()),
                };
                egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(about).font(mono(12.5)).color(if open.in_diff { ON_INK } else { Color32::from_rgb(102, 102, 110) })).truncate());
                });
                ui.painter().hline(ui.clip_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, Color32::from_rgb(31, 31, 35)));
                let area = egui::ScrollArea::both().auto_shrink(false);
                let area = match (turned, cursor.filter(|_| marked.is_some())) {
                    (true, Some(row)) => area.vertical_scroll_offset((row as f32 * TALL - ui.available_height() / 2.0).max(0.0)).horizontal_scroll_offset(0.0),
                    (true, None) => area.vertical_scroll_offset(0.0).horizontal_scroll_offset(0.0),
                    _ => area,
                };
                area.show_rows(ui, TALL, rows.len(), |ui, seen| {
                    if let (true, true, Some(cursor)) = (open.in_diff, open.moved, cursor) {
                        let top = ui.max_rect().top() + (cursor as f32 - seen.start as f32) * TALL;
                        ui.scroll_to_rect(egui::Rect::from_min_size(egui::pos2(ui.max_rect().left(), top), egui::vec2(1.0, TALL)), None);
                    }
                    let across = egui::Layout::left_to_right(egui::Align::Center);
                    for row in &rows[seen] {
                        let band = egui::Rect::from_min_size(egui::pos2(ui.clip_rect().left(), ui.cursor().top()), egui::vec2(ui.clip_rect().width(), TALL));
                        let wide = egui::vec2(ui.available_width(), TALL);
                        match *row {
                            Shown::Gap => drop(ui.allocate_space(egui::vec2(1.0, TALL))),
                            Shown::File(file) => {
                                ui.painter().rect_filled(band, 0.0, Color32::from_rgb(16, 16, 18));
                                ui.painter().hline(band.x_range(), band.top(), Stroke::new(1.0, Color32::from_rgb(31, 31, 35)));
                                ui.allocate_ui_with_layout(wide, across, |ui| {
                                    ui.add_space(18.0);
                                    ui.add(egui::Label::new(RichText::new(&open.data.files[file].path).font(mono(12.5)).color(ON_INK)).extend());
                                });
                            }
                            Shown::Hunk(file, hunk) => {
                                ui.painter().rect_filled(band, 0.0, Color32::from_rgb(19, 19, 22));
                                ui.allocate_ui_with_layout(wide, across, |ui| {
                                    ui.add_space(58.0);
                                    ui.add(egui::Label::new(RichText::new(&open.data.files[file].hunks[hunk].header).font(mono(12.0)).color(Color32::from_rgb(102, 102, 110))).extend());
                                });
                            }
                            Shown::Line { file, hunk, nth, number, sign, at } => {
                                let (shown, lines) = (&open.data.files[file], &open.data.files[file].hunks[hunk].lines);
                                let painted = open.painted.entry((file, hunk)).or_insert_with(|| paint(&shown.path, lines));
                                let (fill, ink) = match sign {
                                    '+' => (ADDED, Color32::WHITE),
                                    '-' => (REMOVED, Color32::from_white_alpha(170)),
                                    _ => (Color32::TRANSPARENT, Color32::from_white_alpha(140)),
                                };
                                let cited = sign != '\\' && marked == Some(at);
                                let under = open.in_diff && sign != '\\' && at == open.line;
                                let chosen = open.in_diff && sign != '\\' && open.range.is_some_and(|from| (from.min(open.line)..=from.max(open.line)).contains(&at));
                                ui.painter().rect_filled(band, 0.0, fill);
                                if cited {
                                    ui.painter().rect_filled(band, 0.0, Color32::from_rgb(37, 37, 43));
                                    ui.painter().rect_filled(egui::Rect::from_min_size(band.min, egui::vec2(2.0, TALL)), 0.0, ON_INK);
                                }
                                if chosen {
                                    ui.painter().rect_filled(band, 0.0, Color32::from_white_alpha(28));
                                    ui.painter().rect_filled(egui::Rect::from_min_size(band.min, egui::vec2(3.0, TALL)), 0.0, Color32::WHITE);
                                }
                                ui.allocate_ui_with_layout(wide, across, |ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.label(RichText::new(format!("{number:>6}  ")).font(mono(12.0)).color(Color32::from_rgb(102, 102, 110)));
                                    ui.label(RichText::new(format!("{sign} ")).font(mono(12.5)).color(match sign {
                                        '+' => Color32::from_rgb(95, 196, 138),
                                        '-' => Color32::from_rgb(255, 115, 105),
                                        _ => ink,
                                    }));
                                    let mut code = egui::text::LayoutJob::default();
                                    for (color, text) in &painted[nth] {
                                        let color = if sign == '-' { color.gamma_multiply(0.6) } else if sign == '+' { *color } else { color.gamma_multiply(0.75) };
                                        code.append(text, 0.0, egui::TextFormat { font_id: mono(12.5), color, ..Default::default() });
                                    }
                                    ui.add(egui::Label::new(code).extend());
                                });
                                if under {
                                    ui.painter().rect_filled(band, 0.0, Color32::from_white_alpha(22));
                                    ui.painter().rect_filled(egui::Rect::from_min_size(band.min, egui::vec2(2.0, TALL)), 0.0, ON_INK);
                                }
                                if sign != '\\' && ui.interact(band, ui.id().with(("diff line", at)), egui::Sense::click()).clicked() {
                                    clicked = Some(at);
                                }
                            }
                        }
                    }
                });
                bar(ui, open.in_diff);
            });
            if let Some(row) = clicked {
                (open.in_diff, open.rail, open.line, open.leave) = (true, false, row, true);
            }
        }

        let beside = open.diff && open.group().is_some();
        egui::CentralPanel::default().frame(frame.inner_margin(if beside { egui::Margin { left: 22, right: 22, top: 18, bottom: 0 } } else { egui::Margin { left: 30, right: 30, top: 26, bottom: 0 } })).show_inside(ui, |ui| {
            if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|input| input.pointer.primary_pressed()) {
                open.rail = false;
            }
            // The card is one column: beside the diff it keeps to the left, alone it sits in the middle.
            const COLUMN: f32 = 800.0;
            let inset = if beside { 0.0 } else { ((ui.available_width() - COLUMN) / 2.0).max(0.0) };
            let area = egui::ScrollArea::vertical().auto_shrink(false);
            let area = match glide.1 {
                Some(bottom) => area.vertical_scroll_offset(if bottom { f32::MAX } else { 0.0 }),
                None => area,
            };
            area.show(ui, |ui| {
                ui.scroll_with_delta(egui::vec2(0.0, -glide.0));
                let mut column = ui.available_rect_before_wrap();
                column.min.x += inset;
                column.max.x = column.max.x.min(column.min.x + COLUMN);
                ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                    let arrived = fx::arrive(ui.ctx(), open.at);
                    ui.set_opacity(arrived);
                    ui.add_space((1.0 - arrived) * 10.0);
                    match open.cards[open.at] {
                        Card::Summary => {
                            let review = &open.data.review;
                            let slab = egui::Frame::new().fill(tones.pane).stroke(Stroke::new(1.0, tones.line)).corner_radius(16.0).inner_margin(egui::Margin::symmetric(32, 28)).show(ui, |ui| {
                                let under = ui.painter().add(egui::Shape::Noop);
                                ui.set_width(ui.available_width());
                                let set = balanced(ui, &review.title, display(28.0), 39.0, tones.strong);
                                ui.label(set);
                                ui.add_space(20.0);
                                let count = |kind: &str| review.items.iter().filter(|item| item.kind == kind).count().to_string();
                                let stats = [(review.sections.len().to_string(), "parts"), (count("finding"), "findings"), (count("decision"), "decisions"), (count("good"), "good calls"), (open.data.files.len().to_string(), "files")];
                                ui.columns(stats.len(), |columns| {
                                    for (column, (value, name)) in columns.iter_mut().zip(&stats) {
                                        column.add(egui::Label::new(RichText::new(value).font(display(28.0)).color(tones.strong)).extend());
                                        column.label(RichText::new(*name).size(14.0).color(tones.strong.gamma_multiply(0.6)));
                                    }
                                });
                                under
                            });
                            ui.painter().set(slab.inner, field(ui, slab.response.rect, tones.strong));
                            ui.add_space(28.0);
                            if !review.plain.trim().is_empty() {
                                prose_at(ui, &Tones { text: tones.dim, ..tones }, &links, &review.plain, 16.5);
                                head(ui, &tones, "In detail");
                            }
                            prose(ui, &tones, &links, &review.summary);
                            head(ui, &tones, "Where it sits");
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
                            let labels = named.unwrap_or(nodes.len() <= 24);
                            egui::Frame::new().fill(tones.bg).stroke(Stroke::new(1.0, tones.line)).corner_radius(12.0).inner_margin(14.0).show(ui, |ui| {
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
                                ui.label(RichText::new(&caption).size(13.0).color(tones.faint));
                                if !text.is_empty() {
                                    ui.label(RichText::new(&text).size(13.0).color(tones.dim));
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
                            ui.add_space(16.0);
                            head(ui, &tones, "Read in this order");
                            for (part, section) in review.sections.iter().enumerate() {
                                ui.horizontal_top(|ui| {
                                    ui.add_sized([22.0, leading(15.5)], egui::Label::new(RichText::new(format!("{}.", part + 1)).size(15.5).color(tones.text)).selectable(false));
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        let hit = ui.add(egui::Label::new(RichText::new(&section.title).size(15.5).line_height(Some(leading(15.5))).color(tones.strong).underline()).selectable(false).sense(egui::Sense::click()));
                                        if fx::touch(ui, hit, tones.strong).clicked() {
                                            jump = open.cards.iter().position(|card| matches!(card, Card::Part(at) if *at == part));
                                        }
                                        ui.label(RichText::new(tier(&section.tier)).size(13.0).color(tones.faint));
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 5.0;
                                            for (n, item) in review.items.iter().enumerate().filter(|(_, item)| item.section == Some(part)) {
                                                if chip(ui, &tones, item).on_hover_text(&item.title).clicked() {
                                                    jump = open.cards.iter().position(|card| matches!(card, Card::Item(at) if *at == n));
                                                }
                                            }
                                        });
                                    });
                                });
                                ui.add_space(8.0);
                            }
                        }
                        Card::Verify => {
                            let run = open.data.verify.clone().unwrap_or_default();
                            let part = |color| egui::TextFormat { font_id: mono(13.0), color, ..Default::default() };
                            let mut over = egui::text::LayoutJob::default();
                            over.append("Verification", 0.0, part(tones.strong));
                            over.append(if run.clean() { " · smoke test passed" } else { " · the run did not finish clean" }, 0.0, part(tones.dim));
                            ui.label(over);
                            title(ui, &tones, "Sandbox verification");
                            prose(ui, &Tones { text: tones.dim, ..tones }, &links, "The change was started in a throwaway copy of the stack inside the pipeline job, and these are the steps and checks that ran against it.");
                            prose(ui, &Tones { text: tones.dim, ..tones }, &links, if run.scope.trim().is_empty() { "This is a fixed smoke test, the same for every merge request: it shows the branch still starts and answers. It does not exercise what this change altered." } else { &run.scope });
                            head(ui, &tones, "Steps");
                            for step in &run.steps {
                                ran(ui, &tones, step.ok == Some(true), &step.step, &format!("{} s", step.seconds), if step.log.is_empty() { "This step printed nothing." } else { &step.log });
                            }
                            if !run.checks.is_empty() {
                                ui.add_space(8.0);
                                head(ui, &tones, "Checks");
                            }
                            for check in &run.checks {
                                let status = check.status.as_str().map(String::from).unwrap_or_else(|| if check.status.is_null() { String::new() } else { check.status.to_string() });
                                ran(ui, &tones, check.ok.unwrap_or(status == "200"), &check.name, &status, &check.detail);
                            }
                            if !run.untested.is_empty() {
                                ui.add_space(8.0);
                                head(ui, &tones, "Not exercised");
                                prose(ui, &tones, &links, &run.untested.iter().map(|one| format!("- {one}")).collect::<Vec<_>>().join("\n"));
                            }
                            if !run.api.is_empty() {
                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("API under test:").size(13.0).color(tones.faint));
                                    egui::Frame::new().fill(tones.raised).corner_radius(5.0).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| ui.label(RichText::new(&run.api).font(mono(11.5)).color(tones.dim)));
                                });
                            }
                            for shown in run.media.iter().filter(|shown| shown.path.is_some()) {
                                ui.add_space(14.0);
                                artifact(ui, &open.dir, shown.path.as_deref().unwrap_or_default());
                                ui.label(RichText::new(&shown.caption).size(13.0).color(tones.faint));
                            }
                        }
                        Card::Part(part) => {
                            let section = &open.data.review.sections[part];
                            let mut over = egui::text::LayoutJob::default();
                            over.append(&(part + 1).to_string(), 0.0, egui::TextFormat { font_id: mono(13.0), color: tones.strong, ..Default::default() });
                            over.append(&format!(" · {}", tier(&section.tier)), 0.0, egui::TextFormat { font_id: mono(13.0), color: tones.dim, ..Default::default() });
                            ui.label(over);
                            title(ui, &tones, &section.title);
                            prose(ui, &tones, &links, &section.why);
                            let within: Vec<(usize, &Item)> = open.data.review.items.iter().enumerate().filter(|(_, item)| item.section == Some(part)).collect();
                            if within.is_empty() {
                                ui.label(RichText::new("Nothing to raise in this part.").size(13.0).color(tones.faint));
                            }
                            ui.add_space(8.0);
                            for (n, item) in within {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let pressed = chip(ui, &tones, item).clicked();
                                    let hit = ui.add(egui::Label::new(RichText::new(&item.title).size(15.5).line_height(Some(leading(15.5))).color(tones.text)).wrap().selectable(false).sense(egui::Sense::click()));
                                    if pressed || fx::touch(ui, hit, tones.strong).clicked() {
                                        jump = open.cards.iter().position(|card| matches!(card, Card::Item(at) if *at == n));
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        }
                        Card::Thread(n) => {
                            let thread = open.data.threads[n].clone();
                            title(ui, &tones, open.said(n));
                            ui.label(RichText::new(if thread.resolved { "resolved · already on the merge request" } else { "already on the merge request" }).font(mono(11.5)).color(tones.dim));
                            for (who, body, when) in &thread.notes {
                                ui.add_space(20.0);
                                ui.label(RichText::new(format!("@{} · {when}", who.trim_start_matches('@'))).font(mono(13.0)).color(tones.dim));
                                ui.add_space(6.0);
                                prose(ui, &tones, &links, body);
                            }
                        }
                        Card::Note(n) => {
                            let text = open.data.state["notes"][n]["text"].as_str().unwrap_or("").to_string();
                            title(ui, &tones, open.about(n));
                            ui.add_space(16.0);
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
                            title(ui, &tones, "Code in no part");
                            ui.add_space(16.0);
                            prose(ui, &tones, &links, "These hunks are in the diff, but the guide put them in no section. Nothing vouches for them: read them.");
                        }
                        Card::Item(n) => {
                            let item = open.data.review.items[n].clone();
                            let verdict = open.verdict(&item.id).map(String::from);
                            let proof = |how: &str| match how {
                                "observed" => "confirmed by a run".to_string(),
                                "inferred" => "reasoned from the code".to_string(),
                                other => other.to_string(),
                            };
                            let part = |color| egui::TextFormat { font_id: mono(13.0), color, ..Default::default() };
                            let mut over = egui::text::LayoutJob::default();
                            over.append(&item.id, 0.0, part(tones.strong));
                            over.append(" · ", 0.0, part(tones.dim));
                            let named = kind(&item);
                            over.append(&format!("{}{}", named[..1].to_uppercase(), &named[1..]), 0.0, part(hue(&tones, &item)));
                            if let Some(first) = item.evidence.first().filter(|first| !first.provenance.is_empty()) {
                                over.append(&format!(" · {}", proof(&first.provenance)), 0.0, part(tones.dim));
                            }
                            ui.label(over);
                            title(ui, &tones, &item.title);
                            if let Some(at) = &item.at {
                                let place = egui::Frame::new().fill(tones.raised).corner_radius(5.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| ui.add(egui::Label::new(RichText::new(format!("{}:{}", at.file, at.line)).font(mono(12.0)).color(tones.dim)).wrap().selectable(false)));
                                let hosted = open.data.hosted.get(&item.id);
                                if hosted.is_some() && fx::touch(ui, place.response.interact(egui::Sense::click()), tones.strong).on_hover_text("Open on the code host").clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(hosted.expect("checked")));
                                }
                                ui.add_space(10.0);
                            }
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
                            ui.add_space(14.0);
                            if !item.plain.trim().is_empty() {
                                prose(ui, &Tones { text: tones.dim, ..tones }, &links, &item.plain);
                            }
                            prose(ui, &tones, &links, &item.body);
                            if !item.evidence.is_empty() {
                                head(ui, &tones, format!("Evidence ({})", item.evidence.len()));
                            }
                            // Each is a line saying how it was established and what it shows. One that holds text opens to it; a picture stands under its line.
                            for (at, evidence) in item.evidence.iter().enumerate() {
                                let held = evidence.text.as_deref().map(str::trim).filter(|text| !text.is_empty());
                                let fold = egui::Id::new(("evidence open", &open.dir, n, at));
                                let mut opened = ui.data(|data| data.get_temp::<bool>(fold)).unwrap_or(false);
                                let current = !open.in_diff && !open.rail && at == open.proof;
                                ui.add_space(4.0);
                                let row = ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    if held.is_some() {
                                        ui.add_sized([18.0, leading(15.5)], egui::Label::new(RichText::new(if opened { Icon::ChevronDown } else { Icon::ChevronRight }.unicode().to_string()).size(13.0).color(tones.faint)).selectable(false));
                                    }
                                    let caption = if !evidence.caption.is_empty() { evidence.caption.as_str() } else if evidence.path.is_some() { "Figure" } else { "Trace" };
                                    let mut line = egui::text::LayoutJob::default();
                                    line.append(&proof(&evidence.provenance), 0.0, egui::TextFormat { font_id: mono(11.5), color: tones.faint, line_height: Some(leading(15.5)), valign: egui::Align::Center, ..Default::default() });
                                    line.append(caption, 12.0, egui::TextFormat { font_id: FontId::proportional(15.5), color: if current && item.evidence.len() > 1 { tones.strong } else { tones.text }, line_height: Some(leading(15.5)), valign: egui::Align::Center, ..Default::default() });
                                    ui.add(egui::Label::new(line).wrap().selectable(false));
                                });
                                let hit = ui.interact(row.response.rect, fold.with("line"), egui::Sense::click());
                                if held.is_some() && fx::touch(ui, hit, tones.strong).clicked() {
                                    opened = !opened;
                                    ui.data_mut(|data| data.insert_temp(fold, opened));
                                }
                                if current && open.moved {
                                    ui.scroll_to_rect(row.response.rect, None);
                                }
                                if let Some(path) = &evidence.path {
                                    ui.add_space(6.0);
                                    artifact(ui, &open.dir, path);
                                }
                                if let (true, Some(text)) = (opened, held) {
                                    ui.add_space(4.0);
                                    egui::Frame::new().fill(tones.pane).corner_radius(8.0).inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 2 }).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        prose_at(ui, &tones, &links, text, 14.0);
                                    });
                                }
                                ui.add_space(6.0);
                            }
                        }
                    }
                    ui.add_space(14.0);
                    // A label for the box under it, not a heading: what the comment lands on is set small, in mono.
                    let (lead, on) = match (open.picked(), open.cards[open.at]) {
                        (Some((file, line, _, Some(first))), _) => ("Comment on", format!("{file}:{first}-{line}")),
                        (Some((file, line, _, None)), _) => ("Comment on", format!("{file}:{line}")),
                        (None, Card::Item(n)) => ("Comment on", open.data.review.items[n].id.clone()),
                        (None, Card::Thread(_)) => ("Reply in this thread", String::new()),
                        _ => ("Comment on the whole merge request", String::new()),
                    };
                    let mut about = egui::text::LayoutJob::default();
                    about.append(lead, 0.0, egui::TextFormat { font_id: display(14.0), color: tones.strong, line_height: Some(21.0), ..Default::default() });
                    about.append(&on, 8.0, egui::TextFormat { font_id: mono(12.5), color: tones.dim, line_height: Some(21.0), ..Default::default() });
                    ui.add_space(12.0);
                    ui.label(about);
                    ui.add_space(6.0);
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
                    ui.add_space(28.0);
                });
            });
            bar(ui, !open.in_diff && !open.asking && !open.rail);
        });
        if let Some(at) = jump {
            (open.at, open.in_diff, open.line, open.proof, open.range) = (at, false, 0, 0, None);
        }
        if open.agent && open.floating && !open.minimized {
            let at = ctx.content_rect().right_bottom() - egui::vec2(460.0, 620.0);
            egui::Window::new("agent").title_bar(false).resizable(true).default_pos(at).default_size([420.0, 540.0]).min_size([320.0, 320.0]).frame(egui::Frame::new().fill(tones.raised).stroke(Stroke::new(1.0, tones.strong)).inner_margin(14.0)).show(&ctx, |ui| agent(ui, open, &tones));
        }
        (open.moved, open.leave) = (false, false);

        if let Some((mut choice, mut text)) = open.publish.take() {
            let (mut keep, mut send) = (true, false);
            egui::Window::new(RichText::new("Publish your review").font(display(20.0)).color(tones.strong)).collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).fixed_pos(middle).show(&ctx, |ui| {
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
            let (down, enter, leave) = ctx.input(|input| ((input.key_pressed(Key::J) || input.key_pressed(Key::ArrowDown)) as isize - (input.key_pressed(Key::K) || input.key_pressed(Key::ArrowUp)) as isize, input.key_pressed(Key::Enter) || input.key_pressed(Key::L) || input.key_pressed(Key::ArrowRight), input.key_pressed(Key::Escape)));
            on = (on as isize + down).clamp(0, open.cards.len() as isize - 1) as usize;
            let seen = egui::Id::new("contents row");
            let moved = ctx.data(|data| data.get_temp::<usize>(seen)) != Some(on);
            ctx.data_mut(|data| data.insert_temp(seen, on));
            let frame = egui::Frame::new().fill(tones.bg).stroke(Stroke::new(1.0, tones.line)).corner_radius(12.0).inner_margin(egui::Margin { left: 10, right: 10, top: 12, bottom: 16 }).shadow(egui::Shadow { offset: [0, 16], blur: 48, spread: 0, color: Color32::from_black_alpha(64) });
            let (room, id) = (ctx.content_rect(), egui::Id::new("contents"));
            let place = egui::Modal::default_area(id).anchor(egui::Align2::LEFT_TOP, egui::vec2(18.0, under + 13.0)).default_height(room.height());
            let shown = egui::Modal::new(id).area(place).backdrop_color(Color32::from_rgba_unmultiplied(8, 9, 12, 128)).frame(frame).show(&ctx, |ui| {
                ui.set_width(520.0_f32.min(room.width() - 36.0) - 20.0);
                egui::ScrollArea::vertical().max_height(room.height() - under - 80.0).show(ui, |ui| go = contents(ui, open, &tones, false, Some(on), moved));
            });
            let leave = leave || (listed && ctx.input(|input| input.key_pressed(Key::Q) || input.key_pressed(Key::B)));
            let leave = leave || shown.should_close();
            if leave || go.is_some() || enter {
                ctx.data_mut(|data| data.remove::<usize>(seen));
            }
            open.contents = if leave { None } else { Some(on) };
            if let Some(n) = go.or(enter.then_some(on)) {
                (open.at, open.contents, open.in_diff, open.line, open.proof) = (n, None, false, 0, 0);
            }
        }
        if let Some(which) = sheet {
            let frame = egui::Frame::new().fill(tones.bg).stroke(Stroke::new(1.0, tones.line)).corner_radius(14.0).inner_margin(egui::Margin { left: 22, right: 22, top: 6, bottom: 16 });
            let shown = egui::Modal::new(egui::Id::new("sheet shown")).frame(frame).show(&ctx, |ui| {
                ui.set_width(if which == "keys" { 620.0_f32 } else { 520.0 }.min(ui.ctx().content_rect().width() - 32.0));
                let rows = |ui: &mut egui::Ui, name: &str, wide: f32, rows: &[(&str, String)]| {
                    head(ui, &tones, name);
                    egui::Grid::new(name).num_columns(2).spacing([12.0, 6.0]).min_col_width(wide).show(ui, |ui| {
                        for (left, right) in rows {
                            match which {
                                "keys" => drop(ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for key in left.split(' ') {
                                        egui::Frame::new().fill(tones.raised).corner_radius(5.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| ui.label(RichText::new(key).font(mono(12.0)).color(tones.strong)));
                                    }
                                })),
                                _ => drop(ui.label(RichText::new(*left).size(14.0).color(tones.text))),
                            }
                            ui.add(egui::Label::new(RichText::new(right).size(14.0).color(tones.text)).wrap());
                            ui.end_row();
                        }
                    });
                };
                let said = |rows: &[(&'static str, &str)]| rows.iter().map(|(keys, does)| (*keys, does.to_string())).collect::<Vec<_>>();
                if which == "keys" {
                    rows(ui, "Moving", 92.0, &said(&[("→", "Next card: the summary, each part, each finding"), ("←", "Previous card"), ("l h", "Move to the next or previous column, as tab does: contents, card, diff, agent"), ("j k", "Scroll the card; in the contents column, move between cards; in the diff, move one line"), ("u U", "Page down, page up"), ("g G", "The top, the bottom"), ("b", "Contents: list every card and jump to one"), ("B", "Collapse the contents column to a strip, or open it"), ("q", "Back to your reviews")]));
                    rows(ui, "Diff and evidence", 92.0, &said(&[("d", "Show or hide the diff"), ("V", "In the diff, select lines from here"), ("e E", "Evidence: next, opening its details, or the one before"), ("space", "Evidence: open or close the details"), ("enter o", "Open the picture or recording shown"), ("m", "Open the merge request in the browser"), ("S", "Stats: the change and the guide")]));
                    rows(ui, "Your review", 92.0, &said(&[("a x", "Agree with a finding, or dismiss it"), ("c", "Comment: on the card, or on the diff lines selected"), ("t", "Ask the agent"), ("v i", "Have it verify this card, or draw a diagram for it"), ("p", "Publish: see exactly what goes out first")]));
                } else {
                    let review = &open.data.review;
                    let kinds = ["blocker", "fix", "suggestion", "question", "decision", "good"].iter().filter_map(|named| Some(review.items.iter().filter(|item| kind(item) == *named).count()).filter(|count| *count > 0).map(|count| format!("{count} {named}"))).collect::<Vec<_>>();
                    rows(ui, "This review", 130.0, &[("Merge request", format!("{} {}", source.repo, source.reference)), ("Head", format!("{:.8} {} → {}", source.head_sha, source.branch, source.target)), ("Change", format!("{} files · +{added} −{removed}", open.data.files.len())), ("Guide", format!("{} parts · {} items", review.sections.len(), review.items.len())), ("Sandbox", open.data.verify.as_ref().map_or("not run".into(), |run| format!("{} of {} steps passed · {} s", run.steps.iter().filter(|step| step.ok == Some(true)).count(), run.steps.len(), run.steps.iter().map(|step| step.seconds).sum::<f64>()))), ("Items", if kinds.is_empty() { "none".into() } else { kinds.join(" · ") }), ("Marked", format!("{} of {}", review.items.iter().filter(|item| open.verdict(&item.id).is_some()).count(), review.items.len()))]);
                }
            });
            if shown.should_close() {
                sheet = None;
            }
        }
        match sheet {
            Some(which) => drop(ctx.data_mut(|data| data.insert_temp(sheets, which))),
            None => ctx.data_mut(|data| data.remove::<&'static str>(sheets)),
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
