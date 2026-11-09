//! Buddy: a cube that peeks from the screen edges, keeps an OLED monitor
//! from dimming, and (soon) tells you about pending Claude Code work.

// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod cube;
mod geometry;
mod gpu;
mod palette;
mod peek;
#[cfg(target_os = "windows")]
mod platform_windows;
mod settings;
mod tray;

use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("buddy=info"))
        .init();

    let settings_path = settings::default_path();
    let settings = settings_path
        .as_deref()
        .map(settings::Settings::load_from)
        .unwrap_or_default();
    log::info!("settings file: {settings_path:?}");

    let mut builder = EventLoop::<app::UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // Menu bar icon only: no Dock icon, no app menu.
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build()?;
    tray::forward_events(
        event_loop.create_proxy(),
        app::UserEvent::Menu,
        app::UserEvent::Hotkey,
    );

    let mut app = app::App::new(settings, settings_path)?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
