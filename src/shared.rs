use crate::bundle::{Bundle, Finished, Item, Kind, Section, State};
use crate::mrs;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Ask,
    Verify,
    Artifact,
    Guide,
}

pub struct Point {
    pub heading: String,
    pub text: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub start: Option<u32>,
    pub old: bool,
    pub reply_to: Option<String>,
    pub item: Option<String>,
}

pub const FINISH: [&str; 4] = [
    "Publish as one comment on the merge request",
    "Publish as one comment, and approve",
    "Publish each point as its own thread on the diff",
    "Copy the text instead, and post nothing",
];

/// Where a run happens is the person's choice per repo, kept in the repo's notes: the agent asks once, the first time it matters.
const WHERE: &str = "Run it off my screen, where `revui drive` says this repo's evidence comes from. Not asked yet: ask me first.";

/// An item that goes out on its own when you publish: a finding you agreed with, a decision you pushed back on.
pub fn raised(state: &State, item: &Item) -> bool {
    let verdict = state.verdicts.get(&item.id).map(String::as_str);
    matches!((item.kind, verdict), (Kind::Finding, Some("agree")) | (Kind::Decision, Some("dismiss")))
}

/// Whether an item's text goes out as a quote above your comments on it. Unless you chose, raised items are quoted and the rest are not.
pub fn quoted(bundle: &Bundle, state: &State, id: &str) -> bool {
    let raised = || bundle.review.items.iter().any(|item| item.id == id && raised(state, item));
    state.quotes.get(id).copied().unwrap_or_else(raised)
}

pub fn points(bundle: &Bundle, state: &State) -> Vec<Point> {
    let mut points = Vec::new();
    for item in &bundle.review.items {
        let notes: Vec<_> = state.notes.iter().filter(|note| !note.posted && note.item.as_ref() == Some(&item.id)).collect();
        let quote = quoted(bundle, state, &item.id);
        if (quote || !notes.is_empty()) && !state.published.contains(&item.id) {
            let (file, line) = item.at.as_ref().map_or((None, None), |at| (Some(at.file.clone()), Some(at.line)));
            let mut parts = Vec::new();
            if quote {
                let quoted = item.body.trim().lines().map(|line| match line.strip_prefix("# ") {
                    Some(label) => format!("> **{label}**"),
                    None => format!("> {line}").trim_end().to_string(),
                });
                parts.push(format!("> **{}**\n>\n{}", item.title, quoted.collect::<Vec<_>>().join("\n")));
            }
            parts.extend(notes.iter().map(|note| note.text.clone()));
            let text = parts.join("\n\n");
            points.push(Point { heading: String::new(), text, file, line, start: None, old: false, reply_to: None, item: Some(item.id.clone()) });
        }
    }
    let raised: Vec<&String> = points.iter().filter_map(|point| point.item.as_ref()).collect();
    let mut loose = Vec::new();
    for note in state.notes.iter().filter(|note| !note.posted && !note.item.as_ref().is_some_and(|id| raised.contains(&id))) {
        let heading = note.item.as_ref().filter(|_| note.file.is_none()).map_or(String::new(), |id| format!("On {id}"));
        loose.push(Point { heading, text: note.text.clone(), file: note.file.clone(), line: note.line, start: note.start, old: note.old, reply_to: note.reply_to.clone(), item: None });
    }
    points.extend(loose);
    points
}

pub fn compose(bundle: &Bundle, points: &[&Point]) -> String {
    let source = bundle.review.sources.first();
    let (url, head) = source.map_or(("", ""), |s| (s.url.as_str(), s.head_sha.as_str()));
    let mut body = format!("Reviewed at `{head:.8}`.");
    if points.is_empty() {
        return body + " No comments.";
    }
    for point in points {
        let place = match (&point.file, point.line) {
            (Some(file), Some(line)) => {
                let lines = point.start.map_or(line.to_string(), |start| format!("{start}-{line}"));
                match mrs::file_url(url, head, file, point.start.unwrap_or(line)).filter(|_| !point.old) {
                    Some(link) => format!("[`{file}:{lines}`]({link})"),
                    None => format!("`{file}:{lines}`"),
                }
            }
            _ => String::new(),
        };
        let lead = match (point.heading.is_empty(), place.is_empty()) {
            (true, true) => String::new(),
            (true, false) if point.text.starts_with("> ") => format!("{place}\n  "),
            (true, false) => format!("{place}: "),
            (false, true) => format!("**{}**\n  ", point.heading),
            (false, false) => format!("**{}** ({place})\n  ", point.heading),
        };
        body += &format!("\n\n- {lead}{}", point.text.trim().replace('\n', "\n  "));
    }
    body
}

/// The review as it would be posted now: every point that is not a reply in a thread.
pub fn preview(bundle: &Bundle, state: &State) -> String {
    let points = points(bundle, state);
    compose(bundle, &points.iter().filter(|point| point.reply_to.is_none()).collect::<Vec<_>>())
}

/// The work of publishing, to run off the screen's thread: it posts `body`, the text the person saw, and returns what went wrong.
pub fn posting(bundle: &Bundle, state: &State, choice: usize, body: String) -> Result<impl FnOnce() -> Vec<String> + Send + use<>, String> {
    let source = bundle.review.sources.first().filter(|s| !s.repo.is_empty() && !s.reference.is_empty());
    let source = source.ok_or("This bundle does not name a merge request to post to.")?;
    let (repo, reference, head) = (source.repo.clone(), source.reference.clone(), source.head_sha.clone());
    let points = points(bundle, state);
    let general: Vec<&Point> = points.iter().filter(|p| p.reply_to.is_none() && (p.file.is_none() || p.line.is_none())).collect();
    let placed = points.iter().any(|p| p.reply_to.is_none() && p.file.is_some() && p.line.is_some());
    let general_body = (!general.is_empty()).then(|| compose(bundle, &general));
    let jobs: Vec<(Option<String>, Option<(String, u32, bool, Option<u32>)>, String)> = points
        .iter()
        .map(|p| {
            let text = if p.heading.is_empty() { p.text.clone() } else { format!("**{}**\n\n{}", p.heading, p.text) };
            (p.reply_to.clone(), p.file.clone().zip(p.line).map(|(f, l)| (f, l, p.old, p.start)), text)
        })
        .collect();
    let threads_wanted = choice == 2 && placed;
    Ok(move || {
        let mut problems = Vec::new();
        let mut failed_inline = Vec::new();
        for (reply_to, place, text) in &jobs {
            let result = match (reply_to, place, threads_wanted) {
                (Some(thread), _, _) => mrs::post_reply(&repo, &reference, thread, text),
                (None, Some((file, line, old, start)), true) => {
                    let posted = mrs::post_on_line(&repo, &reference, &head, file, *start, *line, *old, text);
                    if posted.is_err() {
                        failed_inline.push(format!("- `{file}:{line}`: {}", text.replace('\n', "\n  ")));
                    }
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                problems.push(e);
            }
        }
        let note = match threads_wanted {
            false => Some(body),
            true => {
                let mut summary = general_body.unwrap_or_default();
                if !failed_inline.is_empty() {
                    summary += &format!("\n\nThese could not be placed on the diff:\n\n{}", failed_inline.join("\n"));
                }
                (!summary.trim().is_empty()).then_some(summary)
            }
        };
        if let Some(Err(e)) = note.map(|text| mrs::post_note(&repo, &reference, &text)) {
            problems.push(e);
        }
        if choice == 1 {
            if let Err(e) = mrs::approve(&repo, &reference) {
                problems.push(format!("approve: {e}"));
            }
        }
        problems
    })
}

/// Records that everything `posting` was built from went out. Call it with the state as it was then.
pub fn mark_published(bundle: &Bundle, state: &mut State, choice: usize) {
    let raised: Vec<String> = points(bundle, state).into_iter().filter_map(|point| point.item).collect();
    for note in &mut state.notes {
        note.posted = true;
    }
    state.published.extend(raised);
    let how = ["published as one comment", "published and approved", "published as threads"][choice.min(2)];
    state.finished = Some(Finished { how: how.into(), posted: true });
    let _ = bundle.save_state(state);
}

/// The mark a pipeline's comment carries, so the next run finds it and replaces it where a new one would pile up.
pub const COMMENT_MARK: &str = "<!-- revui-guide -->";

/// Text from a model, made safe to post under a bot's name: a line that opens with `/` is a command on GitLab
/// (`/approve`, `/close`) and `@name` notifies that person, so each gets a zero-width space that stops it being read as one.
/// With `code`, what the host shows as code is left as written, since neither works there and a hidden character
/// would break whatever is copied out of it. Only code that cannot be read any other way counts: a fence that opens
/// after a blank line and closes at the next line of backticks, and a span on one line with no stray backtick before
/// it in its paragraph. A `<` or a backslash in prose ends that for the rest of the text: raw HTML and escapes let a
/// host read backticks as text. Anything else is treated as prose.
fn defused(text: &str, code: bool) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let fence = |line: &str| {
        let ticks = line.len() - line.trim_start_matches('`').len();
        (ticks >= 3 && !line[ticks..].contains('`')).then_some(ticks)
    };
    let closes = |line: &str, ticks: usize| line.trim_end().len() >= ticks && line.trim_end().chars().all(|c| c == '`');
    let mut out = Vec::new();
    let (mut n, mut stray, mut html) = (0, false, false);
    while n < lines.len() {
        let line = lines[n];
        let after_blank = n == 0 || lines[n - 1].trim().is_empty();
        let next = |ticks| (n + 1..lines.len()).find(|&m| lines[m].starts_with("```")).filter(|&m| closes(lines[m], ticks));
        if let Some(end) = fence(line).filter(|_| code && after_blank && !html).and_then(next) {
            out.extend(lines[n..=end].iter().map(|line| line.to_string()));
            (n, stray) = (end + 1, false);
            continue;
        }
        stray &= !line.trim().is_empty();
        let indent = line.len() - line.trim_start().len();
        let mut safe = line[..indent].to_string();
        if line[indent..].starts_with('/') {
            safe.push('\u{200b}');
        }
        let mut rest = &line[indent..];
        while !rest.is_empty() {
            let prose = &rest[..rest.find('`').unwrap_or(rest.len())];
            html |= prose.contains(['<', '\\']);
            safe += &prose.replace('@', "@\u{200b}");
            rest = &rest[prose.len()..];
            if !code || stray || html {
                safe += &rest.replace('@', "@\u{200b}");
                break;
            }
            let ticks = rest.len() - rest.trim_start_matches('`').len();
            let mut from = ticks;
            let close = loop {
                let Some(at) = rest[from..].find('`').map(|at| from + at) else { break None };
                let run = rest[at..].len() - rest[at..].trim_start_matches('`').len();
                if run == ticks {
                    break Some(at + run);
                }
                from = at + run;
            };
            stray = close.is_none() && ticks > 0;
            let taken = close.unwrap_or(ticks);
            safe += &rest[..taken];
            rest = &rest[taken..];
        }
        out.push(safe);
        n += 1;
    }
    out.join("\n")
}

/// The guide as one comment a pipeline posts on the merge request: the summary, then each finding folded under its
/// title. `link` is where the whole guide opens.
pub fn comment(bundle: &Bundle, link: Option<&str>) -> String {
    let review = &bundle.review;
    let source = review.sources.first();
    let mut out = format!("{COMMENT_MARK}\n### Guided review\n\n{}\n", defused(review.summary.trim(), true));
    let label = |item: &Item| match item.kind {
        Kind::Good => "good".to_string(),
        Kind::Decision => "decision".to_string(),
        Kind::Finding if item.severity.is_empty() => "finding".to_string(),
        Kind::Finding => item.severity.clone(),
    };
    for (n, section) in review.sections.iter().enumerate() {
        let items: Vec<&Item> = review.items.iter().filter(|item| item.section == Some(n)).collect();
        if items.is_empty() {
            continue;
        }
        out += &format!("\n**{}. {}**\n\n", n + 1, defused(&section.title, false));
        for item in items {
            let at = item.at.as_ref().map(|at| {
                let place = format!("`{}:{}`", at.file, at.line);
                let url = source.and_then(|source| mrs::file_url(&source.url, &source.head_sha, &at.file, at.line));
                url.map_or(format!(" · {place}"), |url| format!(" · [{place}]({url})"))
            });
            let how = match (item.kind, item.evidence.iter().any(|one| !matches!(one.provenance, crate::bundle::Provenance::Inferred))) {
                (Kind::Good, _) => "",
                (_, true) => " · checked in a run",
                (_, false) => " · reasoned from the code",
            };
            let head = format!("<b>{}</b> · {} · {}", item.id, label(item), defused(&item.title, false).replace('<', "&lt;"));
            out += &format!("<details><summary>{head}</summary>\n\n{}{how}\n\n{}\n\n</details>\n", at.unwrap_or_default().trim_start_matches(" · "), defused(item.body.trim(), true));
        }
    }
    if let Some(run) = crate::html::verify(&bundle.dir) {
        let steps = run["steps"].as_array().map_or(0, Vec::len) + run["checks"].as_array().map_or(0, Vec::len);
        let failed = ["steps", "checks"].iter().flat_map(|key| run[key].as_array().into_iter().flatten()).filter(|one| one["ok"] == false).count();
        out += &format!("\nThe change was run in a sandbox: {steps} steps and checks, {failed} failed.\n");
    }
    let head = source.map_or("", |source| &source.head_sha[..source.head_sha.len().min(8)]);
    out += &format!("\n---\n<sub>{}Written for `{head}` by an agent in this pipeline, as a first pass. A person still owns the review.</sub>\n", link.map_or(String::new(), |link| format!("[Open the whole guide]({link}) · ")));
    // ponytail: GitHub refuses a comment over 65536 characters; cut there and point at the guide. Per-finding comments if guides outgrow it.
    if out.len() > 60000 {
        let mut end = 60000;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out += "\n\n</details>\n\nCut short here: the whole guide has the rest.\n";
    }
    out
}

/// What an agent is told. `place` is the file and line the person is on, `picked` the claims they ticked.
pub fn prompt(bundle: &Bundle, mode: Mode, fresh: bool, item: Option<&Item>, place: (&str, u32), section: Option<&Section>, picked: &[String]) -> String {
    let url = bundle.review.sources.first().map_or("", |s| s.url.as_str());
    let dir = bundle.dir.to_string_lossy();
    let (file, line) = place;
    let looking_at = match item {
        Some(item) => format!("{} \"{}\" ({file}:{line})", item.id, item.title),
        None => format!("{file}:{line}"),
    };
    let target = match (item, section) {
        _ if !picked.is_empty() => format!("these claims: {}", picked.join("; ")),
        (Some(item), _) => format!("{} \"{}\"", item.id, item.title),
        (None, Some(section)) => format!("every finding in the section \"{}\" that has no observed evidence", section.title),
        _ => "every finding that has no observed evidence".into(),
    };
    let guide = format!("the revui guide at {dir} (run `revui skill` first)");
    match (mode, fresh) {
        // A guide for an earlier head is updated into the bundle of the current one, which `revui fetch` already made.
        (Mode::Guide, _) => {
            let into = bundle.successor.as_ref().unwrap_or(&bundle.dir).display().to_string();
            mrs::guide_prompt(url, &into)
        }
        (Mode::Ask, true) => format!("I'm reviewing {url} with {guide}. About {looking_at}: "),
        (Mode::Ask, false) => format!("About {looking_at}: "),
        (Mode::Verify, true) => format!("Verify {target} in {guide}. {WHERE}"),
        (Mode::Verify, false) => format!("Verify {target}. {WHERE}"),
        (Mode::Artifact, true) => format!("Draw a diagram that makes {target} easy to follow, in {guide}. Run nothing: verifying is a separate ask."),
        (Mode::Artifact, false) => format!("Draw a diagram that makes {target} easy to follow. Run nothing: verifying is a separate ask."),
    }
}

type Conversation = (Vec<mrs::Thread>, Option<(String, String)>);

/// What is already said on the merge request, asked of the host and kept beside the bundle for the next open.
pub fn threads(bundle: &Bundle) -> Option<impl FnOnce() -> Conversation + Send + use<>> {
    let source = bundle.review.sources.first().filter(|s| !s.repo.is_empty() && !s.reference.is_empty())?;
    let (repo, reference) = (source.repo.clone(), source.reference.clone());
    let cache = bundle.dir.join("threads.json");
    Some(move || {
        let fetched = std::thread::scope(|scope| {
            let dates = scope.spawn(|| mrs::dates(&repo, &reference));
            (mrs::threads(&repo, &reference), dates.join().ok().flatten())
        });
        if !cfg!(test) {
            let _ = std::fs::write(cache, serde_json::to_string(&fetched).unwrap_or_default());
        }
        fetched
    })
}

pub fn cached_threads(bundle: &Bundle) -> Option<Conversation> {
    let saved = std::fs::read_to_string(bundle.dir.join("threads.json")).ok()?;
    serde_json::from_str(&saved).ok()
}

/// Everything another front end needs to show a review: the guide, the diff, the reviewer's marks and what would go out.
/// The repo's system map and how many of this change's files land in each component. The map's `%% id: folder/, folder/`
/// lines say which folders a component owns; a file counts for the component with the longest folder it sits under.
fn system(bundle: &Bundle) -> Option<serde_json::Value> {
    let text = mrs::system_map(&bundle.review.sources.first().filter(|source| !source.repo.is_empty())?.repo)?;
    let owned = |line: &str| {
        let (id, folders) = line.trim().strip_prefix("%%")?.split_once(':')?;
        Some(folders.split(',').map(|folder| (id.trim().to_string(), folder.trim().to_string())).filter(|(_, folder)| !folder.is_empty()).collect::<Vec<_>>())
    };
    let owns: Vec<(String, String)> = text.lines().filter_map(owned).flatten().collect();
    let mut touched: BTreeMap<&str, usize> = BTreeMap::new();
    for file in &bundle.files {
        if let Some((id, _)) = owns.iter().filter(|(_, folder)| file.path.starts_with(folder.as_str())).max_by_key(|(_, folder)| folder.len()) {
            *touched.entry(id.as_str()).or_default() += 1;
        }
    }
    Some(serde_json::json!({ "text": text, "touched": touched }))
}

pub fn json(bundle: &Bundle) -> String {
    let state = bundle.load_state();
    let review = &bundle.review;
    let quotes: BTreeMap<&str, bool> = review.items.iter().map(|item| (item.id.as_str(), quoted(bundle, &state, &item.id))).collect();
    let source = review.sources.first();
    let links: BTreeMap<&str, String> = review
        .items
        .iter()
        .filter_map(|item| {
            let (at, source) = (item.at.as_ref()?, source?);
            Some((item.id.as_str(), mrs::file_url(&source.url, &source.head_sha, &at.file, at.line)?))
        })
        .collect();
    let session = std::fs::read_to_string(bundle.dir.join("session.json")).ok().and_then(|saved| serde_json::from_str::<serde_json::Value>(&saved).ok());
    let (threads, dates) = cached_threads(bundle).unwrap_or_default();
    let (ok, report) = bundle.check();
    let outgoing = points(bundle, &state).len();
    serde_json::json!({
        "system": system(bundle),
        "verify": crate::html::verify(&bundle.dir),
        "dir": bundle.dir,
        "review": review,
        "files": bundle.files,
        "groups": bundle.groups,
        "state": state,
        "quoted": quotes,
        "links": links,
        "preview": preview(bundle, &state),
        "outgoing": outgoing,
        "finish": FINISH,
        "threads": threads,
        "dates": dates,
        "session": session,
        "product": mrs::product_view(),
        "refs": { "change": bundle.review.sources.first().map(|source| source.url.as_str()).filter(|url| !url.is_empty()), "tickets": mrs::tickets() },
        "claude": mrs::claude(),
        "check": { "ok": ok, "report": report },
    })
    .to_string()
}
