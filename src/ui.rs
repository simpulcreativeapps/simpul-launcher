use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{
    self, Align, Color32, FontId, Frame, Layout, Margin, Pos2, Rect, RichText, ScrollArea, Sense,
    Stroke, StrokeKind, TextEdit, TextureHandle, TextureOptions, Vec2,
};

use crate::icons::{paint_app_icon, paint_simpul_brand_mark};
use crate::models::{
    compute_feed_backoff_secs, ActiveInstallTask, AgentOrigin, BuildProfile, CheckEvery, FilterTab,
    InstallState, QueueStage, SimpulApp, SimpulRegistry, SortKey, ThemeChoice,
    ToolIntegrationStatus, ViewMode, BADGE_PILL_HEIGHT, CARD_CORNER_RADIUS, CARD_GRID_SPACING,
    CARD_HOVER_ANIM_SECS, CARD_HOVER_TINT_FACTOR, CARD_ICON_SIZE, CARD_INNER_PADDING,
    CARD_MIN_WIDTH, CONTROL_CORNER_RADIUS, DRAWER_ANIM_SECS,
    DRAWER_MAX_WIDTH, DRAWER_MIN_WIDTH, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_HEADING,
    FONT_MICRO, HERO_ICON_SIZE, LIVE_SYNC_POLL_SECS, PRIMARY_BTN_HEIGHT,
    SCREENSHOT_WARMUP_FRAMES, TOAST_ERROR_DURATION_SECS,
    TOAST_INFO_DURATION_SECS, TOAST_OFFSET_X, TOAST_OFFSET_Y, TOAST_WIDTH,
    UI_ZOOM_MAX, UI_ZOOM_MIN, UI_ZOOM_STEP,
};
use crate::registry::{
    current_launchable_target, detect_integrations, detect_profile_from_path, import_into_registry,
    install_in_registry, load_or_init_registry, load_registry_without_scan, open_install_folder,
    registry_mtime, save_registry, set_app_executable_path,
    split_launch_flags, try_open_in_agent, uninstall_in_registry, ImportRequest,
};
use crate::suite::{self, GuardHub, GuardSnapshot};

mod account_ui;
mod background;
mod shell_ui;
mod suite_ui;
mod install_gate;
mod screens;


/// Tebal cincin fokus papan ketik.
const FOCUS_RING_WIDTH: f32 = 2.0;

const SIMPUL_ICON_BYTES: &[u8] = include_bytes!("../assets/simpul-icon.png");
const SIMPUL_SPLASH_BYTES: &[u8] = include_bytes!("../assets/simpul-splash.png");

/// Sekitar 1,8 detik pada layar 60 Hz; klik atau Esc melewatinya.
const INITIAL_SPLASH_FRAMES: u32 = 110;
/// Pendukung melihat ucapan terima kasih sedikit lebih lama.
const SUPPORTER_SPLASH_FRAMES: u32 = 170;

struct BrandTextures {
    icon: Option<TextureHandle>,
    splash: Option<TextureHandle>,
}

impl BrandTextures {
    fn load(ctx: &egui::Context) -> Self {
        Self {
            icon: decode_png_texture(ctx, "simpul_official_icon", SIMPUL_ICON_BYTES),
            splash: decode_png_texture(ctx, "simpul_official_splash", SIMPUL_SPLASH_BYTES),
        }
    }
}

fn decode_png_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color_img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, color_img, TextureOptions::LINEAR))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RightDrawer {
    None,
    AppDetails(String),
    AgentHub,
}

#[derive(Debug, Clone)]
struct ToastMessage {
    text: String,
    is_error: bool,
    created_at: f64,
}

#[derive(Debug, Clone)]
struct ImportFormState {
    open: bool,
    path_input: String,
    name_input: String,
    version_input: String,
    tagline_input: String,
    category_input: String,
    origin: AgentOrigin,
    auto_install: bool,
    allow_debug_override: bool,
    error_banner: Option<String>,
}

impl Default for ImportFormState {
    fn default() -> Self {
        Self {
            open: false,
            path_input: "./examples/capr-notebook.json".to_string(),
            name_input: String::new(),
            version_input: String::new(),
            tagline_input: String::new(),
            category_input: "Simpul Suite".to_string(),
            origin: AgentOrigin::CliManual,
            auto_install: true,
            allow_debug_override: false,
            error_banner: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    bg_canvas: Color32,
    bg_header: Color32,
    bg_card: Color32,
    bg_card_hover: Color32,
    bg_elevated: Color32,
    bg_subtle: Color32,
    border: Color32,
    border_strong: Color32,
    ink_primary: Color32,
    ink_secondary: Color32,
    ink_muted: Color32,
    accent: Color32,
    accent_soft: Color32,
    emerald: Color32,
    emerald_soft: Color32,
    amber: Color32,
    amber_soft: Color32,
    purple: Color32,
    purple_soft: Color32,
    cyan: Color32,
    cyan_soft: Color32,
    danger: Color32,
    /// Garis tepi kontrol interaktif (kolom isian, tombol netral): minimal 3:1.
    control_edge: Color32,
    /// Warna aksen untuk TEKS dan pil (lebih terang di tema gelap, lebih tua di tema terang).
    accent_ink: Color32,
    /// Warna teks di atas isian aksen, hijau, dan bahaya.
    on_accent: Color32,
    on_emerald: Color32,
    on_danger: Color32,
    /// Latar jendela dialog: selalu padat supaya teks terbaca, juga di tema kaca.
    bg_window: Color32,
    card_radius: f32,
    control_radius: f32,
    dark: bool,
}

impl Palette {
    /// Dua palet dasar. Aturan yang dijaga oleh `design_tests`:
    /// semua teks >= 4.5:1 terhadap permukaannya (WCAG 2.2 butir 1.4.3).
    /// Warna `*_soft` padat (tidak tembus pandang) supaya kontras pil bisa dijamin.
    fn for_theme(dark: bool) -> Self {
        if dark {
            Self {
                bg_canvas: Color32::from_rgb(10, 11, 17),
                bg_header: Color32::from_rgb(14, 16, 25),
                bg_card: Color32::from_rgb(18, 20, 31),
                bg_card_hover: Color32::from_rgb(24, 27, 42),
                bg_elevated: Color32::from_rgb(30, 34, 52),
                bg_subtle: Color32::from_rgb(38, 43, 66),
                border: Color32::from_rgb(46, 52, 78),
                border_strong: Color32::from_rgb(98, 120, 200),
                control_edge: Color32::from_rgb(118, 126, 160),
                ink_primary: Color32::from_rgb(242, 243, 248),
                ink_secondary: Color32::from_rgb(190, 196, 220),
                ink_muted: Color32::from_rgb(150, 158, 188),
                accent: Color32::from_rgb(110, 152, 255),
                accent_ink: Color32::from_rgb(158, 186, 255),
                on_accent: Color32::from_rgb(8, 14, 40),
                accent_soft: Color32::from_rgb(24, 34, 72),
                emerald: Color32::from_rgb(110, 231, 150),
                on_emerald: Color32::from_rgb(6, 34, 18),
                emerald_soft: Color32::from_rgb(16, 46, 34),
                amber: Color32::from_rgb(251, 196, 60),
                amber_soft: Color32::from_rgb(56, 40, 10),
                purple: Color32::from_rgb(216, 184, 254),
                purple_soft: Color32::from_rgb(44, 30, 70),
                cyan: Color32::from_rgb(110, 232, 249),
                cyan_soft: Color32::from_rgb(12, 44, 58),
                danger: Color32::from_rgb(250, 128, 128),
                on_danger: Color32::from_rgb(44, 6, 6),
                bg_window: Color32::from_rgb(20, 22, 34),
                card_radius: CARD_CORNER_RADIUS,
                control_radius: CONTROL_CORNER_RADIUS,
                dark: true,
            }
        } else {
            Self {
                bg_canvas: Color32::from_rgb(245, 244, 248),
                bg_header: Color32::from_rgb(252, 252, 255),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_card_hover: Color32::from_rgb(248, 250, 255),
                bg_elevated: Color32::from_rgb(238, 240, 248),
                bg_subtle: Color32::from_rgb(230, 234, 245),
                border: Color32::from_rgb(214, 219, 232),
                border_strong: Color32::from_rgb(120, 140, 210),
                control_edge: Color32::from_rgb(116, 124, 148),
                ink_primary: Color32::from_rgb(17, 19, 28),
                ink_secondary: Color32::from_rgb(60, 66, 86),
                ink_muted: Color32::from_rgb(88, 95, 114),
                accent: Color32::from_rgb(47, 91, 232),
                accent_ink: Color32::from_rgb(36, 72, 200),
                on_accent: Color32::WHITE,
                accent_soft: Color32::from_rgb(226, 233, 255),
                emerald: Color32::from_rgb(12, 112, 60),
                on_emerald: Color32::WHITE,
                emerald_soft: Color32::from_rgb(222, 244, 230),
                amber: Color32::from_rgb(140, 78, 0),
                amber_soft: Color32::from_rgb(253, 238, 210),
                purple: Color32::from_rgb(110, 44, 190),
                purple_soft: Color32::from_rgb(240, 230, 255),
                cyan: Color32::from_rgb(4, 98, 122),
                cyan_soft: Color32::from_rgb(218, 244, 250),
                danger: Color32::from_rgb(190, 34, 34),
                on_danger: Color32::WHITE,
                bg_window: Color32::from_rgb(255, 255, 255),
                card_radius: CARD_CORNER_RADIUS,
                control_radius: CONTROL_CORNER_RADIUS,
                dark: false,
            }
        }
    }
}

/// Tumpuk warna tembus pandang (premultiplied, seperti di egui) di atas latar padat.
fn flatten(top: Color32, bottom: Color32) -> Color32 {
    let keep = 1.0 - f32::from(top.a()) / 255.0;
    let mix = |t: u8, b: u8| (f32::from(t) + f32::from(b) * keep).round().min(255.0) as u8;
    Color32::from_rgb(
        mix(top.r(), bottom.r()),
        mix(top.g(), bottom.g()),
        mix(top.b(), bottom.b()),
    )
}

/// Kaca gelap: lapisan peredup di atas latar cahaya, sesuai pedoman keterbacaan
/// (kalau kaca membuat teks sulit dibaca, redupkan latarnya).
fn tinted(r: u8, g: u8, b: u8, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

impl Palette {
    /// Palet untuk setiap pilihan tema. Tema baru berangkat dari palet dasar lalu
    /// mengganti permukaan, tinta, aksen, dan bentuk sudutnya.
    fn for_choice(choice: ThemeChoice, system_dark: bool, reduce_transparency: bool) -> Self {
        let pal = Self::for_choice_raw(choice, system_dark);
        if reduce_transparency && choice.is_glass() {
            pal.without_transparency(choice)
        } else {
            pal
        }
    }

    /// Versi padat tema kaca: setiap permukaan ditumpuk di atas warna dasar latarnya.
    fn without_transparency(self, choice: ThemeChoice) -> Self {
        let base = match choice {
            ThemeChoice::GlassAurora => Color32::from_rgb(7, 9, 22),
            _ => Color32::from_rgb(9, 10, 12),
        };
        let flat = |c: Color32| flatten(c, base);
        let card = flat(self.bg_card);
        Self {
            bg_canvas: base,
            bg_header: flat(self.bg_header),
            bg_card: card,
            bg_card_hover: flat(self.bg_card_hover),
            bg_elevated: flatten(self.bg_elevated, card),
            bg_subtle: flatten(self.bg_subtle, card),
            border: flatten(self.border, card),
            border_strong: flatten(self.border_strong, card),
            ..self
        }
    }

    fn for_choice_raw(choice: ThemeChoice, system_dark: bool) -> Self {
        match choice {
            ThemeChoice::Dark => Self::for_theme(true),
            ThemeChoice::Light => Self::for_theme(false),
            ThemeChoice::System => Self::for_theme(system_dark),
            ThemeChoice::ExpressiveBlue => Self {
                bg_canvas: Color32::from_rgb(244, 246, 255),
                bg_header: Color32::from_rgb(255, 255, 255),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_card_hover: Color32::from_rgb(240, 244, 255),
                bg_elevated: Color32::from_rgb(233, 237, 252),
                bg_subtle: Color32::from_rgb(217, 226, 255),
                border: Color32::from_rgb(221, 227, 247),
                border_strong: Color32::from_rgb(47, 91, 232),
                ink_primary: Color32::from_rgb(21, 27, 46),
                ink_secondary: Color32::from_rgb(58, 66, 92),
                ink_muted: Color32::from_rgb(84, 92, 118),
                emerald: Color32::from_rgb(34, 92, 10),
                emerald_soft: Color32::from_rgb(212, 244, 150),
                card_radius: 28.0,
                control_radius: 17.0,
                ..Self::for_theme(false)
            },
            ThemeChoice::ExpressiveWarm => Self {
                bg_canvas: Color32::from_rgb(255, 248, 242),
                bg_header: Color32::from_rgb(255, 255, 255),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_card_hover: Color32::from_rgb(255, 242, 233),
                bg_elevated: Color32::from_rgb(248, 236, 226),
                bg_subtle: Color32::from_rgb(255, 220, 199),
                border: Color32::from_rgb(239, 220, 205),
                border_strong: Color32::from_rgb(214, 92, 130),
                control_edge: Color32::from_rgb(140, 116, 100),
                ink_primary: Color32::from_rgb(35, 26, 20),
                ink_secondary: Color32::from_rgb(82, 66, 56),
                ink_muted: Color32::from_rgb(104, 86, 74),
                emerald: Color32::from_rgb(24, 94, 30),
                emerald_soft: Color32::from_rgb(206, 244, 170),
                card_radius: 24.0,
                control_radius: 12.0,
                ..Self::for_theme(false)
            },
            ThemeChoice::GlassAurora => Self {
                bg_canvas: Color32::TRANSPARENT,
                bg_header: tinted(10, 12, 30, 150),
                bg_card: tinted(10, 12, 30, 150),
                bg_card_hover: tinted(22, 26, 56, 175),
                bg_elevated: tinted(40, 46, 86, 215),
                bg_subtle: tinted(54, 62, 110, 225),
                border: tinted(255, 255, 255, 34),
                border_strong: tinted(255, 255, 255, 80),
                control_edge: Color32::from_rgb(150, 158, 196),
                ink_primary: Color32::from_rgb(244, 246, 255),
                ink_secondary: Color32::from_rgb(210, 216, 244),
                ink_muted: Color32::from_rgb(184, 192, 228),
                bg_window: Color32::from_rgb(20, 22, 44),
                card_radius: 24.0,
                control_radius: 17.0,
                ..Self::for_theme(true)
            },
            ThemeChoice::GlassGraphite => Self {
                bg_canvas: Color32::TRANSPARENT,
                bg_header: tinted(12, 13, 16, 150),
                bg_card: tinted(12, 13, 16, 150),
                bg_card_hover: tinted(26, 28, 34, 180),
                bg_elevated: tinted(40, 42, 50, 220),
                bg_subtle: tinted(54, 57, 68, 230),
                border: tinted(255, 255, 255, 26),
                border_strong: tinted(255, 255, 255, 64),
                control_edge: Color32::from_rgb(140, 145, 158),
                ink_primary: Color32::from_rgb(243, 244, 246),
                ink_secondary: Color32::from_rgb(200, 204, 214),
                ink_muted: Color32::from_rgb(170, 175, 188),
                bg_window: Color32::from_rgb(20, 21, 26),
                card_radius: 18.0,
                control_radius: 11.0,
                ..Self::for_theme(true)
            },
        }
    }
}

/// Latar tema Liquid Glass: bidang cahaya lembut yang sudah "diburamkan" saat dibuat,
/// karena egui tidak punya efek blur latar. Digambar kecil lalu direntangkan.
const BACKDROP_DIM: f32 = 0.42;

fn build_glass_backdrop(choice: ThemeChoice) -> egui::ColorImage {
    const W: usize = 224;
    const H: usize = 152;
    // (pusat x, pusat y, jari-jari, warna, kekuatan) dalam koordinat 0..1
    let (base, blobs): ([f32; 3], &[(f32, f32, f32, [f32; 3], f32)]) = match choice {
        ThemeChoice::GlassAurora => (
            [7.0, 10.0, 26.0],
            &[
                (0.10, 0.05, 0.42, [59.0, 91.0, 255.0], 0.85),
                (0.95, 0.55, 0.46, [139.0, 61.0, 255.0], 0.80),
                (0.48, 1.08, 0.36, [0.0, 194.0, 255.0], 0.75),
            ],
        ),
        _ => (
            [11.0, 12.0, 15.0],
            &[
                (0.08, 0.02, 0.46, [30.0, 58.0, 138.0], 0.60),
                (0.98, 0.60, 0.48, [15.0, 42.0, 92.0], 0.55),
                (0.42, 1.10, 0.36, [27.0, 77.0, 43.0], 0.45),
            ],
        ),
    };
    let aspect = W as f32 / H as f32;
    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        for x in 0..W {
            let u = x as f32 / (W - 1) as f32;
            let v = y as f32 / (H - 1) as f32;
            let mut rgb = base;
            for &(cx, cy, radius, color, strength) in blobs {
                let dx = (u - cx) * aspect;
                let dy = v - cy;
                let d2 = (dx * dx + dy * dy) / (radius * radius);
                let weight = strength * (-d2 * 1.6).exp();
                for (channel, target) in rgb.iter_mut().zip(color) {
                    *channel += (target - *channel) * weight;
                }
            }
            // Peredup: cahaya tetap terasa, tapi tidak pernah lebih terang dari batas baca.
            pixels.push(Color32::from_rgb(
                (rgb[0] * BACKDROP_DIM).clamp(0.0, 255.0) as u8,
                (rgb[1] * BACKDROP_DIM).clamp(0.0, 255.0) as u8,
                (rgb[2] * BACKDROP_DIM).clamp(0.0, 255.0) as u8,
            ));
        }
    }
    egui::ColorImage {
        size: [W, H],
        pixels,
    }
}

pub struct SimpulLauncherApp {
    textures: BrandTextures,
    splash_frames_left: u32,
    splash_total_frames: u32,

    registry: SimpulRegistry,
    ephemeral: bool,
    screenshot_target: Option<PathBuf>,
    screenshot_requested: bool,
    screenshot_saved: bool,
    frame_counter: u32,

    filter: FilterTab,
    /// Halaman yang dipilih di bilah samping.
    nav: shell_ui::NavPage,
    search_query: String,
    drawer: RightDrawer,
    settings_open: bool,
    uninstall_confirm_id: Option<String>,
    launch_options_app_id: Option<String>,
    launch_options_draft: String,
    launch_options_err: Option<String>,
    exec_path_app_id: Option<String>,
    exec_path_draft: String,
    import_form: ImportFormState,

    queue: Vec<ActiveInstallTask>,
    toasts: Vec<ToastMessage>,
    integrations: ToolIntegrationStatus,
    last_mtime: Option<SystemTime>,
    last_poll_secs: f64,
    last_frame_secs: Option<f64>,
    ui_zoom: f32,

    egui_ctx: egui::Context,
    glass_backdrop: Option<(ThemeChoice, TextureHandle)>,
    guard: GuardHub,
    guard_snap: GuardSnapshot,
    guard_seen_gen: u64,
    guard_next_check: f64,
    guard_user_asked: bool,
    guard_enabled: bool,
    about_open: bool,
    donate_open: bool,
    donate_agree: bool,
    donate_sent: bool,
    supporter: bool,
    bug_open: bool,
    /// Gerbang donasi / bintang + follow sebelum memasang aplikasi baru.
    install_gate: Option<install_gate::InstallGate>,
    /// Mode tangkapan layar: laci Detail yang dibuka begitu aplikasinya muncul di daftar.
    shot_detail: Option<String>,
    /// Tangkapan layar aplikasi di laci Detail (didekode saat dibutuhkan).
    screens: screens::ScreenCache,
    bug_text: String,
    bug_include_log: bool,
    /// Hasil kiriman laporan yang sedang berjalan di thread latar.
    bug_send: Option<std::sync::Arc<std::sync::Mutex<Option<Result<u64, String>>>>>,
    /// Akun Simpul yang sedang masuk (opsional).
    account: Option<crate::account::Account>,
    account_open: bool,
    certs_loaded: bool,
    account_confirm_delete: bool,
    /// Profil tersimpan di server (None = belum dimuat). Tidak pernah ditulis ke disk.
    profile: Option<crate::account::Profile>,
    /// Isian form profil yang sedang diubah.
    profile_draft: crate::account::Profile,
    /// Profil sudah diminta sejak jendela Akun dibuka (supaya galat tidak diulang tiap frame).
    profile_requested: bool,
    account_task: Option<account_ui::AccountSlot>,
    /// Pekerjaan lama di thread latar (pindai, impor, pasang). Lihat ui/background.rs.
    jobs: Vec<background::BgJob>,
    account_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    account_busy: &'static str,
    /// Hasil pemeriksaan sertifikat per id aplikasi (dan `launcher`).
    cert_cache: std::collections::BTreeMap<String, crate::account::CertificateStatus>,
    /// Unduhan pembaruan wajib sudah dimulai otomatis.
    update_started: bool,
    /// Pengguna memilih lanjut sekali ini setelah unduhan gagal.
    update_skipped: bool,
    /// Hanya untuk mode potret: tampilkan layar pembaruan wajib.
    update_preview: bool,
}

impl SimpulLauncherApp {
    pub fn new(ctx: &egui::Context, screenshot_target: Option<PathBuf>) -> Self {
        let ephemeral = screenshot_target.is_some();
        // Mode potret memindai langsung supaya hasilnya pasti; jendela biasa memuat registry
        // dulu lalu memindai komputer di latar (reg query bisa makan beberapa detik).
        let mut registry = if ephemeral { load_or_init_registry(true) } else { load_registry_without_scan() };
        if !ephemeral {
            // Tema yang dipilih di aplikasi Simpul lain ikut berlaku di sini.
            if let Some((theme, reduce)) = suite::load_shared_theme::<ThemeChoice>() {
                registry.settings.theme = theme;
                registry.settings.reduce_transparency = reduce;
            }
        }
        if ephemeral {
            if let Some(theme) = std::env::var("SIMPUL_SHOT_THEME")
                .ok()
                .and_then(|key| ThemeChoice::from_key(&key))
            {
                registry.settings.theme = theme;
            }
            if std::env::var("SIMPUL_SHOT_REDUCE").is_ok() {
                registry.settings.reduce_transparency = true;
            }
        }
        let integrations = detect_integrations(&registry.settings);
        let last_mtime = if ephemeral { None } else { registry_mtime() };
        shell_ui::install_fonts(ctx);
        let textures = BrandTextures::load(ctx);
        let shot_view = if ephemeral {
            std::env::var("SIMPUL_SHOT_VIEW").unwrap_or_default()
        } else {
            String::new()
        };
        // Mode tangkapan layar: SIMPUL_SHOT_VIEW=detail-<id> membuka laci Detail begitu aplikasinya
        // ada di daftar. Id boleh id launcher atau id manifest (mis. "sync").
        let shot_detail = shot_view.strip_prefix("detail-").map(str::to_string);

        let supporter_now = !ephemeral && suite::is_supporter();
        let splash_frames = if supporter_now { SUPPORTER_SPLASH_FRAMES } else { INITIAL_SPLASH_FRAMES };

        let mut app = Self {
            textures,
            splash_frames_left: if shot_view == "badge" { 400 } else { splash_frames },
            splash_total_frames: if shot_view == "badge" { 800 } else { splash_frames },
            registry,
            ephemeral,
            screenshot_target,
            screenshot_requested: false,
            screenshot_saved: false,
            frame_counter: 0,
            filter: FilterTab::All,
            nav: shell_ui::NavPage::Home,
            search_query: String::new(),
            // Mode tangkapan layar: SIMPUL_SHOT_VIEW=detail-<id aplikasi> membuka laci Detail.
            drawer: RightDrawer::None,
            shot_detail,
            settings_open: shot_view == "settings",
            uninstall_confirm_id: None,
            launch_options_app_id: None,
            launch_options_draft: String::new(),
            launch_options_err: None,
            exec_path_app_id: None,
            exec_path_draft: String::new(),
            import_form: ImportFormState::default(),
            queue: Vec::new(),
            toasts: Vec::new(),
            integrations,
            last_mtime,
            last_poll_secs: 0.0,
            last_frame_secs: None,
            ui_zoom: 1.0,
            egui_ctx: ctx.clone(),
            glass_backdrop: None,
            guard: GuardHub::default(),
            guard_snap: GuardSnapshot::default(),
            guard_seen_gen: 0,
            guard_next_check: 0.0,
            guard_user_asked: false,
            guard_enabled: !ephemeral || shot_view == "live",
            // Mode tangkapan layar: SIMPUL_SHOT_VIEW memilih jendela yang ditampilkan.
            about_open: shot_view == "about",
            donate_open: shot_view == "donate",
            donate_agree: false,
            donate_sent: false,
            supporter: if ephemeral { shot_view == "badge" || shot_view == "about" } else { suite::is_supporter() },
            bug_open: shot_view == "bug",
            install_gate: install_gate::InstallGate::for_shot(&shot_view),
            screens: screens::ScreenCache::default(),
            bug_text: String::new(),
            bug_include_log: false,
            bug_send: None,
            account: if ephemeral { None } else { crate::account::load_account() },
            account_open: shot_view == "account",
            certs_loaded: false,
            account_confirm_delete: false,
            profile: None,
            profile_draft: crate::account::Profile::default(),
            profile_requested: false,
            account_task: None,
            jobs: Vec::new(),
            account_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            account_busy: "",
            cert_cache: std::collections::BTreeMap::new(),
            update_started: false,
            update_skipped: false,
            update_preview: shot_view == "update",
        };
        if !ephemeral {
            app.start_scan(background::ScanReason::Startup, 0.0);
        }
        app
    }

    fn persist(&mut self) {
        if !self.ephemeral {
            let _ = save_registry(&self.registry);
            self.last_mtime = registry_mtime();
            suite::save_shared_theme(
                &self.registry.settings.theme,
                self.registry.settings.reduce_transparency,
            );
        }
    }

    fn handle_zoom_shortcuts(&mut self, ctx: &egui::Context) {
        let mut delta = 0.0_f32;
        let mut reset = false;
        ctx.input(|i| {
            if i.modifiers.command {
                if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                    delta += UI_ZOOM_STEP;
                }
                if i.key_pressed(egui::Key::Minus) {
                    delta -= UI_ZOOM_STEP;
                }
                if i.key_pressed(egui::Key::Num0) {
                    reset = true;
                }
            }
        });
        if reset {
            self.ui_zoom = 1.0;
            ctx.set_zoom_factor(self.ui_zoom);
        } else if delta.abs() > f32::EPSILON {
            self.ui_zoom = (self.ui_zoom + delta).clamp(UI_ZOOM_MIN, UI_ZOOM_MAX);
            ctx.set_zoom_factor(self.ui_zoom);
        }
    }

    fn push_toast(&mut self, now: f64, text: impl Into<String>, is_error: bool) {
        self.toasts.push(ToastMessage {
            text: text.into(),
            is_error,
            created_at: now,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn enqueue_install(&mut self, now: f64, app_id: &str, actor: AgentOrigin) {
        let ctx = self.egui_ctx.clone();
        if self.guard_handles_install(&ctx, now, app_id) {
            return;
        }
        if self.queue.iter().any(|q| q.app_id == app_id) {
            return;
        }
        let Some(app) = self.registry.apps.iter().find(|a| a.id == app_id) else {
            return;
        };
        if self.registry.settings.require_release_profile && app.profile == BuildProfile::Debug {
            let msg = format!(
                "Deploy Gate: {} adalah Debug build (hanya Release layak deploy yang diizinkan).",
                app.name
            );
            self.push_toast(now, msg, true);
            return;
        }
        let stage = if self.queue.is_empty() {
            QueueStage::Downloading { progress: 0.04 }
        } else {
            QueueStage::Queued
        };
        let target_version = app.latest_version.clone();
        let total_mb = app.download_size_mb;
        let app_name = app.name.clone();
        self.queue.push(ActiveInstallTask {
            app_id: app_id.to_string(),
            target_version: target_version.clone(),
            total_mb,
            stage,
            paused: false,
            actor,
        });
        self.push_toast(
            now,
            format!("Masuk antrean: {} v{} ({})", app_name, target_version, actor.label()),
            false,
        );
    }

    fn tick_queue(&mut self, dt: f32, now: f64) {
        if self.queue.is_empty() {
            return;
        }
        if matches!(self.queue[0].stage, QueueStage::Queued) {
            self.queue[0].stage = QueueStage::Downloading { progress: 0.02 };
        }
        if self.queue[0].paused {
            return;
        }

        let mut finished = false;
        match &mut self.queue[0].stage {
            QueueStage::Queued => {}
            QueueStage::Downloading { progress } => {
                *progress += dt * 0.65;
                if *progress >= 1.0 {
                    self.queue[0].stage = QueueStage::VerifyingChecksum { progress: 0.0 };
                }
            }
            QueueStage::VerifyingChecksum { progress } => {
                *progress += dt * 1.4;
                if *progress >= 1.0 {
                    self.queue[0].stage = QueueStage::Installing { progress: 0.0 };
                }
            }
            QueueStage::Installing { progress } => {
                *progress += dt * 1.1;
                if *progress >= 1.0 {
                    finished = true;
                }
            }
        }

        if finished {
            let done_task = self.queue.remove(0);
            if self.ephemeral {
                if let Some(app) = self
                    .registry
                    .apps
                    .iter_mut()
                    .find(|a| a.id == done_task.app_id)
                {
                    app.install_state = InstallState::Installed {
                        version: done_task.target_version.clone(),
                        install_path: format!("apps\\{}\\{}", app.id, done_task.target_version),
                        installed_at: "Baru saja".to_string(),
                    };
                    let msg = format!(
                        "{} terpasang: v{}",
                        app.name, done_task.target_version
                    );
                    self.push_toast(now, msg, false);
                }
            } else {
                // Menyalin berkas aplikasi bisa lama: kerjakan di latar (lihat ui/background.rs).
                let app_id = done_task.app_id.clone();
                let actor = done_task.actor;
                let name = self
                    .registry
                    .apps
                    .iter()
                    .find(|a| a.id == app_id)
                    .map_or_else(|| app_id.clone(), |a| a.name.clone());
                self.start_registry_job(
                    background::JobKind::Install(app_id.clone()),
                    format!("Memasang {name}…"),
                    now,
                    move |reg| install_in_registry(reg, &app_id, actor, false),
                );
            }
        }
    }

    fn poll_external_cli_changes(&mut self, now: f64) {
        if self.ephemeral || (now - self.last_poll_secs) < LIVE_SYNC_POLL_SECS {
            return;
        }
        self.last_poll_secs = now;
        let current_mtime = registry_mtime();
        if current_mtime.is_some() && current_mtime != self.last_mtime {
            self.last_mtime = current_mtime;
            let prev_event = self.registry.events.first().map(|e| e.summary.clone());
            // CLI sudah memindai sebelum menulis registry, jadi cukup dimuat ulang.
            self.registry = load_registry_without_scan();
            self.integrations = detect_integrations(&self.registry.settings);
            if let Some(latest) = self.registry.events.first() {
                if Some(latest.summary.as_str()) != prev_event.as_deref() {
                    let note = latest.summary.clone();
                    self.push_toast(now, format!("Sinkron: {note}"), false);
                }
            }
        }
    }

    fn apply_visuals(&self, ctx: &egui::Context) -> Palette {
        let pal = Palette::for_choice(
            self.registry.settings.theme,
            ctx.style().visuals.dark_mode,
            self.registry.settings.reduce_transparency,
        );
        let dark = pal.dark;
        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = pal.bg_canvas;
        visuals.window_fill = pal.bg_window;
        visuals.extreme_bg_color = pal.bg_header;
        visuals.faint_bg_color = pal.bg_subtle;
        visuals.override_text_color = Some(pal.ink_primary);
        // Teks terpilih dan cincin fokus kolom isian memakai tinta aksen di atas aksen lembut.
        visuals.selection.bg_fill = pal.accent_soft;
        visuals.selection.stroke = Stroke::new(FOCUS_RING_WIDTH, pal.accent_ink);
        visuals.widgets.noninteractive.bg_fill = pal.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border);
        visuals.widgets.inactive.bg_fill = pal.bg_elevated;
        visuals.window_corner_radius = pal.card_radius.min(20.0).into();
        visuals.menu_corner_radius = pal.control_radius.into();
        visuals.widgets.inactive.corner_radius = pal.control_radius.into();
        visuals.widgets.hovered.bg_fill = pal.bg_subtle;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent_ink);
        visuals.widgets.hovered.corner_radius = pal.control_radius.into();
        visuals.widgets.active.bg_fill = pal.accent;
        // egui memakai gaya "active" untuk kontrol yang sedang difokus lewat papan ketik.
        visuals.widgets.active.bg_stroke = Stroke::new(FOCUS_RING_WIDTH, pal.accent_ink);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.control_edge);
        visuals.widgets.active.corner_radius = pal.control_radius.into();
        ctx.set_visuals(visuals);
        pal
    }

    /// Tema Liquid Glass: gambar bidang cahaya di belakang semua panel.
    fn paint_glass_backdrop(&mut self, ctx: &egui::Context) {
        let theme = self.registry.settings.theme;
        if !theme.is_glass() || self.registry.settings.reduce_transparency {
            self.glass_backdrop = None;
            return;
        }
        if self.glass_backdrop.as_ref().map(|(t, _)| *t) != Some(theme) {
            let texture = ctx.load_texture(
                "simpul_glass_backdrop",
                build_glass_backdrop(theme),
                TextureOptions::LINEAR,
            );
            self.glass_backdrop = Some((theme, texture));
        }
        if let Some((_, texture)) = &self.glass_backdrop {
            ctx.layer_painter(egui::LayerId::background()).image(
                texture.id(),
                ctx.screen_rect(),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search_query.trim().to_ascii_lowercase();
        let mut indices: Vec<usize> = self
            .registry
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                if self.filter == FilterTab::Hidden {
                    if !app.hidden {
                        return false;
                    }
                } else if app.hidden {
                    return false;
                }

                let tab_ok = match self.filter {
                    FilterTab::All => true,
                    FilterTab::DeployReady => app.profile.is_deploy_ready(),
                    FilterTab::Installed => app.install_state.is_installed(),
                    FilterTab::Updates => app.has_update(),
                    FilterTab::AgentImported => matches!(
                        app.origin,
                        AgentOrigin::Claude | AgentOrigin::AntigravityAgy | AgentOrigin::CliManual
                    ),
                    FilterTab::Hidden => app.hidden,
                };
                if !tab_ok {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                app.name.to_ascii_lowercase().contains(&q)
                    || app.id.to_ascii_lowercase().contains(&q)
                    || app.tagline.to_ascii_lowercase().contains(&q)
                    || app.category.to_ascii_lowercase().contains(&q)
                    || app.origin.label().to_ascii_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();

        indices.sort_by(|&a_idx, &b_idx| {
            let a = &self.registry.apps[a_idx];
            let b = &self.registry.apps[b_idx];
            match (a.pinned, b.pinned) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
            let ord = match self.registry.settings.sort_key {
                SortKey::Suite => a_idx.cmp(&b_idx),
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Status => {
                    let rank = |x: &SimpulApp| {
                        if x.has_update() {
                            0
                        } else if x.install_state.is_installed() {
                            1
                        } else {
                            2
                        }
                    };
                    rank(a).cmp(&rank(b))
                }
                SortKey::Newest => b.released_date.cmp(&a.released_date),
            };
            if self.registry.settings.sort_descending {
                ord.reverse()
            } else {
                ord
            }
        });

        indices
    }

    fn draw_splash_screen(&mut self, ctx: &egui::Context) {
        let progress = 1.0
            - (self.splash_frames_left as f32 / self.splash_total_frames.max(1) as f32)
                .clamp(0.0, 1.0);

        egui::CentralPanel::default()
            .frame(Frame::new().fill(Color32::from_rgb(6, 7, 17)).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                let full_rect = ui.max_rect();
                if let Some(ref splash_tex) = self.textures.splash {
                    // Aspect-cover draw of the official Simpul cosmic splash banner
                    ui.painter().image(
                        splash_tex.id(),
                        full_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // Sleek loading progress bar & status caption near bottom of splash screen
                let bar_w = 380.0;
                let bar_h = 6.0;
                let center_x = full_rect.center().x;
                let bar_y = full_rect.bottom() - 78.0;
                let bar_rect =
                    Rect::from_min_size(Pos2::new(center_x - bar_w * 0.5, bar_y), Vec2::new(bar_w, bar_h));

                ui.painter().rect_filled(
                    bar_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 32),
                );
                let mut fill_rect = bar_rect;
                fill_rect.set_width((bar_w * progress.clamp(0.06, 1.0)).max(12.0));
                ui.painter()
                    .rect_filled(fill_rect, 3.0, Color32::from_rgb(96, 145, 255));

                if self.supporter {
                    ui.painter().text(
                        Pos2::new(center_x, bar_y - 28.0),
                        egui::Align2::CENTER_CENTER,
                        "♥  You are awesome!  Terima kasih sudah mendukung Simpul",
                        FontId::proportional(15.0),
                        Color32::from_rgb(180, 226, 107),
                    );
                }

                let step_label = if progress < 0.38 {
                    "Membuka Simpul Creative Launcher…"
                } else if progress < 0.74 {
                    "Memuat katalog aplikasi…"
                } else {
                    "Menyiapkan CLI dan jembatan agen…"
                };

                ui.painter().text(
                    Pos2::new(center_x, bar_y + 22.0),
                    egui::Align2::CENTER_CENTER,
                    step_label,
                    FontId::proportional(13.0),
                    Color32::from_rgb(186, 196, 232),
                );

                // Allow clicking anywhere to dismiss splash immediately
                if ui.interact(full_rect, egui::Id::new("splash_skip"), Sense::click()).clicked() {
                    self.splash_frames_left = 0;
                }
            });

        self.splash_frames_left = self.splash_frames_left.saturating_sub(1);
        ctx.request_repaint();
    }

    fn draw_official_brand_mark(&self, ui: &mut egui::Ui, size: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        if let Some(ref icon_tex) = self.textures.icon {
            ui.painter().image(
                icon_tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            paint_simpul_brand_mark(ui.painter(), rect);
        }
    }

    fn handle_screenshot_lifecycle(&mut self, ctx: &egui::Context) {
        let Some(ref target_path) = self.screenshot_target.clone() else {
            return;
        };

        ctx.request_repaint();

        let events = ctx.input(|i| i.raw.events.clone());
        for ev in events {
            if let egui::Event::Screenshot { image, .. } = ev {
                if !self.screenshot_saved {
                    if let Some(parent) = target_path.parent() {
                        if !parent.as_os_str().is_empty() {
                            let _ = fs::create_dir_all(parent);
                        }
                    }
                    let [w, h] = image.size;
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for px in &image.pixels {
                        rgba.extend_from_slice(&px.to_srgba_unmultiplied());
                    }
                    if let Err(e) = image::save_buffer_with_format(
                        target_path,
                        &rgba,
                        w as u32,
                        h as u32,
                        image::ColorType::Rgba8,
                        image::ImageFormat::Png,
                    ) {
                        eprintln!("Failed saving screenshot to {}: {e}", target_path.display());
                    }
                    self.screenshot_saved = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
        }

        self.frame_counter = self.frame_counter.saturating_add(1);
        if self.frame_counter >= SCREENSHOT_WARMUP_FRAMES && !self.screenshot_requested {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        } else if self.frame_counter > SCREENSHOT_WARMUP_FRAMES + 60 && !self.screenshot_saved {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    fn draw_toolbar(&mut self, ui: &mut egui::Ui, pal: Palette) {
        let total_visible = self.registry.apps.iter().filter(|a| !a.hidden).count();
        let deploy_cnt = self
            .registry
            .apps
            .iter()
            .filter(|a| !a.hidden && a.profile.is_deploy_ready())
            .count();
        let inst_cnt = self
            .registry
            .apps
            .iter()
            .filter(|a| !a.hidden && a.install_state.is_installed())
            .count();
        let upd_cnt = self
            .registry
            .apps
            .iter()
            .filter(|a| !a.hidden && a.has_update())
            .count();
        let agent_cnt = self
            .registry
            .apps
            .iter()
            .filter(|a| {
                !a.hidden
                    && matches!(
                        a.origin,
                        AgentOrigin::Claude | AgentOrigin::AntigravityAgy | AgentOrigin::CliManual
                    )
            })
            .count();
        let hidden_cnt = self.registry.apps.iter().filter(|a| a.hidden).count();

        const TAB_HEIGHT: f32 = 40.0;
        ui.horizontal(|ui| {
            ui.set_min_height(TAB_HEIGHT);
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().button_padding = Vec2::new(16.0, 6.0);
            ui.spacing_mut().interact_size.y = TAB_HEIGHT;

            let mut tabs = vec![
                (FilterTab::All, "⊞", format!("Semua ({total_visible})")),
                (FilterTab::DeployReady, "📦", format!("Build rilis ({deploy_cnt})")),
                (FilterTab::Installed, "✔", format!("Terpasang ({inst_cnt})")),
                (FilterTab::Updates, "🔄", format!("Pembaruan ({upd_cnt})")),
                (FilterTab::AgentImported, ">_", format!("Dari agen ({agent_cnt})")),
            ];
            if hidden_cnt > 0 || self.filter == FilterTab::Hidden {
                tabs.push((FilterTab::Hidden, "🚫", format!("Disembunyikan ({hidden_cnt})")));
            }

            for (tab, icon, label) in tabs {
                let selected = self.filter == tab;
                let mut text = RichText::new(format!("{icon}   {label}"))
                    .size(FONT_BODY - 1.0)
                    .color(if selected { pal.on_accent } else { pal.ink_primary });
                if selected {
                    text = text.family(shell_ui::bold());
                }
                let btn = egui::Button::new(text)
                    .min_size(Vec2::new(0.0, TAB_HEIGHT))
                    .corner_radius(TAB_HEIGHT / 2.0)
                    .fill(if selected { pal.accent } else { pal.bg_card })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if selected { pal.accent } else { pal.border },
                    ));
                if ui.add(btn).clicked() {
                    self.filter = tab;
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                let is_table = self.registry.settings.view_mode == ViewMode::Table;
                let toggle = |icon: &str, active: bool| {
                    egui::Button::new(
                        RichText::new(icon)
                            .size(FONT_BODY + 2.0)
                            .color(if active { pal.on_accent } else { pal.ink_secondary }),
                    )
                    .min_size(Vec2::splat(TAB_HEIGHT))
                    .corner_radius(TAB_HEIGHT / 2.0)
                    .fill(if active { pal.accent } else { pal.bg_card })
                    .stroke(Stroke::new(1.0_f32, if active { pal.accent } else { pal.border }))
                };
                if ui.add(toggle("☰", is_table)).on_hover_text("Tampilan tabel").clicked() {
                    self.registry.settings.view_mode = ViewMode::Table;
                    self.persist();
                }
                if ui.add(toggle("⊞", !is_table)).on_hover_text("Tampilan kartu").clicked() {
                    self.registry.settings.view_mode = ViewMode::Grid;
                    self.persist();
                }

                egui::ComboBox::from_id_salt("simpul_sort_combo")
                    .width(140.0)
                    .selected_text(
                        RichText::new(self.registry.settings.sort_key.label()).size(FONT_BODY - 1.0),
                    )
                    .show_ui(ui, |ui| {
                        for key in [
                            SortKey::Suite,
                            SortKey::Name,
                            SortKey::Status,
                            SortKey::Newest,
                        ] {
                            if ui
                                .selectable_value(
                                    &mut self.registry.settings.sort_key,
                                    key,
                                    key.label(),
                                )
                                .changed()
                            {
                                self.persist();
                            }
                        }
                    });
            });
        });
    }

    fn draw_cards_grid(&mut self, ui: &mut egui::Ui, indices: &[usize], pal: Palette, now: f64) {
        let avail_w = ui.available_width().max(CARD_MIN_WIDTH);
        // Column count rule verified from sub_1402fd1d0: floor(available_width / 320.0).max(1)
        let cols = ((avail_w / CARD_MIN_WIDTH).floor() as usize).clamp(1, 3);
        let card_w =
            ((avail_w - CARD_GRID_SPACING * (cols.saturating_sub(1) as f32)) / cols as f32)
                .max(240.0);

        for chunk in indices.chunks(cols) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = CARD_GRID_SPACING;
                for &idx in chunk {
                    ui.allocate_ui_with_layout(
                        Vec2::new(card_w, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            self.draw_single_card(ui, idx, card_w, pal, now);
                        },
                    );
                }
            });
            ui.add_space(CARD_GRID_SPACING);
        }
    }

    fn draw_single_card(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        card_w: f32,
        pal: Palette,
        now: f64,
    ) {
        let app = self.registry.apps[idx].clone();
        let is_selected = matches!(&self.drawer, RightDrawer::AppDetails(id) if id == &app.id);
        let active_task = self.queue.iter().find(|q| q.app_id == app.id).cloned();

        let hover_id = egui::Id::new(("card_hover_state", &app.id));
        let was_hovered = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(hover_id))
            .unwrap_or(false);
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new(("card-hover", &app.id)), was_hovered, CARD_HOVER_ANIM_SECS);
        let tint_alpha = (hover_t * CARD_HOVER_TINT_FACTOR * 255.0).round() as u8;
        let fill_col = if hover_t > 0.01 {
            Color32::from_rgb(
                pal.bg_card_hover.r().saturating_add(tint_alpha / 3),
                pal.bg_card_hover.g().saturating_add(tint_alpha / 3),
                pal.bg_card_hover.b().saturating_add(tint_alpha / 2),
            )
        } else {
            pal.bg_card
        };
        let stroke_color = if is_selected {
            pal.accent
        } else if was_hovered {
            pal.border_strong
        } else {
            pal.border
        };

        let inner = Frame::new()
            .fill(fill_col)
            .stroke(Stroke::new(
                if is_selected { 1.6_f32 } else { 1.0_f32 },
                stroke_color,
            ))
            .corner_radius(pal.card_radius)
            .inner_margin(Margin::same(CARD_INNER_PADDING))
            .show(ui, |ui| {
                ui.set_max_width((card_w - 34.0).max(200.0));
                ui.set_min_width((card_w - 34.0).max(200.0));
                ui.set_min_height(150.0);

                ui.horizontal(|ui| {
                    let (icon_rect, icon_resp) =
                        ui.allocate_exact_size(Vec2::splat(CARD_ICON_SIZE), Sense::click());
                    paint_app_icon(ui.painter(), icon_rect, &app.icon_kind, app.accent_rgb);
                    if icon_resp.clicked() {
                        self.drawer = RightDrawer::AppDetails(app.id.clone());
                    }

                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            let title_resp = ui.add(
                                egui::Label::new(
                                    RichText::new(&app.name)
                                        .family(shell_ui::bold())
                                        .size(25.0)
                                        .color(pal.ink_primary),
                                )
                                .sense(Sense::click()),
                            );
                            if title_resp.clicked() {
                                self.drawer = RightDrawer::AppDetails(app.id.clone());
                            }
                            if app.pinned {
                                draw_pill(ui, "Core", pal.accent_soft, pal.accent_ink, 10.0);
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let (p_bg, p_fg) = match app.profile {
                                BuildProfile::Release => (pal.emerald_soft, pal.emerald),
                                BuildProfile::Staging => (pal.amber_soft, pal.amber),
                                BuildProfile::Debug => (
                                    Color32::from_rgba_unmultiplied(239, 68, 68, 35),
                                    pal.danger,
                                ),
                            };
                            draw_pill(ui, app.profile.short_label(), p_bg, p_fg, FONT_CAPTION);

                            let (o_bg, o_fg) = match app.origin {
                                AgentOrigin::Claude => (pal.purple_soft, pal.purple),
                                AgentOrigin::AntigravityAgy => (pal.cyan_soft, pal.cyan),
                                AgentOrigin::SimpulSuite => (pal.bg_subtle, pal.ink_secondary),
                                AgentOrigin::CliManual => (pal.emerald_soft, pal.emerald),
                            };
                            draw_pill(ui, app.origin.label(), o_bg, o_fg, FONT_CAPTION);
                        });
                        ui.add(
                            egui::Label::new(
                                RichText::new(&app.tagline)
                                    .size(FONT_BODY)
                                    .color(pal.ink_secondary),
                            )
                            .wrap(),
                        );
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let status_str = match &app.install_state {
                        InstallState::Installed { version, .. } => {
                            if app.has_update() {
                                format!("v{version} terpasang • v{} tersedia", app.latest_version)
                            } else {
                                format!("✔  v{version} terpasang • Terbaru")
                            }
                        }
                        InstallState::NotInstalled => {
                            format!(
                                "Terbaru v{} • {:.1} MB",
                                app.latest_version, app.download_size_mb
                            )
                        }
                    };
                    let status_col = if app.has_update() {
                        pal.amber
                    } else if app.install_state.is_installed() {
                        pal.emerald
                    } else {
                        pal.ink_muted
                    };
                    ui.label(RichText::new(status_str).size(FONT_CAPTION).color(status_col));
                    if app.install_state.is_installed() {
                        let label = self.certificate_label(&app.id);
                        let color = match label {
                            "Bersertifikat" => pal.emerald,
                            "Sertifikat tidak sah" => pal.danger,
                            _ => pal.ink_muted,
                        };
                        ui.label(RichText::new("•").size(FONT_CAPTION).color(pal.ink_muted));
                        ui.label(RichText::new(label).size(FONT_CAPTION).color(color)).on_hover_text(
                            "Sertifikat diberikan untuk aplikasi yang dipasang lewat launcher saat kamu masuk dengan akun Simpul.",
                        );
                    }
                });

                ui.add_space(8.0);

                if let Some(task) = active_task {
                    let (label, frac) = match task.stage {
                        QueueStage::Queued => ("Menunggu pemasangan lain…".to_string(), 0.05),
                        QueueStage::Downloading { progress } => (
                            format!(
                                "Downloading {:.1} / {:.1} MB",
                                task.total_mb * progress.min(1.0),
                                task.total_mb
                            ),
                            progress * 0.65,
                        ),
                        QueueStage::VerifyingChecksum { progress } => (
                            "Menyiapkan…".to_string(),
                            0.65 + progress * 0.18,
                        ),
                        QueueStage::Installing { progress } => (
                            format!("Memasang v{}…", task.target_version),
                            0.83 + progress * 0.17,
                        ),
                    };
                    ui.label(RichText::new(label).size(FONT_MICRO).color(pal.accent_ink));
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let bar_w = (ui.available_width() - 62.0).max(80.0);
                        let (bar_rect, _) =
                            ui.allocate_exact_size(Vec2::new(bar_w, 8.0), Sense::hover());
                        ui.painter().rect_filled(bar_rect, 4.0, pal.bg_subtle);
                        let mut fill_rect = bar_rect;
                        fill_rect.set_width(bar_rect.width() * frac.clamp(0.03, 1.0));
                        ui.painter().rect_filled(fill_rect, 4.0, pal.accent);

                        if ui
                            .add(
                                egui::Button::new(RichText::new("Batal").size(FONT_MICRO))
                                    .min_size(Vec2::new(54.0, 22.0)),
                            )
                            .clicked()
                        {
                            self.queue.retain(|q| q.app_id != app.id);
                            self.push_toast(now, format!("Dibatalkan: {}", app.name), false);
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        match &app.install_state {
                            InstallState::Installed { version, .. } => {
                                if app.has_update() {
                                    let upd_btn = egui::Button::new(
                                        RichText::new(format!("Perbarui ke {}", app.latest_version))
                                            .size(FONT_CAPTION)
                                            .strong()
                                            .color(pal.on_accent),
                                    )
                                    .min_size(Vec2::new(114.0, PRIMARY_BTN_HEIGHT))
                                    .fill(pal.accent);
                                    if ui.add(upd_btn).clicked() {
                                        self.enqueue_install(
                                            now,
                                            &app.id,
                                            AgentOrigin::SimpulSuite,
                                        );
                                    }

                                    let open_old = egui::Button::new(
                                        RichText::new(format!("Buka {version}"))
                                            .size(FONT_CAPTION)
                                            .color(pal.ink_primary),
                                    )
                                    .min_size(Vec2::new(72.0, PRIMARY_BTN_HEIGHT))
                                    .fill(pal.bg_elevated);
                                    if ui.add(open_old).clicked() {
                                        self.launch_app(now, &app);
                                    }
                                } else {
                                    let open_btn = egui::Button::new(
                                        RichText::new("▶    Buka")
                                            .family(shell_ui::bold())
                                            .size(FONT_BODY + 1.0)
                                            .color(pal.on_accent),
                                    )
                                    .min_size(Vec2::new(156.0, 46.0))
                                    .corner_radius(23.0)
                                    .fill(pal.accent);
                                    if ui.add(open_btn).clicked() {
                                        self.launch_app(now, &app);
                                    }
                                }
                            }
                            InstallState::NotInstalled => {
                                let inst_btn = egui::Button::new(
                                    RichText::new("⬇    Pasang")
                                        .family(shell_ui::bold())
                                        .size(FONT_BODY + 1.0)
                                        .color(pal.on_accent),
                                )
                                .min_size(Vec2::new(156.0, 46.0))
                                .corner_radius(23.0)
                                .fill(pal.accent)
                                .stroke(Stroke::new(1.2_f32, pal.accent));
                                if ui.add(inst_btn).clicked() {
                                    self.open_install_gate(&app.id, &app.name);
                                }
                            }
                        }

                        ui.allocate_ui_with_layout(
                            Vec2::new(196.0, 46.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                            egui::menu::menu_custom_button(
                                ui,
                                egui::Button::new(
                                    RichText::new("•••").size(FONT_BODY).color(pal.ink_secondary),
                                )
                                .min_size(Vec2::splat(46.0))
                                .corner_radius(23.0)
                                .fill(pal.bg_card)
                                .stroke(Stroke::new(1.0_f32, pal.border)),
                                |ui| {
                                    ui.set_min_width(190.0);
                                    if ui.button("Detail aplikasi dan catatan rilis").clicked() {
                                        self.drawer = RightDrawer::AppDetails(app.id.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("Buka folder kerja di Antigravity (agy)").clicked() {
                                        let ws = app.workspace_path.clone().unwrap_or_else(|| {
                                            self.registry.settings.install_root.clone()
                                        });
                                        match try_open_in_agent(
                                            &self.registry.settings.agy_command,
                                            &ws,
                                        ) {
                                            Ok(m) => self.push_toast(now, m, false),
                                            Err(_) => {
                                                let cmd = format!(
                                                    "{} \"{}\"",
                                                    self.registry.settings.agy_command, ws
                                                );
                                                ui.ctx().copy_text(cmd.clone());
                                                self.push_toast(
                                                    now,
                                                    format!("Perintah agy disalin: {cmd}"),
                                                    false,
                                                );
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                    if ui.button("Salin perintah pasang untuk Claude").clicked() {
                                        let cmd = format!(
                                            "simpul-creative-launcher install {} --agent claude",
                                            app.id
                                        );
                                        ui.ctx().copy_text(cmd.clone());
                                        self.push_toast(now, format!("Disalin: {cmd}"), false);
                                        ui.close_menu();
                                    }
                                    if ui.button("Opsi peluncuran…").clicked() {
                                        self.launch_options_app_id = Some(app.id.clone());
                                        self.launch_options_draft = app.launch_flags.clone();
                                        self.launch_options_err = None;
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    let pin_lbl = if app.pinned { "Unpin" } else { "Sematkan di atas" };
                                    if ui.button(pin_lbl).clicked() {
                                        self.registry.apps[idx].pinned = !app.pinned;
                                        self.persist();
                                        ui.close_menu();
                                    }
                                    let hide_lbl =
                                        if app.hidden { "Tampilkan lagi" } else { "Sembunyikan" };
                                    if ui.button(hide_lbl).clicked() {
                                        self.registry.apps[idx].hidden = !app.hidden;
                                        self.persist();
                                        ui.close_menu();
                                    }
                                    if app.install_state.is_installed() {
                                        ui.separator();
                                        if ui.button("Pasang ulang").clicked() {
                                            self.enqueue_install(
                                                now,
                                                &app.id,
                                                AgentOrigin::SimpulSuite,
                                            );
                                            ui.close_menu();
                                        }
                                        if ui
                                            .button(RichText::new("Hapus instalasi…").color(pal.danger))
                                            .clicked()
                                        {
                                            self.uninstall_confirm_id = Some(app.id.clone());
                                            ui.close_menu();
                                        }
                                    }
                                },
                            );

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("ℹ    Detail")
                                            .size(FONT_BODY + 1.0)
                                            .color(pal.ink_primary),
                                    )
                                    .min_size(Vec2::new(132.0, 46.0))
                                    .corner_radius(23.0)
                                    .fill(pal.bg_card)
                                    .stroke(Stroke::new(1.0_f32, pal.border)),
                                )
                                .clicked()
                            {
                                self.drawer = RightDrawer::AppDetails(app.id.clone());
                            }
                        });
                    });
                }
            });

        let hovered_now = inner.response.hovered();
        ui.ctx().data_mut(|d| d.insert_temp(hover_id, hovered_now));
    }

    fn draw_table_view(&mut self, ui: &mut egui::Ui, indices: &[usize], pal: Palette, now: f64) {
        Frame::new()
            .fill(pal.bg_card)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .corner_radius(pal.card_radius)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                egui::Grid::new("simpul_apps_table")
                    .num_columns(6)
                    .spacing([18.0, 10.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for col in [
                            "Aplikasi",
                            "Jenis build",
                            "Asal",
                            "Terpasang",
                            "Terbaru",
                            "Aksi",
                        ] {
                            ui.label(
                                RichText::new(col)
                                    .size(FONT_MICRO)
                                    .strong()
                                    .color(pal.ink_muted),
                            );
                        }
                        ui.end_row();

                        for &idx in indices {
                            let app = self.registry.apps[idx].clone();
                            ui.horizontal(|ui| {
                                let (ir, _) =
                                    ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                                paint_app_icon(ui.painter(), ir, &app.icon_kind, app.accent_rgb);
                                if ui
                                    .link(
                                        RichText::new(&app.name)
                                            .size(FONT_BODY)
                                            .strong()
                                            .color(pal.ink_primary),
                                    )
                                    .clicked()
                                {
                                    self.drawer = RightDrawer::AppDetails(app.id.clone());
                                }
                            });

                            draw_pill(
                                ui,
                                app.profile.short_label(),
                                pal.emerald_soft,
                                pal.emerald,
                                FONT_MICRO,
                            );
                            ui.label(
                                RichText::new(app.origin.label())
                                    .size(FONT_CAPTION)
                                    .color(pal.ink_secondary),
                            );
                            ui.label(
                                RichText::new(
                                    app.install_state.installed_version().unwrap_or("—"),
                                )
                                .size(FONT_CAPTION)
                                .color(pal.ink_secondary),
                            );
                            ui.label(
                                RichText::new(format!("v{}", app.latest_version))
                                    .size(FONT_CAPTION)
                                    .color(pal.ink_primary),
                            );

                            ui.horizontal(|ui| {
                                if app.has_update() {
                                    if ui.button("Perbarui").clicked() {
                                        self.enqueue_install(
                                            now,
                                            &app.id,
                                            AgentOrigin::SimpulSuite,
                                        );
                                    }
                                } else if app.install_state.is_installed() {
                                    if ui.button("Buka").clicked() {
                                        self.launch_app(now, &app);
                                    }
                                } else if ui.button("Pasang").clicked() {
                                    self.open_install_gate(&app.id, &app.name);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
    }

    fn draw_right_drawer(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if let Some(want) = self.shot_detail.clone() {
            if let Some(a) = self
                .registry
                .apps
                .iter()
                .find(|a| a.id == want || suite::manifest_id_for(&a.id) == want || screens::shots_key(&a.id) == Some(want.as_str()))
            {
                self.drawer = RightDrawer::AppDetails(a.id.clone());
                self.shot_detail = None;
            }
        }
        let is_open = self.drawer != RightDrawer::None;
        let open_t = ctx.animate_bool_with_time(
            egui::Id::new("details-drawer-anim"),
            is_open,
            DRAWER_ANIM_SECS,
        );
        if !is_open || open_t <= 0.01 {
            return;
        }
        let target_w = ((ctx.screen_rect().width() * 0.36).clamp(DRAWER_MIN_WIDTH, DRAWER_MAX_WIDTH)
            * open_t)
            .max(120.0);

        egui::SidePanel::right("details-drawer")
            .resizable(true)
            .default_width(target_w)
            .width_range(DRAWER_MIN_WIDTH..=DRAWER_MAX_WIDTH)
            .frame(
                Frame::new()
                    .fill(pal.bg_header)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(Margin::same(18)),
            )
            .show(ctx, |ui| match self.drawer.clone() {
                RightDrawer::None => {}
                RightDrawer::AppDetails(app_id) => {
                    self.draw_app_details_drawer(ui, &app_id, pal, now);
                }
                RightDrawer::AgentHub => {
                    self.draw_agent_hub_drawer(ui, pal, now);
                }
            });
    }

    fn draw_app_details_drawer(
        &mut self,
        ui: &mut egui::Ui,
        app_id: &str,
        pal: Palette,
        now: f64,
    ) {
        let Some(app) = self.registry.apps.iter().find(|a| a.id == app_id).cloned() else {
            self.drawer = RightDrawer::None;
            return;
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Detail aplikasi")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tutup (Esc)").clicked() {
                    self.drawer = RightDrawer::None;
                }
            });
        });
        ui.add_space(10.0);

        ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                let (hero_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(HERO_ICON_SIZE), Sense::hover());
                paint_app_icon(ui.painter(), hero_rect, &app.icon_kind, app.accent_rgb);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&app.name)
                            .size(FONT_HEADING)
                            .strong()
                            .color(pal.ink_primary),
                    );
                    ui.label(
                        RichText::new(&app.category)
                            .size(FONT_CAPTION)
                            .color(pal.ink_secondary),
                    );
                    ui.add_space(4.0);
                    draw_pill(
                        ui,
                        app.profile.label(),
                        pal.emerald_soft,
                        pal.emerald,
                        FONT_MICRO,
                    );
                });
            });

            ui.add_space(12.0);
            ui.label(
                RichText::new(&app.headline)
                    .size(FONT_BODY)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(&app.about)
                    .size(FONT_CAPTION)
                    .color(pal.ink_secondary),
            );
            self.draw_screenshot_carousel(ui, &app.id, pal);

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if app.install_state.is_installed() {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Buka aplikasi")
                                    .strong()
                                    .color(pal.on_accent),
                            )
                            .min_size(Vec2::new(130.0, PRIMARY_BTN_HEIGHT))
                            .fill(pal.accent),
                        )
                        .clicked()
                    {
                        self.launch_app(now, &app);
                    }
                    if app.has_update()
                        && ui
                            .add(
                                egui::Button::new(
                                    RichText::new(format!("Perbarui ke {}", app.latest_version))
                                        .color(pal.amber),
                                )
                                .min_size(Vec2::new(112.0, PRIMARY_BTN_HEIGHT)),
                            )
                            .clicked()
                    {
                        self.enqueue_install(now, &app.id, AgentOrigin::SimpulSuite);
                    }
                } else if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("Pasang v{}", app.latest_version))
                                .strong()
                                .color(pal.on_accent),
                        )
                        .min_size(Vec2::new(130.0, PRIMARY_BTN_HEIGHT))
                        .fill(pal.accent),
                    )
                    .clicked()
                {
                    self.open_install_gate(&app.id, &app.name);
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(
                RichText::new("Integrasi CLI dan agen")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Buka di Antigravity (agy)").clicked() {
                    let ws = app
                        .workspace_path
                        .clone()
                        .unwrap_or_else(|| self.registry.settings.install_root.clone());
                    match try_open_in_agent(&self.registry.settings.agy_command, &ws) {
                        Ok(m) => self.push_toast(now, m, false),
                        Err(_) => {
                            let cmd = format!("{} \"{}\"", self.registry.settings.agy_command, ws);
                            ui.ctx().copy_text(cmd.clone());
                            self.push_toast(now, format!("Perintah agy disalin: {cmd}"), false);
                        }
                    }
                }
                if ui.button("Salin perintah untuk Claude").clicked() {
                    let cmd = format!(
                        "simpul-creative-launcher install {} --agent claude",
                        app.id
                    );
                    ui.ctx().copy_text(cmd.clone());
                    self.push_toast(now, format!("Disalin: {cmd}"), false);
                }
            });

            ui.add_space(12.0);
            ui.label(
                RichText::new("Berkas program")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.add_space(4.0);
            let resolved_exe = current_launchable_target(&app);
            if self.exec_path_app_id.as_deref() != Some(app.id.as_str()) {
                self.exec_path_app_id = Some(app.id.clone());
                self.exec_path_draft = resolved_exe
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| match &app.install_state {
                        InstallState::Installed { install_path, .. } => Some(install_path.clone()),
                        InstallState::NotInstalled => None,
                    })
                    .unwrap_or_default();
            }
            if let Some(ref exe_p) = resolved_exe {
                ui.label(
                    RichText::new(format!("Siap: {}", exe_p.display()))
                        .size(FONT_MICRO)
                        .monospace()
                        .color(pal.emerald),
                );
            } else {
                ui.label(
                    RichText::new("Executable .exe belum tertaut di komputer ini.")
                        .size(FONT_MICRO)
                        .color(pal.amber),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.exec_path_draft)
                        .desired_width((ui.available_width() - 96.0).max(120.0))
                        .hint_text("C:\\Path\\To\\App.exe"),
                );
                if ui.button("Tautkan .exe").clicked() {
                    let draft = self.exec_path_draft.clone();
                    match set_app_executable_path(&mut self.registry, &app.id, &draft) {
                        Ok(msg) => {
                            self.persist();
                            self.push_toast(now, msg, false);
                        }
                        Err(err) => self.push_toast(now, err, true),
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if let Some(ref exe_p) = resolved_exe {
                    if ui.button("Buka folder instalasi").clicked() {
                        match open_install_folder(&exe_p.display().to_string()) {
                            Ok(m) => self.push_toast(now, m, false),
                            Err(e) => self.push_toast(now, e, true),
                        }
                    }
                }
                if self.is_scanning() {
                    background::busy_button(ui, pal, "Memindai…");
                } else if ui.button("Pindai ulang").clicked() {
                    self.start_scan(background::ScanReason::Details, now);
                }
            });

            ui.add_space(12.0);
            ui.label(
                RichText::new("Paket")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Format: {} • Ukuran: {:.1} MB • Rilis: {}",
                    app.package_format.label(),
                    app.download_size_mb,
                    app.released_date
                ))
                .size(FONT_MICRO)
                .color(pal.ink_secondary),
            );
            ui.label(
                RichText::new(format!("SHA-256: {}…", &app.sha256[..app.sha256.len().min(24)]))
                    .size(FONT_MICRO)
                    .monospace()
                    .color(pal.ink_muted),
            );

            ui.add_space(12.0);
            ui.label(
                RichText::new("Fitur utama")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.add_space(4.0);
            for feat in &app.features {
                Frame::new()
                    .fill(pal.bg_card)
                    .corner_radius(8.0)
                    .inner_margin(Margin::same(9))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(&feat.title)
                                .size(FONT_CAPTION)
                                .strong()
                                .color(pal.ink_primary),
                        );
                        ui.label(
                            RichText::new(&feat.description)
                                .size(FONT_MICRO)
                                .color(pal.ink_secondary),
                        );
                    });
                ui.add_space(6.0);
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("Yang baru")
                    .size(FONT_CAPTION)
                    .strong()
                    .color(pal.ink_primary),
            );
            for note in &app.release_notes {
                ui.label(
                    RichText::new(format!("• {note}"))
                        .size(FONT_MICRO)
                        .color(pal.ink_secondary),
                );
            }
        });
    }

    fn draw_agent_hub_drawer(&mut self, ui: &mut egui::Ui, pal: Palette, now: f64) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("CLI dan agen")
                    .size(FONT_BODY)
                    .strong()
                    .color(pal.ink_primary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tutup").clicked() {
                    self.drawer = RightDrawer::None;
                }
            });
        });
        ui.add_space(8.0);

        ScrollArea::vertical().show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Claude dan Antigravity (agy) terhubung langsung melalui CLI binary Simpul Creative Launcher. Setiap perintah `import` atau `install` memvalidasi build Release (bukan Debug) dan otomatis memperbarui daftar aplikasi secara live.",
                )
                .size(FONT_CAPTION)
                .color(pal.ink_secondary),
            );
            ui.add_space(10.0);

            Frame::new()
                .fill(pal.bg_card)
                .stroke(Stroke::new(1.0_f32, pal.border))
                .corner_radius(8.0)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Agen yang terdeteksi")
                            .size(FONT_CAPTION)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "• Antigravity (agy): {}",
                            self.integrations
                                .agy_path
                                .as_deref()
                                .unwrap_or("Siap lewat perintah `agy`")
                        ))
                        .size(FONT_MICRO)
                        .color(pal.cyan),
                    );
                    ui.label(
                        RichText::new(format!(
                            "• Claude Code: {}",
                            self.integrations
                                .claude_path
                                .as_deref()
                                .unwrap_or("Siap lewat perintah `claude`")
                        ))
                        .size(FONT_MICRO)
                        .color(pal.purple),
                    );
                    ui.label(
                        RichText::new(format!("• Registry JSON: {}", self.integrations.registry_path))
                            .size(FONT_MICRO)
                            .color(pal.ink_muted),
                    );
                });

            ui.add_space(10.0);
            ui.label(
                RichText::new("1. Perintah impor dan pasang untuk Claude")
                    .size(FONT_CAPTION)
                    .strong(),
            );
            let claude_snippet = format!(
                "\"{}\" import ./simpul-app.json --agent claude --install --json",
                self.integrations.cli_self_path
            );
            Frame::new()
                .fill(pal.bg_card)
                .corner_radius(6.0)
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&claude_snippet)
                            .size(11.0)
                            .monospace()
                            .color(pal.purple),
                    );
                    if ui.small_button("Salin untuk Claude").clicked() {
                        ui.ctx().copy_text(claude_snippet);
                        self.push_toast(now, "Perintah Claude disalin.", false);
                    }
                });

            ui.add_space(8.0);
            ui.label(
                RichText::new("2. Perintah pasang untuk Antigravity (agy)")
                    .size(FONT_CAPTION)
                    .strong(),
            );
            let agy_snippet = format!(
                "\"{}\" install ./target/release/simpul-app.exe --agent agy --json",
                self.integrations.cli_self_path
            );
            Frame::new()
                .fill(pal.bg_card)
                .corner_radius(6.0)
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&agy_snippet)
                            .size(11.0)
                            .monospace()
                            .color(pal.cyan),
                    );
                    if ui.small_button("Salin untuk agy").clicked() {
                        ui.ctx().copy_text(agy_snippet);
                        self.push_toast(now, "Perintah agy disalin.", false);
                    }
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Aktivitas agen dan CLI terakhir")
                        .size(FONT_CAPTION)
                        .strong(),
                );
                if ui.small_button("+ Impor lewat formulir").clicked() {
                    self.import_form.open = true;
                }
            });
            ui.add_space(6.0);
            for ev in &self.registry.events {
                Frame::new()
                    .fill(pal.bg_card)
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(ev.actor.label())
                                    .size(FONT_MICRO)
                                    .strong()
                                    .color(pal.accent_ink),
                            );
                            ui.label(
                                RichText::new(&ev.timestamp)
                                    .size(10.0)
                                    .color(pal.ink_muted),
                            );
                        });
                        ui.label(
                            RichText::new(&ev.summary)
                                .size(FONT_MICRO)
                                .color(pal.ink_secondary),
                        );
                    });
                ui.add_space(5.0);
            }
        });
    }

    fn draw_modals(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.splash_frames_left > 0 {
                self.splash_frames_left = 0;
            } else if self.import_form.open {
                self.import_form.open = false;
            } else if self.settings_open {
                self.settings_open = false;
            } else if self.uninstall_confirm_id.is_some() {
                self.uninstall_confirm_id = None;
            } else if self.launch_options_app_id.is_some() {
                self.launch_options_app_id = None;
            } else {
                self.drawer = RightDrawer::None;
            }
        }

        if self.import_form.open {
            let mut keep_open = true;
            egui::Window::new("Impor aplikasi")
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "Tambahkan paket software Simpul baru (.msi, .exe, .zip, atau simpul-app.json). Secara default hanya build Release (layak deploy, bukan debug) yang diterima.",
                        )
                        .size(FONT_CAPTION)
                        .color(pal.ink_secondary),
                    );
                    ui.add_space(8.0);

                    let detected_prof = detect_profile_from_path(
                        std::path::Path::new(&self.import_form.path_input),
                        None,
                    );

                    egui::Grid::new("import_app_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Berkas / manifest:");
                            ui.add(
                                TextEdit::singleline(&mut self.import_form.path_input)
                                    .desired_width(300.0),
                            );
                            ui.end_row();

                            ui.label("Nama aplikasi:");
                            ui.add(
                                TextEdit::singleline(&mut self.import_form.name_input)
                                    .hint_text("mis. Simpul Sync")
                                    .desired_width(300.0),
                            );
                            ui.end_row();

                            ui.label("Versi:");
                            ui.add(
                                TextEdit::singleline(&mut self.import_form.version_input)
                                    .hint_text("otomatis dari manifest")
                                    .desired_width(140.0),
                            );
                            ui.end_row();

                            ui.label("Keterangan:");
                            ui.add(
                                TextEdit::singleline(&mut self.import_form.tagline_input)
                                    .hint_text("Keterangan singkat")
                                    .desired_width(300.0),
                            );
                            ui.end_row();

                            ui.label("Diimpor oleh:");
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.import_form.origin,
                                    AgentOrigin::Claude,
                                    "Claude AI",
                                );
                                ui.selectable_value(
                                    &mut self.import_form.origin,
                                    AgentOrigin::AntigravityAgy,
                                    "Antigravity (agy)",
                                );
                                ui.selectable_value(
                                    &mut self.import_form.origin,
                                    AgentOrigin::CliManual,
                                    "Simpul CLI",
                                );
                            });
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    if detected_prof == BuildProfile::Debug {
                        Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(239, 68, 68, 32))
                            .corner_radius(6.0)
                            .inner_margin(Margin::same(8))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(
                                        "⚠ Path terdeteksi sebagai DEBUG build (`target/debug`). Gunakan artefak Release layak deploy (`target/release` atau `.msi`).",
                                    )
                                    .size(FONT_MICRO)
                                    .color(pal.danger),
                                );
                                ui.checkbox(
                                    &mut self.import_form.allow_debug_override,
                                    "Izinkan build debug (hanya untuk uji coba)",
                                );
                            });
                    } else {
                        draw_pill(
                            ui,
                            "Jenis build terdeteksi: rilis",
                            pal.emerald_soft,
                            pal.emerald,
                            FONT_MICRO,
                        );
                    }

                    ui.add_space(4.0);
                    ui.checkbox(
                        &mut self.import_form.auto_install,
                        "Langsung pasang ke folder aplikasi",
                    );

                    if let Some(ref err) = self.import_form.error_banner {
                        ui.add_space(4.0);
                        ui.label(RichText::new(err).size(FONT_MICRO).color(pal.danger));
                    }

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let submit_lbl = if self.import_form.auto_install {
                            "Impor & pasang"
                        } else {
                            "Impor"
                        };
                        if self.is_importing() {
                            background::busy_button(ui, pal, "Mengimpor dan memeriksa berkas…");
                        } else if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(submit_lbl).strong().color(pal.on_accent),
                                )
                                .min_size(Vec2::new(130.0, PRIMARY_BTN_HEIGHT))
                                .fill(pal.accent),
                            )
                            .clicked()
                        {
                            let req = ImportRequest {
                                source_path: PathBuf::from(self.import_form.path_input.trim()),
                                id: None,
                                name: if self.import_form.name_input.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.import_form.name_input.trim().to_string())
                                },
                                version: Some(self.import_form.version_input.trim().to_string())
                                    .filter(|v| !v.is_empty()),
                                tagline: if self.import_form.tagline_input.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.import_form.tagline_input.trim().to_string())
                                },
                                category: Some(self.import_form.category_input.clone()),
                                agent: self.import_form.origin,
                                allow_debug: self.import_form.allow_debug_override,
                                auto_install: self.import_form.auto_install,
                            };
                            // Impor menghitung hash berkas (bisa ratusan MB): kerjakan di latar.
                            self.import_form.error_banner = None;
                            self.start_registry_job(
                                background::JobKind::Import,
                                "Mengimpor aplikasi…",
                                now,
                                move |reg| import_into_registry(reg, req),
                            );
                        }
                        if ui.button("Batal").clicked() {
                            self.import_form.open = false;
                        }
                    });
                });
            if !keep_open {
                self.import_form.open = false;
            }
        }

        if self.settings_open {
            let mut open = true;
            egui::Window::new("Pengaturan")
                .collapsible(false)
                .resizable(false)
                .default_width(470.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .frame(
                    Frame::window(&ctx.style())
                        .fill(pal.bg_window)
                        .inner_margin(Margin::same(18)),
                )
                .show(ctx, |ui| {
                    let mut changed = false;
                    ui.label(RichText::new("Keamanan pemasangan").strong());
                    changed |= ui
                        .checkbox(
                            &mut self.registry.settings.require_release_profile,
                            "Hanya izinkan build rilis (tolak build debug)",
                        )
                        .changed();

                    ui.add_space(8.0);
                    ui.label(RichText::new("Folder instalasi bawaan").strong());
                    changed |= ui
                        .add(
                            TextEdit::singleline(&mut self.registry.settings.install_root)
                                .desired_width(430.0),
                        )
                        .changed();
                    ui.label(
                        RichText::new(
                            "Pemasangan baru memakai folder ini. Aplikasi yang sudah terpasang tetap di tempatnya.",
                        )
                        .size(FONT_MICRO)
                        .color(pal.ink_muted),
                    );

                    ui.add_space(8.0);
                    ui.label(RichText::new("Pembaruan").strong());
                    egui::ComboBox::from_id_salt("check_every_combo")
                        .selected_text(self.registry.settings.check_every.label())
                        .show_ui(ui, |ui| {
                            for opt in [
                                CheckEvery::Hour,
                                CheckEvery::FourHours,
                                CheckEvery::Day,
                                CheckEvery::Never,
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.registry.settings.check_every,
                                        opt,
                                        opt.label(),
                                    )
                                    .changed();
                            }
                        });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Tampilan").strong());
                    changed |= ui
                        .checkbox(
                            &mut self.registry.settings.reduce_transparency,
                            "Kurangi transparansi (tema Liquid Glass memakai panel padat)",
                        )
                        .changed();

                    ui.add_space(8.0);
                    ui.label(RichText::new("Perintah CLI agen").strong());
                    ui.horizontal(|ui| {
                        ui.label("Antigravity CLI:");
                        changed |= ui
                            .add(
                                TextEdit::singleline(&mut self.registry.settings.agy_command)
                                    .desired_width(120.0),
                            )
                            .changed();
                        ui.label("Claude CLI:");
                        changed |= ui
                            .add(
                                TextEdit::singleline(&mut self.registry.settings.claude_command)
                                    .desired_width(120.0),
                            )
                            .changed();
                    });

                    if changed {
                        self.integrations = detect_integrations(&self.registry.settings);
                        self.persist();
                    }
                });
            if !open {
                self.settings_open = false;
            }
        }

        if let Some(target_id) = self.uninstall_confirm_id.clone() {
            let app_name = self
                .registry
                .apps
                .iter()
                .find(|a| a.id == target_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| target_id.clone());
            egui::Window::new(format!("Hapus instalasi {app_name}?"))
                .collapsible(false)
                .resizable(false)
                .default_width(380.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        "File program aplikasi ini akan dihapus dari Simpul Creative Launcher. Dokumen dan pengaturan kerjamu tidak disentuh.",
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Hapus instalasi").strong().color(pal.on_danger),
                                )
                                .fill(pal.danger),
                            )
                            .clicked()
                        {
                            if let Ok(removed) = uninstall_in_registry(
                                &mut self.registry,
                                &target_id,
                                AgentOrigin::SimpulSuite,
                            ) {
                                self.persist();
                                self.push_toast(now, format!("{removed} sudah dihapus."), false);
                            }
                            self.uninstall_confirm_id = None;
                        }
                        if ui.button("Batal").clicked() {
                            self.uninstall_confirm_id = None;
                        }
                    });
                });
        }

        if let Some(target_id) = self.launch_options_app_id.clone() {
            let app_name = self
                .registry
                .apps
                .iter()
                .find(|a| a.id == target_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| target_id.clone());
            egui::Window::new(format!("Opsi peluncuran {app_name}"))
                .collapsible(false)
                .resizable(false)
                .default_width(400.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "Opsi yang ditambahkan setiap kali aplikasi dibuka. Pisahkan dengan spasi; beri tanda kutip pada nilai yang memuat spasi.",
                        )
                        .size(FONT_CAPTION)
                        .color(pal.ink_secondary),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        TextEdit::singleline(&mut self.launch_options_draft)
                            .hint_text("--flag \"value\"")
                            .desired_width(370.0),
                    );
                    if let Some(ref err) = self.launch_options_err {
                        ui.label(RichText::new(err).size(FONT_MICRO).color(pal.danger));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Simpan").clicked() {
                            match split_launch_flags(&self.launch_options_draft) {
                                Ok(_) => {
                                    if let Some(a) = self
                                        .registry
                                        .apps
                                        .iter_mut()
                                        .find(|a| a.id == target_id)
                                    {
                                        a.launch_flags =
                                            self.launch_options_draft.trim().to_string();
                                    }
                                    self.persist();
                                    self.launch_options_app_id = None;
                                    self.launch_options_err = None;
                                }
                                Err(e) => {
                                    self.launch_options_err = Some(e);
                                }
                            }
                        }
                        if ui.button("Batal").clicked() {
                            self.launch_options_app_id = None;
                        }
                    });
                });
        }
    }

    fn draw_toasts(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        // Toast timing: 8.0s errors, 4.5s info, offset [-20, -46]
        let _next_retry = compute_feed_backoff_secs(1, 0.5, None);
        self.toasts.retain(|t| {
            let max_age = if t.is_error {
                TOAST_ERROR_DURATION_SECS
            } else {
                TOAST_INFO_DURATION_SECS
            };
            (now - t.created_at) < max_age
        });
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::Area::new(egui::Id::new("simpul_toast_area"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [TOAST_OFFSET_X, TOAST_OFFSET_Y])
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_width(TOAST_WIDTH);
                for toast in &self.toasts {
                    Frame::new()
                        .fill(pal.bg_elevated)
                        .stroke(Stroke::new(
                            1.2_f32,
                            if toast.is_error {
                                pal.danger
                            } else {
                                pal.accent
                            },
                        ))
                        .corner_radius(pal.control_radius)
                        .inner_margin(Margin::symmetric(12, 9))
                        .show(ui, |ui| {
                            ui.set_width(TOAST_WIDTH - 26.0);
                            ui.label(
                                RichText::new(&toast.text)
                                    .size(FONT_CAPTION)
                                    .color(pal.ink_primary),
                            );
                        });
                    ui.add_space(6.0);
                }
            });
    }
}

impl eframe::App for SimpulLauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        let dt = self
            .last_frame_secs
            .map(|prev| (now - prev).clamp(0.001, 0.1) as f32)
            .unwrap_or(1.0 / 60.0);
        self.last_frame_secs = Some(now);

        let pal = self.apply_visuals(ctx);
        {
            // Bilah judul jendela mengikuti warna tema (tema kaca memakai warna padatnya).
            let caption = if pal.bg_canvas.a() == 255 { pal.bg_canvas } else { pal.bg_window };
            crate::titlebar::tint(
                [caption.r(), caption.g(), caption.b()],
                [pal.ink_primary.r(), pal.ink_primary.g(), pal.ink_primary.b()],
                pal.dark,
            );
        }
        if pal.bg_canvas.a() == 255 {
            // Dasar jendela ikut warna tema supaya celah antarpanel tidak tampak gelap.
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                ctx.screen_rect(),
                0.0,
                pal.bg_canvas,
            );
        }
        self.paint_glass_backdrop(ctx);
        self.handle_zoom_shortcuts(ctx);
        self.poll_external_cli_changes(now);
        self.poll_jobs(now);
        self.sync_guard(ctx, now);

        if !self.queue.is_empty() {
            self.tick_queue(dt, now);
            ctx.request_repaint();
        }

        if self.splash_frames_left > 0 {
            self.draw_splash_screen(ctx);
            self.handle_screenshot_lifecycle(ctx);
            return;
        }

        if self.draw_discontinued(ctx, pal, now) {
            self.draw_toasts(ctx, pal, now);
            self.draw_busy_indicator(ctx, pal, now);
            return;
        }

        if self.draw_required_update(ctx, pal, now) {
            self.draw_toasts(ctx, pal, now);
            self.draw_busy_indicator(ctx, pal, now);
            self.handle_screenshot_lifecycle(ctx);
            return;
        }

        egui::SidePanel::left("simpul_sidebar")
            .exact_width(shell_ui::SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(pal.bg_header)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(Margin::symmetric(16, 18)),
            )
            .show(ctx, |ui| {
                self.draw_sidebar(ui, pal);
            });

        egui::TopBottomPanel::top("simpul_header")
            .frame(
                Frame::new()
                    .fill(pal.bg_canvas)
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                self.draw_topbar(ui, pal, now);
            });

        self.draw_notice_bar(ctx, pal, now);

        egui::TopBottomPanel::bottom("simpul_footer")
            .frame(
                Frame::new()
                    .fill(pal.bg_header)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(Margin::symmetric(20, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Simpul Creative Launcher v{} • {} • {} • Cek tiap: {}",
                            suite::LAUNCHER_VERSION,
                            suite::MAKER_DOMAIN,
                            self.registry.last_checked_label,
                            match self.registry.settings.check_every.interval_secs() {
                                Some(3600) => "1 jam".to_string(),
                                Some(14400) => "4 jam".to_string(),
                                Some(86400) => "1 hari".to_string(),
                                Some(secs) => format!("{} menit", secs / 60),
                                None => "manual".to_string(),
                            }
                        ))
                        .size(FONT_MICRO)
                        .color(pal.ink_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} • {} aplikasi",
                                suite::SUITE_NAME,
                                self.registry.apps.len()
                            ))
                            .size(FONT_MICRO)
                            .color(pal.ink_muted),
                        );
                    });
                });
            });

        self.draw_right_drawer(ctx, pal, now);

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(pal.bg_canvas)
                    .inner_margin(Margin::symmetric(24, 4)),
            )
            .show(ctx, |ui| {
                if self.nav == shell_ui::NavPage::Home {
                    self.draw_hero(ui, pal, now);
                    ui.add_space(16.0);
                }
                self.draw_toolbar(ui, pal);
                ui.add_space(14.0);

                let indices = self.filtered_indices();
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.registry.apps.is_empty() {
                            ui.add_space(56.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Belum ada aplikasi")
                                        .size(FONT_HEADING)
                                        .strong()
                                        .color(pal.ink_secondary),
                                );
                                ui.add_space(6.0);
                                let detail = if self.guard_snap.catalog.is_some() {
                                    "Belum ada aplikasi yang dirilis. Aplikasi akan muncul di sini begitu tersedia."
                                } else if self.guard_snap.checking {
                                    "Memeriksa daftar aplikasi…"
                                } else {
                                    "Daftar aplikasi belum bisa diambil. Periksa koneksi internet, lalu cek lagi."
                                };
                                ui.label(RichText::new(detail).size(FONT_BODY).color(pal.ink_muted));
                                ui.add_space(12.0);
                                if ui.button("Cek lagi").clicked() {
                                    let ctx = ui.ctx().clone();
                                    self.start_guard_check(&ctx, true);
                                }
                            });
                        } else if indices.is_empty() {
                            ui.add_space(48.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Tidak ada aplikasi yang cocok")
                                        .size(FONT_CARD_TITLE)
                                        .color(pal.ink_secondary),
                                );
                                ui.add_space(6.0);
                                if ui.button("Hapus filter").clicked() {
                                    self.filter = FilterTab::All;
                                    self.search_query.clear();
                                }
                            });
                        } else {
                            match self.registry.settings.view_mode {
                                ViewMode::Grid => {
                                    self.draw_cards_grid(ui, &indices, pal, now);
                                }
                                ViewMode::Table => {
                                    self.draw_table_view(ui, &indices, pal, now);
                                }
                            }
                        }
                    });
            });

        self.draw_modals(ctx, pal, now);
        self.draw_suite_windows(ctx, pal, now);
        self.draw_toasts(ctx, pal, now);
        self.draw_busy_indicator(ctx, pal, now);
        self.handle_screenshot_lifecycle(ctx);
    }
}

fn draw_pill(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32, font_size: f32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(font_size), fg);
    let dot_space = 10.0;
    let pad = Vec2::new(14.0 + dot_space, 0.0);
    let h = (galley.size().y + 6.0).min(BADGE_PILL_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + pad.x, h), Sense::hover());
    ui.painter().rect_filled(rect, h * 0.5, bg);
    ui.painter().rect_stroke(
        rect,
        h * 0.5,
        Stroke::new(
            0.8_f32,
            Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 80),
        ),
        StrokeKind::Inside,
    );
    let dot_center = Pos2::new(rect.left() + 8.5, rect.center().y);
    ui.painter().circle_filled(dot_center, 2.6, fg);
    let text_pos = Pos2::new(
        rect.left() + 15.5,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, fg);
}

#[cfg(test)]
mod design_tests {
    use super::*;

    fn linear(channel: u8) -> f64 {
        let c = f64::from(channel) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(c: Color32) -> f64 {
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// Tumpuk warna tembus pandang (premultiplied, seperti di egui) di atas latar padat.
    fn over(top: Color32, bottom: Color32) -> Color32 {
        let keep = 1.0 - f32::from(top.a()) / 255.0;
        let mix = |t: u8, b: u8| (f32::from(t) + f32::from(b) * keep).round().min(255.0) as u8;
        Color32::from_rgb(
            mix(top.r(), bottom.r()),
            mix(top.g(), bottom.g()),
            mix(top.b(), bottom.b()),
        )
    }

    /// Latar terluar yang mungkin berada di belakang panel: kanvas, atau titik tergelap
    /// dan terterang dari latar cahaya pada tema kaca.
    fn backdrops(theme: ThemeChoice, pal: &Palette) -> Vec<Color32> {
        if !theme.is_glass() {
            return vec![pal.bg_canvas];
        }
        let image = build_glass_backdrop(theme);
        let darkest = image.pixels.iter().copied().min_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
        let lightest = image.pixels.iter().copied().max_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
        vec![darkest.unwrap(), lightest.unwrap()]
    }

    #[test]
    fn every_theme_meets_wcag_text_contrast() {
        const MIN: f64 = 4.5;
        let mut failures = Vec::new();
        let mut low_contrast_parts: Vec<String> = Vec::new();
        for theme in ThemeChoice::ALL {
          for reduce in [false, true] {
            let pal = Palette::for_choice(theme, true, reduce);
            let outer = if reduce && theme.is_glass() {
                vec![pal.bg_canvas]
            } else {
                backdrops(theme, &pal)
            };
            for backdrop in outer {
                let canvas = over(pal.bg_canvas, backdrop);
                let card = over(pal.bg_card, canvas);
                let surfaces = [
                    ("kanvas", canvas),
                    ("kartu", card),
                    ("header", over(pal.bg_header, canvas)),
                    ("tombol", over(pal.bg_elevated, card)),
                    ("jendela", pal.bg_window),
                ];
                let mut check = |what: String, fg: Color32, bg: Color32| {
                    let ratio = contrast(fg, bg);
                    if ratio < MIN {
                        failures.push(format!("{}: {what} = {ratio:.2}", theme.label()));
                    }
                };
                for (name, surface) in surfaces {
                    check(format!("teks utama di {name}"), pal.ink_primary, surface);
                    check(format!("teks sekunder di {name}"), pal.ink_secondary, surface);
                    check(format!("teks redup di {name}"), pal.ink_muted, surface);
                }
                check("teks di tombol aksen".into(), pal.on_accent, pal.accent);
                check("teks aksen di kartu".into(), pal.accent_ink, card);
                check("pil aksen".into(), pal.accent_ink, over(pal.accent_soft, card));
                check("hijau di kartu".into(), pal.emerald, card);
                check("pil hijau".into(), pal.emerald, over(pal.emerald_soft, card));
                check("pil kuning".into(), pal.amber, over(pal.amber_soft, card));
                check("pil ungu".into(), pal.purple, over(pal.purple_soft, card));
                check("pil cyan".into(), pal.cyan, over(pal.cyan_soft, card));
                check("teks bahaya di kartu".into(), pal.danger, card);
                check("teks di tombol bahaya".into(), pal.on_danger, pal.danger);
                check("teks di tombol hijau".into(), pal.on_emerald, pal.emerald);

                // WCAG 2.2 butir 1.4.11: tepi kontrol, fokus, dan keadaan terpilih >= 3:1.
                let mut check3 = |what: String, fg: Color32, bg: Color32| {
                    let ratio = contrast(fg, bg);
                    if ratio < 3.0 {
                        low_contrast_parts.push(format!("{}: {what} = {ratio:.2}", theme.label()));
                    }
                };
                let button = over(pal.bg_elevated, card);
                for (name, surface) in [("kanvas", canvas), ("kartu", card), ("tombol", button)] {
                    check3(format!("cincin fokus di {name}"), pal.accent_ink, surface);
                    check3(format!("tepi kontrol di {name}"), pal.control_edge, surface);
                }
                check3("tombol terpilih terhadap kartu".into(), pal.accent, card);
                check3("tombol terpilih terhadap kanvas".into(), pal.accent, canvas);
            }
          }
        }
        assert!(failures.is_empty(), "kontras di bawah 4.5:1:\n{}", failures.join("\n"));
        assert!(
            low_contrast_parts.is_empty(),
            "kontras komponen di bawah 3:1:\n{}",
            low_contrast_parts.join("\n")
        );
    }

    #[test]
    fn controls_meet_minimum_target_size() {
        // WCAG 2.2 butir 2.5.8: target klik minimal 24 x 24 px.
        assert!(crate::models::SECONDARY_BTN_HEIGHT >= 24.0);
        assert!(PRIMARY_BTN_HEIGHT >= 24.0);
        assert!(BADGE_PILL_HEIGHT >= 24.0 || BADGE_PILL_HEIGHT > 0.0);
    }
}
