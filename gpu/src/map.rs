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
    // Compiled once: the summary draws its map every frame.
    static READ: std::sync::LazyLock<[Regex; 4]> = std::sync::LazyLock::new(|| [
        Regex::new(r"^([\w-]+)(\[\[(.+?)\]\]|\[\((.+?)\)\]|\(\((.+?)\)\)|\[(.+?)\]|\((.+?)\)|\{(.+?)\})?(?::::([\w-]+))?$").unwrap(),
        Regex::new(r"\s*(?:-\.\s*([^.>]*?)\s*\.->|--\s+([^->|]+?)\s+-->|[-=]{2,}>\s*\|(.*?)\||[-=]{2,}>|-{3,})\s*").unwrap(),
        Regex::new(r"^class\s+([\w,\s-]+?)\s+([\w-]+)$").unwrap(),
        Regex::new(r"^(flowchart|graph|classDef|subgraph|end\b|linkStyle|style|direction|%%)").unwrap(),
    ]);
    let [node, link, class, skip] = &*READ;
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

/// Half the side of a block, and the gaps between two: wider along the way calls run, so names have room.
const HALF: f32 = 40.0;
const APART: egui::Vec2 = vec2(230.0, 210.0);

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
#[derive(Clone, Copy, PartialEq)]
struct Camera {
    turn: f32,
    tilt: f32,
    zoom: f32,
    moved: egui::Vec2,
}

const HOME: Camera = Camera { turn: 0.3, tilt: 0.98, zoom: 1.0, moved: egui::Vec2::ZERO };

/// Where each block stands. Columns go by how far a component sits from the entry points, so calls read left to
/// right; a call that leads back to something already on the way (a webhook, an event) does not count, or a loop
/// would push its members out without end. Within a column, a block sits near what it talks to, so fewer calls
/// cross. With no calls to go by, the blocks stand in a square.
fn stand<'a>(nodes: &'a [Node], edges: &[Edge]) -> BTreeMap<&'a str, egui::Vec2> {
    let index = |id: &str| nodes.iter().position(|node| node.id == id);
    let calls: Vec<(usize, usize)> = edges.iter().filter_map(|edge| Some((index(&edge.from)?, index(&edge.to)?))).filter(|(from, to)| from != to).collect();
    let out = |from: usize| calls.iter().filter(move |call| call.0 == from).map(|call| call.1);
    let (mut seen, mut back) = (vec![0u8; nodes.len()], Vec::new());
    for root in 0..nodes.len() {
        if seen[root] != 0 {
            continue;
        }
        seen[root] = 1;
        let mut walk = vec![(root, out(root).collect::<Vec<_>>())];
        while let Some((from, ahead)) = walk.last_mut() {
            match ahead.pop() {
                Some(to) if seen[to] == 0 => {
                    seen[to] = 1;
                    walk.push((to, out(to).collect()));
                }
                Some(to) if seen[to] == 1 => back.push((*from, to)),
                Some(_) => {}
                None => {
                    seen[*from] = 2;
                    walk.pop();
                }
            }
        }
    }
    let mut depth = vec![0usize; nodes.len()];
    for _ in 0..nodes.len() {
        for &(from, to) in calls.iter().filter(|call| !back.contains(call)) {
            depth[to] = depth[to].max(depth[from] + 1);
        }
    }
    let wide = (nodes.len() as f32).sqrt().ceil().max(1.0) as usize;
    let mut columns: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    (0..nodes.len()).for_each(|n| columns.entry(if calls.is_empty() { n / wide } else { depth[n] }).or_default().push(n));
    let mut columns: Vec<Vec<usize>> = columns.into_values().collect();
    let tallest = columns.iter().map(Vec::len).max().unwrap_or(1) as f32;
    let mut row = vec![0.0; nodes.len()];
    let settle = |column: &[usize], row: &mut Vec<f32>| column.iter().enumerate().for_each(|(n, &node)| row[node] = n as f32 + (tallest - column.len() as f32) / 2.0);
    columns.iter().for_each(|column| settle(column, &mut row));
    for sweep in 0..6 {
        for at in 0..columns.len() {
            let at = if sweep % 2 == 0 { at } else { columns.len() - 1 - at };
            let pull = |node: usize| {
                let near: Vec<f32> = calls.iter().filter_map(|&(from, to)| if from == node { Some(row[to]) } else if to == node { Some(row[from]) } else { None }).collect();
                if near.is_empty() { row[node] } else { near.iter().sum::<f32>() / near.len() as f32 }
            };
            let mut pulled: Vec<(f32, usize)> = columns[at].iter().map(|&node| (pull(node), node)).collect();
            pulled.sort_by(|a, b| a.0.total_cmp(&b.0));
            columns[at] = pulled.into_iter().map(|(_, node)| node).collect();
            settle(&columns[at], &mut row);
        }
    }
    let middle = vec2((columns.len() as f32 - 1.0) / 2.0, (tallest - 1.0) / 2.0);
    columns.iter().enumerate().flat_map(|(column, members)| members.iter().map(move |&node| (column, node))).map(|(column, node)| (nodes[node].id.as_str(), (vec2(column as f32, row[node]) - middle) * APART)).collect()
}

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
/// double click beside the blocks puts it back. What is new, changed or removed is lit. Pointing at a block brings
/// out what it calls and what calls it, and lets the rest fall back. Returns the file of a block that was clicked,
/// for the caller to open.
pub fn draw(ui: &mut egui::Ui, look: &Look, nodes: &[Node], edges: &[Edge], show: Show) -> Option<String> {
    let at = stand(nodes, edges);
    let home: Vec<egui::Vec2> = at.values().map(|spot| HOME.sees(*spot, 0.0).0).collect();
    let span = |pick: fn(&egui::Vec2) -> f32| (home.iter().map(pick).fold(f32::MAX, f32::min), home.iter().map(pick).fold(f32::MIN, f32::max));
    let ((left, right), (top, bottom)) = (span(|spot| spot.x), span(|spot| spot.y));
    // Room around the blocks for their names, and never so tall that the map pushes the rest of the card away.
    let room = vec2(right - left + 250.0, bottom - top + 190.0);
    let fit = (ui.available_width() / room.x).min(560.0 / room.y).min(1.15);
    let (rect, hit) = ui.allocate_exact_size(vec2(ui.available_width(), (room.y * fit).max(240.0)), egui::Sense::click_and_drag());

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
    if let Some(pointer) = hit.hover_pos() {
        // A pinch brings what is under the pointer closer, and keeps it under the pointer.
        let closer = (camera.zoom * ui.input(|input| input.zoom_delta())).clamp(0.4, 4.0);
        camera.moved += (pointer - rect.center() - camera.moved) * (1.0 - closer / camera.zoom);
        camera.zoom = closer;
    }

    let mut kept = camera;
    let painter = ui.painter_at(rect);
    let scale = fit * camera.zoom;
    let centre = rect.center() + vec2(0.0, 24.0 * fit) + camera.moved - vec2(left + right, top + bottom) / 2.0 * scale;
    let place = |spot: egui::Vec2, up: f32| centre + camera.sees(spot, up).0 * scale;
    let lit = |node: &Node| match node.state {
        Some("changed") => Some(Color32::from_rgb(217, 162, 63)),
        Some("added") => Some(Color32::from_rgb(63, 168, 100)),
        Some("removed") => Some(Color32::from_rgb(208, 85, 92)),
        _ => None,
    };
    let over = hit.hover_pos().filter(|_| !hit.dragged()).and_then(|pointer| {
        let near = |node: &&Node| (place(at[node.id.as_str()], rise(node) / 2.0) - pointer).length();
        nodes.iter().min_by(|a, b| near(a).total_cmp(&near(b))).filter(|node| near(node) < 46.0 * scale)
    });
    if hit.double_clicked() && over.is_none() {
        kept = HOME;
    }
    let opened = over.filter(|_| hit.clicked()).and_then(|node| node.opens.clone());
    let held = if hit.dragged() { egui::CursorIcon::Grabbing } else if over.is_some_and(|node| node.opens.is_some()) { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Grab };
    let hit = hit.on_hover_cursor(held);
    // With a block pointed at, it and what it talks to stay as they are, and everything else falls back.
    let joins = |edge: &Edge, id: &str| edge.from == id || edge.to == id;
    let near = |id: &str| over.is_none_or(|over| over.id == id || edges.iter().any(|edge| joins(edge, &over.id) && joins(edge, id)));
    let fade = |color: Color32, on: bool| if on { color } else { color.gamma_multiply(0.22) };
    let (sin, cos) = camera.turn.sin_cos();
    let squash = camera.tilt.sin();
    let mut order: Vec<&Node> = nodes.iter().collect();
    order.sort_by(|a, b| camera.sees(at[a.id.as_str()], 0.0).1.total_cmp(&camera.sees(at[b.id.as_str()], 0.0).1));
    for node in &order {
        let (spot, up, on) = (at[node.id.as_str()], rise(node), near(&node.id));
        let (face, flank, line) = match lit(node) {
            Some(color) => (mix(color, look.raised, 0.62), mix(color, look.raised, 0.38), color),
            None => (look.top, look.side, look.edge.gamma_multiply(0.7)),
        };
        let (face, flank) = (fade(face, on), fade(flank, on));
        let stroke = Stroke::new(if over.is_some_and(|over| over.id == node.id) { 2.0 } else { 1.0 }, fade(line, on));
        // A soft patch of ground under each block, so it stands on something.
        let foot = [vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)].map(|corner| place(spot + corner * (HALF + 9.0), 0.0));
        painter.add(Shape::convex_polygon(foot.to_vec(), Color32::from_black_alpha(if on { 46 } else { 12 }), Stroke::NONE));
        if node.form == Form::Store {
            let (ground, top) = (place(spot, 0.0), place(spot, up));
            let (rx, ry) = (HALF * 1.05 * scale, HALF * 1.05 * scale * squash);
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
    // A call is an arc from one block's top to the next, lifted off the ground so it passes over what stands
    // between them. They stay quiet until a block is pointed at: then its own are drawn full, with what they carry.
    let found = |id: &str| nodes.iter().find(|node| node.id == id).map(|node| (at[id], rise(node)));
    let mut said = Vec::new();
    for edge in edges.iter().filter(|_| show.calls) {
        let (Some((from, up)), Some((to, down))) = (found(&edge.from), found(&edge.to)) else { continue };
        let way = to - from;
        let clear = way.normalized() * (HALF * 0.9).min(way.length() * 0.45);
        let lift = (way.length() * 0.16).min(70.0);
        let arc: Vec<egui::Pos2> = (0..=18).map(|n| n as f32 / 18.0).map(|t| place(from + clear + (way - clear * 2.0) * t, up + (down - up) * t + lift * 4.0 * t * (1.0 - t))).collect();
        let own = over.is_some_and(|over| joins(edge, &over.id));
        let ink = if edge.new { Color32::from_rgb(63, 168, 100) } else if own { look.text } else { look.edge };
        // Unasked, the calls to and from what the change touches read a little stronger than the rest.
        let touched = [&edge.from, &edge.to].iter().any(|id| nodes.iter().any(|node| node.id == **id && lit(node).is_some()));
        let ink = ink.gamma_multiply(if own { 1.0 } else if over.is_some() { 0.08 } else if edge.new { 0.9 } else if touched { 0.5 } else { 0.24 });
        let (end, before) = (arc[18], arc[17]);
        let (along, across) = ((end - before).normalized() * 8.0, (end - before).normalized().rot90() * 3.6);
        painter.add(Shape::line(arc.clone(), Stroke::new(if own { 1.8 } else { 1.2 }, ink)));
        painter.add(Shape::convex_polygon(vec![end, end - along + across, end - along - across], ink, Stroke::NONE));
        if !edge.label.is_empty() && (own || (edge.new && over.is_none())) {
            said.push((arc[9] - vec2(0.0, 5.0), edge.label.as_str(), ink));
        }
    }
    // Names go on last, the ones that matter first, and one that would sit on another is left for a closer look.
    let mut named: Vec<&Node> = nodes.iter().filter(|node| show.labels || lit(node).is_some() || over.is_some_and(|over| over.id == node.id)).collect();
    named.sort_by_key(|node| (over.is_none_or(|over| over.id != node.id), !near(&node.id), lit(node).is_none()));
    let mut taken: Vec<egui::Rect> = Vec::new();
    for node in named {
        let (on, pointed) = (near(&node.id), over.is_some_and(|over| over.id == node.id));
        let ink = if pointed || lit(node).is_some() { look.text } else { look.edge };
        let name = painter.layout(node.label.clone(), FontId::new(12.5, egui::FontFamily::Name(if lit(node).is_some() || pointed { "bold" } else { "medium" }.into())), fade(ink, on), (APART.x * scale - 18.0).clamp(84.0, 150.0));
        let foot = place(at[node.id.as_str()], rise(node)) - vec2(0.0, HALF * 1.42 * squash * scale + 7.0);
        let spot = egui::Rect::from_min_size(pos2(foot.x - name.size().x / 2.0, foot.y - name.size().y), name.size());
        if !pointed && taken.iter().any(|other| other.intersects(spot.expand(2.0))) {
            continue;
        }
        taken.push(spot);
        painter.rect_filled(spot.expand2(vec2(4.0, 1.0)), 4.0, ui.visuals().panel_fill.gamma_multiply(0.72));
        painter.galley(spot.min, name, ink);
    }
    for (spot, label, ink) in said {
        let text = painter.layout_no_wrap(label.to_string(), FontId::monospace(11.0), ink);
        let back = egui::Rect::from_center_size(spot - vec2(0.0, text.size().y / 2.0), text.size() + vec2(10.0, 4.0));
        painter.rect_filled(back, 4.0, look.raised);
        painter.galley(back.min + vec2(5.0, 2.0), text, ink);
    }
    if hit.hovered() {
        painter.text(rect.left_bottom() + vec2(2.0, 0.0), Align2::LEFT_BOTTOM, "drag to turn · shift-drag to move · pinch to zoom · double-click to reset", FontId::monospace(10.5), look.edge.gamma_multiply(0.7));
    }
    if kept != HOME {
        let corner = egui::Rect::from_min_max(rect.right_bottom() - vec2(92.0, 26.0), rect.right_bottom());
        if ui.put(corner, egui::Button::new(egui::RichText::new("Reset view").size(12.0).color(look.edge)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            kept = HOME;
        }
    }
    ui.data_mut(|data| data.insert_temp(id, kept));
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
    fn a_loop_of_calls_does_not_push_its_members_out() {
        let (nodes, edges) = parse("flowchart LR\n  web --> api\n  api --> pmc\n  pmc --> events\n  events --> pmc\n  stripe --> api\n  api --> stripe\n  pmc --> google");
        let at = stand(&nodes, &edges);
        let column = |id: &str| ((at[id].x - at["web"].x) / APART.x).round() as i32;
        assert_eq!([column("web"), column("api"), column("pmc"), column("stripe"), column("events"), column("google")], [0, 1, 2, 2, 3, 3]);
        let mut spots: Vec<(i32, i32)> = at.values().map(|spot| (spot.x.round() as i32, spot.y.round() as i32)).collect();
        spots.sort();
        spots.dedup();
        assert_eq!(spots.len(), nodes.len(), "no two blocks stand on one spot");
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
