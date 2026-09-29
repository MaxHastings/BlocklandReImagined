//! Native window and GPU ownership. Nothing creates a window until `run` is
//! explicitly called by the executable. Mapping tests never start an event loop.
use anyhow::{Context, Result, bail};
use bri_ui::api::{DisplayModes, RequestId, UiUpdate};
use bri_ui::binds::Platform;
use bri_ui::gpu::UiRenderer;
use bri_ui::input::{InputEvent, Key, Modifiers, MouseButton};
use bri_ui::ui::Ui;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

pub struct PlatformConfig {
    pub title: String,
    pub size: (u32, u32),
    pub fullscreen: bool,
    pub vsync: bool,
    /// Frame-rate cap while focused (`$pref::Video::MaxFps`), None for none.
    pub max_fps: Option<u32>,
    pub app: Box<dyn PlatformApp>,
}

#[derive(Debug)]
pub enum PlatformCommand {
    Quit,
    ApplyDisplay {
        request: RequestId,
        resolution: (u32, u32),
        fullscreen: bool,
        vsync: bool,
    },
    ToggleFullscreen,
    /// Cap the focused frame rate, or stop capping it.
    FrameLimit(Option<u32>),
    /// Save the next presented frame as PNG. `hud` includes the interface.
    Screenshot {
        path: std::path::PathBuf,
        hud: bool,
    },
}

/// The bridge owns simulation/content/settings and consumes UiActions in
/// `pump`. The platform calls `Ui::update` itself; `tick` must not do so again.
/// GPU references are borrowed, and must not be retained across `gpu_stopped`.
pub trait PlatformApp {
    fn ui(&self) -> &Ui;
    fn ui_mut(&mut self) -> &mut Ui;
    fn tick(&mut self, _elapsed: Duration) -> Result<()> {
        Ok(())
    }
    fn pump(&mut self) -> Result<Vec<PlatformCommand>>;
    fn gpu_ready(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _format: wgpu::TextureFormat,
    ) -> Result<()> {
        Ok(())
    }
    fn gpu_stopped(&mut self) {}
    /// The window's close button (or Alt+F4). Return false to keep running,
    /// for example while the player is asked about unsaved changes.
    fn close_requested(&mut self) -> bool {
        true
    }
    /// The window gained or lost keyboard focus.
    fn focus_changed(&mut self, _focused: bool) {}
    /// The device was lost (driver reset, TDR); `gpu_stopped` follows.
    fn gpu_lost(&mut self) {}
    /// Return true after clearing/rendering a scene; false asks the platform to
    /// clear to its neutral background before compositing UI.
    fn render_scene(&mut self, _frame: &mut RenderContext<'_>) -> Result<bool> {
        Ok(false)
    }
    /// Whether to time frames (the performance overlay is showing). While
    /// false, no GPU timestamps are written and `frame_timed` is not called.
    fn wants_frame_timing(&self) -> bool {
        false
    }
    /// A presented frame's timing.
    fn frame_timed(&mut self, _timing: crate::perf::FrameTiming) {}
}

pub struct RenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub target: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    /// Allows the bridge to register original avatar/brick preview textures.
    pub ui_renderer: &'a mut UiRenderer,
}

struct Graphics {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    renderer: UiRenderer,
    reconfigure: bool,
    /// Created the first time frames are timed; None inside when the GPU
    /// has no timestamps.
    frame_timer: Option<Option<crate::perf::GpuFrameTimer>>,
    device_lost: Arc<Mutex<Option<String>>>,
    /// Kept to rebuild the GPU after a device loss without an event loop.
    display: winit::event_loop::OwnedDisplayHandle,
}

/// Open the discrete GPU on the first backend that works. A broken or missing
/// driver for one API (a common DX12 or Vulkan failure on older machines)
/// falls through to the next instead of ending the game; software rendering
/// (WARP) is the last resort so a player still reaches the menus and can
/// report the problem. `WGPU_BACKEND=dx12|vulkan|...` forces one backend.
fn open_gpu(
    window: &Arc<Window>,
    display: winit::event_loop::OwnedDisplayHandle,
) -> Result<(
    wgpu::Instance,
    wgpu::Surface<'static>,
    wgpu::Adapter,
    wgpu::Device,
    wgpu::Queue,
)> {
    let forced = wgpu::Backends::from_env();
    let order: Vec<(wgpu::Backends, bool)> = match forced {
        Some(backends) => vec![(backends, false), (backends, true)],
        None => vec![
            (wgpu::Backends::PRIMARY & !wgpu::Backends::VULKAN, false),
            (wgpu::Backends::VULKAN, false),
            (wgpu::Backends::PRIMARY, true),
        ],
    };
    let mut failures = Vec::new();
    for (backends, software) in order {
        if backends.is_empty() {
            continue;
        }
        let mut descriptor =
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display.clone()));
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let attempt = (|| -> Result<_> {
            let surface = instance
                .create_surface(window.clone())
                .context("creating the render surface")?;
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: software,
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .context("no compatible adapter")?;
            // Timestamps, where the GPU has them, time Add-On code's layers.
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_features: bri_client_sandbox::gpu::timing_features(&adapter),
                    ..Default::default()
                }))
                .context("creating the GPU device")?;
            Ok((surface, adapter, device, queue))
        })();
        match attempt {
            Ok((surface, adapter, device, queue)) => {
                let info = adapter.get_info();
                bri_console::echo(format!(
                    "GPU: {} ({:?}, {:?}, driver {} {})",
                    info.name, info.backend, info.device_type, info.driver, info.driver_info
                ));
                if !failures.is_empty() {
                    bri_console::warn(format!(
                        "Fell back to {:?} after: {}",
                        info.backend,
                        failures.join("; ")
                    ));
                }
                return Ok((instance, surface, adapter, device, queue));
            }
            Err(error) => failures.push(format!(
                "{backends:?}{}: {error:#}",
                if software { " (software)" } else { "" }
            )),
        }
    }
    anyhow::bail!("No usable GPU backend: {}", failures.join("; "))
}
fn present_mode(vsync: bool, modes: &[wgpu::PresentMode]) -> Result<wgpu::PresentMode> {
    if vsync {
        return Ok(wgpu::PresentMode::Fifo);
    }
    [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
        .into_iter()
        .find(|m| modes.contains(m))
        .context("The display backend does not support an unsynchronized/low-latency present mode")
}

impl Graphics {
    fn new(
        window: Arc<Window>,
        vsync: bool,
        display: winit::event_loop::OwnedDisplayHandle,
    ) -> Result<Self> {
        let (instance, surface, adapter, device, queue) = open_gpu(&window, display.clone())?;
        let caps = surface.get_capabilities(&adapter);
        // UiRenderer samples authored art as unorm; a non-sRGB swapchain matches
        // the existing offscreen reference output without a second gamma curve.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .context("surface exposes no texture formats")?;
        let device_lost = Arc::new(Mutex::new(None));
        let lost_callback = device_lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut error) = lost_callback.lock() {
                *error = Some(format!("{reason:?}: {message}"));
            }
        });
        let size = window.inner_size();
        // Screenshots copy the swapchain image when the backend allows it.
        let usage = if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            // A saved preference can outlive the GPU/monitor it was applied on.
            // Interactive changes still reject unsupported modes explicitly.
            present_mode: present_mode(vsync, &caps.present_modes).unwrap_or_else(|error| {
                bri_console::warn(format!("Saved display mode unavailable: {error}; starting with VSync."));
                wgpu::PresentMode::Fifo
            }),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        if size.width > 0 && size.height > 0 {
            surface.configure(&device, &config);
        }
        let renderer = UiRenderer::new(&device, &queue);
        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            present_modes: caps.present_modes,
            renderer,
            reconfigure: false,
            frame_timer: None,
            device_lost,
            display,
        })
    }
    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.reconfigure = false;
    }
}

struct DisplayChange {
    /// None when the platform changed the mode itself (Alt+Enter).
    request: Option<RequestId>,
    expected: PhysicalSize<u32>,
    deadline: Instant,
    old_size: PhysicalSize<u32>,
    old_fullscreen: bool,
    old_vsync: bool,
}

fn set_mode(window: &Window, fullscreen: bool, size: PhysicalSize<u32>) {
    if fullscreen {
        window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
    } else {
        window.set_fullscreen(None);
        // A maximized window ignores size requests.
        window.set_maximized(false);
        let _ = window.request_inner_size(size);
        // A game launched fullscreen has no earlier window spot to return
        // to; keep the whole window on its monitor.
        if let Some(m) = window.current_monitor()
            && let Ok(pos) = window.outer_position()
        {
            let (mp, ms, os) = (m.position(), m.size(), window.outer_size());
            let inside = pos.x >= mp.x
                && pos.y >= mp.y
                && pos.x + os.width as i32 <= mp.x + ms.width as i32
                && pos.y + os.height as i32 <= mp.y + ms.height as i32;
            if !inside {
                window.set_outer_position(PhysicalPosition::new(
                    mp.x + (ms.width as i32 - os.width as i32).max(0) / 2,
                    mp.y + (ms.height as i32 - os.height as i32).max(0) / 2,
                ));
            }
        }
    }
}

/// Window frame and caption around the client area, as v20 subtracted them
/// from the desktop (`getWindowFrameSize`, `getWindowCaptionHeight`).
fn decorations(window: &Window) -> (u32, u32) {
    let outer = window.outer_size();
    let inner = window.inner_size();
    if window.fullscreen().is_none()
        && !window.is_maximized()
        && outer.width > inner.width
        && outer.height > inner.height
    {
        (outer.width - inner.width, outer.height - inner.height)
    } else {
        let s = window.scale_factor();
        ((16.0 * s).round() as u32, (39.0 * s).round() as u32)
    }
}

fn display_modes(window: &Window) -> Option<DisplayModes> {
    let monitor = window.current_monitor()?;
    let native = monitor.size();
    let deco = decorations(window);
    let desk = (
        native.width.saturating_sub(deco.0),
        native.height.saturating_sub(deco.1),
    );
    let modes = monitor
        .video_modes()
        .map(|m| (m.size().width, m.size().height));
    Some(DisplayModes {
        native: (native.width, native.height),
        windowed: windowed_sizes(modes, desk),
    })
}

/// v20's windowed list: the monitor's modes strictly inside the desktop
/// minus the window frame, at least 640 x 480.
pub fn windowed_sizes(
    modes: impl IntoIterator<Item = (u32, u32)>,
    desk: (u32, u32),
) -> Vec<(u32, u32)> {
    let mut list: Vec<_> = modes
        .into_iter()
        .filter(|&(w, h)| w >= 640 && h >= 480 && w < desk.0 && h < desk.1)
        .collect();
    list.sort_unstable();
    list.dedup();
    list
}

/// Whether a window size fits inside some listed windowed size.
fn fits(windowed: &[(u32, u32)], (w, h): (u32, u32)) -> bool {
    windowed.is_empty() || windowed.iter().any(|&(mw, mh)| w <= mw && h <= mh)
}

struct Runner {
    config: PlatformConfig,
    /// Recent GPU device losses, to stop retrying a GPU that keeps failing.
    gpu_losses: Vec<Instant>,
    /// `BRI_RECORD_INPUT=<file>`: every input and tick, for replay tests.
    recorder: Option<crate::playback::Recorder>,
    gamepads: crate::gamepad::Gamepads,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    focused: bool,
    occluded: bool,
    grabbed: bool,
    regrab: bool,
    next_regrab: Instant,
    focus_click: FocusClick,
    ime_allowed: bool,
    composing: bool,
    mods: Modifiers,
    cursor: PhysicalPosition<f64>,
    wheel_pixels: f64,
    last_tick: Instant,
    next_tick: Instant,
    /// When the next capped frame is due (unused without a cap).
    next_frame: Instant,
    display: Option<DisplayChange>,
    modes: Option<DisplayModes>,
    /// Last client size while windowed and not maximized.
    windowed: PhysicalSize<u32>,
    error: Option<anyhow::Error>,
    screenshot: Option<(std::path::PathBuf, bool)>,
    screenshots: Screenshots,
    /// Main-thread work since the last presented frame (update and pump).
    frame_cpu: Duration,
    last_present: Option<Instant>,
}

/// Launch only from an explicitly requested interactive execution path. This
/// function blocks until the native window closes; headless clients need not use it.
pub fn run(config: PlatformConfig) -> Result<()> {
    if config.size.0 < 640 || config.size.1 < 480 {
        bail!("Native client requires at least 640 x 480 pixels");
    }
    let now = Instant::now();
    let windowed = PhysicalSize::new(config.size.0, config.size.1);
    let mut runner = Runner {
        gpu_losses: Vec::new(),
        gamepads: crate::gamepad::Gamepads::new(),
        recorder: std::env::var_os("BRI_RECORD_INPUT").and_then(|path| {
            crate::playback::Recorder::create(std::path::Path::new(&path))
                .map_err(|error| bri_console::warn(format!("{error:#}")))
                .ok()
        }),
        config,
        window: None,
        graphics: None,
        focused: false,
        occluded: false,
        grabbed: false,
        regrab: false,
        next_regrab: now,
        focus_click: FocusClick::default(),
        ime_allowed: false,
        composing: false,
        mods: Modifiers::NONE,
        cursor: PhysicalPosition::new(0.0, 0.0),
        wheel_pixels: 0.0,
        last_tick: now,
        next_tick: now,
        next_frame: now,
        display: None,
        modes: None,
        windowed,
        error: None,
        screenshot: None,
        screenshots: Screenshots::default(),
        frame_cpu: Duration::ZERO,
        last_present: None,
    };
    let event_loop = EventLoop::new().context("creating the native event loop")?;
    event_loop
        .run_app(&mut runner)
        .context("running the native event loop")?;
    if let Some(e) = runner.error {
        return Err(e);
    }
    Ok(())
}

impl Runner {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }
    fn input(&mut self, event: InputEvent) {
        if let Some(recorder) = &mut self.recorder {
            recorder.event(event);
        }
        self.config.app.ui_mut().handle_input(event);
    }
    fn focused_text(&self) -> bool {
        self.config.app.ui().takes_text()
    }
    fn sync_cursor(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        let minimized =
            window.is_minimized().unwrap_or(false) || size.width == 0 || size.height == 0;
        let grab = self.focused && !minimized && !self.config.app.ui().cursor_visible();
        if grab && (!self.grabbed || self.regrab) {
            // Windows locks the cursor wherever it sits, which after Alt+Tab
            // or a taskbar restore is outside the client area, and the OS
            // drops the clip on activation changes. Like v20's
            // setMouseClipping on WM_ACTIVATE, park it inside and re-clip.
            let _ =
                window.set_cursor_position(PhysicalPosition::new(size.width / 2, size.height / 2));
            window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .context("capturing the gameplay cursor")?;
            self.grabbed = true;
        } else if !grab && self.grabbed {
            window
                .set_cursor_grab(CursorGrabMode::None)
                .context("releasing the gameplay cursor")?;
            self.grabbed = false;
        }
        self.regrab = false;
        window.set_cursor_visible(!grab);
        let ime = self.focused && self.focused_text();
        if ime != self.ime_allowed {
            window.set_ime_allowed(ime);
            self.ime_allowed = ime;
            if !ime {
                self.composing = false;
            }
        }
        if ime {
            let ui = self.config.app.ui();
            if let Some(screen) = ui.screen(ui.top_id())
                && let Some(n) = screen.view().text_node()
            {
                let r = screen.view().node(n).rect;
                let scale = ui.scale();
                window.set_ime_cursor_area(
                    PhysicalPosition::new(
                        (r.x as f32 * scale) as i32,
                        ((r.y + r.h) as f32 * scale) as i32,
                    ),
                    PhysicalSize::new((r.w as f32 * scale).max(1.0) as u32, 1),
                );
            }
        }
        Ok(())
    }
    fn resize(&mut self, size: PhysicalSize<u32>) {
        if let Some(g) = &mut self.graphics {
            g.resize(size);
        }
        if size.width > 0 && size.height > 0 {
            let scale = self.config.app.ui().host_scale();
            self.config
                .app
                .ui_mut()
                .resize((size.width, size.height), scale);
        }
    }
    fn reject_display(&mut self, request: RequestId, reason: String) {
        self.config.app.ui_mut().apply(UiUpdate::ActionResult {
            id: request,
            result: Err(reason),
        });
    }
    fn start_display(
        &mut self,
        request: RequestId,
        resolution: (u32, u32),
        fullscreen: bool,
        vsync: bool,
    ) -> Result<()> {
        if self.display.is_some() {
            bail!("A display change is already pending");
        }
        if resolution.0 < 640 || resolution.1 < 480 {
            bail!("Resolution must be at least 640 x 480");
        }
        let window = self.window.as_ref().context("The window is suspended")?;
        let graphics = self
            .graphics
            .as_mut()
            .context("The GPU surface is suspended")?;
        let limit = graphics.device.limits().max_texture_dimension_2d;
        if resolution.0 > limit || resolution.1 > limit {
            bail!("Requested resolution exceeds the GPU's {limit}-pixel texture limit");
        }
        let mode = present_mode(vsync, &graphics.present_modes)?;
        let target = if fullscreen {
            window
                .current_monitor()
                .context("No current monitor is available")?
                .size()
        } else {
            PhysicalSize::new(resolution.0, resolution.1)
        };
        let old_vsync = self.config.vsync;
        self.config.vsync = vsync;
        graphics.config.present_mode = mode;
        graphics.reconfigure = true;
        self.change_mode(Some(request), fullscreen, target, old_vsync);
        Ok(())
    }
    /// Fullscreen is always borderless on the window's monitor: no display
    /// mode switch, so Alt+Tab and focus changes stay clean. Windowed sizes
    /// are client-area pixels, applied after leaving fullscreen or maximized.
    fn change_mode(
        &mut self,
        request: Option<RequestId>,
        fullscreen: bool,
        target: PhysicalSize<u32>,
        old_vsync: bool,
    ) {
        let Some(window) = &self.window else {
            return;
        };
        self.display = Some(DisplayChange {
            request,
            expected: target,
            deadline: Instant::now() + Duration::from_secs(5),
            old_size: window.inner_size(),
            old_fullscreen: window.fullscreen().is_some(),
            old_vsync,
        });
        set_mode(window, fullscreen, target);
        self.regrab = true;
        self.finish_display();
    }
    fn finish_display(&mut self) {
        let Some(change) = self.display.as_ref() else {
            return;
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let actual = window.inner_size();
        if actual == change.expected && self.graphics.is_some() {
            let change = self.display.take().unwrap();
            self.config.size = (actual.width, actual.height);
            self.config.fullscreen = window.fullscreen().is_some();
            self.resize(actual);
            let update = match change.request {
                Some(id) => UiUpdate::ActionResult { id, result: Ok(()) },
                None => UiUpdate::DisplayChanged {
                    resolution: self.config.size,
                    fullscreen: self.config.fullscreen,
                },
            };
            self.config.app.ui_mut().apply(update);
        } else if Instant::now() >= change.deadline {
            let change = self.display.take().unwrap();
            self.config.vsync = change.old_vsync;
            set_mode(&window, change.old_fullscreen, change.old_size);
            if let Some(g) = &mut self.graphics
                && let Ok(mode) = present_mode(change.old_vsync, &g.present_modes)
            {
                g.config.present_mode = mode;
                g.reconfigure = true;
            }
            self.resize(window.inner_size());
            if let Some(id) = change.request {
                self.reject_display(
                    id,
                    "The requested resolution was not observed; reverting to the previous display settings.".into(),
                );
            }
        }
    }
    /// Alt+Enter (v20's toggleFullScreen): swap between borderless fullscreen
    /// and the last windowed size, and remember the result as Options does.
    fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.display.is_some() {
            return;
        }
        let vsync = self.config.vsync;
        if window.fullscreen().is_some() {
            let size = self.windowed_size();
            self.change_mode(None, false, size, vsync);
        } else if let Some(monitor) = window.current_monitor() {
            self.change_mode(None, true, monitor.size(), vsync);
        }
    }
    /// The size to leave fullscreen at: the last window size if it still fits
    /// on this monitor, else the largest listed size up to 1280 x 720.
    fn windowed_size(&self) -> PhysicalSize<u32> {
        let size = self.windowed;
        match &self.modes {
            Some(m) if !fits(&m.windowed, (size.width, size.height)) => m
                .windowed
                .iter()
                .rev()
                .find(|&&(w, h)| w <= 1280 && h <= 720)
                .or(m.windowed.last())
                .map_or(size, |&(w, h)| PhysicalSize::new(w, h)),
            _ => size,
        }
    }
    /// Tell the UI what this monitor can show; called when the window is
    /// created and whenever it changes monitor or scale.
    fn report_modes(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let Some(modes) = display_modes(window) else {
            return;
        };
        if self.modes.as_ref() != Some(&modes) {
            self.modes = Some(modes.clone());
            self.config.app.ui_mut().apply(UiUpdate::DisplayModes(modes));
        }
    }
    fn pump(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        // Acknowledgments may produce persistence actions. Bound re-entrant
        // pumping so a faulty app cannot starve native window events forever.
        for _ in 0..8 {
            let commands = self.config.app.pump()?;
            if commands.is_empty() {
                break;
            }
            for command in commands {
                match command {
                    PlatformCommand::Quit => event_loop.exit(),
                    PlatformCommand::ApplyDisplay {
                        request,
                        resolution,
                        fullscreen,
                        vsync,
                    } => {
                        if let Err(e) = self.start_display(request, resolution, fullscreen, vsync) {
                            self.reject_display(request, e.to_string());
                        }
                    }
                    PlatformCommand::ToggleFullscreen => self.toggle_fullscreen(),
                    PlatformCommand::FrameLimit(fps) => self.config.max_fps = fps,
                    PlatformCommand::Screenshot { path, hud } => {
                        self.screenshot = Some((path, hud));
                    }
                }
            }
        }
        self.sync_cursor()
    }
    /// A lost GPU device (driver reset or update, TDR, eGPU unplug) rebuilds
    /// the renderer the way a resume does instead of ending the game. Losing
    /// it again and again means the GPU cannot run the game: then it fails.
    fn recover_gpu(&mut self, reason: String) -> Result<()> {
        let now = Instant::now();
        self.gpu_losses
            .retain(|lost| now.duration_since(*lost) < Duration::from_secs(60));
        self.gpu_losses.push(now);
        anyhow::ensure!(
            self.gpu_losses.len() <= 3,
            "The GPU device was lost repeatedly: {reason}"
        );
        bri_console::warn(format!("GPU device lost ({reason}); restarting the renderer."));
        let (Some(window), Some(lost)) = (self.window.clone(), self.graphics.take()) else {
            return Ok(());
        };
        let display = lost.display.clone();
        self.config.app.gpu_lost();
        self.config.app.gpu_stopped();
        drop(lost);
        let gpu = Graphics::new(window.clone(), self.config.vsync, display)?;
        self.config
            .app
            .gpu_ready(&gpu.device, &gpu.queue, gpu.config.format)?;
        self.graphics = Some(gpu);
        self.resize(window.inner_size());
        Ok(())
    }
    fn render(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        if self.occluded || size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let Some(g) = &mut self.graphics else {
            return Ok(());
        };
        if let Some(reason) = g.device_lost.lock().ok().and_then(|mut error| error.take()) {
            return self.recover_gpu(reason);
        }
        // Resized can trail the real size (restore, DPI, fullscreen toggles);
        // never present a swapchain that disagrees with the window.
        if g.reconfigure || (g.config.width, g.config.height) != (size.width, size.height) {
            g.resize(size);
        }
        let timing = self.config.app.wants_frame_timing();
        let acquiring = Instant::now();
        let surface = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                g.reconfigure = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                g.resize(size);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                g.surface = g
                    .instance
                    .create_surface(window.clone())
                    .context("recreating a lost native surface")?;
                g.resize(size);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                bail!("GPU validation failed while acquiring the native surface")
            }
        };
        let acquired = Instant::now();
        let target = surface.texture.create_view(&Default::default());
        let mut encoder = g
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bri-client frame"),
            });
        if !timing {
            g.frame_timer = None;
        }
        let timer = if timing {
            let (device, queue) = (&g.device, &g.queue);
            g.frame_timer
                .get_or_insert_with(|| crate::perf::GpuFrameTimer::new(device, queue))
                .as_mut()
        } else {
            None
        };
        if let Some(timer) = timer {
            timer.begin(&mut encoder);
        }
        let scene = self.config.app.render_scene(&mut RenderContext {
            device: &g.device,
            queue: &g.queue,
            encoder: &mut encoder,
            target: &target,
            format: g.config.format,
            size: (size.width, size.height),
            ui_renderer: &mut g.renderer,
        })?;
        let screenshot = self.screenshot.take();
        let scene_capture = match &screenshot {
            Some((path, false)) => Some((
                path.clone(),
                capture_copy(&g.device, &mut encoder, &surface.texture, g.config.format)?,
            )),
            _ => None,
        };
        let ui = self.config.app.ui();
        g.renderer.render(
            &g.device,
            &g.queue,
            &mut encoder,
            &target,
            g.config.format,
            (size.width, size.height),
            ui.scale(),
            &ui.core.pack,
            &ui.draw(),
            (!scene).then_some(wgpu::Color {
                r: 0.12,
                g: 0.12,
                b: 0.15,
                a: 1.0,
            }),
        );
        let hud_capture = match &screenshot {
            Some((path, true)) => Some((
                path.clone(),
                capture_copy(&g.device, &mut encoder, &surface.texture, g.config.format)?,
            )),
            _ => None,
        };
        if let Some(Some(timer)) = &mut g.frame_timer {
            timer.end(&mut encoder);
        }
        g.queue.submit([encoder.finish()]);
        if let Some((path, capture)) = scene_capture.or(hud_capture) {
            self.screenshots.start(path, capture);
        }
        for text in self.screenshots.poll(&g.device) {
            self.config.app.ui_mut().apply(bri_ui::api::UiUpdate::BottomPrint {
                text,
                seconds: 3.0,
                hide_bar: false,
            });
        }
        window.pre_present_notify();
        let presenting = Instant::now();
        g.queue.present(surface);
        let now = Instant::now();
        let frame = self.last_present.replace(now).map(|at| now.duration_since(at));
        let cpu = std::mem::take(&mut self.frame_cpu);
        if timing && let Some(frame) = frame {
            let gpu = match &mut g.frame_timer {
                Some(Some(timer)) => timer.collect(&g.device),
                _ => None,
            };
            self.config.app.frame_timed(crate::perf::FrameTiming {
                frame,
                cpu: cpu + presenting.duration_since(acquired),
                wait: acquired.duration_since(acquiring) + now.duration_since(presenting),
                gpu,
            });
        }
        Ok(())
    }
}

/// A frame copy queued in the frame's own encoder.
struct Capture {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    row: u32,
    bgra: bool,
}
fn capture_copy(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
) -> Result<Capture> {
    anyhow::ensure!(
        texture.usage().contains(wgpu::TextureUsages::COPY_SRC),
        "This display backend cannot read back frames"
    );
    let (width, height) = (texture.width(), texture.height());
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    Ok(Capture {
        buffer,
        width,
        height,
        row,
        bgra: matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ),
    })
}
/// Screenshots in flight. A frame that takes one only queues a copy; the
/// readback is polled on later frames and the PNG is encoded and written on
/// a worker thread, so taking a screenshot never stalls the game.
#[derive(Default)]
struct Screenshots {
    reading: Vec<Reading>,
    written: Option<(
        std::sync::mpsc::Sender<Written>,
        std::sync::mpsc::Receiver<Written>,
    )>,
}
/// A screenshot's file and whether writing it succeeded.
type Written = (std::path::PathBuf, Result<()>);
struct Reading {
    path: std::path::PathBuf,
    capture: Capture,
    mapped: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    since: Instant,
}
/// How long a readback may wait for the GPU before the screenshot fails.
const SCREENSHOT_READBACK_LIMIT: Duration = Duration::from_secs(5);
impl Screenshots {
    /// Start reading back a copy queued in a frame just submitted.
    fn start(&mut self, path: std::path::PathBuf, capture: Capture) {
        let (tx, mapped) = std::sync::mpsc::channel();
        capture
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
        self.reading.push(Reading {
            path,
            capture,
            mapped,
            since: Instant::now(),
        });
    }
    /// Hand finished readbacks to writer threads and return the messages
    /// for screenshots written or failed since the last call. Never blocks.
    fn poll(&mut self, device: &wgpu::Device) -> Vec<String> {
        let mut messages = Vec::new();
        if !self.reading.is_empty() {
            let _ = device.poll(wgpu::PollType::Poll);
            let (done, _) = self.written.get_or_insert_with(std::sync::mpsc::channel);
            let mut waiting = Vec::new();
            for reading in self.reading.drain(..) {
                match reading.mapped.try_recv() {
                    Ok(Ok(())) => {
                        let done = done.clone();
                        let Reading { path, capture, .. } = reading;
                        let spawned =
                            std::thread::Builder::new()
                                .name("screenshot".into())
                                .spawn({
                                    let path = path.clone();
                                    move || {
                                        let result = capture.write(&path);
                                        let _ = done.send((path, result));
                                    }
                                });
                        if let Err(error) = spawned {
                            messages.push(format!("Screenshot failed: {error}"));
                        }
                    }
                    Ok(Err(error)) => {
                        messages.push(format!("Screenshot failed: screenshot readback: {error}"))
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        messages.push("Screenshot failed: the GPU dropped the readback".into())
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty)
                        if reading.since.elapsed() >= SCREENSHOT_READBACK_LIMIT =>
                    {
                        messages.push("Screenshot failed: the GPU did not finish the copy".into())
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => waiting.push(reading),
                }
            }
            self.reading = waiting;
        }
        if let Some((_, written)) = &self.written {
            while let Ok((path, result)) = written.try_recv() {
                messages.push(match result {
                    Ok(()) => format!(
                        "Screenshot saved: {}",
                        path.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into())
                    ),
                    Err(error) => format!("Screenshot failed: {error:#}"),
                });
            }
        }
        messages
    }
}
impl Capture {
    /// Convert a mapped readback to RGBA and write it as a PNG.
    fn write(self, path: &std::path::Path) -> Result<()> {
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| anyhow::anyhow!("screenshot readback: {e:?}"))?;
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for line in mapped.chunks_exact(self.row as usize) {
            for p in line[..self.width as usize * 4].chunks_exact(4) {
                if self.bgra {
                    pixels.extend_from_slice(&[p[2], p[1], p[0], 255]);
                } else {
                    pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
        }
        drop(mapped);
        self.buffer.unmap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image::save_buffer(
            path,
            &pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let result = (|| -> Result<()> {
            if self.window.is_none() {
                let attributes = Window::default_attributes()
                    .with_title(self.config.title.clone())
                    .with_inner_size(PhysicalSize::new(self.config.size.0, self.config.size.1))
                    .with_min_inner_size(PhysicalSize::new(640, 480));
                let window = Arc::new(
                    event_loop
                        .create_window(attributes)
                        .context("creating the native client window")?,
                );
                self.focused = window.has_focus();
                self.window = Some(window);
                self.report_modes();
                // Saved prefs may name a size this monitor cannot show, or an
                // exclusive fullscreen mode from before fullscreen went
                // borderless; start in the nearest mode and record it.
                let saved = self.config.size;
                let fits_here = self.modes.as_ref().is_none_or(|m| fits(&m.windowed, saved));
                if self.config.fullscreen || !fits_here {
                    let fullscreen = self.config.fullscreen;
                    let target = if fullscreen {
                        let window = self.window.as_ref().unwrap();
                        window.current_monitor().map(|m| m.size())
                    } else {
                        Some(self.windowed_size())
                    };
                    match target {
                        Some(t) if t == PhysicalSize::new(saved.0, saved.1) => {
                            set_mode(self.window.as_ref().unwrap(), fullscreen, t);
                        }
                        Some(t) => {
                            let vsync = self.config.vsync;
                            self.change_mode(None, fullscreen, t, vsync);
                        }
                        None => self.config.fullscreen = false,
                    }
                }
            }
            if self.graphics.is_none() {
                let gpu = Graphics::new(
                    self.window.as_ref().unwrap().clone(),
                    self.config.vsync,
                    event_loop.owned_display_handle(),
                )?;
                self.config
                    .app
                    .gpu_ready(&gpu.device, &gpu.queue, gpu.config.format)?;
                self.graphics = Some(gpu);
            }
            self.resize(self.window.as_ref().unwrap().inner_size());
            self.focused = self.window.as_ref().unwrap().has_focus();
            self.regrab = true;
            self.last_tick = Instant::now();
            self.next_tick = self.last_tick;
            self.sync_cursor()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.fail(event_loop, e);
        }
    }
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.input(InputEvent::FocusLost);
        self.mods = Modifiers::NONE;
        self.focused = false;
        self.composing = false;
        if let Some(w) = &self.window {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            w.set_ime_allowed(false);
        }
        self.grabbed = false;
        self.ime_allowed = false;
        if self.graphics.is_some() {
            self.config.app.gpu_stopped();
            self.graphics = None;
        }
        if let Some(change) = self.display.take() {
            self.config.vsync = change.old_vsync;
            self.config.size = (change.old_size.width, change.old_size.height);
            self.config.fullscreen = change.old_fullscreen;
            if let Some(window) = &self.window {
                set_mode(window, change.old_fullscreen, change.old_size);
            }
            if let Some(id) = change.request {
                self.reject_display(id, "Display change interrupted by suspension.".into());
            }
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().is_none_or(|w| w.id() != window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.config.app.close_requested() {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                self.regrab = true;
                if let Some(w) = &self.window
                    && w.fullscreen().is_none()
                    && !w.is_maximized()
                    && w.is_minimized() != Some(true)
                    && size.width > 0
                    && size.height > 0
                {
                    self.windowed = size;
                }
                self.resize(size)
            }
            WindowEvent::Moved(_) => {
                self.regrab = true;
                self.report_modes();
            }
            WindowEvent::ScaleFactorChanged {
                mut inner_size_writer,
                ..
            } => {
                self.regrab = true;
                if let Some(w) = &self.window {
                    // Resolutions are physical pixels: a DPI change keeps the
                    // chosen size rather than growing the window with it.
                    if w.fullscreen().is_none() && !w.is_maximized() {
                        let _ = inner_size_writer.request_inner_size(w.inner_size());
                    }
                    self.resize(w.inner_size());
                }
                self.report_modes();
            }
            WindowEvent::Occluded(hidden) => {
                self.regrab |= !hidden;
                self.occluded = hidden
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.config.app.focus_changed(focused);
                self.regrab = true;
                if focused {
                    self.focus_click.gained(Instant::now());
                } else {
                    self.focus_click = FocusClick::default();
                    self.input(InputEvent::FocusLost);
                    self.mods = Modifiers::NONE;
                    self.composing = false;
                    self.wheel_pixels = 0.0;
                }
            }
            WindowEvent::ModifiersChanged(mods) => self.mods = translate_modifiers(mods.state()),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if self.focused {
                    self.input(InputEvent::MouseMove {
                        x: position.x as f32,
                        y: position.y as f32,
                    });
                }
            }
            WindowEvent::MouseInput { state, button, .. } if self.focused => {
                let pressed = state == ElementState::Pressed;
                let capturing = !self.config.app.ui().cursor_visible();
                if let Some(button) = translate_button(button)
                    && !self
                        .focus_click
                        .filter(button, pressed, Instant::now(), capturing)
                {
                    let (x, y) = (self.cursor.x as f32, self.cursor.y as f32);
                    self.input(if pressed {
                        InputEvent::MouseDown { button, x, y }
                    } else {
                        InputEvent::MouseUp { button, x, y }
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.focused => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => {
                        pixel_wheel_steps(&mut self.wheel_pixels, p.y)
                    }
                };
                if steps.is_finite() && steps != 0.0 {
                    self.input(InputEvent::Wheel { delta: steps });
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if self.focused => {
                let down = event.state == ElementState::Pressed;
                let typing = self.focused_text();
                if !(is_synthetic || down && self.composing) {
                    if let PhysicalKey::Code(code) = event.physical_key
                        && let Some(key) =
                            translate_key(code, self.config.app.ui().config().platform)
                    {
                        self.input(if down {
                            InputEvent::KeyDown {
                                key,
                                mods: self.mods,
                                repeat: event.repeat,
                            }
                        } else {
                            InputEvent::KeyUp {
                                key,
                                mods: self.mods,
                            }
                        });
                    }
                    if down
                        && typing
                        && self.focused_text()
                        && !self.composing
                        && let Some(text) = event.text
                    {
                        for ch in committed_chars(&text, self.mods) {
                            self.input(InputEvent::Char(ch));
                        }
                    }
                } else if !down
                    && let PhysicalKey::Code(code) = event.physical_key
                    && let Some(key) = translate_key(code, self.config.app.ui().config().platform)
                {
                    self.input(InputEvent::KeyUp {
                        key,
                        mods: self.mods,
                    });
                }
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.composing = !text.is_empty() || cursor.is_some()
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                if self.focused && self.focused_text() {
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        self.input(InputEvent::Char(ch));
                    }
                }
            }
            WindowEvent::Ime(Ime::Disabled) => self.composing = false,
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    self.fail(event_loop, e);
                    return;
                }
            }
            _ => {}
        }
        self.finish_display();
        if let Err(e) = self.pump(event_loop) {
            self.fail(event_loop, e);
        }
    }
    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if self.focused
            && self.grabbed
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && dx.is_finite()
            && dy.is_finite()
        {
            self.input(InputEvent::MouseDelta {
                dx: dx as f32,
                dy: dy as f32,
            });
            if let Err(e) = self.pump(event_loop) {
                self.fail(event_loop, e);
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        // A focused, visible window renders continuously so simulation ticks
        // and camera interpolation line up with every presented frame (paced
        // by VSync). Background windows fall back to a slow timer.
        let active = self.focused && !self.occluded && self.graphics.is_some();
        // A frame cap paces the focused loop by deadline instead of VSync.
        let period = self
            .config
            .max_fps
            .filter(|_| active)
            .map(|fps| Duration::from_secs(1) / fps.max(1));
        let due = if active {
            period.is_none_or(|_| now >= self.next_frame)
        } else {
            now >= self.next_tick
        };
        if due {
            if let Some(period) = period {
                // Keep to the schedule, but never try to catch up on
                // frames missed by a long hitch.
                let next = self.next_frame + period;
                self.next_frame = if next > now { next } else { now + period };
            }
            let elapsed = now.saturating_duration_since(self.last_tick);
            self.last_tick = now;
            let working = Instant::now();
            // Avoid minutes of UI repeat catch-up after suspension/debug pauses.
            let dt_ms = elapsed.as_millis().min(250) as u64;
            self.gamepads
                .poll(self.config.app.ui_mut(), dt_ms, self.focused);
            self.config.app.ui_mut().update(dt_ms);
            if let Some(recorder) = &mut self.recorder
                && let Err(error) = recorder.frame(elapsed)
            {
                bri_console::warn(format!("Input recording stopped: {error:#}"));
                self.recorder = None;
            }
            if let Err(e) = self.config.app.tick(elapsed) {
                self.fail(event_loop, e);
                return;
            }
            self.finish_display();
            if let Err(e) = self.pump(event_loop) {
                self.fail(event_loop, e);
                return;
            }
            self.frame_cpu += working.elapsed();
            if let Some(w) = &self.window
                && !self.occluded
                && self.graphics.is_some()
            {
                w.request_redraw();
            }
            self.next_tick = now + Duration::from_millis(if self.focused { 16 } else { 50 });
        }
        // Windows can drop the clip without telling the window (another
        // app's ClipCursor, the secure desktop); re-assert it while playing.
        if self.grabbed && now >= self.next_regrab {
            self.next_regrab = now + Duration::from_secs(1);
            self.regrab = true;
            if let Err(e) = self.sync_cursor() {
                self.fail(event_loop, e);
                return;
            }
        }
        event_loop.set_control_flow(if period.is_some() {
            ControlFlow::WaitUntil(self.next_frame)
        } else if active {
            ControlFlow::Poll
        } else {
            ControlFlow::WaitUntil(self.next_tick)
        });
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.input(InputEvent::FocusLost);
        if self.graphics.is_some() {
            self.config.app.gpu_stopped();
            self.graphics = None;
        }
    }
}

/// The click that activates the window only refocuses the game, as in v20
/// whose DirectInput mouse is acquired after WM_ACTIVATE; it must not fire a
/// tool or place a brick. Clicking into a menu still passes through.
#[derive(Default)]
struct FocusClick {
    gained: Option<Instant>,
    swallowed: [bool; 5],
}

/// Windows delivers the activating button-down right after WM_SETFOCUS.
const FOCUS_CLICK_WINDOW: Duration = Duration::from_millis(250);

impl FocusClick {
    fn gained(&mut self, now: Instant) {
        self.gained = Some(now);
    }
    /// Whether to drop this button event: the first press shortly after focus
    /// arrives while the game captures the mouse, and that press's release.
    fn filter(
        &mut self,
        button: MouseButton,
        pressed: bool,
        now: Instant,
        capturing: bool,
    ) -> bool {
        let i = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
        };
        if pressed {
            let swallow = capturing
                && self
                    .gained
                    .take()
                    .is_some_and(|t| now.saturating_duration_since(t) <= FOCUS_CLICK_WINDOW);
            self.swallowed[i] = swallow;
            swallow
        } else {
            std::mem::take(&mut self.swallowed[i])
        }
    }
}

/// UI text uses composed layout text; binds deliberately use physical positions
/// so numpad identities survive NumLock and host keyboard layout differences.
pub fn translate_key(code: KeyCode, platform: Platform) -> Option<Key> {
    use KeyCode::*;
    Some(match code {
        KeyA => Key::Letter('a'),
        KeyB => Key::Letter('b'),
        KeyC => Key::Letter('c'),
        KeyD => Key::Letter('d'),
        KeyE => Key::Letter('e'),
        KeyF => Key::Letter('f'),
        KeyG => Key::Letter('g'),
        KeyH => Key::Letter('h'),
        KeyI => Key::Letter('i'),
        KeyJ => Key::Letter('j'),
        KeyK => Key::Letter('k'),
        KeyL => Key::Letter('l'),
        KeyM => Key::Letter('m'),
        KeyN => Key::Letter('n'),
        KeyO => Key::Letter('o'),
        KeyP => Key::Letter('p'),
        KeyQ => Key::Letter('q'),
        KeyR => Key::Letter('r'),
        KeyS => Key::Letter('s'),
        KeyT => Key::Letter('t'),
        KeyU => Key::Letter('u'),
        KeyV => Key::Letter('v'),
        KeyW => Key::Letter('w'),
        KeyX => Key::Letter('x'),
        KeyY => Key::Letter('y'),
        KeyZ => Key::Letter('z'),
        Digit0 => Key::Digit(0),
        Digit1 => Key::Digit(1),
        Digit2 => Key::Digit(2),
        Digit3 => Key::Digit(3),
        Digit4 => Key::Digit(4),
        Digit5 => Key::Digit(5),
        Digit6 => Key::Digit(6),
        Digit7 => Key::Digit(7),
        Digit8 => Key::Digit(8),
        Digit9 => Key::Digit(9),
        Numpad0 => Key::Numpad(0),
        Numpad1 => Key::Numpad(1),
        Numpad2 => Key::Numpad(2),
        Numpad3 => Key::Numpad(3),
        Numpad4 => Key::Numpad(4),
        Numpad5 => Key::Numpad(5),
        Numpad6 => Key::Numpad(6),
        Numpad7 => Key::Numpad(7),
        Numpad8 => Key::Numpad(8),
        Numpad9 => Key::Numpad(9),
        F1 => Key::F(1),
        F2 => Key::F(2),
        F3 => Key::F(3),
        F4 => Key::F(4),
        F5 => Key::F(5),
        F6 => Key::F(6),
        F7 => Key::F(7),
        F8 => Key::F(8),
        F9 => Key::F(9),
        F10 => Key::F(10),
        F11 => Key::F(11),
        F12 => Key::F(12),
        F13 => Key::F(13),
        F14 => Key::F(14),
        F15 => Key::F(15),
        F16 => Key::F(16),
        F17 => Key::F(17),
        F18 => Key::F(18),
        F19 => Key::F(19),
        F20 => Key::F(20),
        F21 => Key::F(21),
        F22 => Key::F(22),
        F23 => Key::F(23),
        F24 => Key::F(24),
        Escape => Key::Escape,
        Enter => Key::Return,
        NumpadEnter => Key::NumpadEnter,
        Tab => Key::Tab,
        Space => Key::Space,
        Backspace => Key::Backspace,
        Delete => Key::Delete,
        Insert => Key::Insert,
        Home => Key::Home,
        End => Key::End,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        ArrowUp => Key::Up,
        ArrowDown => Key::Down,
        ArrowLeft => Key::Left,
        ArrowRight => Key::Right,
        ShiftLeft => Key::LShift,
        ShiftRight => Key::RShift,
        ControlLeft => Key::LControl,
        ControlRight => Key::RControl,
        AltLeft => {
            if platform == Platform::MacOs {
                Key::LOpt
            } else {
                Key::LAlt
            }
        }
        AltRight => {
            if platform == Platform::MacOs {
                Key::ROpt
            } else {
                Key::RAlt
            }
        }
        Backquote => Key::Tilde,
        Minus => Key::Minus,
        Equal => Key::Equals,
        BracketLeft => Key::LBracket,
        BracketRight => Key::RBracket,
        Backslash | IntlBackslash => Key::Backslash,
        Semicolon => Key::Semicolon,
        Quote => Key::Apostrophe,
        Comma => Key::Comma,
        Period => Key::Period,
        Slash => Key::Slash,
        NumpadAdd => Key::NumpadAdd,
        NumpadSubtract => Key::NumpadMinus,
        NumpadMultiply => Key::NumpadMultiply,
        NumpadDivide => Key::NumpadDivide,
        NumpadDecimal | NumpadComma => Key::NumpadDecimal,
        CapsLock => Key::CapsLock,
        PrintScreen => Key::PrintScreen,
        Pause => Key::Pause,
        _ => return None,
    })
}
pub fn translate_modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        cmd: state.super_key(),
    }
}
fn translate_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    Some(match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        _ => return None,
    })
}
fn committed_chars(text: &str, mods: Modifiers) -> Vec<char> {
    if mods.cmd || (mods.ctrl && !mods.alt) {
        vec![]
    } else {
        text.chars().filter(|c| !c.is_control()).collect()
    }
}
fn pixel_wheel_steps(acc: &mut f64, delta: f64) -> f32 {
    if !delta.is_finite() {
        return 0.0;
    }
    *acc = (*acc + delta).clamp(-4000.0, 4000.0);
    let steps = (*acc / 40.0).trunc().clamp(-100.0, 100.0);
    *acc -= steps * 40.0;
    steps as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windowed_sizes_fit_inside_the_desktop_like_v20() {
        // A 1920 x 1080 monitor with a 16 x 39 window frame.
        let modes = [
            (640, 480),
            (1920, 1080),
            (1280, 720),
            (1280, 720),
            (1600, 900),
            (1904, 1000),
            (320, 200),
        ];
        let list = windowed_sizes(modes, (1904, 1041));
        assert_eq!(list, vec![(640, 480), (1280, 720), (1600, 900)]);
        assert!(fits(&list, (1280, 720)));
        assert!(fits(&list, (1000, 700)));
        assert!(!fits(&list, (1920, 1080)));
        assert!(fits(&[], (1920, 1080)));
    }
    #[test]
    fn activating_click_only_refocuses_the_game() {
        let t = Instant::now();
        let soon = t + Duration::from_millis(5);
        let mut f = FocusClick::default();
        f.gained(t);
        assert!(f.filter(MouseButton::Left, true, soon, true));
        assert!(f.filter(MouseButton::Left, false, soon, true));
        // Later clicks fire normally.
        assert!(!f.filter(MouseButton::Left, true, soon, true));
        assert!(!f.filter(MouseButton::Left, false, soon, true));
        // Alt+Tab back, then a deliberate click later, is not swallowed.
        f.gained(t);
        let late = t + FOCUS_CLICK_WINDOW + Duration::from_millis(1);
        assert!(!f.filter(MouseButton::Right, true, late, true));
        assert!(!f.filter(MouseButton::Right, false, late, true));
        // Clicking into a menu keeps the click.
        f.gained(t);
        assert!(!f.filter(MouseButton::Left, true, soon, false));
        assert!(!f.filter(MouseButton::Left, false, soon, false));
    }
    #[test]
    fn physical_key_mapping_preserves_numpad_and_platform_option() {
        assert_eq!(
            translate_key(KeyCode::Numpad8, Platform::Windows),
            Some(Key::Numpad(8))
        );
        assert_eq!(
            translate_key(KeyCode::Digit8, Platform::Windows),
            Some(Key::Digit(8))
        );
        assert_eq!(
            translate_key(KeyCode::AltLeft, Platform::MacOs),
            Some(Key::LOpt)
        );
        assert_eq!(
            translate_key(KeyCode::AltLeft, Platform::Linux),
            Some(Key::LAlt)
        );
        assert_eq!(
            translate_key(KeyCode::F24, Platform::Windows),
            Some(Key::F(24))
        );
        assert_eq!(translate_key(KeyCode::SuperLeft, Platform::Windows), None);
    }
    #[test]
    fn text_composition_filters_controls_shortcuts_but_allows_altgr() {
        assert_eq!(
            committed_chars("é好\r\n\t", Modifiers::NONE),
            vec!['é', '好']
        );
        assert!(
            committed_chars(
                "c",
                Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                }
            )
            .is_empty()
        );
        assert_eq!(
            committed_chars(
                "@",
                Modifiers {
                    ctrl: true,
                    alt: true,
                    ..Modifiers::NONE
                }
            ),
            vec!['@']
        );
        assert_eq!(
            translate_modifiers(ModifiersState::SHIFT | ModifiersState::SUPER),
            Modifiers {
                shift: true,
                cmd: true,
                ..Modifiers::NONE
            }
        );
    }
    #[test]
    fn high_resolution_wheel_accumulates_and_invalid_deltas_do_not_poison() {
        let mut acc = 0.0;
        assert_eq!(pixel_wheel_steps(&mut acc, 12.0), 0.0);
        assert_eq!(pixel_wheel_steps(&mut acc, 28.0), 1.0);
        assert_eq!(pixel_wheel_steps(&mut acc, f64::NAN), 0.0);
        assert_eq!(pixel_wheel_steps(&mut acc, -80.0), -2.0);
        assert_eq!(acc, 0.0);
    }
    #[test]
    fn unsupported_no_vsync_is_explicit() {
        assert!(present_mode(false, &[wgpu::PresentMode::Fifo]).is_err());
        assert_eq!(
            present_mode(true, &[wgpu::PresentMode::Fifo]).unwrap(),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            present_mode(
                false,
                &[wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate]
            )
            .unwrap(),
            wgpu::PresentMode::Immediate
        );
    }
}
