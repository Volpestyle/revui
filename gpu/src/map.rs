use eframe::egui::{self, Align2, Color32, FontId, Shape, Stroke, pos2, vec2};
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Form {
    Block,
    Store,
    Queue,
}

#[derive(Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub form: Form,
    pub state: Option<&'static str>,
    /// How tall it stands, or 0 for the height of its form.
    pub tall: f32,
    /// What hovering it adds to its name.
    pub note: String,
    /// The changed file a click on it opens.
    pub opens: Option<String>,
}

pub struct Edge {
    from: String,
    to: String,
    label: String,
    new: bool,
}

fn state(name: &str) -> Option<&'static str> {
    match name {
        "changed" => Some("changed"),
        "added" | "new" => Some("added"),
        "removed" => Some("removed"),
        _ => None,
    }
}

/// Reads the flowchart the guide writes for the map: nodes, who calls whom, and what the change does to each.
pub fn parse(text: &str) -> (Vec<Node>, Vec<Edge>) {
    let node = Regex::new(r"^([\w-]+)(\[\[(.+?)\]\]|\[\((.+?)\)\]|\(\((.+?)\)\)|\[(.+?)\]|\((.+?)\)|\{(.+?)\})?(?::::([\w-]+))?$").unwrap();
    let link = Regex::new(r"\s*(?:-\.\s*([^.>]*?)\s*\.->|--\s+([^->|]+?)\s+-->|[-=]{2,}>\s*\|(.*?)\||[-=]{2,}>|-{3,})\s*").unwrap();
    let class = Regex::new(r"^class\s+([\w,\s-]+?)\s+([\w-]+)$").unwrap();
    let skip = Regex::new(r"^(flowchart|graph|classDef|subgraph|end\b|linkStyle|style|direction|%%)").unwrap();
    let (mut nodes, mut edges): (Vec<Node>, Vec<Edge>) = (Vec::new(), Vec::new());
    let mut touch = |nodes: &mut Vec<Node>, raw: &str| -> Option<String> {
        let found = node.captures(raw.trim())?;
        let id = found[1].to_string();
        let label = [3, 4, 5, 6, 7, 8].iter().find_map(|&n| found.get(n)).map(|label| label.as_str().trim_matches('"').replace("<br>", " ").replace("<br/>", " "));
        let form = if found.get(3).is_some() { Form::Queue } else if found.get(4).is_some() { Form::Store } else { Form::Block };
        let marked = found.get(9).and_then(|class| state(class.as_str()));
        match nodes.iter_mut().find(|known| known.id == id) {
            Some(known) => {
                if let Some(label) = label {
                    (known.label, known.form) = (label, form);
                }
                known.state = marked.or(known.state);
            }
            None => nodes.push(Node { label: label.unwrap_or_else(|| id.clone()), id: id.clone(), form, state: marked, tall: 0.0, note: String::new(), opens: None }),
        }
        Some(id)
    };
    for line in text.lines().map(|line| line.trim().trim_end_matches(';')) {
        if line.is_empty() || skip.is_match(line) {
            continue;
        }
        if let Some(classed) = class.captures(line) {
            for id in classed[1].split(',').map(str::trim) {
                touch(&mut nodes, id);
                if let Some(known) = nodes.iter_mut().find(|known| known.id == id) {
                    known.state = state(&classed[2]);
                }
            }
            continue;
        }
        let links: Vec<_> = link.captures_iter(line).collect();
        let mut ends = Vec::new();
        let mut from = 0;
        for found in &links {
            let whole = found.get(0).unwrap();
            ends.push(&line[from..whole.start()]);
            from = whole.end();
        }
        ends.push(&line[from..]);
        let groups: Vec<Vec<String>> = ends.iter().map(|group| group.split('&').filter_map(|raw| touch(&mut nodes, raw)).collect()).collect();
        for (n, found) in links.iter().enumerate() {
            let label = [1, 2, 3].iter().find_map(|&n| found.get(n)).map_or("", |label| label.as_str()).trim().to_string();
            for from in &groups[n] {
                for to in &groups[n + 1] {
                    edges.push(Edge { from: from.clone(), to: to.clone(), label: label.clone(), new: found[0].contains('.') });
                }
            }
        }
    }
    (nodes, edges)
}

/// Half the side of a block and the gap between two, on the ground the map stands on.
const HALF: f32 = 44.0;
const APART: f32 = 181.0;

fn rise(node: &Node) -> f32 {
    if node.tall > 0.0 { node.tall } else if node.form == Form::Store { 35.0 } else { 21.0 }
}

fn mix(a: Color32, b: Color32, share: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 * share + y as f32 * (1.0 - share)) as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

pub struct Look {
    pub raised: Color32,
    pub top: Color32,
    pub side: Color32,
    pub edge: Color32,
    pub text: Color32,
}

/// What of the map is drawn besides its blocks.
#[derive(Clone, Copy)]
pub struct Show {
    pub calls: bool,
    pub labels: bool,
}

/// Where the map is seen from: turned about its middle, tilted, moved and brought closer by the pointer.
#[derive(Clone, Copy)]
struct Camera {
    turn: f32,
    tilt: f32,
    zoom: f32,
    moved: egui::Vec2,
}

const HOME: Camera = Camera { turn: std::f32::consts::FRAC_PI_4, tilt: std::f32::consts::FRAC_PI_6, zoom: 1.0, moved: egui::Vec2::ZERO };

impl Camera {
    /// A point of the ground, `up` above it, as the eye sees it: across, down the screen, and how near.
    fn sees(&self, at: egui::Vec2, up: f32) -> (egui::Vec2, f32) {
        let (sin, cos) = self.turn.sin_cos();
        let near = at.x * sin + at.y * cos;
        (vec2(at.x * cos - at.y * sin, near * self.tilt.sin() - up * self.tilt.cos()), near)
    }
}

/// The repo as blocks: a folder each, as tall as it is full, the ones this change touches lit. `tree` is what
/// `revui tree` lists, folders two deep; one deep when that is too many to read. Returns how many were left out.
pub fn folders(tree: &BTreeMap<String, usize>, changed: &[&str]) -> (Vec<Node>, usize) {
    let deep = tree.len() <= 36;
    let name = |path: &str, file: bool| {
        let parts: Vec<&str> = path.split('/').collect();
        let folders = parts.len() - file as usize;
        if folders == 0 { ".".to_string() } else { parts[..folders.min(if deep { 2 } else { 1 })].join("/") }
    };
    let mut holds: BTreeMap<String, usize> = BTreeMap::new();
    tree.iter().for_each(|(folder, files)| *holds.entry(name(folder, folder == ".")).or_default() += files);
    let mut touched: BTreeMap<String, usize> = BTreeMap::new();
    changed.iter().for_each(|path| *touched.entry(name(path, true)).or_default() += 1);
    let first = |folder: &String| changed.iter().find(|path| name(path, true) == *folder).map(|path| path.to_string());
    for folder in touched.keys() {
        holds.entry(folder.clone()).or_insert(1);
    }
    let mut kept: Vec<&String> = holds.keys().collect();
    kept.sort_by_key(|folder| (!touched.contains_key(*folder), std::cmp::Reverse(holds[*folder])));
    let more = kept.len().saturating_sub(36);
    kept.truncate(36);
    kept.sort();
    let block = |folder: &String| {
        let here = touched.get(folder).map_or(String::new(), |files| format!(", {files} changed here"));
        Node { id: folder.clone(), label: if folder == "." { "top level".into() } else { folder.clone() }, form: Form::Block, state: touched.contains_key(folder).then_some("changed"), tall: 10.0 + 9.0 * (1.0 + holds[folder] as f32).ln(), note: format!("{} files{here}", holds[folder]), opens: first(folder) }
    };
    (kept.into_iter().map(block).collect(), more)
}

/// The map as blocks you can turn: drag turns and tilts it, shift-drag moves it, a pinch brings it closer and a
/// double click beside the blocks puts it back. What is new, changed or removed is lit. Returns the file of a
/// block that was clicked, for the caller to open. Columns go by how far a component sits from
/// the entry points, so calls read left to right; with no calls to go by, the blocks stand in a square.
pub fn draw(ui: &mut egui::Ui, look: &Look, nodes: &[Node], edges: &[Edge], show: Show) -> Option<String> {
    let mut depth: BTreeMap<&str, usize> = nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    for _ in 0..nodes.len() {
        for edge in edges.iter().filter(|edge| edge.from != edge.to) {
            let (from, to) = (depth[edge.from.as_str()], depth[edge.to.as_str()]);
            if to <= from && from < nodes.len() {
                depth.insert(edge.to.as_str(), from + 1);
            }
        }
    }
    let mut columns: BTreeMap<usize, Vec<&Node>> = BTreeMap::new();
    let wide = (nodes.len() as f32).sqrt().ceil() as usize;
    for (n, node) in nodes.iter().enumerate() {
        columns.entry(if edges.is_empty() { n / wide.max(1) } else { depth[node.id.as_str()] }).or_default().push(node);
    }
    let tallest = columns.values().map(Vec::len).max().unwrap_or(1) as f32;
    let mut at: BTreeMap<&str, egui::Vec2> = BTreeMap::new();
    for (&column, members) in &columns {
        for (n, node) in members.iter().enumerate() {
            at.insert(node.id.as_str(), vec2(column as f32, n as f32 + (tallest - members.len() as f32) / 2.0) * APART);
        }
    }
    let middle = vec2((columns.len() as f32 - 1.0) / 2.0, (tallest - 1.0) / 2.0) * APART;
    at.values_mut().for_each(|spot| *spot -= middle);

    let home: Vec<egui::Vec2> = at.values().map(|spot| HOME.sees(*spot, 0.0).0).collect();
    let span = |pick: fn(&egui::Vec2) -> f32| (home.iter().map(pick).fold(f32::MAX, f32::min), home.iter().map(pick).fold(f32::MIN, f32::max));
    let ((left, right), (top, bottom)) = (span(|spot| spot.x), span(|spot| spot.y));
    let fit = (ui.available_width() / (right - left + 220.0)).min(1.0);
    let (rect, hit) = ui.allocate_exact_size(vec2(ui.available_width(), (bottom - top + 150.0) * fit), egui::Sense::click_and_drag());

    let id = egui::Id::new("map camera");
    let mut camera = ui.data(|data| data.get_temp::<Camera>(id)).unwrap_or(HOME);
    if hit.dragged() {
        let by = hit.drag_delta();
        if ui.input(|input| input.modifiers.shift) || hit.dragged_by(egui::PointerButton::Secondary) {
            camera.moved += by;
        } else {
            (camera.turn, camera.tilt) = (camera.turn - by.x * 0.008, (camera.tilt + by.y * 0.005).clamp(0.2, 1.35));
        }
    }
    if hit.hovered() {
        camera.zoom = (camera.zoom * ui.input(|input| input.zoom_delta())).clamp(0.4, 4.0);
    }

    let painter = ui.painter_at(rect);
    let scale = fit * camera.zoom;
    let centre = rect.center() + vec2(0.0, 21.0 * fit) + camera.moved - vec2(left + right, top + bottom) / 2.0 * scale;
    let place = |spot: egui::Vec2, up: f32| centre + camera.sees(spot, up).0 * scale;
    let lit = |node: &Node| match node.state {
        Some("changed") => Some(Color32::from_rgb(217, 162, 63)),
        Some("added") => Some(Color32::from_rgb(63, 168, 100)),
        Some("removed") => Some(Color32::from_rgb(208, 85, 92)),
        _ => None,
    };
    let over = hit.hover_pos().and_then(|pointer| {
        let near = |node: &&Node| (place(at[node.id.as_str()], rise(node) / 2.0) - pointer).length();
        nodes.iter().min_by(|a, b| near(a).total_cmp(&near(b))).filter(|node| near(node) < 50.0 * scale)
    });
    let kept = if hit.double_clicked() && over.is_none() { HOME } else { camera };
    ui.data_mut(|data| data.insert_temp(id, kept));
    let opened = over.filter(|_| hit.clicked()).and_then(|node| node.opens.clone());
    let held = if hit.dragged() { egui::CursorIcon::Grabbing } else if over.is_some_and(|node| node.opens.is_some()) { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Grab };
    let hit = hit.on_hover_cursor(held);
    let (sin, cos) = camera.turn.sin_cos();
    let squash = camera.tilt.sin();
    let mut order: Vec<&Node> = nodes.iter().collect();
    order.sort_by(|a, b| camera.sees(at[a.id.as_str()], 0.0).1.total_cmp(&camera.sees(at[b.id.as_str()], 0.0).1));
    for node in &order {
        let (spot, up) = (at[node.id.as_str()], rise(node));
        let (face, flank, line) = match lit(node) {
            Some(color) => (mix(color, look.raised, 0.62), mix(color, look.raised, 0.38), color),
            None => (look.top, look.side, look.edge),
        };
        let stroke = Stroke::new(if over.is_some_and(|over| over.id == node.id) { 2.0 } else { 1.0 }, line);
        if node.form == Form::Store {
            let (ground, top) = (place(spot, 0.0), place(spot, up));
            let (rx, ry) = (46.0 * scale, 46.0 * scale * squash);
            painter.add(Shape::ellipse_filled(ground, vec2(rx, ry), flank));
            painter.add(Shape::ellipse_stroke(ground, vec2(rx, ry), stroke));
            painter.rect_filled(egui::Rect::from_min_max(pos2(top.x - rx, top.y), pos2(top.x + rx, ground.y)), 0.0, flank);
            painter.line_segment([pos2(top.x - rx, top.y), pos2(top.x - rx, ground.y)], stroke);
            painter.line_segment([pos2(top.x + rx, top.y), pos2(top.x + rx, ground.y)], stroke);
            painter.add(Shape::ellipse_filled(top, vec2(rx, ry), face));
            painter.add(Shape::ellipse_stroke(top, vec2(rx, ry), stroke));
            continue;
        }
        let corners = [vec2(-HALF, -HALF), vec2(HALF, -HALF), vec2(HALF, HALF), vec2(-HALF, HALF)].map(|corner| spot + corner);
        for (n, out) in [vec2(0.0, -1.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(-1.0, 0.0)].into_iter().enumerate() {
            // A side seen edge-on is a sliver, and its outline would spike: leave it out until it has some width.
            if out.x * sin + out.y * cos <= 0.03 {
                continue;
            }
            let (a, b) = (corners[n], corners[(n + 1) % 4]);
            let shade = if out.x * cos - out.y * sin < 0.0 { mix(flank, Color32::BLACK, 0.86) } else { flank };
            painter.add(Shape::convex_polygon(vec![place(a, up), place(b, up), place(b, 0.0), place(a, 0.0)], shade, stroke));
            if node.form == Form::Queue {
                painter.line_segment([place(a, up / 2.0), place(b, up / 2.0)], stroke);
            }
        }
        painter.add(Shape::convex_polygon(corners.map(|corner| place(corner, up)).to_vec(), face, stroke));
    }
    let found = |id: &str| nodes.iter().find(|node| node.id == id).map(|node| (at[id], rise(node)));
    for edge in edges.iter().filter(|_| show.calls) {
        let (Some((from, up)), Some((to, down))) = (found(&edge.from), found(&edge.to)) else { continue };
        let way = to - from;
        let clear = way.normalized() * (HALF * 1.3).min(way.length() * 0.45);
        let (start, end) = (place(from + clear, up), place(to - clear, down));
        let stroke = Stroke::new(1.5, if edge.new { Color32::from_rgb(63, 168, 100) } else { look.edge });
        painter.line_segment([start, end], stroke);
        let (along, across) = ((end - start).normalized() * 9.0, (end - start).normalized().rot90() * 4.5);
        painter.add(Shape::convex_polygon(vec![end, end - along + across, end - along - across], stroke.color, Stroke::NONE));
        if !edge.label.is_empty() && show.labels {
            painter.text(start + (end - start) / 2.0 - vec2(0.0, 6.0), Align2::CENTER_BOTTOM, &edge.label, FontId::monospace(11.0), look.edge);
        }
    }
    for node in nodes.iter().filter(|node| show.labels || lit(node).is_some() || over.is_some_and(|over| over.id == node.id)) {
        let spot = place(at[node.id.as_str()], rise(node)) - vec2(0.0, HALF * 1.42 * squash * scale + 9.0);
        painter.text(spot, Align2::CENTER_BOTTOM, &node.label, FontId::new(13.0, egui::FontFamily::Name("bold".into())), look.text);
    }
    if hit.hovered() {
        painter.text(rect.right_bottom() - vec2(2.0, 0.0), Align2::RIGHT_BOTTOM, "drag to turn · shift-drag to move · pinch to zoom · double-click to reset", FontId::monospace(10.5), look.edge);
    }
    if let Some(node) = over {
        let does = node.opens.as_ref().map_or(String::new(), |file| format!("\nClick to open {file}"));
        hit.on_hover_text_at_pointer(if node.note.is_empty() { format!("{}{does}", node.label) } else { format!("{} · {}{does}", node.label, node.note) });
    }
    opened
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_flowchart_the_guide_writes() {
        let (nodes, edges) = parse("flowchart LR\n  web[Customer web] --> api[Orders API]\n  api --> db[(Orders table)]\n  api -. new .-> queue[[Retry queue]]\n  classDef changed stroke:#e8a33d\n  class api changed\n  class queue added\n%% api: src/");
        let found = |id: &str| nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(nodes.len(), 4);
        assert_eq!((found("api").label.as_str(), found("api").state), ("Orders API", Some("changed")));
        assert!(found("db").form == Form::Store && found("queue").form == Form::Queue && found("queue").state == Some("added"));
        assert_eq!(edges.len(), 3);
        assert!(edges[2].new && edges[2].label == "new" && edges[2].to == "queue");
    }

    #[test]
    fn a_repo_becomes_blocks_with_the_touched_folders_lit() {
        let tree: BTreeMap<String, usize> = [(".", 3), ("src", 2), ("src/ui", 40), ("docs/guide", 9)].into_iter().map(|(folder, files)| (folder.to_string(), files)).collect();
        let (nodes, more) = folders(&tree, &["src/ui/button.tsx", "src/ui/deep/card.tsx", "README.md", "tools/new/run.sh"]);
        let found = |id: &str| nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!((nodes.len(), more), (5, 0));
        assert_eq!((found("src/ui").state, found("src/ui").note.as_str()), (Some("changed"), "40 files, 2 changed here"));
        assert!(found(".").state.is_some() && found("tools/new").state.is_some() && found("src").state.is_none() && found("docs/guide").state.is_none());
        assert!(found("src/ui").tall > found("src").tall);
        assert_eq!((found("src/ui").opens.as_deref(), found("src").opens.as_deref()), (Some("src/ui/button.tsx"), None));
    }
}
