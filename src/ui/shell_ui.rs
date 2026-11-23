//! Kerangka layar: huruf, bilah samping, bilah atas, dan banner.

use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Frame, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, TextEdit, UiBuilder, Vec2,
};

use super::{Palette, RightDrawer, SimpulLauncherApp};
use crate::models::{FilterTab, ThemeChoice, FONT_BODY, FONT_CAPTION, FONT_MICRO};
use crate::registry::{current_launchable_target, detect_integrations, reconcile_installed_apps};
use crate::suite::{self, LAUNCHER_VERSION};

pub(super) const SIDEBAR_WIDTH: f32 = 232.0;
const NAV_ROW_HEIGHT: f32 = 44.0;
const TOPBAR_CONTROL_HEIGHT: f32 = 42.0;
const HERO_HEIGHT: f32 = 268.0;
/// Banner selalu berlatar gelap di semua tema, jadi tintanya tetap.
const HERO_FILL: Color32 = Color32::from_rgb(12, 15, 48);
const HERO_INK: Color32 = Color32::from_rgb(244, 246, 255);
const HERO_INK_SOFT: Color32 = Color32::from_rgb(200, 208, 240);
const HERO_HIGHLIGHT: Color32 = Color32::from_rgb(96, 190, 255);

/// Halaman yang dipilih di bilah samping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavPage {
    Home,
    Apps,
    Updates,
}

/// Keluarga huruf tebal untuk judul dan tombol.
pub(super) fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Pasang huruf antarmuka. Di Windows dipakai Segoe UI milik sistem (dibaca saat berjalan,
/// tidak ikut dibagikan); di tempat lain huruf bawaan egui. Keluarga "bold" selalu ada.
pub(super) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let read = |names: &[&str]| -> Option<Vec<u8>> {
        if !cfg!(target_os = "windows") {
            return None;
        }
        let root = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        names
            .iter()
            .find_map(|name| std::fs::read(format!("{root}\\Fonts\\{name}")).ok())
    };
    if let Some(bytes) = read(&["segoeui.ttf"]) {
        fonts
            .font_data
            .insert("ui-regular".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui-regular".to_owned());
    }
    let mut bold_family = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(bytes) = read(&["seguisb.ttf", "segoeuib.ttf"]) {
        fonts
            .font_data
            .insert("ui-bold".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        bold_family.insert(0, "ui-bold".to_owned());
    }
    fonts.families.insert(bold(), bold_family);
    ctx.set_fonts(fonts);
}

fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

impl SimpulLauncherApp {
    // ------------------------------------------------------------ bilah samping

    fn nav_row(
        &self,
        ui: &mut egui::Ui,
        pal: Palette,
        icon: &str,
        label: &str,
        selected: bool,
        badge: usize,
    ) -> bool {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), NAV_ROW_HEIGHT), Sense::click());
        let hovered = resp.hovered();
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 12.0, pal.accent_soft);
        } else if hovered {
            painter.rect_filled(rect, 12.0, pal.bg_subtle);
        }
        let ink = if selected { pal.accent_ink } else { pal.ink_secondary };
        painter.text(
            Pos2::new(rect.left() + 26.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(17.0),
            ink,
        );
        painter.text(
            Pos2::new(rect.left() + 52.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            if selected { FontId::new(FONT_BODY, bold()) } else { FontId::proportional(FONT_BODY) },
            if selected { pal.accent_ink } else { pal.ink_primary },
        );
        if badge > 0 {
            let center = Pos2::new(rect.right() - 22.0, rect.center().y);
            painter.circle_filled(center, 11.0, pal.accent);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                badge.to_string(),
                FontId::new(FONT_MICRO, bold()),
                pal.on_accent,
            );
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    pub(super) fn draw_sidebar(&mut self, ui: &mut egui::Ui, pal: Palette) {
        let update_count = self.pending_update_count();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(76.0), Sense::hover());
            if let Some(ref icon) = self.textures.icon {
                egui::Image::new(icon).corner_radius(18.0).paint_at(ui, mark);
            } else {
                crate::icons::paint_simpul_brand_mark(ui.painter(), mark);
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("Simpul").family(bold()).size(25.0).color(pal.ink_primary));
                ui.label(
                    RichText::new("Creative Apps Launcher")
                        .size(FONT_CAPTION + 1.0)
                        .color(pal.ink_muted),
                );
            });
        });
        ui.add_space(22.0);

        ui.spacing_mut().item_spacing.y = 6.0;
        if self.nav_row(ui, pal, "🏠", "Beranda", self.nav == NavPage::Home, 0) {
            self.nav = NavPage::Home;
            self.filter = FilterTab::All;
        }
        if self.nav_row(ui, pal, "⊞", "Aplikasi", self.nav == NavPage::Apps, 0) {
            self.nav = NavPage::Apps;
            self.filter = FilterTab::Installed;
        }
        if self.nav_row(ui, pal, "🔄", "Pembaruan", self.nav == NavPage::Updates, update_count) {
            self.nav = NavPage::Updates;
            self.filter = FilterTab::Updates;
        }
        if self.nav_row(ui, pal, "⚙", "Pengaturan", self.settings_open, 0) {
            self.settings_open = true;
        }
        if self.nav_row(ui, pal, "ℹ", "Tentang", self.about_open, 0) {
            self.about_open = true;
            self.bug_open = false;
            self.donate_open = false;
            self.account_open = false;
        }

        // Kartu ajakan di dasar bilah samping.
        ui.add_space((ui.available_height() - 196.0).max(10.0));
        {
            Frame::new()
                .fill(pal.bg_subtle)
                .stroke(Stroke::new(1.0_f32, pal.border))
                .corner_radius(16.0)
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                  ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(
                        RichText::new("Kreativitas\nlebih mudah")
                            .family(bold())
                            .size(FONT_BODY + 2.0)
                            .color(pal.ink_primary),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new("Semua aplikasi kreatif Simpul, gratis dan open source, di satu tempat.")
                                .size(FONT_CAPTION)
                                .color(pal.ink_muted),
                        )
                        .wrap(),
                    );
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Pelajari lebih lanjut  →")
                                    .family(bold())
                                    .size(FONT_CAPTION)
                                    .color(pal.accent_ink),
                            )
                            .min_size(Vec2::new(ui.available_width(), 34.0))
                            .corner_radius(17.0)
                            .fill(pal.bg_card)
                            .stroke(Stroke::new(1.0_f32, pal.border)),
                        )
                        .on_hover_text(suite::MAKER_URL)
                        .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(suite::MAKER_URL));
                    }
                  });
                });
        }
    }

    fn pending_update_count(&self) -> usize {
        let apps = self.registry.apps.iter().filter(|a| a.has_update()).count();
        let launcher = self
            .guard_snap
            .launcher
            .as_ref()
            .is_some_and(|r| r.update.is_some());
        apps + usize::from(launcher)
    }

    // ------------------------------------------------------------ bilah atas

    pub(super) fn draw_topbar(&mut self, ui: &mut egui::Ui, pal: Palette, now: f64) {
        let update_count = self.pending_update_count();
        ui.horizontal(|ui| {
            ui.set_min_height(TOPBAR_CONTROL_HEIGHT);
            ui.spacing_mut().item_spacing.x = 10.0;

            // Pencarian: satu-satunya kolom cari di layar.
            let search_width = (ui.available_width() - 470.0).clamp(180.0, 900.0);
            Frame::new()
                .fill(pal.bg_subtle)
                .stroke(Stroke::new(1.0_f32, pal.border))
                .corner_radius(TOPBAR_CONTROL_HEIGHT / 2.0)
                .inner_margin(Margin::symmetric(16, 9))
                .show(ui, |ui| {
                    ui.set_width(search_width);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍").size(FONT_BODY + 1.0).color(pal.ink_muted));
                        ui.add(
                            TextEdit::singleline(&mut self.search_query)
                                .frame(false)
                                .hint_text("Cari aplikasi, misalnya: Capr, Sync…")
                                .desired_width(f32::INFINITY)
                                .font(FontId::proportional(FONT_BODY)),
                        );
                    });
                });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let pill = TOPBAR_CONTROL_HEIGHT / 2.0;

                // Akun dan menu lain.
                let initial = self
                    .account
                    .as_ref()
                    .and_then(|a| a.name.chars().next())
                    .map_or_else(|| "Masuk".to_string(), |c| c.to_uppercase().to_string());
                let mut open_account = false;
                let mut check_updates = false;
                let mut toggle_hub = false;
                let mut open_bug = false;
                egui::menu::menu_custom_button(
                    ui,
                    egui::Button::new(
                        RichText::new(format!("{initial}  ⏷")).family(bold()).size(FONT_BODY).color(pal.on_accent),
                    )
                    .min_size(Vec2::new(72.0, TOPBAR_CONTROL_HEIGHT))
                    .corner_radius(pill)
                    .fill(pal.accent),
                    |ui| {
                        ui.set_min_width(210.0);
                        if let Some(found) = &self.account {
                            ui.label(RichText::new(&found.name).family(bold()).color(pal.ink_primary));
                            ui.label(RichText::new(&found.email).size(FONT_CAPTION).color(pal.ink_muted));
                            ui.separator();
                        }
                        let label = if self.account.is_some() { "Akun Simpul" } else { "Masuk dengan Google" };
                        if ui.button(label).clicked() {
                            open_account = true;
                            ui.close_menu();
                        }
                        if ui.button("Cek pembaruan").clicked() {
                            check_updates = true;
                            ui.close_menu();
                        }
                        if ui.button("CLI & agen").clicked() {
                            toggle_hub = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Laporkan bug").clicked() {
                            open_bug = true;
                            ui.close_menu();
                        }
                    },
                );
                if open_account || open_bug {
                    self.account_open = open_account;
                    self.bug_open = open_bug;
                    self.bug_include_log = false;
                    self.about_open = false;
                    self.donate_open = false;
                }
                if check_updates {
                    self.integrations = detect_integrations(&self.registry.settings);
                    let ctx = ui.ctx().clone();
                    self.start_guard_check(&ctx, true);
                    self.push_toast(now, "Memeriksa pembaruan…", false);
                }
                if toggle_hub {
                    self.drawer = if matches!(self.drawer, RightDrawer::AgentHub) {
                        RightDrawer::None
                    } else {
                        RightDrawer::AgentHub
                    };
                }

                // Lonceng: jumlah pembaruan yang menunggu.
                let bell = ui
                    .add(
                        egui::Button::new(RichText::new(" ").size(FONT_BODY + 2.0))
                            .min_size(Vec2::splat(TOPBAR_CONTROL_HEIGHT))
                            .corner_radius(pill)
                            .fill(pal.bg_subtle)
                            .stroke(Stroke::new(1.0_f32, pal.border)),
                    )
                    .on_hover_text(if update_count > 0 {
                        format!("{update_count} pembaruan menunggu")
                    } else {
                        "Tidak ada pembaruan yang menunggu".to_string()
                    });
                {
                    let c = bell.rect.center();
                    let body: Vec<Pos2> = [
                        (-7.0, 4.5), (7.0, 4.5), (5.0, 1.5), (5.0, -2.0), (3.4, -5.0), (0.0, -6.4),
                        (-3.4, -5.0), (-5.0, -2.0), (-5.0, 1.5),
                    ]
                    .iter()
                    .map(|(x, y)| Pos2::new(c.x + x, c.y + y))
                    .collect();
                    ui.painter().add(egui::Shape::convex_polygon(body, pal.ink_primary, Stroke::NONE));
                    ui.painter().circle_filled(Pos2::new(c.x, c.y + 7.0), 1.7, pal.ink_primary);
                }
                if update_count > 0 {
                    ui.painter().circle_filled(
                        Pos2::new(bell.rect.right() - 11.0, bell.rect.top() + 11.0),
                        4.5,
                        pal.danger,
                    );
                }
                if bell.clicked() {
                    self.nav = NavPage::Updates;
                    self.filter = FilterTab::Updates;
                }

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("♥  Donasi").family(bold()).size(FONT_BODY).color(pal.emerald),
                        )
                        .min_size(Vec2::new(112.0, TOPBAR_CONTROL_HEIGHT))
                        .corner_radius(pill)
                        .fill(pal.emerald_soft),
                    )
                    .on_hover_text("Dukung developer Simpul lewat Saweria")
                    .clicked()
                {
                    self.open_donate();
                }

                let mut picked_theme = None;
                egui::menu::menu_custom_button(
                    ui,
                    egui::Button::new(RichText::new("☀  Tema  ⏷").size(FONT_BODY).color(pal.ink_primary))
                        .min_size(Vec2::new(116.0, TOPBAR_CONTROL_HEIGHT))
                        .corner_radius(pill)
                        .fill(pal.bg_subtle)
                        .stroke(Stroke::new(1.0_f32, pal.border)),
                    |ui| {
                        for theme in ThemeChoice::ALL {
                            let current = self.registry.settings.theme == theme;
                            if ui.selectable_label(current, theme.label()).clicked() {
                                picked_theme = Some(theme);
                                ui.close_menu();
                            }
                        }
                    },
                );
                if let Some(theme) = picked_theme {
                    self.registry.settings.theme = theme;
                    self.persist();
                }
            });
        });
    }

    // ------------------------------------------------------------ banner

    pub(super) fn draw_hero(&mut self, ui: &mut egui::Ui, pal: Palette, now: f64) {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HERO_HEIGHT), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 22.0, HERO_FILL);

        // Gambar di sisi kanan: cahaya, cincin, dan tanda Simpul. Semua lingkaran dijaga
        // tetap jauh dari sudut supaya tidak keluar dari bentuk membulat banner.
        let show_art = width > 760.0;
        if show_art {
            let center = Pos2::new(rect.right() - 250.0, rect.center().y);
            for step in 0..14 {
                let t = step as f32 / 13.0;
                let radius = 215.0 - t * 150.0;
                let alpha = (5.0 + t * 7.0) as u8;
                painter.circle_filled(
                    center,
                    radius,
                    Color32::from_rgba_unmultiplied(96, 104, 255, alpha),
                );
            }
            for (radius, alpha) in [(215.0, 22), (150.0, 34)] {
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(170, 186, 255, alpha)),
                );
            }
            let mark = Rect::from_center_size(center, Vec2::splat(140.0));
            if let Some(ref icon) = self.textures.icon {
                // Sudut gambar dibulatkan supaya latar gelap berkasnya tidak tampak sebagai kotak.
                egui::Image::new(icon)
                    .corner_radius(32.0)
                    .paint_at(ui, mark);
            } else {
                crate::icons::paint_simpul_brand_mark(&painter, mark);
            }
        }

        let text_width = if show_art { (width - 520.0).max(380.0) } else { width - 68.0 };
        let content = Rect::from_min_size(
            rect.min + Vec2::new(34.0, 26.0),
            Vec2::new(text_width, HERO_HEIGHT - 52.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
        );
        let ui = &mut child;
        ui.spacing_mut().item_spacing.y = 6.0;

        Frame::new()
            .fill(with_alpha(HERO_HIGHLIGHT, 30))
            .stroke(Stroke::new(1.0_f32, with_alpha(HERO_HIGHLIGHT, 130)))
            .corner_radius(14.0)
            .inner_margin(Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("Simpul v{LAUNCHER_VERSION}"))
                        .size(FONT_CAPTION + 1.0)
                        .color(HERO_INK),
                );
            });
        ui.add_space(4.0);
        ui.label(
            RichText::new("Semua aplikasi kreatifmu,")
                .family(bold())
                .size(30.0)
                .color(HERO_INK),
        );
        ui.label(
            RichText::new("satu tempat")
                .family(bold())
                .size(30.0)
                .color(HERO_HIGHLIGHT),
        );
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Daftar di bawah mengikuti aplikasi yang sudah dirilis. Agen seperti Claude juga bisa mengimpor aplikasi lewat CLI.",
                )
                .size(FONT_BODY + 1.0)
                .color(HERO_INK_SOFT),
            )
            .wrap(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let ghost = |text: &str| {
                egui::Button::new(RichText::new(text).size(FONT_BODY).color(HERO_INK))
                    .min_size(Vec2::new(0.0, 42.0))
                    .corner_radius(21.0)
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 110)))
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("▶   Impor Aplikasi")
                            .family(bold())
                            .size(FONT_BODY)
                            .color(Color32::WHITE),
                    )
                    .min_size(Vec2::new(168.0, 42.0))
                    .corner_radius(21.0)
                    .fill(Color32::from_rgb(52, 92, 246)),
                )
                .on_hover_text("Tambahkan aplikasi dari berkas .exe, .msi, .zip, atau manifest")
                .clicked()
            {
                self.import_form.open = true;
                self.import_form.error_banner = None;
            }
            if ui
                .add(ghost("  🖥   Pindai Komputer  "))
                .on_hover_text("Pindai ulang aplikasi Simpul yang terpasang di komputer ini")
                .clicked()
            {
                let updated = reconcile_installed_apps(&mut self.registry);
                self.persist();
                self.refresh_certificates();
                let linked = self
                    .registry
                    .apps
                    .iter()
                    .filter(|a| current_launchable_target(a).is_some())
                    .count();
                let text = if updated.is_empty() {
                    format!("Pindai selesai: {linked} aplikasi terpasang siap dibuka.")
                } else {
                    format!("Pindai selesai: {} aplikasi diperbarui ({linked} siap dibuka).", updated.len())
                };
                self.push_toast(now, text, false);
            }
            let copy_cmd = "simpul-creative-launcher import ./simpul-app.json --agent claude --install";
            if ui
                .add(ghost("  >_   Salin Perintah CLI  "))
                .on_hover_text(copy_cmd)
                .clicked()
            {
                ui.ctx().copy_text(copy_cmd.to_string());
                self.push_toast(now, "Perintah CLI disalin.", false);
            }
        });
        let _ = pal;
    }
}
