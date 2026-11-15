//! Desktop Siege nested compositor.
//!
//!   cargo run -p compositor --release                 # opens a window inside your desktop
//!   cargo run -p compositor --release -- --no-kill    # destroyed screens don't close apps
//!
//! Test hooks (for automated checks):
//!   --autolaunch N          build a screen automatically and launch APPS[N] into it
//!   --shot SECS:PATH        save a PNG of the output at SECS (repeatable); quits after the last

mod handlers;
mod input;
mod state;

use std::time::{Duration, Instant};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                surface::WaylandSurfaceRenderElement,
                utils::RescaleRenderElement,
                AsRenderElements, Kind,
            },
            gles::GlesRenderer,
            ExportMem, ImportAll, ImportMem, Renderer, TextureFilter,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    desktop::space::SpaceElement,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        wayland_server::{Display, DisplayHandle},
        winit::{dpi::LogicalSize, window::WindowAttributes},
    },
    utils::{Point, Rectangle, Scale, Size, Transform},
};
use tracing::{error, info, warn};

use shared::{GameApi, LAYER_BYTES, VIEW_H, VIEW_W};

fn mk_layer(w: u32, h: u32) -> MemoryRenderBuffer {
    MemoryRenderBuffer::new(Fourcc::Abgr8888, (w as i32, h as i32), 1, Transform::Normal, None)
}
use state::{Options, Siege, ViewXform};

smithay::backend::renderer::element::render_elements! {
    pub SiegeElement<R> where R: ImportAll + ImportMem;
    Layer=MemoryRenderBufferRenderElement<R>,
    Window=RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
}

pub struct CalloopData {
    pub state: Siege,
    pub display_handle: DisplayHandle,
}

struct Frame {
    backend: WinitGraphicsBackend<GlesRenderer>,
    output: Output,
    damage: OutputDamageTracker,
    world_buf: MemoryRenderBuffer,
    fx_buf: MemoryRenderBuffer,
    world_px: Vec<u8>,
    fx_px: Vec<u8>,
    /// Size the layer buffers currently have (follows `game.view_size()`).
    layer_size: (u32, u32),
    last: Instant,
    acc: f32,
    fps_frames: u32,
    fps_at: Instant,
}

const DT: f32 = 1.0 / 60.0;

fn main() {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter("info,smithay=warn").init();
    }
    if let Err(e) = run() {
        error!("fatal: {e}");
        std::process::exit(1);
    }
}

fn parse_opts() -> Options {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Options { no_kill: false, autolaunch: None, shots: Vec::new() };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-kill" => opts.no_kill = true,
            "--autolaunch" => {
                i += 1;
                opts.autolaunch = args.get(i).and_then(|s| s.parse().ok());
            }
            "--shot" => {
                i += 1;
                if let Some((t, p)) = args.get(i).and_then(|s| s.split_once(':')) {
                    if let Ok(t) = t.parse::<f32>() {
                        opts.shots.push((t, p.to_string()));
                    }
                }
            }
            other => warn!("unknown argument {other}"),
        }
        i += 1;
    }
    opts.shots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    opts
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let opts = parse_opts();
    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let display: Display<Siege> = Display::new()?;
    let display_handle = display.handle();
    let state = Siege::new(&mut event_loop, display, opts);
    let mut data = CalloopData { state, display_handle };

    // --- winit backend (a window inside the host desktop) ---
    let attrs = WindowAttributes::default()
        .with_title("Desktop Siege")
        .with_inner_size(LogicalSize::new(1920.0, 1080.0));
    let (mut backend, winit_loop) = winit::init_from_attributes::<GlesRenderer>(attrs)?;
    let size = backend.window_size();
    let mode = Mode { size, refresh: 60_000 };
    let output = Output::new(
        "siege".into(),
        PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "DesktopSiege".into(), model: "Nested".into() },
    );
    let _global = output.create_global::<Siege>(&data.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    data.state.view = ViewXform::for_output(size.w, size.h, VIEW_W, VIEW_H);

    // Crisp pixel art.
    if let Err(e) = backend.renderer().upscale_filter(TextureFilter::Nearest) {
        warn!("nearest filter unavailable: {e:?}");
    }

    std::env::set_var("WAYLAND_DISPLAY", &data.state.socket_name);
    info!("listening on WAYLAND_DISPLAY={:?}", data.state.socket_name);

    let damage = OutputDamageTracker::from_output(&output);
    let frame = std::rc::Rc::new(std::cell::RefCell::new(Frame {
        backend,
        output: output.clone(),
        damage,
        world_buf: mk_layer(VIEW_W, VIEW_H),
        fx_buf: mk_layer(VIEW_W, VIEW_H),
        world_px: vec![0; LAYER_BYTES],
        fx_px: vec![0; LAYER_BYTES],
        layer_size: (VIEW_W, VIEW_H),
        last: Instant::now(),
        acc: 0.0,
        fps_frames: 0,
        fps_at: Instant::now(),
    }));

    // Window/input events.
    let f2 = frame.clone();
    event_loop.handle().insert_source(winit_loop, move |event, _, data| {
        let f = f2.borrow_mut();
        match event {
            WinitEvent::Resized { size, .. } => {
                f.output.change_current_state(Some(Mode { size, refresh: 60_000 }), None, None, None);
            }
            WinitEvent::Input(ev) => {
                let s = f.backend.window_size();
                drop(f);
                data.state.process_input_event(ev, (s.w, s.h));
            }
            WinitEvent::CloseRequested => data.state.loop_signal.stop(),
            _ => {}
        }
    })?;

    // 60 Hz frame clock: step the game, render, send frame callbacks.
    let f3 = frame.clone();
    event_loop.handle().insert_source(Timer::immediate(), move |_, _, data| {
        let started = Instant::now();
        let mut f = f3.borrow_mut();
        render_frame(&mut f, &mut data.state);
        let _ = data.display_handle.flush_clients();
        let spent = started.elapsed();
        TimeoutAction::ToDuration(Duration::from_micros(16_667).saturating_sub(spent).max(Duration::from_millis(1)))
    })?;

    event_loop.run(None, &mut data, |_| {})?;

    // Clean up children on exit.
    for c in data.state.children.iter_mut() {
        state::kill_tree(c);
    }
    Ok(())
}

fn render_frame(f: &mut Frame, st: &mut Siege) {
    // --- fixed-step simulation ---
    let now = Instant::now();
    f.acc += (now - f.last).as_secs_f32().min(0.25);
    f.last = now;
    let mut steps = 0;
    while f.acc >= DT && steps < 5 {
        st.tick_game(DT);
        f.acc -= DT;
        steps += 1;
    }
    st.frame += 1;
    if st.frame <= 3 || st.frame % 120 == 0 {
        tracing::debug!("frame {} (t={:.1}s, screens={}, windows={})", st.frame, st.start_time.elapsed().as_secs_f32(), st.screens.len(), st.windows.len());
    }
    st.configure_windows();

    let size = f.backend.window_size();
    let (ow, oh) = (size.w, size.h);
    if steps > 0 {
        st.refresh_pointer((ow, oh));
    }

    let t_sim = now.elapsed();
    // --- game layers ---
    // Follow the game's view size (world zoom): resize buffers, refit the view.
    let (vw, vh) = st.game.view_size();
    if f.layer_size != (vw, vh) {
        f.world_buf = mk_layer(vw, vh);
        f.fx_buf = mk_layer(vw, vh);
        f.world_px = vec![0; (vw * vh * 4) as usize];
        f.fx_px = vec![0; (vw * vh * 4) as usize];
        f.layer_size = (vw, vh);
    }
    st.view = ViewXform::for_output(ow, oh, vw, vh);
    st.game.draw_world(&mut f.world_px);
    st.game.draw_fx(&mut f.fx_px);
    let world_px = &f.world_px;
    let _ = f.world_buf.render().draw(|mem| {
        mem.copy_from_slice(world_px);
        Ok::<_, ()>(vec![Rectangle::from_size((vw as i32, vh as i32).into())])
    });
    let fx_px = &f.fx_px;
    let _ = f.fx_buf.render().draw(|mem| {
        // GL blending expects premultiplied alpha.
        for (d, s) in mem.chunks_exact_mut(4).zip(fx_px.chunks_exact(4)) {
            let a = s[3] as u32;
            d[0] = ((s[0] as u32 * a + 127) / 255) as u8;
            d[1] = ((s[1] as u32 * a + 127) / 255) as u8;
            d[2] = ((s[2] as u32 * a + 127) / 255) as u8;
            d[3] = s[3];
        }
        Ok::<_, ()>(vec![Rectangle::from_size((vw as i32, vh as i32).into())])
    });

    let t_draw = now.elapsed();
    let view = st.view;
    let layer_loc = Point::<f64, smithay::utils::Physical>::from((view.ox, view.oy));
    let layer_size: Size<i32, smithay::utils::Logical> =
        ((vw as f64 * view.scale).round() as i32, (vh as f64 * view.scale).round() as i32).into();

    let layer_src: Rectangle<f64, smithay::utils::Logical> = Rectangle::from_size((vw as f64, vh as f64).into());
    let order = st.draw_order();
    let t = st.start_time.elapsed().as_secs_f32();

    // Bind and render. Elements are listed front (top) to back.
    let shot_due = st.opts.shots.first().is_some_and(|(at, _)| t >= *at);
    let render_ok = {
        let (renderer, mut fb) = match f.backend.bind() {
            Ok(x) => x,
            Err(e) => {
                warn!("bind failed: {e:?}");
                return;
            }
        };
        let mut elements: Vec<SiegeElement<GlesRenderer>> = Vec::new();
        match MemoryRenderBufferRenderElement::from_buffer(renderer, layer_loc, &f.fx_buf, None, Some(layer_src), Some(layer_size), Kind::Unspecified) {
            Ok(e) => elements.push(SiegeElement::Layer(e)),
            Err(e) => warn!("fx layer: {e:?}"),
        }
        for &i in order.iter().rev() {
            let w = &st.windows[i];
            let Some((mut r, sx, sy)) = st.window_placement(w, ow, oh) else { continue };
            let mut alpha = 1.0;
            let zoomed = st.screens.iter().map(|s| s.zoom).fold(0.0f32, f32::max);
            let my_zoom = w.screen.and_then(|id| st.screens.iter().find(|s| s.id == id)).map(|s| s.zoom).unwrap_or(0.0);
            if zoomed > 0.0 && my_zoom < zoomed {
                // Another screen is zoomed in: fade this window out with the world.
                alpha = 1.0 - zoomed;
                if alpha < 0.02 {
                    continue;
                }
            }
            if let Some(info) = w.screen.and_then(|id| st.screens.iter().find(|s| s.id == id)) {
                if info.glitch {
                    let k = ((t * 60.0) as u32).wrapping_mul(2654435761) >> 16;
                    r.loc.x += ((k % 13) as i32 - 6) * (view.scale as i32).max(1);
                    alpha *= 0.55 + (k % 45) as f32 / 100.0;
                }
            }
            let geo = w.window.geometry();
            let loc = (r.loc - geo.loc).to_physical(1);
            let surf: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = w.window.render_elements(renderer, loc, Scale::from(1.0), alpha);
            for e in surf {
                elements.push(SiegeElement::Window(RescaleRenderElement::from_element(e, r.loc.to_physical(1), Scale { x: sx, y: sy })));
            }
        }
        match MemoryRenderBufferRenderElement::from_buffer(renderer, layer_loc, &f.world_buf, None, Some(layer_src), Some(layer_size), Kind::Unspecified) {
            Ok(e) => elements.push(SiegeElement::Layer(e)),
            Err(e) => warn!("world layer: {e:?}"),
        }

        if st.frame <= 3 {
            tracing::debug!("bound framebuffer, rendering {} elements", elements.len());
        }
        let ok = match f.damage.render_output(renderer, &mut fb, 0, &elements, [0.02, 0.02, 0.04, 1.0]) {
            Ok(_) => true,
            Err(e) => {
                warn!("render failed: {e:?}");
                false
            }
        };

        let t_gl = now.elapsed();
        if st.frame % 30 == 0 {
            tracing::debug!("timing: sim {:?} draw {:?} gl {:?}", t_sim, t_draw, t_gl);
        }
        // Only copy here; mapping makes the context surfaceless, so it waits until
        // after submit (mapping before submit made eglSwapBuffers fail with BAD_SURFACE).
        let shot = if ok && shot_due {
            match renderer.copy_framebuffer(&fb, Rectangle::from_size((ow, oh).into()), Fourcc::Abgr8888) {
                Ok(m) => Some(m),
                Err(e) => {
                    warn!("screenshot copy failed: {e:?}");
                    None
                }
            }
        } else {
            None
        };
        (ok, shot)
    };
    let (render_ok, shot) = render_ok;
    if render_ok {
        if let Err(e) = f.backend.submit(Some(&[Rectangle::from_size(size)])) {
            warn!("submit failed: {e:?}");
        }
    }
    if st.frame % 30 == 0 {
        tracing::debug!("timing: after submit {:?}", now.elapsed());
    }
    if let (Some(mapping), Some((_, path))) = (shot, st.opts.shots.first()) {
        save_shot(f.backend.renderer(), &mapping, ow, oh, path);
    }
    if shot_due {
        st.opts.shots.remove(0);
        if st.opts.shots.is_empty() {
            info!("last screenshot taken, exiting");
            st.loop_signal.stop();
        }
    }

    // Frame callbacks so apps keep drawing.
    let elapsed = st.start_time.elapsed();
    for w in &st.windows {
        w.window.send_frame(&f.output, elapsed, Some(Duration::ZERO), |_, _| Some(f.output.clone()));
        w.window.refresh();
    }
    st.popups.cleanup();

    f.fps_frames += 1;
    if f.fps_at.elapsed() >= Duration::from_secs(1) {
        let fps = f.fps_frames as f32 / f.fps_at.elapsed().as_secs_f32();
        st.game.fps = fps;
        f.backend.window().set_title(&format!("Desktop Siege - {fps:.0} fps"));
        info!("fps {fps:.1} (output {ow}x{oh}, windows {})", st.windows.len());
        f.fps_frames = 0;
        f.fps_at = Instant::now();
    }
}

fn save_shot(renderer: &mut GlesRenderer, mapping: &<GlesRenderer as ExportMem>::TextureMapping, w: i32, h: i32, path: &str) {
    let bytes = match renderer.map_texture(mapping) {
        Ok(b) => b.to_vec(),
        Err(e) => {
            warn!("screenshot map failed: {e:?}");
            return;
        }
    };
    // GL rows are bottom-up.
    let stride = (w * 4) as usize;
    let mut flipped = Vec::with_capacity(bytes.len());
    for row in (0..h as usize).rev() {
        if let Some(r) = bytes.get(row * stride..(row + 1) * stride) {
            flipped.extend_from_slice(r);
        }
    }
    if flipped.len() != stride * h as usize {
        warn!("screenshot: unexpected buffer size {}", bytes.len());
        return;
    }
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&flipped)?;
        Ok(())
    };
    match write() {
        Ok(()) => info!("screenshot -> {path}"),
        Err(e) => warn!("screenshot write failed: {e}"),
    }
}
