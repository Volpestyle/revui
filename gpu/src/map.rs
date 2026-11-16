use eframe::egui::{self, Align2, Color32, FontId, Pos2, Shape, Stroke, pos2, vec2};
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
            None => nodes.push(Node { label: label.unwrap_or_else(|| id.clone()), id: id.clone(), form, state: marked }),
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

const A: f32 = 62.0;
const B: f32 = 31.0;

fn rise(node: &Node) -> f32 {
    if node.form == Form::Store { 30.0 } else { 18.0 }
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

/// The map as tilted blocks, with what is new, changed or removed lit. Columns go by how far a component sits from the entry points, so calls read left to right.
pub fn draw(ui: &mut egui::Ui, look: &Look, nodes: &[Node], edges: &[Edge]) {
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
    for node in nodes {
        columns.entry(depth[node.id.as_str()]).or_default().push(node);
    }
    let tallest = columns.values().map(Vec::len).max().unwrap_or(1) as f32;
    let mut at: BTreeMap<&str, (Pos2, f32)> = BTreeMap::new();
    for (&column, members) in &columns {
        for (n, node) in members.iter().enumerate() {
            let row = n as f32 + (tallest - members.len() as f32) / 2.0;
            at.insert(node.id.as_str(), (pos2((column as f32 - row) * 128.0, (column as f32 + row) * 66.0), column as f32 + row));
        }
    }
    let spots = || at.values().map(|(spot, _)| *spot);
    let min = pos2(spots().map(|p| p.x).fold(f32::MAX, f32::min) - 110.0, spots().map(|p| p.y).fold(f32::MAX, f32::min) - 96.0);
    let max = pos2(spots().map(|p| p.x).fold(f32::MIN, f32::max) + 110.0, spots().map(|p| p.y).fold(f32::MIN, f32::max) + 54.0);
    let scale = (ui.available_width() / (max.x - min.x)).min(1.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), (max.y - min.y) * scale), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let origin = rect.left_top() + vec2((rect.width() - (max.x - min.x) * scale) / 2.0, 0.0);
    let place = |spot: Pos2| origin + (spot - min) * scale;
    let lit = |node: &Node| match node.state {
        Some("changed") => Some(Color32::from_rgb(217, 162, 63)),
        Some("added") => Some(Color32::from_rgb(63, 168, 100)),
        Some("removed") => Some(Color32::from_rgb(208, 85, 92)),
        _ => None,
    };
    let mut order: Vec<&Node> = nodes.iter().collect();
    order.sort_by(|a, b| at[a.id.as_str()].1.total_cmp(&at[b.id.as_str()].1));
    for node in &order {
        let ground = place(at[node.id.as_str()].0);
        let (a, b, up) = (A * scale, B * scale, rise(node) * scale);
        let top = ground - vec2(0.0, up);
        let color = lit(node);
        let (face, flank, line) = match color {
            Some(color) => (mix(color, look.raised, 0.62), mix(color, look.raised, 0.38), color),
            None => (look.top, look.side, look.edge),
        };
        let stroke = Stroke::new(1.0, line);
        match node.form {
            Form::Store => {
                let (rx, ry) = (46.0 * scale, 23.0 * scale);
                painter.add(Shape::ellipse_filled(ground, vec2(rx, ry), flank));
                painter.add(Shape::ellipse_stroke(ground, vec2(rx, ry), stroke));
                painter.rect_filled(egui::Rect::from_min_max(pos2(top.x - rx, top.y), pos2(top.x + rx, ground.y)), 0.0, flank);
                painter.line_segment([pos2(top.x - rx, top.y), pos2(top.x - rx, ground.y)], stroke);
                painter.line_segment([pos2(top.x + rx, top.y), pos2(top.x + rx, ground.y)], stroke);
                painter.add(Shape::ellipse_filled(top, vec2(rx, ry), face));
                painter.add(Shape::ellipse_stroke(top, vec2(rx, ry), stroke));
            }
            _ => {
                painter.add(Shape::convex_polygon(vec![top - vec2(a, 0.0), top + vec2(0.0, b), ground + vec2(0.0, b), ground - vec2(a, 0.0)], mix(flank, Color32::BLACK, 0.86), stroke));
                painter.add(Shape::convex_polygon(vec![top + vec2(0.0, b), top + vec2(a, 0.0), ground + vec2(a, 0.0), ground + vec2(0.0, b)], flank, stroke));
                painter.add(Shape::convex_polygon(vec![top - vec2(0.0, b), top + vec2(a, 0.0), top + vec2(0.0, b), top - vec2(a, 0.0)], face, stroke));
                if node.form == Form::Queue {
                    let mid = vec2(0.0, up / 2.0);
                    painter.line([top - vec2(a, 0.0) + mid, top + vec2(0.0, b) + mid, top + vec2(a, 0.0) + mid].to_vec(), stroke);
                }
            }
        }
    }
    let high = |id: &str| nodes.iter().find(|node| node.id == id).map(|node| place(at[id].0) - vec2(0.0, rise(node) * scale));
    for edge in edges {
        let (Some(from), Some(to)) = (high(&edge.from), high(&edge.to)) else { continue };
        let way = to - from;
        let reach = (1.06 / ((way.x.abs() / (A * scale)) + (way.y.abs() / (B * scale))).max(0.001)).min(0.45);
        let (start, end) = (from + way * reach, to - way * reach);
        let stroke = Stroke::new(1.5, if edge.new { Color32::from_rgb(63, 168, 100) } else { look.edge });
        painter.line_segment([start, end], stroke);
        let (along, across) = (way.normalized() * 9.0, way.normalized().rot90() * 4.5);
        painter.add(Shape::convex_polygon(vec![end, end - along + across, end - along - across], stroke.color, Stroke::NONE));
        if !edge.label.is_empty() {
            painter.text(from + way / 2.0 - vec2(0.0, 6.0), Align2::CENTER_BOTTOM, &edge.label, FontId::monospace(11.0), look.edge);
        }
    }
    for node in nodes {
        let spot = place(at[node.id.as_str()].0) - vec2(0.0, (rise(node) + B + 9.0) * scale);
        painter.text(spot, Align2::CENTER_BOTTOM, &node.label, FontId::new(13.0, egui::FontFamily::Name("bold".into())), look.text);
    }
}
