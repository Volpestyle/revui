// The effects the interface draws itself: one triangle over a rectangle of the page, and a fragment shader for each effect.

struct Slot {
    // Left, top, width and height, in points.
    @location(0) rect: vec4f,
    // The pointer inside the rectangle, then pixels per point, then seconds or how far along the effect is.
    @location(1) at: vec4f,
    // Premultiplied, as egui draws it.
    @location(2) ink: vec4f,
    @location(3) more: vec4f,
}

struct Drawn {
    @builtin(position) pixel: vec4f,
    @location(0) @interpolate(flat) rect: vec4f,
    @location(1) @interpolate(flat) at: vec4f,
    @location(2) @interpolate(flat) ink: vec4f,
    @location(3) @interpolate(flat) more: vec4f,
}

@vertex
fn vs(@builtin(vertex_index) n: u32, slot: Slot) -> Drawn {
    let corner = vec2f(f32((n << 1u) & 2u), f32(n & 2u));
    return Drawn(vec4f(corner * 2.0 - 1.0, 0.0, 1.0), slot.rect, slot.at, slot.ink, slot.more);
}

fn within(pixel: vec4f, rect: vec4f, at: vec4f) -> vec2f {
    return pixel.xy / at.z - rect.xy;
}

// A ring that spreads from where the pointer pressed and fades as it reaches the far corner.
@fragment
fn ripple(d: Drawn) -> @location(0) vec4f {
    let t = d.at.w;
    let far = length(max(d.at.xy, d.rect.zw - d.at.xy));
    let reach = far * (1.0 - pow(1.0 - t, 3.0));
    let away = distance(within(d.pixel, d.rect, d.at), d.at.xy);
    let body = 1.0 - smoothstep(reach - 10.0, reach, away);
    let lead = body * smoothstep(reach - 28.0, reach - 6.0, away);
    return d.ink * ((body * 0.45 + lead * 0.55) * (1.0 - t) * (1.0 - t) * d.more.x);
}

// A soft light that follows the pointer over what it can press.
@fragment
fn glow(d: Drawn) -> @location(0) vec4f {
    let away = within(d.pixel, d.rect, d.at) - d.at.xy;
    let near = exp(-dot(away, away) / (d.more.y * d.more.y));
    return d.ink * (d.more.x * (0.35 + 0.65 * near));
}

// A band of light that travels along a thin bar while the agent works.
@fragment
fn shimmer(d: Drawn) -> @location(0) vec4f {
    let x = within(d.pixel, d.rect, d.at).x / d.rect.z;
    let wave = 0.5 + 0.5 * sin((x * 1.5 - d.at.w * 0.9) * 6.2832);
    return d.ink * (0.12 + 0.88 * wave * wave * wave);
}
