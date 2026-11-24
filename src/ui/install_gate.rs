//! Gerbang sebelum memasang aplikasi baru: sama dengan dialog download di situs Simpul.
//!
//! - Nominal di atas 0: pengguna berdonasi lewat Saweria dengan kode `SIMPUL-XXXXXX` di kolom Pesan.
//!   Kode dibuat server situs (`/api/donasi/niat`, sumber "launcher") bersama pengajuan agar nama
//!   tidak ditampilkan di daftar pendukung. Kalau server gagal, launcher tidak membuat kode sendiri.
//!   Launcher menanyakan API verifikasi situs sampai Saweria mengonfirmasi donasinya, baru tombol
//!   Pasang terbuka. Kalau verifikasi belum siap atau tidak bisa dihubungi, dipakai konfirmasi manual.
//! - Rp0: syaratnya membuka "Beri bintang" di GitHub dan "Follow" Instagram, lalu menyatakan sudah.
//!   GitHub dan Instagram tidak bisa dicek dari aplikasi, jadi ini konfirmasi pengguna.
//!
//! Hanya pemasangan baru dari antarmuka yang lewat gerbang ini. Pembaruan, pasang ulang, dan
//! pemasangan oleh agen lewat CLI tidak terpengaruh.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, RichText, Vec2};

use super::{Palette, SimpulLauncherApp};
use crate::models::{AgentOrigin, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_MICRO, PRIMARY_BTN_HEIGHT};
use crate::suite::{CONTACT_EMAIL, DONATE_URL, LAUNCHER_VERSION};

/// API verifikasi donasi di situs Simpul Creative Apps (Pages Functions + webhook Saweria).
const VERIFY_API: &str = "https://simpul-creative-apps.pages.dev/api/donasi/status";
/// Pembuat kode donasi di server: mencatat aplikasi, sumber "launcher", nominal, dan pengajuan
/// agar nama tidak ditampilkan. Kode TIDAK pernah dibuat sendiri oleh launcher (gagal tertutup).
const NIAT_API: &str = "https://simpul-creative-apps.pages.dev/api/donasi/niat";
/// Syarat Rp0.
const STAR_URL: &str = "https://github.com/simpulcreativeapps/simpul-launcher";
const STAR_LABEL: &str = "simpulcreativeapps/simpul-launcher";
/// Repo publik aplikasi yang sudah punya repo sendiri; selain itu bintang untuk repo launcher.
fn star_for(app_id: &str) -> (&'static str, &'static str) {
    match super::screens::shots_key(app_id) {
        Some("sync") => ("https://github.com/simpulcreativeapps/simpulsync", "simpulcreativeapps/simpulsync"),
        _ => (STAR_URL, STAR_LABEL),
    }
}
const INSTAGRAM_URL: &str = "https://www.instagram.com/simpulproject.id/";
const INSTAGRAM_LABEL: &str = "@simpulproject.id";

const PRESETS: [u64; 5] = [0, 5_000, 10_000, 25_000, 100_000];
const MIN_DONATION: u64 = 1_000;
const MAX_DONATION: u64 = 100_000_000;
const POLL_EVERY_SECS: f64 = 4.0;
const POLL_STOP_SECS: f64 = 30.0 * 60.0;
const HELP_AFTER_SECS: f64 = 180.0;
/// Konfirmasi manual terbuka setelah pengguna pindah ke browser lalu kembali, atau setelah jeda ini.
const MANUAL_FALLBACK_SECS: f64 = 15.0;
/// Harus sama dengan KODE_RE di lib/donasi.js pada repo situs.
const CODE_CHARS: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Choose,
    Confirm,
    Saweria,
    Free,
}

#[derive(Default)]
struct Verify {
    /// None = belum ada jawaban sah; Some(false) = server (bertanda tangan) menyatakan verifikasi
    /// otomatis sedang dijeda, jadi konfirmasi manual boleh dipakai.
    ready: Option<bool>,
    paid: bool,
    in_flight: bool,
    /// Jawaban terakhir tidak bisa dipastikan keasliannya (tanpa bukti, bukti salah, atau galat
    /// jaringan). Gagal tertutup: tombol tetap terkunci dan launcher mencoba lagi.
    blocked: bool,
}

pub(super) struct InstallGate {
    app_id: String,
    app_name: String,
    step: Step,
    amount: Option<u64>,
    amount_text: String,
    agree: bool,
    /// Pengajuan agar nama dan jumlah donasi tidak ditampilkan di daftar pendukung.
    hide: bool,
    /// Permintaan kode ke server: None = belum/selesai diproses, Some(hasil) = jawaban tiba.
    niat: Arc<Mutex<Option<Result<String, String>>>>,
    niat_busy: bool,
    /// Kunci idempotensi percobaan donasi ini (baru tiap kali isi permintaan berubah).
    idem: String,
    code: String,
    opened_at: f64,
    left_app: bool,
    last_poll: f64,
    verify: Arc<Mutex<Verify>>,
    star_done: bool,
    ig_done: bool,
    free_agree: bool,
    error: String,
}

enum Action {
    None,
    Install,
}

fn fmt_rp(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    format!("Rp{out}")
}

fn saweria_quick(n: u64) -> Option<&'static str> {
    match n {
        5_000 => Some("5k"),
        10_000 => Some("10k"),
        25_000 => Some("25k"),
        100_000 => Some("100k"),
        _ => None,
    }
}

/// Kode SIMPUL-XXXXXX yang sah: sama dengan KODE_RE di lib/donasi.js pada repo situs.
fn kode_sah(k: &str) -> bool {
    k.len() == 13 && k.starts_with("SIMPUL-") && k.as_bytes()[7..].iter().all(|b| CODE_CHARS.contains(b))
}

/// ID aplikasi di database situs (launcher, sync, video, design, 3d, audio).
/// Aplikasi lain yang dipasang lewat launcher dicatat sebagai donasi untuk launcher.
fn site_app_id(app_id: &str) -> &'static str {
    let id = app_id.to_ascii_lowercase();
    for (kunci, site) in [("sync", "sync"), ("video", "video"), ("design", "design"), ("3d", "3d"), ("audio", "audio")] {
        if id.contains(kunci) {
            return site;
        }
    }
    "launcher"
}

/// Minta kode donasi ke server situs.
fn request_code(app_id: &str, nominal: u64, hide: bool, idem: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "aplikasi": site_app_id(app_id),
        "sumber": "launcher",
        "nominal": nominal,
        "minta_sembunyi": hide,
        // Kunci idempotensi: kirim ulang / klik ganda mendapat kode yang sama dari server.
        "idem": idem,
    })
    .to_string();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("simpul-launcher/{LAUNCHER_VERSION}"))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(NIAT_API)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = resp.text().map_err(|e| e.to_string())?;
    if text.len() > 1024 {
        return Err("jawaban terlalu besar".into());
    }
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    match (v.get("ok").and_then(serde_json::Value::as_bool), v.get("kode").and_then(serde_json::Value::as_str)) {
        (Some(true), Some(k)) if kode_sah(k) => Ok(k.to_string()),
        _ => Err("jawaban server tidak sah".into()),
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Kunci publik server verifikasi (Ed25519). Pasangan dari rahasia STATUS_SIGNING_KEY di Cloudflare;
/// cadangan kunci ada di ~/.simpul-keys/status-signing-s1.*. Sama dengan STATUS_KEYS di donate.js.
const STATUS_KEYS: &[(&str, &str)] = &[("s1", "/VzgQH4GxroV3PdHgEfN8VonwcVHFKs70RBHry31xxc=")];

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn make_nonce() -> Result<String, String> {
    use base64::Engine;
    let mut buf = [0u8; 18];
    getrandom::fill(&mut buf).map_err(|_| "sumber acak sistem tidak tersedia".to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// Teks kanonis yang ditandatangani server. Harus sama dengan teksBukti di lib/donasi.js.
fn proof_text(kode: &str, nominal: u64, siap: bool, lunas: bool, nonce: &str, ts: i64) -> String {
    format!(
        "simpul-status-v1\n{kode}\n{nominal}\n{}\n{}\n{nonce}\n{ts}",
        if siap { "1" } else { "0" },
        if lunas { "1" } else { "0" }
    )
}

/// Periksa bukti bertanda tangan pada jawaban status. Ok((siap, lunas)) hanya bila tanda tangan sah
/// untuk kode, nominal, dan nonce yang kita kirim; jawaban yang diubah proxy/pencegat atau diputar
/// ulang selalu Err.
fn verify_status(v: &serde_json::Value, kode: &str, nominal: u64, nonce: &str) -> Result<(bool, bool), String> {
    use base64::Engine;
    let siap = v.get("siap").and_then(serde_json::Value::as_bool).ok_or("tanpa siap")?;
    let lunas = v.get("lunas").and_then(serde_json::Value::as_bool).ok_or("tanpa lunas")?;
    let bukti = v.get("bukti").ok_or("tanpa bukti")?;
    let kid = bukti.get("kid").and_then(serde_json::Value::as_str).ok_or("tanpa kid")?;
    let ts = bukti.get("ts").and_then(serde_json::Value::as_i64).ok_or("tanpa ts")?;
    let sig_b64 = bukti.get("sig").and_then(serde_json::Value::as_str).ok_or("tanpa sig")?;
    let (_, pub_b64) = STATUS_KEYS.iter().find(|(k, _)| *k == kid).ok_or("kunci tidak dikenal")?;
    let pub_bytes: [u8; 32] = b64()
        .decode(pub_b64)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("kunci publik rusak")?;
    let sig_bytes: [u8; 64] = b64()
        .decode(sig_b64)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("tanda tangan rusak")?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).map_err(|_| "kunci publik tidak sah")?;
    key.verify_strict(
        proof_text(kode, nominal, siap, lunas, nonce, ts).as_bytes(),
        &ed25519_dalek::Signature::from_bytes(&sig_bytes),
    )
    .map_err(|_| "tanda tangan tidak sah".to_string())?;
    Ok((siap, lunas))
}

/// Tanya API situs: (siap, lunas), hanya bila jawabannya bertanda tangan sah.
fn fetch_status(kode: &str, nominal: u64) -> Result<(bool, bool), String> {
    let nonce = make_nonce()?;
    let url = format!(
        "{VERIFY_API}?kode={}&nominal={nominal}&n={}",
        percent_encode(kode),
        percent_encode(&nonce)
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("simpul-launcher/{LAUNCHER_VERSION}"))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = resp.text().map_err(|e| e.to_string())?;
    if text.len() > 4096 {
        return Err("jawaban terlalu besar".into());
    }
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    verify_status(&v, kode, nominal, &nonce)
}

impl InstallGate {
    /// Mode tangkapan layar (SIMPUL_SHOT_VIEW): gate, gate-free, gate-saweria.
    pub(super) fn for_shot(view: &str) -> Option<Self> {
        let mut g = Self::new("sync", "Simpul Sync");
        match view {
            "gate" => {}
            "gate-free" => {
                g.amount = Some(0);
                g.step = Step::Free;
                g.star_done = true;
            }
            "gate-saweria" => {
                g.amount = Some(25_000);
                g.code = "SIMPUL-7K3Q9X".into();
                g.step = Step::Saweria;
                g.last_poll = f64::INFINITY; // jangan menghubungi jaringan saat memotret
            }
            _ => return None,
        }
        Some(g)
    }

    fn new(app_id: &str, app_name: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            app_name: app_name.to_string(),
            step: Step::Choose,
            amount: None,
            amount_text: String::new(),
            agree: false,
            hide: false,
            niat: Arc::new(Mutex::new(None)),
            niat_busy: false,
            idem: String::new(),
            code: String::new(),
            opened_at: 0.0,
            left_app: false,
            last_poll: f64::NEG_INFINITY,
            verify: Arc::new(Mutex::new(Verify::default())),
            star_done: false,
            ig_done: false,
            free_agree: false,
            error: String::new(),
        }
    }

    fn verify_snapshot(&self) -> (Option<bool>, bool, bool) {
        let v = self.verify.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        (v.ready, v.paid, v.blocked)
    }

    fn poll(&mut self, ctx: &egui::Context, now: f64) {
        if self.step != Step::Saweria || now - self.opened_at > POLL_STOP_SECS {
            return;
        }
        {
            let v = self.verify.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if v.paid || v.in_flight || v.ready == Some(false) {
                return;
            }
        }
        if now - self.last_poll < POLL_EVERY_SECS {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.last_poll = now;
        self.verify.lock().unwrap_or_else(std::sync::PoisonError::into_inner).in_flight = true;
        let kode = self.code.clone();
        let nominal = self.amount.unwrap_or(0);
        let shared = Arc::clone(&self.verify);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = fetch_status(&kode, nominal);
            let mut v = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            v.in_flight = false;
            match result {
                Ok((ready, paid)) => {
                    v.blocked = false;
                    v.ready = Some(ready || paid);
                    if paid {
                        v.paid = true;
                    }
                }
                // Gagal tertutup: jawaban yang tidak bisa dipastikan keasliannya tidak membuka apa pun.
                Err(_) => v.blocked = true,
            }
            repaint.request_repaint();
        });
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn help_mailto(&self) -> String {
        let subject = if self.code.is_empty() {
            "Kendala donasi Saweria".to_string()
        } else {
            format!("Kendala donasi Saweria {}", self.code)
        };
        let body = format!(
            "Halo tim Simpul,\r\n\r\nSaya mengalami kendala saat donasi lewat Saweria dari Simpul Launcher.\r\n\r\n\
             Aplikasi: {}\r\nNominal: {}\r\nKode donasi: {}\r\n\r\nCeritakan kendalanya:\r\n",
            self.app_name,
            fmt_rp(self.amount.unwrap_or(0)),
            if self.code.is_empty() { "-" } else { &self.code },
        );
        format!(
            "mailto:{CONTACT_EMAIL}?subject={}&body={}",
            percent_encode(&subject),
            percent_encode(&body)
        )
    }
}

fn primary(ui: &mut egui::Ui, pal: Palette, enabled: bool, text: &str) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).strong().color(pal.on_accent))
            .min_size(Vec2::new(ui.available_width(), PRIMARY_BTN_HEIGHT + 6.0))
            .fill(pal.accent),
    )
    .clicked()
}

fn para(ui: &mut egui::Ui, text: impl Into<String>, size: f32, color: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size).color(color)).wrap());
}

impl SimpulLauncherApp {
    /// Pasang aplikasi baru lewat gerbang donasi / bintang + follow.
    pub(super) fn open_install_gate(&mut self, app_id: &str, app_name: &str) {
        self.install_gate = Some(InstallGate::new(app_id, app_name));
        self.donate_open = false;
        self.about_open = false;
        self.bug_open = false;
    }

    pub(super) fn draw_install_gate(&mut self, ctx: &egui::Context, pal: Palette, now: f64) {
        let Some(mut gate) = self.install_gate.take() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return; // Esc menutup dialog
        }
        if gate.step == Step::Saweria && ctx.input(|i| i.viewport().focused) == Some(false) {
            gate.left_app = true;
        }
        gate.poll(ctx, now);

        let mut open = true;
        let mut action = Action::None;
        egui::Window::new(format!("Pasang {}", gate.app_name))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(460.0, 0.0))
            .frame(Self::window_frame(ctx, pal))
            .show(ctx, |ui| {
                action = match gate.step {
                    Step::Choose => draw_choose(ui, pal, &mut gate),
                    Step::Confirm => draw_confirm(ui, ctx, pal, &mut gate, now),
                    Step::Saweria => draw_saweria(ui, ctx, pal, &mut gate, now),
                    Step::Free => draw_free(ui, ctx, pal, &mut gate),
                };
            });

        match action {
            Action::Install => {
                let thanks = gate.amount.unwrap_or(0) > 0;
                self.enqueue_install(now, &gate.app_id, AgentOrigin::SimpulSuite);
                if thanks {
                    self.push_toast(now, "♥ Terima kasih sudah mendukung Simpul!", false);
                }
            }
            Action::None => {
                if open {
                    self.install_gate = Some(gate);
                }
            }
        }
    }
}

fn draw_choose(ui: &mut egui::Ui, pal: Palette, gate: &mut InstallGate) -> Action {
    ui.label(RichText::new(format!("Dukung pengembangan {}", gate.app_name)).size(FONT_CARD_TITLE).strong().color(pal.ink_primary));
    ui.add_space(4.0);
    para(
        ui,
        format!("{} gratis untuk semua orang. Kalau aplikasi ini membantu, kamu bisa ikut membiayai pengembangannya. Bayar sesukanya, termasuk Rp0.", gate.app_name),
        FONT_CAPTION,
        pal.ink_secondary,
    );
    ui.add_space(10.0);
    ui.label(RichText::new("Pilih nominal donasi").size(FONT_CAPTION).strong().color(pal.ink_primary));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for v in PRESETS {
            let label = if v == 0 { "Gratis (Rp0)".to_string() } else { fmt_rp(v) };
            let selected = gate.amount == Some(v) && gate.amount_text.is_empty();
            if ui.selectable_label(selected, RichText::new(label).size(FONT_BODY)).clicked() {
                gate.amount = Some(v);
                gate.amount_text.clear();
                gate.error.clear();
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Nominal lain  Rp").size(FONT_CAPTION).color(pal.ink_secondary));
        let resp = ui.add(egui::TextEdit::singleline(&mut gate.amount_text).hint_text("0").desired_width(150.0));
        if resp.changed() {
            gate.amount_text.retain(|c| c.is_ascii_digit());
            gate.amount = gate.amount_text.parse::<u64>().ok();
            gate.error.clear();
        }
    });
    ui.add_space(4.0);
    para(
        ui,
        format!("Pilih Gratis (Rp0) kalau tidak ingin berdonasi: syaratnya beri bintang di GitHub dan follow Instagram Simpul. Minimal donasi {}.", fmt_rp(MIN_DONATION)),
        FONT_MICRO,
        pal.ink_muted,
    );
    if !gate.error.is_empty() {
        ui.add_space(4.0);
        para(ui, gate.error.clone(), FONT_CAPTION, pal.danger);
    }
    ui.add_space(10.0);
    let label = match gate.amount {
        None => "Pilih nominal dulu".to_string(),
        Some(0) => "Lanjut dengan Rp0".to_string(),
        Some(n) => format!("♥  Donasi {} & pasang", fmt_rp(n)),
    };
    if primary(ui, pal, true, &label) {
        match gate.amount {
            None => gate.error = "Pilih atau isi nominal dulu. Pilih Gratis (Rp0) kalau tidak ingin berdonasi.".into(),
            Some(n) if n > 0 && n < MIN_DONATION => {
                gate.error = format!("Minimal donasi {}. Pilih Gratis (Rp0) kalau tidak ingin berdonasi.", fmt_rp(MIN_DONATION));
            }
            Some(n) if n > MAX_DONATION => gate.error = "Nominal terlalu besar. Periksa lagi angkanya.".into(),
            Some(0) => {
                gate.step = Step::Free;
                gate.error.clear();
            }
            Some(_) => {
                gate.agree = false;
                gate.step = Step::Confirm;
                gate.error.clear();
            }
        }
    }
    Action::None
}

fn draw_confirm(ui: &mut egui::Ui, ctx: &egui::Context, pal: Palette, gate: &mut InstallGate, now: f64) -> Action {
    let amount = gate.amount.unwrap_or(0);
    ui.label(RichText::new("Kamu akan diarahkan ke Saweria").size(FONT_CARD_TITLE).strong().color(pal.ink_primary));
    ui.add_space(6.0);
    let how = match saweria_quick(amount) {
        Some(q) => format!("Di Saweria, klik tombol {q} di bawah kolom Nominal, lalu tempel kode donasi di kolom Pesan supaya donasimu terdeteksi."),
        None => "Di Saweria, ketik nominalnya di kolom Nominal, lalu tempel kode donasi di kolom Pesan supaya donasimu terdeteksi.".to_string(),
    };
    for point in [
        "• Pembayaran diproses langsung oleh Saweria. Launcher tidak menerima atau menyimpan data pembayaranmu.".to_string(),
        format!("• Isi nominal {} sendiri di Saweria. {how}", fmt_rp(amount)),
        format!("• Tombol Pasang terbuka sendiri begitu Saweria mengonfirmasi donasimu."),
        format!(
            "• Daftar pendukung. Nama (sesuai kolom \"Dari\" di Saweria) dan jumlah donasimu akan ditampilkan di halaman {} sebagai tanda terima kasih. \
             Email tidak pernah ditampilkan; kami menyimpannya hanya untuk menghubungimu soal donasi ini. Ingin memakai nama samaran? Isi saja kolom \"Dari\" sesukamu.",
            gate.app_name
        ),
    ] {
        para(ui, point, FONT_CAPTION, pal.ink_secondary);
        ui.add_space(3.0);
    }
    ui.add_space(6.0);
    ui.checkbox(
        &mut gate.hide,
        RichText::new("Ajukan agar nama dan jumlah donasi saya tidak ditampilkan. Pengajuan akan kami pertimbangkan; untuk donasi tertentu kami akan menghubungimu lewat email lebih dulu. Tanpa centang, nama dan jumlah donasimu ditampilkan.")
            .size(FONT_CAPTION)
            .color(pal.ink_secondary),
    );
    ui.add_space(4.0);
    ui.checkbox(
        &mut gate.agree,
        RichText::new("Saya paham ini donasi sukarela untuk mendukung developer Simpul, bukan pembayaran untuk membeli aplikasi. Aplikasinya tetap gratis, dan donasi tidak dapat dikembalikan.")
            .size(FONT_CAPTION)
            .color(pal.ink_primary),
    );

    // Jawaban server untuk permintaan kode.
    let arrived = gate.niat.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    if let Some(result) = arrived {
        gate.niat_busy = false;
        match result {
            Ok(kode) => {
                gate.code = kode;
                ctx.copy_text(gate.code.clone());
                ctx.open_url(egui::OpenUrl::new_tab(DONATE_URL));
                gate.opened_at = now;
                gate.left_app = false;
                gate.last_poll = f64::NEG_INFINITY;
                *gate.verify.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Verify::default();
                gate.step = Step::Saweria;
                gate.error.clear();
                return Action::None;
            }
            // Gagal tertutup: launcher tidak membuat kode sendiri.
            Err(_) => {
                gate.error = "Kode donasi belum bisa dibuat. Periksa koneksi internet lalu coba lagi, atau kembali dan pilih Gratis (Rp0).".into();
            }
        }
    }

    if !gate.error.is_empty() {
        para(ui, gate.error.clone(), FONT_CAPTION, pal.danger);
    }
    ui.add_space(10.0);
    let label = if gate.niat_busy { "Membuat kode donasi…" } else { "Buat kode donasi & buka Saweria" };
    if primary(ui, pal, !gate.niat_busy, label) && !gate.niat_busy {
        if gate.agree {
            gate.niat_busy = true;
            gate.error.clear();
            let kunci = format!("{}|{}|{}", amount, gate.hide, gate.app_id);
            if !gate.idem.ends_with(&kunci) {
                // Isi permintaan berubah (atau percobaan pertama): kunci idempotensi baru.
                gate.idem = format!("{}|{kunci}", make_nonce().unwrap_or_default());
            }
            let idem = gate.idem.split('|').next().unwrap_or_default().to_string();
            let (app_id, hide, shared, repaint) = (gate.app_id.clone(), gate.hide, Arc::clone(&gate.niat), ctx.clone());
            std::thread::spawn(move || {
                let result = request_code(&app_id, amount, hide, &idem);
                *shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
                repaint.request_repaint();
            });
        } else {
            gate.error = "Centang dulu pernyataan di atas, atau kembali dan pilih Gratis (Rp0).".into();
        }
    }
    if gate.niat_busy {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    ui.add_space(4.0);
    if !gate.niat_busy && ui.button("← Ubah nominal").clicked() {
        gate.step = Step::Choose;
        gate.error.clear();
    }
    Action::None
}

fn draw_saweria(ui: &mut egui::Ui, ctx: &egui::Context, pal: Palette, gate: &mut InstallGate, now: f64) -> Action {
    let amount = gate.amount.unwrap_or(0);
    let (ready, paid, blocked) = gate.verify_snapshot();
    ui.label(RichText::new("Sudah selesai di Saweria?").size(FONT_CARD_TITLE).strong().color(pal.ink_primary));
    ui.add_space(4.0);
    para(
        ui,
        "Saweria dibuka di browser. Kolom Nominal di sana masih kosong, jadi isi seperti ini:",
        FONT_CAPTION,
        pal.ink_secondary,
    );
    ui.add_space(4.0);
    let first = match saweria_quick(amount) {
        Some(q) => format!("1. Di kolom Nominal, klik tombol {q} (atau ketik {amount})."),
        None => format!("1. Di kolom Nominal, ketik {amount}."),
    };
    for step in [
        first,
        format!("2. Di kolom Pesan, tempel kode {} (sudah disalin). Boleh ditambah ucapanmu.", gate.code),
        "3. Isi Dari dan Email.".to_string(),
        "4. Pilih metode bayar, lalu kirim dukungan.".to_string(),
    ] {
        para(ui, step, FONT_CAPTION, pal.ink_primary);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&gate.code).monospace().size(FONT_BODY + 2.0).strong().color(pal.ink_primary));
        if ui.button("Salin kode").clicked() {
            ctx.copy_text(gate.code.clone());
        }
        ui.label(RichText::new(fmt_rp(amount)).monospace().size(FONT_BODY).color(pal.ink_secondary));
        if ui.button("Salin nominal").clicked() {
            ctx.copy_text(amount.to_string());
        }
    });
    if ui.button("↗  Buka Saweria lagi").clicked() {
        ctx.open_url(egui::OpenUrl::new_tab(DONATE_URL));
        gate.left_app = true;
    }
    ui.add_space(8.0);

    let manual = ready == Some(false);
    let unlocked = paid || (manual && (gate.left_app || now - gate.opened_at > MANUAL_FALLBACK_SECS));
    let status = if paid {
        (format!("✔ Donasi {} sudah diterima Saweria. Terima kasih! Tidak perlu membayar lagi; tab Saweria boleh ditutup.", fmt_rp(amount)), pal.emerald)
    } else if blocked && !manual {
        (
            "Jawaban server verifikasi tidak bisa dipastikan keasliannya, atau server tidak bisa dihubungi. Mencoba lagi… Kalau terus gagal, kembali dan pilih Rp0.".to_string(),
            pal.amber,
        )
    } else if manual {
        ("Selesaikan donasi di Saweria, lalu kembali ke sini.".to_string(), pal.ink_secondary)
    } else if ready.is_none() {
        ("Memeriksa donasi…".to_string(), pal.ink_muted)
    } else {
        let mut t = "Menunggu donasi terdeteksi. Tombol Pasang terbuka sendiri begitu Saweria mengonfirmasi, biasanya kurang dari satu menit setelah bayar.".to_string();
        if now - gate.opened_at > HELP_AFTER_SECS {
            t.push_str(&format!(" Belum terdeteksi? Pastikan kode {} ada di kolom Pesan, atau kembali dan pilih Rp0.", gate.code));
        }
        (t, pal.ink_secondary)
    };
    para(ui, status.0, FONT_CAPTION, status.1);
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Saweria nyangkut?").size(FONT_CAPTION).strong().color(pal.ink_primary));
        if ui.link(RichText::new("Kirim keluhan via email, kami hubungi secepatnya").size(FONT_CAPTION)).clicked() {
            ctx.open_url(egui::OpenUrl::new_tab(gate.help_mailto()));
        }
    });
    ui.add_space(10.0);
    let label = if paid { "Donasi diterima. Pasang sekarang" } else { "Ya, sudah. Pasang sekarang" };
    let mut action = Action::None;
    if primary(ui, pal, unlocked, label) {
        action = Action::Install;
    }
    ui.add_space(4.0);
    if ui.button("← Ubah nominal").clicked() {
        gate.step = Step::Choose;
    }
    action
}

fn draw_free(ui: &mut egui::Ui, ctx: &egui::Context, pal: Palette, gate: &mut InstallGate) -> Action {
    ui.label(RichText::new("Dukung gratis dengan dua klik").size(FONT_CARD_TITLE).strong().color(pal.ink_primary));
    ui.add_space(4.0);
    para(
        ui,
        format!("{} gratis dengan Rp0. Sebagai gantinya, bantu Simpul dikenal lebih banyak kreator: beri bintang di GitHub dan follow Instagram kami. Keduanya terbuka di browser.", gate.app_name),
        FONT_CAPTION,
        pal.ink_secondary,
    );
    ui.add_space(8.0);
    let (star_url, star_label) = star_for(&gate.app_id);
    for (done, title, detail, button, url) in [
        (&mut gate.star_done, "1. Beri bintang di GitHub", star_label, "★  Beri bintang", star_url),
        (&mut gate.ig_done, "2. Follow Instagram", INSTAGRAM_LABEL, "Follow", INSTAGRAM_URL),
    ] {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let mark = if *done { "✔ " } else { "" };
                ui.label(RichText::new(format!("{mark}{title}")).size(FONT_BODY).strong().color(if *done { pal.emerald } else { pal.ink_primary }));
                ui.label(RichText::new(detail).size(FONT_MICRO).color(pal.ink_muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(button).clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                    *done = true;
                }
            });
        });
        ui.add_space(6.0);
    }
    ui.checkbox(
        &mut gate.free_agree,
        RichText::new("Saya sudah memberi bintang di GitHub dan follow Instagram Simpul.").size(FONT_CAPTION).color(pal.ink_primary),
    );
    if !gate.error.is_empty() {
        para(ui, gate.error.clone(), FONT_CAPTION, pal.danger);
    }
    ui.add_space(10.0);
    let mut action = Action::None;
    if primary(ui, pal, true, "⬇  Pasang gratis") {
        if !gate.star_done || !gate.ig_done {
            gate.error = "Buka dulu kedua tombol di atas: beri bintang di GitHub dan follow Instagram.".into();
        } else if !gate.free_agree {
            gate.error = "Centang dulu pernyataan bahwa kamu sudah memberi bintang dan follow.".into();
        } else {
            action = Action::Install;
        }
    }
    ui.add_space(4.0);
    if ui.button("← Ubah nominal").clicked() {
        gate.step = Step::Choose;
        gate.error.clear();
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kode_dari_server_diperiksa_formatnya() {
        assert!(kode_sah("SIMPUL-7K3Q9X"));
        for buruk in ["SIMPUL-7K3Q9", "SIMPUL-7K3Q9XX", "SIMPUL-7K3QOX", "SIMPUL-7K3Q1X", "simpul-7K3Q9X", "SIMPUL-7k3q9x", "<script>"] {
            assert!(!kode_sah(buruk), "{buruk}");
        }
    }

    #[test]
    fn id_aplikasi_dipetakan_ke_database_situs() {
        assert_eq!(site_app_id("simpul-sync-4"), "sync");
        assert_eq!(site_app_id("SimpulSync"), "sync");
        assert_eq!(site_app_id("simpul-video"), "video");
        assert_eq!(site_app_id("capr-notebook"), "launcher");
    }

    #[test]
    fn format_rupiah() {
        assert_eq!(fmt_rp(0), "Rp0");
        assert_eq!(fmt_rp(25_000), "Rp25.000");
        assert_eq!(fmt_rp(1_000_000), "Rp1.000.000");
    }

    /// Contoh jawaban asli yang ditandatangani kunci produksi s1 (tanda tangan bukan rahasia).
    fn contoh_sah() -> serde_json::Value {
        serde_json::json!({
            "siap": true, "lunas": true, "nominal": 25000,
            "bukti": { "kid": "s1", "ts": 1795450000_i64,
                "sig": "ZltBN2irO2JyeqgNDAclQWCPWkNwI5yDmqqcM2EpNmVJHISbQazx7j0iIG3au/Ci/WM23999DAw8QnlgoU/9Aw==" }
        })
    }

    #[test]
    fn jawaban_bertanda_tangan_sah_diterima() {
        assert_eq!(verify_status(&contoh_sah(), "SIMPUL-7K3Q9X", 25_000, "nonceUjiLauncher0001"), Ok((true, true)));
    }

    #[test]
    fn jawaban_yang_diubah_atau_diputar_ulang_ditolak() {
        let n = "nonceUjiLauncher0001";
        // Proxy mengubah hasil.
        let mut v = contoh_sah();
        v["lunas"] = serde_json::json!(false);
        assert!(verify_status(&v, "SIMPUL-7K3Q9X", 25_000, n).is_err());
        // Jawaban lama diputar ulang untuk permintaan baru (nonce lain), kode lain, atau nominal lain.
        assert!(verify_status(&contoh_sah(), "SIMPUL-7K3Q9X", 25_000, "nonceLainSamaSekali01").is_err());
        assert!(verify_status(&contoh_sah(), "SIMPUL-AAAAA2", 25_000, n).is_err());
        assert!(verify_status(&contoh_sah(), "SIMPUL-7K3Q9X", 5_000, n).is_err());
        // Tanpa bukti atau kunci tak dikenal.
        let mut tanpa = contoh_sah();
        tanpa.as_object_mut().unwrap().remove("bukti");
        assert!(verify_status(&tanpa, "SIMPUL-7K3Q9X", 25_000, n).is_err());
        let mut kid = contoh_sah();
        kid["bukti"]["kid"] = serde_json::json!("palsu");
        assert!(verify_status(&kid, "SIMPUL-7K3Q9X", 25_000, n).is_err());
    }

    #[test]
    fn nonce_acak_dan_aman_untuk_url() {
        let a = make_nonce().unwrap();
        let b = make_nonce().unwrap();
        assert_ne!(a, b);
        assert!(a.len() >= 16 && a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }

    #[test]
    fn mailto_terenkode() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
    }
}
