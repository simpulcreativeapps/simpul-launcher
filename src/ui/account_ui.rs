//! Jendela Akun: masuk dengan Google, keluar, hapus akun, dan tanda sertifikat.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText, Vec2};

use super::{Palette, SimpulLauncherApp};
use crate::account::{self, Account, CertificateStatus};
use crate::models::{FONT_BODY, FONT_CAPTION, FONT_MICRO, PRIMARY_BTN_HEIGHT, SECONDARY_BTN_HEIGHT};
use crate::suite::{self, LAUNCHER_MANIFEST_ID, LAUNCHER_VERSION};

/// Hasil pekerjaan akun yang berjalan di thread latar.
pub(super) enum AccountEvent {
    LoggedIn(Account),
    LoggedOut,
    Deleted,
    CertificateIssued,
    ProfileLoaded(account::Profile),
    ProfileSaved(account::Profile),
    Failed(String),
}

pub(super) type AccountSlot = Arc<Mutex<Option<AccountEvent>>>;

/// Lama menunggu pengguna selesai di browser sebelum menyerah.
const LOGIN_WAIT_SECONDS: u64 = 300;

impl SimpulLauncherApp {
    /// Periksa ulang sertifikat tersimpan untuk launcher dan semua aplikasi terpasang.
    pub(super) fn refresh_certificates(&mut self) {
        self.cert_cache.clear();
        self.cert_cache.insert(
            LAUNCHER_MANIFEST_ID.to_string(),
            account::certificate_status(LAUNCHER_MANIFEST_ID),
        );
        for app in &self.registry.apps {
            if app.install_state.is_installed() {
                self.cert_cache.insert(
                    app.id.clone(),
                    account::certificate_status(suite::manifest_id_for(&app.id)),
                );
            }
        }
    }

    /// Keterangan sertifikat untuk baris status kartu aplikasi.
    pub(super) fn certificate_label(&self, app_id: &str) -> &'static str {
        match self.cert_cache.get(app_id) {
            Some(CertificateStatus::Certified) => "Bersertifikat",
            Some(CertificateStatus::Invalid) => "Sertifikat tidak sah",
            _ => "Tanpa sertifikat",
        }
    }

    fn run_account_task(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        work: impl FnOnce(&AtomicBool) -> AccountEvent + Send + 'static,
    ) {
        if self.account_task.is_some() {
            return;
        }
        let slot: AccountSlot = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        self.account_task = Some(slot.clone());
        self.account_cancel = cancel.clone();
        self.account_busy = label;
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("simpul-akun".into()).spawn(move || {
            let event = work(&cancel);
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(event);
            }
            repaint.request_repaint();
        });
        if spawned.is_err() {
            self.account_task = None;
        }
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        self.run_account_task(ctx, "Menunggu kamu selesai masuk di browser…", |cancel| {
            let (state, url) = match account::login_start() {
                Ok(v) => v,
                Err(e) => return AccountEvent::Failed(e),
            };
            if suite::open_with_system(&url).is_err() {
                return AccountEvent::Failed("browser tidak bisa dibuka".to_string());
            }
            let started = std::time::Instant::now();
            while started.elapsed().as_secs() < LOGIN_WAIT_SECONDS {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if cancel.load(Ordering::Relaxed) {
                    return AccountEvent::Failed("masuk dibatalkan".to_string());
                }
                match account::login_poll(&state) {
                    Ok(Some(found)) => {
                        // Pemasangan launcher ini langsung didaftarkan.
                        if let Err(e) = account::request_certificate(&found, LAUNCHER_MANIFEST_ID, LAUNCHER_VERSION) {
                            suite::log_event(&format!("sertifikat launcher gagal: {e}"));
                        }
                        return AccountEvent::LoggedIn(found);
                    }
                    Ok(None) => {}
                    Err(e) => return AccountEvent::Failed(e),
                }
            }
            AccountEvent::Failed("waktu masuk habis, coba lagi".to_string())
        });
    }

    /// Minta sertifikat untuk aplikasi yang baru dipasang lewat launcher.
    pub(super) fn issue_certificate(&mut self, ctx: &egui::Context, app_id: &str, version: &str) {
        let Some(found) = self.account.clone() else {
            return;
        };
        let manifest_id = suite::manifest_id_for(app_id).to_string();
        let version = version.to_string();
        self.run_account_task(ctx, "Mendaftarkan sertifikat…", move |_| {
            match account::request_certificate(&found, &manifest_id, &version) {
                Ok(()) => AccountEvent::CertificateIssued,
                Err(e) => AccountEvent::Failed(format!("sertifikat gagal: {e}")),
            }
        });
    }

    /// Ambil hasil pekerjaan akun. Dipanggil setiap frame.
    pub(super) fn poll_account(&mut self, now: f64) {
        let event = self
            .account_task
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|mut guard| guard.take()));
        let Some(event) = event else {
            return;
        };
        self.account_task = None;
        match event {
            AccountEvent::LoggedIn(found) => {
                self.push_toast(now, format!("Masuk sebagai {}.", found.email), false);
                self.account = Some(found);
                self.forget_profile();
            }
            AccountEvent::LoggedOut => {
                self.account = None;
                self.forget_profile();
                self.push_toast(now, "Kamu sudah keluar.", false);
            }
            AccountEvent::ProfileLoaded(profile) => {
                self.profile_draft = profile.clone();
                self.profile = Some(profile);
            }
            AccountEvent::ProfileSaved(profile) => {
                self.profile_draft = profile.clone();
                self.profile = Some(profile);
                self.push_toast(now, "Profil tersimpan.", false);
            }
            AccountEvent::Deleted => {
                self.account = None;
                self.forget_profile();
                self.account_confirm_delete = false;
                self.push_toast(now, "Akun dan semua datanya sudah dihapus.", false);
            }
            AccountEvent::CertificateIssued => {
                self.push_toast(now, "Sertifikat pemasangan terdaftar.", false);
            }
            AccountEvent::Failed(reason) => {
                self.push_toast(now, format!("Akun: {reason}."), true);
            }
        }
        self.refresh_certificates();
    }

    /// Buang profil dari memori (keluar, hapus akun, ganti akun).
    fn forget_profile(&mut self) {
        self.profile = None;
        self.profile_draft = account::Profile::default();
        self.profile_requested = false;
    }

    /// Form profil opsional + persetujuan penawaran. Dikembalikan: true bila "Simpan" diklik.
    fn draw_profile_form(&mut self, ui: &mut egui::Ui, pal: Palette, busy: bool) -> bool {
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(RichText::new(text).size(FONT_MICRO).color(pal.ink_muted)).wrap());
        };
        ui.label(RichText::new("Profil & preferensi").size(FONT_BODY).strong().color(pal.ink_primary));
        note(ui, "Semua isian opsional dan disimpan terenkripsi. Hanya kamu dan tim Simpul yang bisa melihatnya.");
        ui.add_space(6.0);
        if self.profile.is_none() {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new("Memuat profil…").size(FONT_CAPTION).color(pal.ink_secondary));
            });
            return false;
        }
        let draft = &mut self.profile_draft;
        egui::Grid::new("simpul_profile_grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Username").size(FONT_CAPTION).color(pal.ink_secondary));
            ui.add(
                egui::TextEdit::singleline(&mut draft.username)
                    .hint_text("contoh: rani.art")
                    .char_limit(20)
                    .desired_width(220.0),
            );
            ui.end_row();
            ui.label(RichText::new("Gender").size(FONT_CAPTION).color(pal.ink_secondary));
            let current = account::GENDERS
                .iter()
                .find(|(v, _)| *v == draft.gender)
                .map_or("Tidak diisi", |(_, l)| l);
            egui::ComboBox::from_id_salt("simpul_profile_gender")
                .selected_text(current)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (value, label) in account::GENDERS {
                        ui.selectable_value(&mut draft.gender, value.to_string(), label);
                    }
                });
            ui.end_row();
            ui.label(RichText::new("WhatsApp").size(FONT_CAPTION).color(pal.ink_secondary));
            ui.add(
                egui::TextEdit::singleline(&mut draft.whatsapp)
                    .hint_text("0812 3456 7890")
                    .char_limit(20)
                    .desired_width(220.0),
            );
            ui.end_row();
        });
        note(ui, "Username unik dan belum ditampilkan ke publik.");
        ui.add_space(10.0);
        ui.label(RichText::new("Penawaran & kemitraan (opsional)").size(FONT_CAPTION).strong().color(pal.ink_primary));
        ui.checkbox(
            &mut draft.consent_email,
            RichText::new(account::CONSENT_EMAIL_TEXT).size(FONT_CAPTION).color(pal.ink_primary),
        );
        let has_wa = !draft.whatsapp.trim().is_empty();
        if !has_wa {
            draft.consent_whatsapp = false;
        }
        ui.add_enabled(
            has_wa,
            egui::Checkbox::new(
                &mut draft.consent_whatsapp,
                RichText::new(account::CONSENT_WHATSAPP_TEXT).size(FONT_CAPTION).color(pal.ink_primary),
            ),
        )
        .on_disabled_hover_text("Isi nomor WhatsApp dulu.");
        note(
            ui,
            "Tanpa centang, kami tidak mengirim penawaran apa pun. Persetujuan bisa dicabut kapan saja: \
             hapus centangnya lalu Simpan.",
        );
        ui.add_space(8.0);
        let changed = self.profile.as_ref() != Some(&self.profile_draft);
        if busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new(self.account_busy).size(FONT_CAPTION).color(pal.ink_secondary));
            });
            false
        } else {
            ui.add_enabled(
                changed,
                egui::Button::new(RichText::new("Simpan profil").strong().color(pal.on_accent))
                    .min_size(Vec2::new(140.0, SECONDARY_BTN_HEIGHT))
                    .fill(pal.accent),
            )
            .clicked()
        }
    }

    pub(super) fn draw_account_window(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if !self.account_open {
            self.profile_requested = false;
            return;
        }
        // Muat profil sekali tiap jendela dibuka (di latar).
        if self.account.is_some() && self.profile.is_none() && !self.profile_requested && self.account_task.is_none() {
            self.profile_requested = true;
            if let Some(found) = self.account.clone() {
                self.run_account_task(ctx, "Memuat profil…", move |_| match account::fetch_profile(&found) {
                    Ok(p) => AccountEvent::ProfileLoaded(p),
                    Err(e) => AccountEvent::Failed(format!("profil tidak bisa dimuat: {e}")),
                });
            }
        }
        let mut want_save = false;
        let mut open = true;
        let busy = self.account_task.is_some();
        let mut want_login = false;
        let mut want_logout = false;
        let mut want_delete = false;
        egui::Window::new("Akun Simpul")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(420.0, 0.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                let note = |ui: &mut egui::Ui, text: &str| {
                    ui.add(
                        egui::Label::new(RichText::new(text).size(FONT_CAPTION).color(pal.ink_muted)).wrap(),
                    );
                };
                match self.account.clone() {
                    None => {
                        ui.label(
                            RichText::new("Masuk itu opsional")
                                .size(FONT_BODY)
                                .strong()
                                .color(pal.ink_primary),
                        );
                        ui.add_space(4.0);
                        note(
                            ui,
                            "Semua aplikasi gratis tetap bisa dipakai tanpa akun. Dengan akun, pemasangan \
                             lewat launcher mendapat sertifikat resmi yang berlaku hanya di komputer ini.",
                        );
                        ui.add_space(6.0);
                        note(
                            ui,
                            "Yang disimpan: alamat email dan nama dari akun Google-mu, profil opsional \
                             (username, gender, WhatsApp), dan daftar sertifikat pemasangan. Data pribadi \
                             disimpan terenkripsi. Tidak ada kata sandi. Akun bisa dihapus kapan saja.",
                        );
                        ui.add_space(12.0);
                        if busy {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(16.0));
                                ui.label(RichText::new(self.account_busy).size(FONT_CAPTION).color(pal.ink_secondary));
                            });
                            ui.add_space(6.0);
                            if ui
                                .add(egui::Button::new("Batal").min_size(Vec2::new(90.0, SECONDARY_BTN_HEIGHT)))
                                .clicked()
                            {
                                self.account_cancel.store(true, Ordering::Relaxed);
                            }
                        } else if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Masuk dengan Google").strong().color(pal.on_accent),
                                )
                                .min_size(Vec2::new(200.0, PRIMARY_BTN_HEIGHT))
                                .fill(pal.accent),
                            )
                            .on_hover_text("Membuka halaman masuk Google di browser")
                            .clicked()
                        {
                            want_login = true;
                        }
                    }
                    Some(found) => {
                        ui.label(RichText::new(&found.name).size(FONT_BODY).strong().color(pal.ink_primary));
                        ui.label(RichText::new(&found.email).size(FONT_CAPTION).color(pal.ink_secondary));
                        ui.add_space(8.0);
                        let launcher_cert = self.cert_cache.get(LAUNCHER_MANIFEST_ID).copied();
                        let (text, color) = match launcher_cert {
                            Some(CertificateStatus::Certified) => {
                                ("Pemasangan launcher ini bersertifikat.", pal.emerald)
                            }
                            Some(CertificateStatus::Invalid) => (
                                "Sertifikat di komputer ini tidak sah (salinan dari pemasangan lain).",
                                pal.danger,
                            ),
                            _ => ("Pemasangan launcher ini belum bersertifikat.", pal.ink_muted),
                        };
                        ui.label(RichText::new(text).size(FONT_CAPTION).color(color));
                        ui.add_space(4.0);
                        note(
                            ui,
                            "Sertifikat terikat ke komputer ini. Disalin ke komputer lain, sertifikat tidak berlaku.",
                        );
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);
                        want_save = self.draw_profile_form(ui, pal, busy);
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);
                        if busy {
                            ui.label(RichText::new("Tunggu sebentar…").size(FONT_CAPTION).color(pal.ink_muted));
                        } else if self.account_confirm_delete {
                            ui.label(
                                RichText::new(
                                    "Hapus akun? Akun, sesi, dan semua sertifikat dihapus dari server. \
                                     Aplikasi dan file kerjamu tidak disentuh.",
                                )
                                .size(FONT_CAPTION)
                                .color(pal.ink_primary),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Ya, hapus akun").strong().color(pal.on_danger),
                                        )
                                        .min_size(Vec2::new(130.0, SECONDARY_BTN_HEIGHT))
                                        .fill(pal.danger),
                                    )
                                    .clicked()
                                {
                                    want_delete = true;
                                }
                                if ui
                                    .add(egui::Button::new("Batal").min_size(Vec2::new(80.0, SECONDARY_BTN_HEIGHT)))
                                    .clicked()
                                {
                                    self.account_confirm_delete = false;
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(egui::Button::new("Keluar").min_size(Vec2::new(90.0, SECONDARY_BTN_HEIGHT)))
                                    .clicked()
                                {
                                    want_logout = true;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Hapus akun…").color(pal.danger))
                                            .min_size(Vec2::new(110.0, SECONDARY_BTN_HEIGHT)),
                                    )
                                    .clicked()
                                {
                                    self.account_confirm_delete = true;
                                }
                            });
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Masuk lewat Google. Launcher tidak pernah melihat kata sandimu.")
                        .size(FONT_MICRO)
                        .color(pal.ink_muted),
                );
            });

        if want_login {
            self.start_login(ctx);
        }
        if want_save {
            if let Some(found) = self.account.clone() {
                let draft = self.profile_draft.clone();
                self.run_account_task(ctx, "Menyimpan profil…", move |_| match account::save_profile(&found, &draft) {
                    Ok(p) => AccountEvent::ProfileSaved(p),
                    Err(e) => AccountEvent::Failed(e),
                });
            }
        }
        if want_logout {
            if let Some(found) = self.account.clone() {
                self.run_account_task(ctx, "Keluar…", move |_| {
                    account::logout(&found);
                    AccountEvent::LoggedOut
                });
            }
        }
        if want_delete {
            if let Some(found) = self.account.clone() {
                self.run_account_task(ctx, "Menghapus akun…", move |_| match account::delete_account(&found) {
                    Ok(()) => AccountEvent::Deleted,
                    Err(e) => AccountEvent::Failed(e),
                });
            }
        }
        if !open {
            self.account_open = false;
            self.account_confirm_delete = false;
        }
        let _ = now;
    }
}
