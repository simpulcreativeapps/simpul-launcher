// Isi bersama untuk dua program: jendela (`main.rs`) dan CLI konsol (`console.rs`).

mod account;
mod cli;
mod icons;
mod models;
mod registry;
mod suite;
mod titlebar;
mod ui;

use std::env;
use std::path::PathBuf;

use eframe::egui;

use crate::cli::{parse_and_dispatch, CliDisposition};
use crate::models::{
    DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, FEED_RATE_LIMIT_COOLDOWN_SECS,
    FEED_RETRY_MIN_BACKOFF_SECS, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH,
};
use crate::ui::SimpulLauncherApp;

const SIMPUL_ICON_PNG: &[u8] = include_bytes!("../assets/simpul-icon.png");

#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // Gagal berarti tidak ada konsol pemanggil (dibuka dari Start menu): tidak masalah.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() {
    let _ = (FEED_RETRY_MIN_BACKOFF_SECS, FEED_RATE_LIMIT_COOLDOWN_SECS);
    #[cfg(windows)]
    attach_parent_console();

    let args: Vec<String> = env::args().skip(1).collect();
    match parse_and_dispatch(&args) {
        CliDisposition::ExitCode(code) => std::process::exit(code),
        CliDisposition::RunGui { screenshot_path } => {
            // Program CLI tanpa argumen membuka program jendela di sebelahnya, supaya
            // tidak ada terminal yang tertinggal terbuka.
            if CONSOLE_BUILD && screenshot_path.is_none() {
                if let Some(gui) = env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("Simpul Creative Launcher.exe")))
                    .filter(|p| p.exists())
                {
                    if std::process::Command::new(gui).spawn().is_ok() {
                        return;
                    }
                }
            }
            if let Err(err) = run_desktop_window(screenshot_path) {
                eprintln!("Simpul Creative Launcher window error: {err}");
                std::process::exit(1);
            }
        }
    }
}

fn run_desktop_window(screenshot_path: Option<PathBuf>) -> eframe::Result<()> {
    let is_screenshot = screenshot_path.is_some();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Simpul — Creative Apps Launcher")
        .with_app_id("id.simpul.creative-launcher")
        .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT])
        .with_icon(build_window_icon());

    if is_screenshot {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    if is_screenshot {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        native_options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "Simpul — Creative Apps Launcher",
        native_options,
        Box::new(move |cc| Ok(Box::new(SimpulLauncherApp::new(&cc.egui_ctx, screenshot_path)))),
    )
}

fn build_window_icon() -> egui::IconData {
    if let Ok(dyn_img) = image::load_from_memory(SIMPUL_ICON_PNG) {
        let resized = dyn_img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (width, height) = rgba.dimensions();
        return egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        };
    }

    egui::IconData {
        rgba: vec![78, 123, 251, 255],
        width: 1,
        height: 1,
    }
}
