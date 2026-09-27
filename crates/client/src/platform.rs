//! Native window and GPU ownership. Nothing creates a window until `run` is
//! explicitly called by the executable. Mapping tests never start an event loop.
use anyhow::{Context, Result, bail};
use bri_ui::api::{RequestId, UiUpdate};
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
    /// Return true after clearing/rendering a scene; false asks the platform to
    /// clear to its neutral background before compositing UI.
    fn render_scene(&mut self, _frame: &mut RenderContext<'_>) -> Result<bool> {
        Ok(false)
    }
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
    device_lost: Arc<Mutex<Option<String>>>,
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
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance
            .create_surface(window.clone())
            .context("creating the native render surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .context("finding a GPU for the native window")?;
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
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .context("creating the native GPU device")?;
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
                eprintln!("Saved display mode unavailable: {error}; starting with VSync.");
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
            device_lost,
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
    request: RequestId,
    expected: PhysicalSize<u32>,
    deadline: Instant,
    old_size: PhysicalSize<u32>,
    old_fullscreen: Option<Fullscreen>,
    old_vsync: bool,
}

struct Runner {
    config: PlatformConfig,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    focused: bool,
    occluded: bool,
    grabbed: bool,
    ime_allowed: bool,
    composing: bool,
    mods: Modifiers,
    cursor: PhysicalPosition<f64>,
    wheel_pixels: f64,
    last_tick: Instant,
    next_tick: Instant,
    display: Option<DisplayChange>,
    error: Option<anyhow::Error>,
    screenshot: Option<(std::path::PathBuf, bool)>,
}

/// Launch only from an explicitly requested interactive execution path. This
/// function blocks until the native window closes; headless clients need not use it.
pub fn run(config: PlatformConfig) -> Result<()> {
    if config.size.0 < 640 || config.size.1 < 480 {
        bail!("Native client requires at least 640 x 480 pixels");
    }
    let now = Instant::now();
    let mut runner = Runner {
        config,
        window: None,
        graphics: None,
        focused: false,
        occluded: false,
        grabbed: false,
        ime_allowed: false,
        composing: false,
        mods: Modifiers::NONE,
        cursor: PhysicalPosition::new(0.0, 0.0),
        wheel_pixels: 0.0,
        last_tick: now,
        next_tick: now,
        display: None,
        error: None,
        screenshot: None,
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
        self.config.app.ui_mut().handle_input(event);
    }
    fn focused_text(&self) -> bool {
        text_focused(self.config.app.ui())
    }
    fn sync_cursor(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let grab = self.focused && !self.config.app.ui().cursor_visible();
        if grab != self.grabbed {
            if grab {
                window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .context("capturing the gameplay cursor")?;
            } else {
                window
                    .set_cursor_grab(CursorGrabMode::None)
                    .context("releasing the gameplay cursor")?;
            }
            self.grabbed = grab;
        }
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
                && let Some(n) = screen.view().focus
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
            let scale = self.config.app.ui().config().scale;
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
        let target = PhysicalSize::new(resolution.0, resolution.1);
        let fs = if fullscreen {
            let monitor = window
                .current_monitor()
                .context("No current monitor is available")?;
            let video = monitor
                .video_modes()
                .filter(|m| m.size() == target)
                .max_by_key(|m| m.refresh_rate_millihertz())
                .context(
                    "Requested fullscreen resolution is not supported by the current monitor",
                )?;
            Some(Fullscreen::Exclusive(video))
        } else {
            None
        };
        self.display = Some(DisplayChange {
            request,
            expected: target,
            deadline: Instant::now() + Duration::from_secs(5),
            old_size: window.inner_size(),
            old_fullscreen: window.fullscreen(),
            old_vsync: self.config.vsync,
        });
        window.set_fullscreen(fs);
        if !fullscreen {
            let _ = window.request_inner_size(target);
        }
        self.config.vsync = vsync;
        graphics.config.present_mode = mode;
        let actual = window.inner_size();
        self.resize(actual);
        Ok(())
    }
    fn finish_display(&mut self) {
        let Some(change) = self.display.as_ref() else {
            return;
        };
        let actual = self.window.as_ref().map(|w| w.inner_size());
        if actual == Some(change.expected) && self.graphics.is_some() {
            let change = self.display.take().unwrap();
            self.config.size = (change.expected.width, change.expected.height);
            self.config.fullscreen = self
                .window
                .as_ref()
                .is_some_and(|w| w.fullscreen().is_some());
            self.config.app.ui_mut().apply(UiUpdate::ActionResult {
                id: change.request,
                result: Ok(()),
            });
        } else if Instant::now() >= change.deadline {
            let change = self.display.take().unwrap();
            self.config.vsync = change.old_vsync;
            if let Some(window) = &self.window {
                window.set_fullscreen(change.old_fullscreen);
                let _ = window.request_inner_size(change.old_size);
            }
            if let Some(g) = &mut self.graphics
                && let Ok(mode) = present_mode(change.old_vsync, &g.present_modes)
            {
                g.config.present_mode = mode;
            }
            if let Some(size) = self.window.as_ref().map(|w| w.inner_size()) {
                self.resize(size);
            }
            self.reject_display(change.request,"The requested resolution was not observed; reverting to the previous display settings.".into());
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
                    PlatformCommand::ToggleFullscreen => {
                        if self.display.is_none()
                            && let Some(w) = &self.window
                        {
                            w.set_fullscreen(if w.fullscreen().is_some() {
                                None
                            } else {
                                Some(Fullscreen::Borderless(w.current_monitor()))
                            });
                        }
                    }
                    PlatformCommand::Screenshot { path, hud } => {
                        self.screenshot = Some((path, hud));
                    }
                }
            }
        }
        self.sync_cursor()
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
            bail!("The native GPU device was lost: {reason}");
        }
        if g.reconfigure {
            g.resize(size);
        }
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
        let target = surface.texture.create_view(&Default::default());
        let mut encoder = g
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bri-client frame"),
            });
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
        g.queue.submit([encoder.finish()]);
        if let Some((path, capture)) = scene_capture.or(hud_capture) {
            let saved = capture.save(&g.device, &path);
            let text = match &saved {
                Ok(()) => format!(
                    "Screenshot saved: {}",
                    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into())
                ),
                Err(error) => format!("Screenshot failed: {error:#}"),
            };
            self.config.app.ui_mut().apply(bri_ui::api::UiUpdate::BottomPrint {
                text,
                seconds: 3.0,
                hide_bar: false,
            });
        }
        window.pre_present_notify();
        g.queue.present(surface);
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
impl Capture {
    fn save(self, device: &wgpu::Device, path: &std::path::Path) -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })?;
        rx.recv_timeout(Duration::from_secs(5))??;
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
        image::save_buffer(path, &pixels, self.width, self.height, image::ColorType::Rgba8)?;
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
                if self.config.fullscreen {
                    let target = PhysicalSize::new(self.config.size.0, self.config.size.1);
                    let video = window.current_monitor().and_then(|monitor| {
                        monitor
                            .video_modes()
                            .filter(|mode| mode.size() == target)
                            .max_by_key(|mode| mode.refresh_rate_millihertz())
                    });
                    if let Some(video) = video {
                        window.set_fullscreen(Some(Fullscreen::Exclusive(video)));
                    } else {
                        self.config.fullscreen = false;
                        eprintln!("Saved fullscreen resolution is unavailable; starting windowed.");
                    }
                }
                self.focused = window.has_focus();
                self.window = Some(window);
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
            self.config.fullscreen = change.old_fullscreen.is_some();
            if let Some(window) = &self.window {
                window.set_fullscreen(change.old_fullscreen);
                let _ = window.request_inner_size(change.old_size);
            }
            self.reject_display(
                change.request,
                "Display change interrupted by suspension.".into(),
            );
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = &self.window {
                    self.resize(w.inner_size());
                }
            }
            WindowEvent::Occluded(hidden) => self.occluded = hidden,
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
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
                if let Some(button) = translate_button(button) {
                    let (x, y) = (self.cursor.x as f32, self.cursor.y as f32);
                    self.input(if state == ElementState::Pressed {
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
        if active || now >= self.next_tick {
            let elapsed = now.saturating_duration_since(self.last_tick);
            self.last_tick = now;
            // Avoid minutes of UI repeat catch-up after suspension/debug pauses.
            self.config
                .app
                .ui_mut()
                .update(elapsed.as_millis().min(250) as u64);
            if let Err(e) = self.config.app.tick(elapsed) {
                self.fail(event_loop, e);
                return;
            }
            self.finish_display();
            if let Err(e) = self.pump(event_loop) {
                self.fail(event_loop, e);
                return;
            }
            if let Some(w) = &self.window
                && !self.occluded
                && self.graphics.is_some()
            {
                w.request_redraw();
            }
            self.next_tick = now + Duration::from_millis(if self.focused { 16 } else { 50 });
        }
        event_loop.set_control_flow(if active {
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

fn text_focused(ui: &Ui) -> bool {
    ui.screen(ui.top_id()).is_some_and(|s| {
        s.view().focus.is_some_and(|n| {
            matches!(
                s.view().node(n).ctrl.class.as_str(),
                "GuiTextEditCtrl" | "GuiMLTextEditCtrl"
            )
        })
    })
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
