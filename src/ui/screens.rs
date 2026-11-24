//! Carousel tangkapan layar di laci Detail aplikasi.
//!
//! Gambarnya sama dengan carousel di situs Simpul Creative Apps (WebP 1280 px, ditanam di
//! binary). Gambar baru didekode saat pertama kali ditampilkan, satu per satu.

use std::collections::HashMap;

use eframe::egui::{self, Align, Layout, RichText, Sense, TextureHandle, TextureOptions, Vec2};

use super::{Palette, SimpulLauncherApp};
use crate::models::{FONT_CAPTION, FONT_MICRO, SECONDARY_BTN_HEIGHT};
use crate::suite::manifest_id_for;

pub(super) struct Shot {
    bytes: &'static [u8],
    caption: &'static str,
}

macro_rules! shot {
    ($file:literal, $caption:literal) => {
        Shot { bytes: include_bytes!(concat!("../../assets/preview/", $file)), caption: $caption }
    };
}

/// Tangkapan layar asli Simpul Sync 4.1.11 (mode demo bawaan aplikasi).
static SYNC_SHOTS: [Shot; 12] = [
    shot!("sync/03-hasil-sinkron.webp", "Sekali klik Synchronize, semua klip sejajar otomatis lewat bentuk gelombang suaranya."),
    shot!("sync/02-media-dimuat.webp", "Rekaman tiap kamera dan perekam audio masuk ke track masing-masing."),
    shot!("sync/04-ekspor-timeline.webp", "Ekspor timeline ke Premiere Pro, DaVinci Resolve, Final Cut Pro, EDIUS, atau Vegas."),
    shot!("sync/01-mulai.webp", "Mulai: seret folder rekaman, video, atau audio ke jendela."),
    shot!("sync/05-pengaturan.webp", "Pengaturan: tampilan timeline dan lokasi file sementara sinkron."),
    shot!("sync/06-tentang.webp", "Tentang Simpul Sync: tanpa telemetri, hanya mengecek versi bertanda tangan."),
    shot!("sync/07-donasi.webp", "Dukung pengembangan lewat Saweria, langsung dari aplikasi."),
    shot!("sync/08-lapor-bug.webp", "Lapor bug: isi laporan ditampilkan dulu sebelum dikirim lewat email."),
    shot!("sync/09-tema-terang.webp", "Tema terang."),
    shot!("sync/10-tema-expressive.webp", "Tema Expressive."),
    shot!("sync/11-tema-liquid-glass.webp", "Tema Liquid Glass. Tema yang sama berlaku untuk semua aplikasi Simpul."),
    shot!("sync/12-installer.webp", "Installer Windows: pintasan Desktop, Start Menu, dan CLI di PATH."),
];

/// Kunci gambar untuk sebuah aplikasi. Aplikasi yang ditemukan lewat pemindaian Windows Registry
/// bisa memakai id dari nama tampilannya (mis. "simpul-sync-version-4-2-0"), jadi awalan id ikut dicek.
pub(super) fn shots_key(app_id: &str) -> Option<&'static str> {
    if manifest_id_for(app_id) == "sync" || app_id.starts_with("simpul-sync") || app_id.starts_with("simpulsync") {
        Some("sync")
    } else {
        None
    }
}

fn shots_for(app_id: &str) -> &'static [Shot] {
    match shots_key(app_id) {
        Some("sync") => &SYNC_SHOTS,
        _ => &[],
    }
}

/// Tekstur yang sudah didekode dan posisi carousel per aplikasi.
#[derive(Default)]
pub(super) struct ScreenCache {
    textures: HashMap<(String, usize), Option<TextureHandle>>,
    index: HashMap<String, usize>,
}

fn decode(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, color, TextureOptions::LINEAR))
}

impl SimpulLauncherApp {
    /// Carousel tangkapan layar; tidak menggambar apa pun bila aplikasi belum punya gambar.
    pub(super) fn draw_screenshot_carousel(&mut self, ui: &mut egui::Ui, app_id: &str, pal: Palette) {
        let shots = shots_for(app_id);
        if shots.is_empty() {
            return;
        }
        let key = shots_key(app_id).unwrap_or(app_id).to_string();
        let count = shots.len();
        let mut idx = self.screens.index.get(&key).copied().unwrap_or(0).min(count - 1);

        let tex = self
            .screens
            .textures
            .entry((key.clone(), idx))
            .or_insert_with(|| decode(ui.ctx(), &format!("shot_{key}_{idx}"), shots[idx].bytes))
            .clone();

        ui.add_space(12.0);
        ui.label(RichText::new("Tampilan aplikasi").size(FONT_CAPTION).strong().color(pal.ink_muted));
        ui.add_space(6.0);

        // Bingkai 16:10 selebar laci; gambar dengan rasio lain (installer) diletakkan di tengah.
        let width = ui.available_width();
        let frame_size = Vec2::new(width, width * 10.0 / 16.0);
        let (frame, resp) = ui.allocate_exact_size(frame_size, Sense::click());
        ui.painter().rect_filled(frame, pal.card_radius.min(12.0), egui::Color32::from_rgb(10, 11, 17));
        if let Some(t) = &tex {
            let [tw, th] = t.size();
            let scale = (frame.width() / tw as f32).min(frame.height() / th as f32);
            let size = Vec2::new(tw as f32 * scale, th as f32 * scale);
            let rect = egui::Rect::from_center_size(frame.center(), size);
            ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        } else {
            ui.painter().text(frame.center(), egui::Align2::CENTER_CENTER, "Gambar tidak bisa dimuat", egui::FontId::proportional(FONT_CAPTION), pal.ink_muted);
        }
        ui.painter().rect_stroke(frame, pal.card_radius.min(12.0), egui::Stroke::new(1.0_f32, pal.border), egui::StrokeKind::Inside);
        // Klik gambar = gambar berikutnya.
        if resp.on_hover_text("Klik untuk gambar berikutnya").clicked() {
            idx = (idx + 1) % count;
        }

        ui.add_space(6.0);
        ui.add(
            egui::Label::new(
                RichText::new(format!("{}/{}  {}", idx + 1, count, shots[idx].caption))
                    .size(FONT_CAPTION)
                    .color(pal.ink_secondary),
            )
            .wrap(),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("‹ Sebelumnya").min_size(Vec2::new(0.0, SECONDARY_BTN_HEIGHT)))
                .clicked()
            {
                idx = (idx + count - 1) % count;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("Berikutnya ›").min_size(Vec2::new(0.0, SECONDARY_BTN_HEIGHT)))
                    .clicked()
                {
                    idx = (idx + 1) % count;
                }
                ui.label(RichText::new(format!("{} dari {}", idx + 1, count)).size(FONT_MICRO).color(pal.ink_muted));
            });
        });
        self.screens.index.insert(key, idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semua_tangkapan_layar_sync_bisa_didekode() {
        for s in &SYNC_SHOTS {
            let img = image::load_from_memory(s.bytes).expect("webp terdekode");
            assert!(img.width() >= 600 && img.height() >= 400);
            assert!(!s.caption.is_empty());
        }
        assert_eq!(shots_for("simpulsync-4").len(), 12);
        assert_eq!(shots_for("simpul-sync-version-4-2-0").len(), 12);
        assert!(shots_for("capr").is_empty());
    }
}
