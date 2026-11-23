//! Bagian UI untuk identitas Simpul: Tentang, donasi, status hijau/kuning/merah,
//! dan pembaruan dari manifest bertanda tangan.

use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Stroke, Vec2};
use simpul_guard::{Notice, Origin, Verdict};

use super::{Palette, SimpulLauncherApp};
use crate::models::{
    SimpulApp, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_HEADING, FONT_MICRO,
    PRIMARY_BTN_HEIGHT, SECONDARY_BTN_HEIGHT,
};
use crate::registry::{now_label, try_launch_app};
use crate::suite::{
    self, DownloadState, CONTACT_EMAIL, DONATE_URL, LAUNCHER_MANIFEST_ID, LAUNCHER_NAME,
    LAUNCHER_VERSION, MAKER_DOMAIN, MAKER_NAME, MAKER_URL, SOURCE_URL, SUITE_NAME,
    UPSTREAM_CREDIT, UPSTREAM_URL,
};

/// Jeda cek ulang kalau pengguna mematikan cek pembaruan: status tetap dicek saat dibuka saja.
const NEVER: f64 = f64::INFINITY;

impl SimpulLauncherApp {
    /// Mulai cek status & pembaruan di latar (tidak menahan UI).
    pub(super) fn start_guard_check(&mut self, ctx: &egui::Context, user_asked: bool) {
        if !self.guard_enabled {
            return;
        }
        if user_asked {
            self.guard_user_asked = true;
        }
        let apps: Vec<(String, String)> = self
            .registry
            .apps
            .iter()
            .filter_map(|a| {
                let version = a.install_state.installed_version()?.to_string();
                Some((a.id.clone(), version))
            })
            .collect();
        let repaint = ctx.clone();
        self.guard.refresh(apps, move || repaint.request_repaint());
    }

    /// Dipanggil setiap frame: ambil hasil terbaru dan jadwalkan cek berkala.
    pub(super) fn sync_guard(&mut self, ctx: &egui::Context, now: f64) {
        if !self.guard_enabled {
            return;
        }
        if now >= self.guard_next_check {
            self.guard_next_check = self
                .registry
                .settings
                .check_every
                .interval_secs()
                .map_or(NEVER, |secs| now + secs as f64);
            self.start_guard_check(ctx, false);
        }

        self.guard_snap = self.guard.snapshot();
        if self.guard_snap.generation == self.guard_seen_gen {
            return;
        }
        self.guard_seen_gen = self.guard_snap.generation;

        self.apply_catalog();
        let mut updates = self
            .registry
            .apps
            .iter()
            .filter(|a| a.install_state.is_installed() && self.guard_snap.app_update(&a.id).is_some())
            .count();
        let launcher = self.guard_snap.launcher.clone();
        if launcher.as_ref().is_some_and(|r| r.update.is_some()) {
            updates += 1;
        }
        let origin = launcher.map(|r| r.origin);
        if matches!(origin, Some(Origin::Fresh(_))) {
            self.registry.last_checked_label = format!("Dicek {}", now_label());
        }
        self.persist();

        if self.guard_user_asked {
            self.guard_user_asked = false;
            let (text, is_error) = match origin {
                Some(Origin::Fresh(_)) if updates == 0 => {
                    ("Semua sudah versi terbaru.".to_string(), false)
                }
                Some(Origin::Fresh(_)) => (format!("{updates} pembaruan tersedia."), false),
                Some(Origin::Cached) => (
                    "Server tidak bisa dihubungi. Memakai data terakhir yang sah.".to_string(),
                    true,
                ),
                _ => ("Belum bisa memeriksa pembaruan saat ini.".to_string(), true),
            };
            self.push_toast(now, text, is_error);
        }
    }

    /// Samakan daftar aplikasi dengan manifest: yang tampil hanya aplikasi yang sudah dirilis,
    /// yang terpasang di komputer ini, dan yang diimpor pengguna atau agen.
    fn apply_catalog(&mut self) {
        let Some(catalog) = self.guard_snap.catalog.clone() else {
            return;
        };
        for entry in &catalog {
            let may_run = self.guard_snap.app_may_run(&entry.app_id);
            match self.registry.apps.iter_mut().find(|a| a.id == entry.app_id) {
                Some(app) => {
                    if let Some(version) = &entry.version {
                        app.latest_version = version.clone();
                    }
                    if let Some(tagline) = &entry.tagline {
                        app.tagline = tagline.clone();
                    }
                }
                None => {
                    if let (Some(version), true) = (&entry.version, may_run) {
                        if crate::registry::is_safe_version(version) {
                            self.registry.apps.push(crate::registry::catalog_app(
                                &entry.app_id,
                                &entry.name,
                                entry.tagline.as_deref(),
                                version,
                            ));
                        }
                    }
                }
            }
        }
        let released = |id: &str| {
            catalog
                .iter()
                .any(|c| c.app_id == id && c.version.is_some())
        };
        let guard = &self.guard_snap;
        self.registry.apps.retain(|a| {
            a.install_state.is_installed()
                || a.origin != crate::models::AgentOrigin::SimpulSuite
                || (released(&a.id) && guard.app_may_run(&a.id))
        });
    }

    /// Buka aplikasi, kecuali manifest menyatakan aplikasi itu sudah dihentikan.
    pub(super) fn launch_app(&mut self, now: f64, app: &SimpulApp) {
        if let Some(Verdict::Disabled(notice)) = self.guard_snap.app_verdict(&app.id) {
            let text = format!("{} dihentikan: {}", app.name, notice.title);
            self.push_toast(now, text, true);
            return;
        }
        match try_launch_app(app, &[]) {
            Ok(m) => self.push_toast(now, m, false),
            Err(e) => self.push_toast(now, e, true),
        }
    }

    /// Dipanggil sebelum antrean pasang. `true` berarti sudah ditangani di sini.
    pub(super) fn guard_handles_install(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        app_id: &str,
    ) -> bool {
        if self.ephemeral {
            return false;
        }
        let Some(app) = self.registry.apps.iter().find(|a| a.id == app_id).cloned() else {
            return true;
        };
        if !self.guard_snap.app_may_run(&app.id) {
            let text = format!("{} sudah dihentikan dan tidak bisa dipasang.", app.name);
            self.push_toast(now, text, true);
            return true;
        }
        if let Some(update) = self.guard_snap.app_update(&app.id).cloned() {
            let version = update.version.to_string();
            let repaint = ctx.clone();
            self.guard
                .download(&app.id, update, move || repaint.request_repaint());
            let text = format!("Mengunduh {} v{version}…", app.name);
            self.push_toast(now, text, false);
            return true;
        }
        let has_local_package = app
            .artifact_path
            .as_deref()
            .is_some_and(|p| std::path::Path::new(p).is_file());
        if !has_local_package {
            let text = format!("Belum ada rilis {} yang bisa dipasang dari sini.", app.name);
            self.push_toast(now, text, true);
            return true;
        }
        false
    }

    fn launcher_verdict(&self) -> Option<Verdict> {
        self.guard_snap.launcher.as_ref().map(|r| r.verdict.clone())
    }

    /// Layar penuh saat launcher sendiri berstatus merah dan masa tenggang habis.
    /// Mengembalikan `true` kalau layar ini ditampilkan (UI lain tidak digambar).
    pub(super) fn draw_discontinued(&mut self, ctx: &egui::Context, pal: Palette, now: f64) -> bool {
        let Some(Verdict::Disabled(notice)) = self.launcher_verdict() else {
            return false;
        };
        egui::CentralPanel::default()
            .frame(Frame::new().fill(pal.bg_canvas).inner_margin(Margin::same(32)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    self.draw_official_brand_mark(ui, 56.0);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(&notice.title)
                            .size(FONT_HEADING)
                            .strong()
                            .color(pal.ink_primary),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&notice.message)
                                .size(FONT_BODY)
                                .color(pal.ink_secondary),
                        )
                        .wrap(),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "Uninstall hanya menghapus file program. File kerja Anda tidak dihapus.",
                        )
                        .size(FONT_CAPTION)
                        .color(pal.ink_muted),
                    );
                    ui.add_space(18.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Uninstall launcher").strong().color(pal.on_danger),
                            )
                            .min_size(Vec2::new(180.0, PRIMARY_BTN_HEIGHT))
                            .fill(pal.danger),
                        )
                        .clicked()
                    {
                        match suite::run_own_uninstaller() {
                            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                            Err(e) => self.push_toast(now, e, true),
                        }
                    }
                    ui.add_space(6.0);
                    if let Some(url) = notice.source_url.as_deref() {
                        if ui
                            .add(
                                egui::Button::new("Buka kode sumber")
                                    .min_size(Vec2::new(180.0, SECONDARY_BTN_HEIGHT)),
                            )
                            .clicked()
                        {
                            ctx.open_url(egui::OpenUrl::new_tab(url));
                        }
                        ui.label(
                            RichText::new("Source code tersedia, Anda bisa membangunnya sendiri.")
                                .size(FONT_MICRO)
                                .color(pal.ink_muted),
                        );
                        ui.add_space(6.0);
                    }
                    if ui
                        .add(
                            egui::Button::new("Tutup")
                                .min_size(Vec2::new(180.0, SECONDARY_BTN_HEIGHT)),
                        )
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        true
    }

    /// Pita pemberitahuan di bawah header: status kuning/merah dan pembaruan launcher.
    pub(super) fn draw_notice_bar(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        let verdict = self.launcher_verdict();
        let notice: Option<(Notice, Color32, Color32, Option<String>)> = match &verdict {
            Some(Verdict::Warn(n)) => Some((n.clone(), pal.amber_soft, pal.amber, None)),
            Some(v @ Verdict::Sunset { notice, .. }) => Some((
                notice.clone(),
                Color32::from_rgba_unmultiplied(pal.danger.r(), pal.danger.g(), pal.danger.b(), 34),
                pal.danger,
                suite::remaining_label(v),
            )),
            _ => None,
        };

        if let Some((n, bg, fg, remaining)) = notice {
            egui::TopBottomPanel::top("simpul_status_notice")
                .frame(Frame::new().fill(bg).inner_margin(Margin::symmetric(20, 7)))
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&n.title).strong().size(FONT_CAPTION).color(fg));
                        if let Some(rem) = remaining {
                            ui.label(
                                RichText::new(format!("Launcher dinonaktifkan {rem}."))
                                    .strong()
                                    .size(FONT_CAPTION)
                                    .color(fg),
                            );
                        }
                        ui.label(
                            RichText::new(&n.message)
                                .size(FONT_CAPTION)
                                .color(pal.ink_primary),
                        );
                        if let Some(url) = n.source_url.as_deref() {
                            ui.hyperlink_to(
                                RichText::new("Kode sumber").size(FONT_CAPTION),
                                url,
                            );
                        }
                    });
                });
        }

        let update = self
            .guard_snap
            .launcher
            .as_ref()
            .and_then(|r| r.update.clone());
        let Some(update) = update else {
            return;
        };
        let state = self.guard_snap.downloads.get(LAUNCHER_MANIFEST_ID).cloned();
        egui::TopBottomPanel::top("simpul_update_notice")
            .frame(
                Frame::new()
                    .fill(pal.accent_soft)
                    .inner_margin(Margin::symmetric(20, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{LAUNCHER_NAME} v{} tersedia (terpasang v{LAUNCHER_VERSION}).",
                            update.version
                        ))
                        .size(FONT_CAPTION)
                        .color(pal.ink_primary),
                    );
                    if let Some(url) = update.notes_url.as_deref() {
                        ui.hyperlink_to(RichText::new("Catatan rilis").size(FONT_CAPTION), url);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| match state {
                        None => {
                            if ui.button("Unduh pembaruan").clicked() {
                                let repaint = ctx.clone();
                                self.guard.download(LAUNCHER_MANIFEST_ID, update.clone(), move || {
                                    repaint.request_repaint();
                                });
                            }
                        }
                        Some(DownloadState::Downloading) => {
                            ui.spinner();
                            ui.label(RichText::new("Mengunduh…").size(FONT_CAPTION));
                        }
                        Some(DownloadState::Ready(path)) => {
                            if ui.button("Pasang sekarang").clicked() {
                                match suite::run_installer(&path) {
                                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                                    Err(e) => self.push_toast(now, e, true),
                                }
                            }
                            ui.label(
                                RichText::new("Unduhan terverifikasi (sha256 cocok).")
                                    .size(FONT_CAPTION)
                                    .color(pal.emerald),
                            );
                        }
                        Some(DownloadState::Failed(err)) => {
                            if ui.button("Coba lagi").clicked() {
                                self.guard.clear_download(LAUNCHER_MANIFEST_ID);
                            }
                            ui.label(
                                RichText::new(format!("Unduhan gagal: {err}"))
                                    .size(FONT_CAPTION)
                                    .color(pal.danger),
                            );
                        }
                    });
                });
            });
    }

    /// Tombol Donasi dan Tentang di header.
    pub(super) fn draw_suite_header_buttons(&mut self, ui: &mut egui::Ui, pal: Palette) {
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Tentang")
                        .size(FONT_CAPTION)
                        .color(pal.ink_secondary),
                )
                .min_size(Vec2::new(78.0, SECONDARY_BTN_HEIGHT))
                .fill(pal.bg_elevated),
            )
            .clicked()
        {
            self.about_open = true;
            self.bug_open = false;
            self.donate_open = false;
        }
        if ui
            .add(
                egui::Button::new(
                    RichText::new("♥ Donasi")
                        .size(FONT_CAPTION)
                        .strong()
                        .color(pal.emerald),
                )
                .min_size(Vec2::new(78.0, SECONDARY_BTN_HEIGHT))
                .fill(pal.emerald_soft),
            )
            .on_hover_text("Dukung developer Simpul lewat Saweria")
            .clicked()
        {
            self.open_donate();
        }
    }

    fn open_donate(&mut self) {
        self.donate_agree = false;
        self.donate_sent = false;
        self.donate_open = true;
        // Satu jendela saja yang terbuka supaya tidak saling menutupi.
        self.about_open = false;
        self.bug_open = false;
    }

    /// Jendela Tentang, donasi, dan daftar unduhan aplikasi.
    pub(super) fn draw_suite_windows(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.donate_open {
                self.donate_open = false;
            } else if self.bug_open {
                self.bug_open = false;
            } else if self.about_open {
                self.about_open = false;
            }
        }
        self.draw_about_window(ctx, pal);
        self.draw_donate_window(ctx, pal, now);
        self.draw_bug_window(ctx, pal, now);
        self.draw_downloads_window(ctx, pal, now);
    }

    fn window_frame(ctx: &egui::Context, pal: Palette) -> Frame {
        Frame::window(&ctx.style())
            .fill(pal.bg_window)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .inner_margin(Margin::same(20))
    }

    fn draw_about_window(&mut self, ctx: &egui::Context, pal: Palette) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut want_donate = false;
        let mut want_check = false;
        let mut want_bug = false;
        egui::Window::new(format!("Tentang {LAUNCHER_NAME}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(420.0, 0.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    self.draw_official_brand_mark(ui, 56.0);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(LAUNCHER_NAME)
                            .size(FONT_HEADING)
                            .strong()
                            .color(pal.ink_primary),
                    );
                    ui.label(
                        RichText::new(format!("Versi {LAUNCHER_VERSION}"))
                            .size(FONT_CAPTION)
                            .color(pal.ink_muted),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "Bagian dari {SUITE_NAME}: aplikasi kreatif gratis dan open source."
                        ))
                        .size(FONT_BODY)
                        .color(pal.ink_secondary),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("Dibuat oleh {MAKER_NAME}"))
                            .size(FONT_BODY)
                            .strong()
                            .color(pal.ink_primary),
                    );
                    ui.hyperlink_to(RichText::new(MAKER_DOMAIN).size(FONT_BODY), MAKER_URL);
                    ui.add_space(8.0);
                });

                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Kode sumber").size(FONT_CAPTION).color(pal.ink_muted));
                    ui.hyperlink_to(
                        RichText::new("github.com/simpulcreativeapps").size(FONT_CAPTION),
                        SOURCE_URL,
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Kontak").size(FONT_CAPTION).color(pal.ink_muted));
                    ui.label(RichText::new(CONTACT_EMAIL).size(FONT_CAPTION).color(pal.ink_secondary));
                });
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(UPSTREAM_CREDIT)
                            .size(FONT_MICRO)
                            .color(pal.ink_muted),
                    )
                    .wrap(),
                );
                ui.hyperlink_to(
                    RichText::new("Lihat proyek aslinya").size(FONT_MICRO),
                    UPSTREAM_URL,
                );
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Tanpa telemetri. Launcher hanya mengambil daftar status dan versi \
                             yang bertanda tangan, dan hanya mengirim laporan bug saat Anda \
                             menekan Kirim laporan.",
                        )
                        .size(FONT_MICRO)
                        .color(pal.ink_muted),
                    )
                    .wrap(),
                );

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("♥ Donasi").strong().color(pal.on_emerald),
                            )
                            .min_size(Vec2::new(110.0, PRIMARY_BTN_HEIGHT))
                            .fill(pal.emerald),
                        )
                        .clicked()
                    {
                        want_donate = true;
                    }
                    if ui
                        .add(
                            egui::Button::new("Cek pembaruan")
                                .min_size(Vec2::new(120.0, PRIMARY_BTN_HEIGHT)),
                        )
                        .clicked()
                    {
                        want_check = true;
                    }
                    if ui
                        .add(
                            egui::Button::new("Laporkan bug")
                                .min_size(Vec2::new(110.0, PRIMARY_BTN_HEIGHT)),
                        )
                        .clicked()
                    {
                        want_bug = true;
                    }
                });
                if self.supporter {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("♥ You are awesome! Terima kasih sudah mendukung Simpul.")
                            .size(FONT_CAPTION)
                            .strong()
                            .color(pal.emerald),
                    );
                }
            });
        if want_bug {
            self.bug_include_log = false;
            self.bug_open = true;
            open = false;
        }
        if want_donate {
            self.open_donate();
        }
        if want_check {
            self.start_guard_check(ctx, true);
        }
        if !open {
            self.about_open = false;
        }
    }

    fn draw_donate_window(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if !self.donate_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new("Dukung Simpul")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(440.0, 0.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                if self.donate_sent {
                    ui.label(
                        RichText::new("Halaman Saweria sudah dibuka di browser")
                            .size(FONT_CARD_TITLE)
                            .strong()
                            .color(pal.ink_primary),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Kalau donasimu sudah selesai, beri tahu kami di sini supaya lencana \
                                 \"You are awesome\" muncul di layar pembuka aplikasi Simpul. \
                                 Ini disimpan di komputermu saja dan tidak dikirim ke mana pun.",
                            )
                            .size(FONT_CAPTION)
                            .color(pal.ink_secondary),
                        )
                        .wrap(),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Saya sudah berdonasi")
                                        .strong()
                                        .color(pal.on_emerald),
                                )
                                .min_size(Vec2::new(170.0, PRIMARY_BTN_HEIGHT))
                                .fill(pal.emerald),
                            )
                            .clicked()
                        {
                            match suite::set_supporter(true) {
                                Ok(()) => {
                                    self.supporter = true;
                                    self.push_toast(
                                        now,
                                        "♥ You are awesome! Terima kasih sudah mendukung Simpul.",
                                        false,
                                    );
                                }
                                Err(e) => self.push_toast(now, e, true),
                            }
                            close = true;
                        }
                        if ui
                            .add(
                                egui::Button::new("Belum")
                                    .min_size(Vec2::new(90.0, PRIMARY_BTN_HEIGHT)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                    return;
                }
                ui.label(
                    RichText::new("Kamu akan diarahkan ke Saweria")
                        .size(FONT_CARD_TITLE)
                        .strong()
                        .color(pal.ink_primary),
                );
                ui.add_space(6.0);
                for point in [
                    "Halaman Saweria terbuka di browser. Pembayaran diproses oleh Saweria; \
                     aplikasi ini tidak menyimpan data pembayaranmu.",
                    "Saweria adalah platform dukungan kreator Indonesia sejak 2019, \
                     bagian dari IDN Media.",
                    "Nominal bebas. Semua aplikasi Simpul tetap gratis dengan atau tanpa donasi.",
                ] {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new("•").color(pal.emerald));
                        ui.add(
                            egui::Label::new(
                                RichText::new(point).size(FONT_CAPTION).color(pal.ink_secondary),
                            )
                            .wrap(),
                        );
                    });
                }
                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.donate_agree,
                    RichText::new(
                        "Saya paham ini donasi sukarela untuk mendukung developer Simpul terus \
                         mengembangkan software, bukan pembayaran untuk membeli aplikasi. \
                         Donasi tidak dapat dikembalikan.",
                    )
                    .size(FONT_CAPTION)
                    .color(pal.ink_primary),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let go = egui::Button::new(
                        RichText::new("Lanjut ke Saweria").strong().color(pal.on_emerald),
                    )
                    .min_size(Vec2::new(150.0, PRIMARY_BTN_HEIGHT))
                    .fill(pal.emerald);
                    if ui.add_enabled(self.donate_agree, go).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(DONATE_URL));
                        self.donate_sent = true;
                    }
                    if ui
                        .add(
                            egui::Button::new("Nanti saja")
                                .min_size(Vec2::new(100.0, PRIMARY_BTN_HEIGHT)),
                        )
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if close || !open {
            self.donate_open = false;
        }
    }

    /// Laporan bug: membuka aplikasi email pengguna dengan isi yang sudah disiapkan.
    /// Tidak ada yang terkirim sebelum pengguna menekan Kirim di aplikasi emailnya.
    fn draw_bug_window(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if !self.bug_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut diagnostics = suite::diagnostics_enabled();

        // Ambil hasil kiriman bila thread latar sudah selesai.
        let finished = self
            .bug_send
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|mut guard| guard.take()));
        if let Some(result) = finished {
            self.bug_send = None;
            match result {
                Ok(id) => {
                    let text = if id > 0 {
                        format!("Laporan #{id} terkirim. Terima kasih!")
                    } else {
                        "Laporan terkirim. Terima kasih!".to_string()
                    };
                    self.push_toast(now, text, false);
                    suite::log_event("laporan bug terkirim");
                    self.bug_text.clear();
                    close = true;
                }
                Err(reason) => {
                    self.push_toast(
                        now,
                        format!("Laporan belum terkirim: {reason}. Tulisanmu masih ada, coba lagi atau pakai Salin isi."),
                        true,
                    );
                }
            }
        }
        let sending = self.bug_send.is_some();

        egui::Window::new("Laporkan bug")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(480.0, 0.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Ceritakan apa yang terjadi dan langkah untuk mengulanginya.")
                        .size(FONT_CAPTION)
                        .color(pal.ink_secondary),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::multiline(&mut self.bug_text)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .char_limit(suite::BUG_REPORT_MAX_CHARS)
                        .hint_text("Contoh: saat klik Open pada Simpul Sync, muncul pesan ..."),
                );
                ui.add_space(8.0);
                if ui
                    .checkbox(
                        &mut diagnostics,
                        RichText::new(
                            "Izinkan log diagnostik di komputer ini untuk membantu pengembangan",
                        )
                        .size(FONT_CAPTION),
                    )
                    .changed()
                {
                    if let Err(e) = suite::set_diagnostics(diagnostics) {
                        self.push_toast(now, e, true);
                    } else if diagnostics {
                        suite::log_event("log diagnostik diaktifkan pengguna");
                    } else {
                        self.bug_include_log = false;
                    }
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Log mencatat kejadian ringkas (hasil cek status, kegagalan unduh atau \
                             buka aplikasi) tanpa nama akun. Log hanya ikut terkirim bila dicentang di bawah; \
                             mematikan izin menghapus log.",
                        )
                        .size(FONT_MICRO)
                        .color(pal.ink_muted),
                    )
                    .wrap(),
                );
                if diagnostics {
                    ui.checkbox(
                        &mut self.bug_include_log,
                        RichText::new("Sertakan log terakhir di laporan ini").size(FONT_CAPTION),
                    );
                }

                let mut description: String = self.bug_text.clone();
                if self.bug_include_log {
                    let tail = suite::log_tail(15, 500);
                    if !tail.is_empty() {
                        description = description.chars().take(600).collect();
                        description.push_str("\n\nLog:\n");
                        description.push_str(&tail);
                    }
                }
                let body = suite::bug_report_body(LAUNCHER_NAME, LAUNCHER_VERSION, &description);

                ui.add_space(8.0);
                ui.label(
                    RichText::new("Yang akan dikirim:")
                        .size(FONT_CAPTION)
                        .strong()
                        .color(pal.ink_primary),
                );
                egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&body).size(FONT_MICRO).monospace().color(pal.ink_muted),
                        )
                        .wrap(),
                    );
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let send = egui::Button::new(
                        RichText::new(if sending { "Mengirim…" } else { "Kirim laporan" })
                            .strong()
                            .color(pal.on_accent),
                    )
                    .min_size(Vec2::new(150.0, PRIMARY_BTN_HEIGHT))
                    .fill(pal.accent);
                    if ui
                        .add_enabled(!sending && self.bug_text.trim().chars().count() >= 5, send)
                        .on_hover_text("Mengirim isi di atas langsung ke tim Simpul")
                        .clicked()
                    {
                        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                        self.bug_send = Some(slot.clone());
                        let text = description.clone();
                        let repaint = ctx.clone();
                        let spawned = std::thread::Builder::new()
                            .name("simpul-lapor".into())
                            .spawn(move || {
                                let result = suite::send_report(&text);
                                if let Ok(mut guard) = slot.lock() {
                                    *guard = Some(result);
                                }
                                repaint.request_repaint();
                            });
                        if spawned.is_err() {
                            self.bug_send = None;
                            self.push_toast(now, "Laporan tidak bisa dikirim sekarang.", true);
                        }
                    }
                    if ui
                        .add(
                            egui::Button::new("Salin isi")
                                .min_size(Vec2::new(90.0, PRIMARY_BTN_HEIGHT)),
                        )
                        .clicked()
                    {
                        ctx.copy_text(body.clone());
                        self.push_toast(now, "Isi laporan disalin.", false);
                    }
                    if ui
                        .add(
                            egui::Button::new("Lewat email")
                                .min_size(Vec2::new(96.0, PRIMARY_BTN_HEIGHT)),
                        )
                        .on_hover_text(format!("Membuka aplikasi email ke {CONTACT_EMAIL}"))
                        .clicked()
                    {
                        let link =
                            suite::bug_report_mailto(LAUNCHER_NAME, LAUNCHER_VERSION, &description);
                        ctx.copy_text(body.clone());
                        let opened = suite::open_with_system(&link).is_ok();
                        self.push_toast(
                            now,
                            format!(
                                "{} Isi laporan sudah disalin: tempel ke email untuk {CONTACT_EMAIL}.",
                                if opened { "Aplikasi email dibuka." } else { "Aplikasi email tidak bisa dibuka." }
                            ),
                            !opened,
                        );
                    }
                });
            });
        if close || !open {
            self.bug_open = false;
        }
    }

    /// Unduhan installer aplikasi (bukan launcher) yang sedang berjalan atau siap dipasang.
    fn draw_downloads_window(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        let rows: Vec<(String, String, DownloadState)> = self
            .guard_snap
            .downloads
            .iter()
            .filter(|(key, _)| key.as_str() != LAUNCHER_MANIFEST_ID)
            .map(|(key, state)| {
                let name = self
                    .registry
                    .apps
                    .iter()
                    .find(|a| &a.id == key)
                    .map_or_else(|| key.clone(), |a| a.name.clone());
                (key.clone(), name, state.clone())
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        egui::Window::new("Unduhan")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -46.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                for (key, name, state) in rows {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&name).strong().color(pal.ink_primary));
                        match state {
                            DownloadState::Downloading => {
                                ui.spinner();
                                ui.label(RichText::new("Mengunduh…").size(FONT_CAPTION));
                            }
                            DownloadState::Ready(path) => {
                                ui.label(
                                    RichText::new("Terverifikasi")
                                        .size(FONT_CAPTION)
                                        .color(pal.emerald),
                                );
                                if ui.button("Pasang").clicked() {
                                    match suite::run_installer(&path) {
                                        Ok(()) => {
                                            self.guard.clear_download(&key);
                                            let text = format!(
                                                "Installer {name} dibuka. Setelah selesai, klik Scan PC."
                                            );
                                            self.push_toast(now, text, false);
                                        }
                                        Err(e) => self.push_toast(now, e, true),
                                    }
                                }
                                if ui.button("Batal").clicked() {
                                    self.guard.clear_download(&key);
                                }
                            }
                            DownloadState::Failed(err) => {
                                ui.label(
                                    RichText::new(format!("Gagal: {err}"))
                                        .size(FONT_CAPTION)
                                        .color(pal.danger),
                                );
                                if ui.button("Tutup").clicked() {
                                    self.guard.clear_download(&key);
                                }
                            }
                        }
                    });
                }
            });
    }
}
