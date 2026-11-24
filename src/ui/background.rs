//! Pekerjaan lama yang dijalankan di thread latar supaya jendela tidak "Not Responding":
//! pindai komputer (`reg query` + cek disk + hash), impor aplikasi (hash berkas sampai 512 MB),
//! dan menyelesaikan pemasangan (menyalin berkas). Selama berjalan, tombolnya menampilkan
//! spinner dan pil "sedang bekerja" tampil di bawah jendela.
//!
//! Impor dan pasang dikerjakan pada salinan registry. Saat selesai, hanya aplikasi yang
//! bersangkutan dan catatan aktivitas barunya yang digabung ke registry saat ini, jadi
//! perubahan lain yang terjadi selama menunggu tidak hilang.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::{self, Frame, Margin, RichText, Stroke};

use super::{Palette, SimpulLauncherApp};
use crate::models::{SimpulApp, SimpulRegistry, FONT_CAPTION};
use crate::registry::{
    apply_discovered_apps, current_launchable_target, discover_installed_apps_on_pc, hash_discovered_apps,
    DiscoveredInstalledApp,
};

/// Untuk apa pemindaian dijalankan: menentukan pesan yang muncul setelahnya.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanReason {
    /// Saat launcher dibuka: tanpa pesan kecuali ada aplikasi baru.
    Startup,
    /// Tombol "Pindai Komputer" di beranda.
    Manual,
    /// Tombol "Pindai ulang" di panel Detail.
    Details,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum JobKind {
    Scan(ScanReason),
    Import,
    Install(String),
}

pub(super) enum JobOutput {
    Scan(Vec<DiscoveredInstalledApp>, BTreeMap<String, String>),
    /// Registry salinan setelah pekerjaan + aplikasi hasilnya, atau pesan galat.
    Registry(Result<(SimpulRegistry, SimpulApp), String>),
}

pub(super) struct BgJob {
    pub(super) kind: JobKind,
    pub(super) label: String,
    started: f64,
    /// Catatan aktivitas teratas saat pekerjaan dimulai, untuk mengenali catatan baru.
    first_event: Option<(String, String)>,
    slot: Arc<Mutex<Option<JobOutput>>>,
}

fn event_key(reg: &SimpulRegistry) -> Option<(String, String)> {
    reg.events.first().map(|e| (e.timestamp.clone(), e.summary.clone()))
}

impl SimpulLauncherApp {
    fn start_job(&mut self, kind: JobKind, label: impl Into<String>, now: f64, work: impl FnOnce() -> JobOutput + Send + 'static) {
        let slot = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&slot);
        let repaint = self.egui_ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("simpul-latar".into())
            .spawn(move || {
                let out = work();
                *shared.lock().unwrap_or_else(PoisonError::into_inner) = Some(out);
                repaint.request_repaint();
            });
        if spawned.is_err() {
            self.push_toast(now, "Pekerjaan latar tidak bisa dimulai. Coba lagi.", true);
            return;
        }
        let first_event = event_key(&self.registry);
        self.jobs.push(BgJob { kind, label: label.into(), started: now, first_event, slot });
        self.egui_ctx.request_repaint();
    }

    pub(super) fn is_scanning(&self) -> bool {
        self.jobs.iter().any(|j| matches!(j.kind, JobKind::Scan(_)))
    }
    pub(super) fn is_importing(&self) -> bool {
        self.jobs.iter().any(|j| j.kind == JobKind::Import)
    }

    /// Pindai aplikasi Simpul yang terpasang. Panggilan ganda selama pemindaian berjalan diabaikan.
    pub(super) fn start_scan(&mut self, reason: ScanReason, now: f64) {
        if self.is_scanning() {
            return;
        }
        self.start_job(JobKind::Scan(reason), "Memindai komputer…", now, || {
            let found = discover_installed_apps_on_pc();
            let hashes = hash_discovered_apps(&found);
            JobOutput::Scan(found, hashes)
        });
    }

    /// Jalankan impor atau pemasangan pada salinan registry di latar.
    pub(super) fn start_registry_job(
        &mut self,
        kind: JobKind,
        label: impl Into<String>,
        now: f64,
        work: impl FnOnce(&mut SimpulRegistry) -> Result<SimpulApp, String> + Send + 'static,
    ) {
        let mut copy = self.registry.clone();
        self.start_job(kind, label, now, move || {
            JobOutput::Registry(work(&mut copy).map(|app| (copy, app)))
        });
    }

    /// Gabungkan hasil impor/pasang: aplikasi yang bersangkutan + catatan aktivitas barunya.
    fn merge_registry_result(&mut self, done: &SimpulRegistry, app: &SimpulApp, first_event: Option<(String, String)>) {
        let fresh = done
            .apps
            .iter()
            .find(|a| a.id == app.id)
            .cloned()
            .unwrap_or_else(|| app.clone());
        if let Some(slot) = self.registry.apps.iter_mut().find(|a| a.id == fresh.id) {
            *slot = fresh;
        } else {
            let pos = done
                .apps
                .iter()
                .position(|a| a.id == fresh.id)
                .unwrap_or(self.registry.apps.len())
                .min(self.registry.apps.len());
            self.registry.apps.insert(pos, fresh);
        }
        let new_events: Vec<_> = done
            .events
            .iter()
            .take_while(|e| Some((e.timestamp.clone(), e.summary.clone())) != first_event)
            .cloned()
            .collect();
        for (i, e) in new_events.into_iter().enumerate() {
            self.registry.events.insert(i, e);
        }
        self.registry.events.truncate(40);
    }

    /// Dipanggil tiap frame: ambil pekerjaan yang sudah selesai dan terapkan hasilnya.
    pub(super) fn poll_jobs(&mut self, now: f64) {
        if self.jobs.is_empty() {
            return;
        }
        let mut finished = Vec::new();
        let mut i = 0;
        while i < self.jobs.len() {
            let out = self.jobs[i].slot.lock().unwrap_or_else(PoisonError::into_inner).take();
            match out {
                Some(out) => finished.push((self.jobs.remove(i), out)),
                None => i += 1,
            }
        }
        for (job, out) in finished {
            match (job.kind, out) {
                (JobKind::Scan(reason), JobOutput::Scan(found, hashes)) => {
                    let updated = apply_discovered_apps(&mut self.registry, found, &hashes);
                    if !updated.is_empty() || reason != ScanReason::Startup {
                        self.persist();
                    }
                    self.refresh_certificates();
                    self.exec_path_app_id = None;
                    let linked = self
                        .registry
                        .apps
                        .iter()
                        .filter(|a| current_launchable_target(a).is_some())
                        .count();
                    let text = match reason {
                        ScanReason::Startup if updated.is_empty() => None,
                        ScanReason::Startup => Some(format!("{} aplikasi terpasang ditemukan di komputer ini.", updated.len())),
                        ScanReason::Manual if updated.is_empty() => {
                            Some(format!("Pindai selesai: {linked} aplikasi terpasang siap dibuka."))
                        }
                        ScanReason::Manual => {
                            Some(format!("Pindai selesai: {} aplikasi diperbarui ({linked} siap dibuka).", updated.len()))
                        }
                        ScanReason::Details if updated.is_empty() => Some("Pindai selesai: semua lokasi sudah cocok.".to_string()),
                        ScanReason::Details => Some(format!("Pindai selesai: {} pembaruan ditemukan.", updated.len())),
                    };
                    if let Some(text) = text {
                        self.push_toast(now, text, false);
                    }
                }
                (JobKind::Import, JobOutput::Registry(result)) => match result {
                    Ok((done, added)) => {
                        self.merge_registry_result(&done, &added, job.first_event);
                        self.persist();
                        self.import_form.open = false;
                        self.import_form.error_banner = None;
                        self.push_toast(
                            now,
                            format!(
                                "{} mengimpor {} v{} ({})",
                                added.origin.label(),
                                added.name,
                                added.latest_version,
                                added.profile.short_label()
                            ),
                            false,
                        );
                    }
                    Err(e) => self.import_form.error_banner = Some(e),
                },
                (JobKind::Install(_), JobOutput::Registry(result)) => match result {
                    Ok((done, updated)) => {
                        self.merge_registry_result(&done, &updated, job.first_event);
                        self.persist();
                        self.push_toast(now, format!("{} v{} terpasang", updated.name, updated.latest_version), false);
                    }
                    Err(e) => self.push_toast(now, e, true),
                },
                _ => {}
            }
        }
    }

    /// Pil "sedang bekerja" di bawah tengah jendela selama ada pekerjaan latar.
    pub(super) fn draw_busy_indicator(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        if self.jobs.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let job = &self.jobs[0];
        let secs = (now - job.started).max(0.0);
        let mut text = job.label.clone();
        if self.jobs.len() > 1 {
            text.push_str(&format!(" (+{} lainnya)", self.jobs.len() - 1));
        }
        if secs >= 3.0 {
            text.push_str(&format!("  {secs:.0} dtk"));
        }
        egui::Area::new(egui::Id::new("simpul_busy_area"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(pal.bg_elevated)
                    .stroke(Stroke::new(1.0_f32, pal.accent))
                    .corner_radius(18.0)
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(14.0).color(pal.accent));
                            ui.label(RichText::new(text).size(FONT_CAPTION).color(pal.ink_primary));
                        });
                    });
            });
    }
}

/// Isi tombol yang sedang menunggu pekerjaan latar: spinner + teks.
pub(super) fn busy_button(ui: &mut egui::Ui, pal: Palette, text: &str) {
    ui.add_enabled_ui(false, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0).color(pal.accent));
            ui.label(RichText::new(text).size(FONT_CAPTION).color(pal.ink_secondary));
        });
    });
}
