//! What drawing on the GPU buys: small effects with a fragment shader of their own (`fx.wgsl`), laid over or under
//! what egui draws. A sheen and a ripple on what the pointer can press, drifting contours in the summary's slab,
//! a band of light while the agent works, and a card that eases in when you turn to it.

use eframe::egui::{self, Color32, Pos2, Rect};
use eframe::egui_wgpu::{self, wgpu};
use std::sync::atomic::{AtomicU32, Ordering};

/// Sixteen numbers an effect: the `Slot` of `fx.wgsl`.
const SIZE: u64 = 64;
// ponytail: effects past this many in one frame are not drawn; grow the buffer if a screen ever shows more.
const ROOM: u32 = 256;
static NEXT: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
enum Kind {
    Ripple,
    Glow,
    Flow,
    Shimmer,
}

struct Gpu {
    pipelines: [wgpu::RenderPipeline; 4],
    slots: wgpu::Buffer,
}

struct Draw {
    kind: Kind,
    slot: u32,
    rect: Rect,
    at: egui::Vec2,
    when: f32,
    ink: [f32; 4],
    more: [f32; 2],
}

impl egui_wgpu::CallbackTrait for Draw {
    fn prepare(&self, _device: &wgpu::Device, queue: &wgpu::Queue, screen: &egui_wgpu::ScreenDescriptor, _encoder: &mut wgpu::CommandEncoder, resources: &mut egui_wgpu::CallbackResources) -> Vec<wgpu::CommandBuffer> {
        if let Some(gpu) = resources.get::<Gpu>() {
            let [r, g, b, a] = self.ink;
            let numbers = [self.rect.left(), self.rect.top(), self.rect.width(), self.rect.height(), self.at.x, self.at.y, screen.pixels_per_point, self.when, r, g, b, a, self.more[0], self.more[1], 0.0, 0.0];
            queue.write_buffer(&gpu.slots, self.slot as u64 * SIZE, &numbers.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<u8>>());
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(gpu) = resources.get::<Gpu>() else { return };
        let from = self.slot as u64 * SIZE;
        pass.set_pipeline(&gpu.pipelines[self.kind as usize]);
        pass.set_vertex_buffer(0, gpu.slots.slice(from..from + SIZE));
        pass.draw(0..3, 0..1);
    }
}

/// Builds the pipelines once, when the window opens.
pub fn install(cc: &eframe::CreationContext<'_>) {
    let Some(state) = cc.wgpu_render_state.as_ref() else { return };
    let device = &state.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fx"), source: wgpu::ShaderSource::Wgsl(include_str!("fx.wgsl").into()) });
    let pipelines = ["ripple", "glow", "flow", "shimmer"].map(|entry| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: SIZE, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4] })],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            // ponytail: colours go out as egui's do, for the gamma framebuffer it asks for; convert here if a platform only offers an sRGB one.
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(entry), targets: &[Some(wgpu::ColorTargetState { format: state.target_format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview_mask: None,
            cache: None,
        })
    });
    let slots = device.create_buffer(&wgpu::BufferDescriptor { label: Some("fx"), size: SIZE * ROOM as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    state.renderer.write().callback_resources.insert(Gpu { pipelines, slots });
}

/// Called as each frame starts: the effects of the last one give their places back.
pub fn begin() {
    NEXT.store(0, Ordering::Relaxed);
}

fn shape(kind: Kind, rect: Rect, at: Option<Pos2>, when: f32, ink: Color32, more: [f32; 2]) -> egui::Shape {
    let slot = NEXT.fetch_add(1, Ordering::Relaxed);
    if slot >= ROOM {
        return egui::Shape::Noop;
    }
    let at = at.map_or(egui::vec2(-1e5, -1e5), |at| at - rect.min);
    egui_wgpu::Callback::new_paint_callback(rect, Draw { kind, slot, rect, at, when, ink: ink.to_normalized_gamma_f32(), more }).into()
}

/// What something you can press does under the pointer: the hand, a sheen that follows it, and a ripple from where it pressed.
pub fn touch(ui: &egui::Ui, hit: egui::Response, ink: Color32) -> egui::Response {
    let ctx = ui.ctx();
    let now = ctx.input(|input| input.time);
    let held = hit.is_pointer_button_down_on();
    let over = ctx.animate_bool_with_time(hit.id.with("over"), hit.hovered(), 0.12) * ui.opacity();
    if over > 0.0 {
        ui.painter().add(shape(Kind::Glow, hit.rect, ctx.pointer_hover_pos(), 0.0, ink, [over * if held { 0.2 } else { 0.11 }, 90.0]));
    }
    let key = hit.id.with("pressed");
    let was = ctx.data_mut(|data| std::mem::replace(data.get_temp_mut_or_default::<bool>(hit.id.with("held")), held));
    if held && !was {
        let at = ctx.pointer_interact_pos().unwrap_or(hit.rect.center());
        ctx.data_mut(|data| data.insert_temp(key, (now, at)));
    }
    if let Some((since, at)) = ctx.data(|data| data.get_temp::<(f64, Pos2)>(key)) {
        let along = ((now - since) / 0.55) as f32;
        if along < 1.0 {
            ui.painter().add(shape(Kind::Ripple, hit.rect, Some(at), along, ink, [0.3, 0.0]));
            ctx.request_repaint();
        } else {
            ctx.data_mut(|data| data.remove::<(f64, Pos2)>(key));
        }
    }
    hit.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Contour lines drifting slowly across `rect`, brighter near the pointer. `under` is a shape reserved before
/// the contents of the rectangle were drawn, so the lines sit behind them.
pub fn flow(ui: &egui::Ui, under: egui::layers::ShapeIdx, rect: Rect, lines: Color32) {
    let ctx = ui.ctx();
    let seconds = ctx.input(|input| input.time % 3600.0) as f32;
    ui.painter().set(under, shape(Kind::Flow, rect, ctx.pointer_hover_pos(), seconds, lines, [ui.opacity(), 0.0]));
    if ctx.input(|input| input.focused) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// A band of light travelling along `rect`, for as long as something is at work.
pub fn shimmer(ui: &egui::Ui, rect: Rect, ink: Color32) {
    let seconds = ui.input(|input| input.time % 3600.0) as f32;
    ui.painter().add(shape(Kind::Shimmer, rect, None, seconds, ink, [0.0, 0.0]));
    ui.ctx().request_repaint();
}

/// How far in a card is: 0 as you turn to it, 1 a moment later.
pub fn arrive(ctx: &egui::Context, card: usize) -> f32 {
    let now = ctx.input(|input| input.time);
    let key = egui::Id::new("card arrives");
    let (on, since) = ctx.data(|data| data.get_temp::<(usize, f64)>(key)).unwrap_or((card, f64::MIN));
    let since = if on == card { since } else { now };
    ctx.data_mut(|data| data.insert_temp(key, (card, since)));
    let along = (((now - since) / 0.18) as f32).clamp(0.0, 1.0);
    if along < 1.0 {
        ctx.request_repaint();
    }
    1.0 - (1.0 - along).powi(3)
}
