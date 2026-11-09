//! The winit application: one transparent, always-on-top, click-through
//! window that shows the cube peeking from a screen edge.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vello::Scene;
use vello::kurbo::Affine;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::cube;
use crate::geometry::{self, Rect};
use crate::gpu::{Gpu, WindowSurface};
use crate::peek;
use crate::settings::{Mode, Settings};
use crate::tray::{self, Command, Tray};

#[derive(Debug)]
pub enum UserEvent {
    Menu(String),
    Hotkey(u32),
}

pub struct App {
    settings: Settings,
    settings_path: Option<PathBuf>,
    /// Mode to go back to when the shortcut turns Off off again.
    last_on_mode: Mode,
    gpu: Gpu,
    window: Option<Arc<Window>>,
    surface: Option<WindowSurface>,
    tray: Option<Tray>,
    tree: vello_svg::usvg::Tree,
    scene: Scene,
    /// When the current peek started, if one is playing.
    peek_started: Option<Instant>,
    next_peek: Instant,
    /// Position along the edge, 0.0–1.0; changes before every peek.
    along: f32,
    rng: u64,
    reveal: f32,
}

impl App {
    pub fn new(settings: Settings, settings_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let tree = cube::parse(&cube::idle_svg(settings.frame_color))?;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            | 1;
        let last_on_mode = if settings.mode == Mode::Off {
            Mode::On
        } else {
            settings.mode
        };
        Ok(Self {
            next_peek: Instant::now() + Duration::from_secs(3),
            settings,
            settings_path,
            last_on_mode,
            gpu: Gpu::new(),
            window: None,
            surface: None,
            tray: None,
            tree,
            scene: Scene::new(),
            peek_started: None,
            along: 0.5,
            rng: seed,
            reveal: 0.0,
        })
    }

    fn random(&mut self) -> f32 {
        // xorshift64: plenty for picking a spot along the edge.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn save(&self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save_to(path)
        {
            log::warn!("saving settings: {e}");
        }
    }

    fn monitor_rect(&self) -> Option<(Rect, f64)> {
        let window = self.window.as_ref()?;
        let monitor = window
            .primary_monitor()
            .or_else(|| window.current_monitor())
            .or_else(|| window.available_monitors().next())?;
        let pos = monitor.position();
        let size = monitor.size();
        Some((
            Rect {
                x: pos.x,
                y: pos.y,
                w: size.width,
                h: size.height,
            },
            monitor.scale_factor(),
        ))
    }

    /// Size and move the window to the configured edge.
    fn place_window(&mut self) {
        let Some((monitor, scale)) = self.monitor_rect() else {
            return;
        };
        let Some(window) = &self.window else { return };
        let size = geometry::window_size(self.settings.cube_size, scale);
        let rect = geometry::window_rect(monitor, size, self.settings.edge, self.along);
        let _ = window.request_inner_size(PhysicalSize::new(rect.w, rect.h));
        window.set_outer_position(PhysicalPosition::new(rect.x, rect.y));
        if let Some(surface) = &mut self.surface {
            self.gpu.resize(surface, rect.w, rect.h);
        }
    }

    fn apply_mode(&mut self) {
        if let Some(window) = &self.window {
            window.set_visible(self.settings.mode != Mode::Off);
        }
        if self.settings.mode != Mode::On {
            self.peek_started = None;
            self.reveal = 0.0;
        }
        self.reschedule();
        self.request_redraw();
    }

    fn reschedule(&mut self) {
        self.next_peek = Instant::now() + Duration::from_secs(self.settings.idle_interval_secs);
    }

    fn start_peek(&mut self) {
        if self.settings.mode != Mode::On {
            return;
        }
        // Each peek comes out at a new spot, so the movement covers more of
        // the panel.
        self.along = 0.1 + 0.8 * self.random();
        self.place_window();
        self.peek_started = Some(Instant::now());
        self.reschedule();
        self.request_redraw();
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn handle(&mut self, cmd: Command, event_loop: &ActiveEventLoop) {
        log::info!("command {cmd:?}");
        match cmd {
            Command::SetMode(mode) => {
                self.settings.mode = mode;
                if mode != Mode::Off {
                    self.last_on_mode = mode;
                }
                self.apply_mode();
            }
            Command::ToggleOff => {
                self.settings.mode = if self.settings.mode == Mode::Off {
                    self.last_on_mode
                } else {
                    Mode::Off
                };
                self.apply_mode();
            }
            Command::PeekNow => self.start_peek(),
            Command::SetInterval(secs) => {
                self.settings.idle_interval_secs = secs;
                self.settings = self.settings.clone().sanitized();
                self.reschedule();
            }
            Command::SetSize(px) => {
                self.settings.cube_size = px;
                self.settings = self.settings.clone().sanitized();
                self.place_window();
                self.request_redraw();
            }
            Command::SetEdge(edge) => {
                self.settings.edge = edge;
                self.place_window();
                self.start_peek();
            }
            Command::SetColor(color) => {
                self.settings.frame_color = color;
                match cube::parse(&cube::idle_svg(color)) {
                    Ok(tree) => self.tree = tree,
                    Err(e) => log::error!("cube svg: {e}"),
                }
                if let Some(t) = &mut self.tray {
                    t.set_color(color);
                }
                self.request_redraw();
            }
            Command::OpenSettingsFile => {
                self.save();
                if let Some(path) = &self.settings_path {
                    open_in_editor(path);
                }
            }
            Command::Quit => {
                self.save();
                event_loop.exit();
                return;
            }
        }
        self.save();
        if let Some(t) = &mut self.tray {
            t.sync(&self.settings);
        }
    }

    fn render(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let scale = geometry::svg_scale(self.settings.cube_size, window.scale_factor());
        let (dx, dy) = geometry::cube_offset(self.settings.edge, self.reveal);
        self.scene.reset();
        vello_svg::append_tree_with_transform(
            &mut self.scene,
            &self.tree,
            Affine::scale(scale) * Affine::translate((dx as f64, dy as f64)),
        );
        if let Err(e) = self.gpu.render(surface, &self.scene) {
            log::error!("render: {e}");
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title("Buddy")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(PhysicalSize::new(400, 300));
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            // No taskbar button; no redirection bitmap so DirectComposition
            // can show per-pixel alpha.
            attrs = attrs
                .with_skip_taskbar(true)
                .with_no_redirection_bitmap(true);
        }
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs.with_has_shadow(false);
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("creating window: {e}");
                event_loop.exit();
                return;
            }
        };
        // Click-through: the cube never blocks clicks on what is behind it.
        if let Err(e) = window.set_cursor_hittest(false) {
            log::warn!("click-through not available: {e}");
        }
        #[cfg(target_os = "windows")]
        crate::platform_windows::keep_layered_window_visible(&window);

        self.window = Some(window.clone());
        match self.gpu.create_surface(window.clone()) {
            Ok(surface) => {
                log::info!("surface: {}", surface.info);
                self.surface = Some(surface);
            }
            Err(e) => {
                log::error!("GPU setup failed: {e:#}");
                event_loop.exit();
                return;
            }
        }
        self.place_window();

        match Tray::new(&self.settings) {
            Ok(t) => self.tray = Some(t),
            Err(e) => log::error!("tray: {e:#}"),
        }

        self.render();
        self.apply_mode();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let cmd = match event {
            UserEvent::Menu(id) => tray::parse_menu_id(&id),
            UserEvent::Hotkey(id) => self
                .tray
                .as_ref()
                .filter(|t| t.is_toggle_hotkey(id))
                .map(|_| Command::ToggleOff),
        };
        if let Some(cmd) = cmd {
            self.handle(cmd, event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(surface) = &mut self.surface {
                    self.gpu.resize(surface, size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.place_window();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(start) = self.peek_started {
                    match peek::reveal_at(start.elapsed().as_secs_f32()) {
                        Some(r) => self.reveal = r,
                        None => {
                            self.peek_started = None;
                            self.reveal = 0.0;
                        }
                    }
                }
                self.render();
                if self.peek_started.is_some() {
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.settings.mode == Mode::On
            && self.peek_started.is_none()
            && Instant::now() >= self.next_peek
        {
            self.start_peek();
        }
        if self.peek_started.is_some() {
            // Redraws are paced by vsync while the cube moves.
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if self.settings.mode == Mode::On {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_peek));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn open_in_editor(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-t")
        .arg(path)
        .spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("notepad").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        log::warn!("opening {}: {e}", path.display());
    }
}
