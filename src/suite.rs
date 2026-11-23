//! Identitas & kredit Simpul Creative Apps, tautan donasi, serta status hijau/kuning/merah
//! dan pembaruan yang dibaca dari manifest bertanda tangan lewat `simpul-guard`.
//!
//! Tidak ada telemetri: satu-satunya akses internet di sini adalah mengambil manifest
//! dan, kalau pengguna memintanya, mengunduh installer yang sha256-nya dicek.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use simpul_guard::{Guard, Report, UpdateInfo, Verdict};

pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SUITE_NAME: &str = "Simpul Creative Apps";
pub const LAUNCHER_NAME: &str = "Simpul Creative Launcher";
pub const MAKER_NAME: &str = "Simpul Project";
pub const MAKER_DOMAIN: &str = "simpulproject.com";
pub const MAKER_URL: &str = "https://simpulproject.com";
pub const SOURCE_URL: &str = "https://github.com/simpulcreativeapps";
pub const DONATE_URL: &str = "https://saweria.co/simpuldevs";
pub const CONTACT_EMAIL: &str = "simpulcreative.apps@gmail.com";

/// Launcher ini turunan dari ArtCraft Launcher (MIT OR Apache-2.0). Lihat `NOTICE`.
pub const UPSTREAM_CREDIT: &str =
    "Berbasis ArtCraft Launcher © 2026 ArtCraft Team dan kontributor (MIT atau Apache-2.0).";
pub const UPSTREAM_URL: &str = "https://github.com/storytold/craft-launcher";

/// Id launcher di `simpul-manifest`.
pub const LAUNCHER_MANIFEST_ID: &str = "launcher";

/// Id aplikasi di launcher -> id di `simpul-manifest`. Sebagian besar sama; Simpul Sync
/// memakai id lama `simpulsync-4` di komputer pengguna (dari pemindaian aplikasi terpasang).
pub fn manifest_id_for(app_id: &str) -> &str {
    match app_id {
        "simpulsync-4" | "simpul-sync" => "sync",
        other => other,
    }
}

/// Kebalikan dari [`manifest_id_for`].
pub fn launcher_id_for(manifest_id: &str) -> &str {
    match manifest_id {
        "sync" => "simpulsync-4",
        other => other,
    }
}

/// Satu aplikasi di manifest bertanda tangan. `version` kosong berarti belum ada rilis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Id di launcher.
    pub app_id: String,
    pub name: String,
    pub tagline: Option<String>,
    pub version: Option<String>,
    /// Versi launcher terendah yang disyaratkan aplikasi ini.
    pub min_launcher: Option<String>,
}

/// Bila `current` lebih rendah dari `min`, kembalikan `min` (versi yang dibutuhkan).
/// Versi yang tidak terbaca tidak pernah memblokir.
pub fn version_below(min: &str, current: &str) -> Option<String> {
    let need = semver::Version::parse(min).ok()?;
    let have = semver::Version::parse(current).ok()?;
    (have < need).then(|| need.to_string())
}

/// Folder cache bersama untuk launcher dan semua aplikasi Simpul, supaya manifest sah
/// terakhir (termasuk status merah) tetap berlaku saat offline.
pub fn guard_cache_dir() -> PathBuf {
    shared_data_dir().join("guard")
}

fn downloads_dir() -> PathBuf {
    shared_data_dir().join("downloads")
}

fn shared_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("SIMPUL_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("shared");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("Simpul");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Simpul");
        }
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".local").join("share").join("simpul");
    }
    std::env::temp_dir().join("Simpul")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    Downloading,
    /// Installer sudah diunduh dan sha256-nya cocok dengan manifest.
    Ready(PathBuf),
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct GuardSnapshot {
    pub checking: bool,
    /// Sudah pernah selesai dicek sejak launcher dibuka.
    pub checked: bool,
    pub launcher: Option<Report>,
    /// Kunci: id aplikasi di katalog launcher.
    pub apps: BTreeMap<String, Report>,
    /// Isi manifest sah terakhir. `None` berarti belum pernah ada manifest sah.
    pub catalog: Option<Vec<CatalogEntry>>,
    /// Kunci: `LAUNCHER_MANIFEST_ID` atau id aplikasi di katalog launcher.
    pub downloads: BTreeMap<String, DownloadState>,
    /// Bertambah setiap pengecekan selesai, supaya UI tahu ada hasil baru.
    pub generation: u64,
}

impl GuardSnapshot {
    pub fn app_verdict(&self, app_id: &str) -> Option<&Verdict> {
        self.apps.get(app_id).map(|r| &r.verdict)
    }

    pub fn app_update(&self, app_id: &str) -> Option<&UpdateInfo> {
        self.apps.get(app_id).and_then(|r| r.update.as_ref())
    }

    /// Versi launcher yang dibutuhkan aplikasi ini, bila launcher yang berjalan lebih rendah.
    pub fn launcher_needed_for(&self, app_id: &str) -> Option<String> {
        let entry = self.catalog.as_ref()?.iter().find(|e| e.app_id == app_id)?;
        version_below(entry.min_launcher.as_deref()?, LAUNCHER_VERSION)
    }

    /// Versi yang wajib dipasang sebelum aplikasi ini boleh dibuka (di bawah `min_version`).
    pub fn app_required_update(&self, app_id: &str) -> Option<String> {
        self.app_update(app_id)
            .filter(|u| u.mandatory)
            .map(|u| u.version.to_string())
    }

    /// Aplikasi boleh dibuka? Aplikasi yang tidak ada di manifest selalu boleh.
    pub fn app_may_run(&self, app_id: &str) -> bool {
        self.app_verdict(app_id).is_none_or(Verdict::may_run)
    }
}

#[derive(Clone, Default)]
pub struct GuardHub {
    inner: Arc<Mutex<GuardSnapshot>>,
}

impl GuardHub {
    fn lock(&self) -> MutexGuard<'_, GuardSnapshot> {
        // Mutex beracun (thread lain panik) tidak boleh ikut menjatuhkan UI.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn snapshot(&self) -> GuardSnapshot {
        self.lock().clone()
    }

    /// Cek status launcher dan aplikasi di latar. `apps` = (id launcher, versi TERPASANG).
    /// `on_done` dipanggil dari thread latar saat hasil siap (untuk meminta repaint).
    pub fn refresh(&self, apps: Vec<(String, String)>, on_done: impl Fn() + Send + 'static) {
        {
            let mut snap = self.lock();
            if snap.checking {
                return;
            }
            snap.checking = true;
        }
        let hub = self.clone();
        let spawned = std::thread::Builder::new()
            .name("simpul-guard-check".into())
            .spawn(move || {
                let cache = guard_cache_dir();
                let launcher_guard = Guard::new(LAUNCHER_MANIFEST_ID, LAUNCHER_VERSION, &cache);
                // Satu-satunya akses jaringan: ambil manifest, verifikasi, simpan ke cache.
                let launcher = launcher_guard.check();
                let manifest = launcher_guard.cached_manifest();
                let (reports, catalog) = match &manifest {
                    Some(m) => {
                        let (r, c) = evaluate_catalog(m, &apps, &cache);
                        (r, Some(c))
                    }
                    None => (BTreeMap::new(), None),
                };
                log_event(&format!(
                    "cek status: launcher={} asal={:?} aplikasi={}",
                    verdict_name(&launcher.verdict),
                    launcher.origin,
                    reports.len()
                ));
                {
                    let mut snap = hub.lock();
                    snap.launcher = Some(launcher);
                    snap.apps = reports;
                    snap.catalog = catalog;
                    snap.checking = false;
                    snap.checked = true;
                    snap.generation = snap.generation.wrapping_add(1);
                }
                on_done();
            });
        if spawned.is_err() {
            self.lock().checking = false;
        }
    }

    /// Unduh installer versi baru di latar. sha256 dicek oleh simpul-guard; file yang tidak
    /// cocok dihapus dan tidak pernah dijalankan.
    pub fn download(&self, key: &str, update: UpdateInfo, on_done: impl Fn() + Send + 'static) {
        {
            let mut snap = self.lock();
            if matches!(snap.downloads.get(key), Some(DownloadState::Downloading)) {
                return;
            }
            snap.downloads.insert(key.to_string(), DownloadState::Downloading);
        }
        let hub = self.clone();
        let key_owned = key.to_string();
        let key_thread = key_owned.clone();
        let spawned = std::thread::Builder::new()
            .name("simpul-guard-download".into())
            .spawn(move || {
                let result = simpul_guard::net::download_update(&update, &downloads_dir());
                let state = match result {
                    Ok(path) => DownloadState::Ready(path),
                    Err(e) => {
                        log_event(&format!("unduhan gagal ({key_thread}): {e}"));
                        DownloadState::Failed(e)
                    }
                };
                hub.lock().downloads.insert(key_thread, state);
                on_done();
            });
        if spawned.is_err() {
            self.lock().downloads.insert(
                key_owned,
                DownloadState::Failed("tidak bisa memulai unduhan".into()),
            );
        }
    }

    pub fn clear_download(&self, key: &str) {
        self.lock().downloads.remove(key);
    }
}

/// Nilai setiap aplikasi di manifest dari cache (tanpa jaringan) dan susun katalognya.
fn evaluate_catalog(
    manifest: &simpul_guard::Manifest,
    installed: &[(String, String)],
    cache: &std::path::Path,
) -> (BTreeMap<String, Report>, Vec<CatalogEntry>) {
    let mut reports = BTreeMap::new();
    let mut catalog = Vec::new();
    for (manifest_id, entry) in &manifest.apps {
        if manifest_id == LAUNCHER_MANIFEST_ID {
            continue;
        }
        let app_id = launcher_id_for(manifest_id).to_string();
        // Aplikasi yang belum terpasang dinilai sebagai versi 0.0.0, supaya rilisnya terbaca.
        let version = installed
            .iter()
            .find(|(id, _)| *id == app_id)
            .map_or("0.0.0", |(_, v)| v.as_str());
        let mut guard = Guard::new(manifest_id, version, cache);
        guard.sources.clear();
        reports.insert(app_id.clone(), guard.check());
        catalog.push(CatalogEntry {
            app_id,
            name: entry.name.clone(),
            tagline: entry.tagline.clone(),
            version: entry.release.as_ref().map(|r| r.version.clone()),
            min_launcher: entry.min_launcher.clone(),
        });
    }
    (reports, catalog)
}

fn verdict_name(v: &Verdict) -> &'static str {
    match v {
        Verdict::Ok => "hijau",
        Verdict::Unknown => "tidak-diketahui",
        Verdict::Warn(_) => "kuning",
        Verdict::Sunset { .. } => "merah-tenggang",
        Verdict::Disabled(_) => "merah",
    }
}

/// Jalankan installer yang sudah lolos cek sha256.
pub fn run_installer(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let res = Command::new("open").arg(path).spawn();
    #[cfg(not(target_os = "macos"))]
    let res = Command::new(path).spawn();
    res.map(|_| ())
        .map_err(|e| format!("Gagal menjalankan {}: {e}", path.display()))
}

/// Uninstaller resmi launcher (dibuat installer Inno Setup di sebelah exe).
/// Hanya menghapus file program; data dan file kerja pengguna tidak disentuh.
pub fn find_own_uninstaller() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut best: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("unins") && name.ends_with(".exe") {
            let p = entry.path();
            if best.as_ref().is_none_or(|b| p > *b) {
                best = Some(p);
            }
        }
    }
    best
}

pub fn run_own_uninstaller() -> Result<(), String> {
    let Some(unins) = find_own_uninstaller() else {
        return Err(
            "Uninstaller tidak ditemukan (versi portabel). Hapus folder aplikasinya secara manual."
                .into(),
        );
    };
    Command::new(&unins)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Gagal menjalankan uninstaller: {e}"))
}

/// Sisa waktu masa tenggang dalam kalimat pendek.
pub fn remaining_label(remaining: &simpul_guard::policy::Verdict) -> Option<String> {
    let Verdict::Sunset { remaining, .. } = remaining else {
        return None;
    };
    let hours = remaining.num_hours().max(0);
    Some(if hours >= 48 {
        format!("{} hari lagi", hours / 24)
    } else if hours >= 1 {
        format!("{hours} jam lagi")
    } else {
        "kurang dari 1 jam lagi".to_string()
    })
}

/// Penanda "pendukung" disimpan lokal dan dipakai bersama semua aplikasi Simpul.
/// Ini pernyataan pengguna sendiri: tidak diverifikasi dan tidak dikirim ke mana pun.
fn supporter_file() -> PathBuf {
    shared_data_dir().join("supporter.json")
}

pub fn is_supporter() -> bool {
    std::fs::read_to_string(supporter_file())
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("supporter").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

pub fn set_supporter(value: bool) -> Result<(), String> {
    let path = supporter_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::json!({ "supporter": value }).to_string();
    std::fs::write(path, body).map_err(|e| e.to_string())
}

/// Nama sistem operasi untuk laporan bug (tanpa nama komputer atau nama pengguna).
pub fn os_label() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------- kirim laporan (v2)
//
// Protokol bertanda tangan, lihat repo `simpul-lapor`:
//   1. Aplikasi meminta tantangan sekali pakai. Jawaban server harus bertanda tangan
//      Ed25519 dengan kunci publik yang ditanam di bawah; kalau tidak, ditolak.
//   2. Aplikasi mengerjakan bukti kerja atas tantangan itu.
//   3. Aplikasi menandatangani tantangan + bukti kerja + sha256 isi dengan kunci
//      pemasangan (dibuat acak di komputer ini, tidak memuat identitas apa pun).
// Tidak ada rahasia bersama di kode ini: yang ditanam hanya kunci publik server.

const REPORT_BASE: &str = "https://simpul-lapor.simpul-lapor.workers.dev";
const REPORT_SERVER_KEY_B64: &str = "9b37rE52vJMf0fhN9pxlnT7xJxfL1NLNXbWCrQ8i2/Q=";
const REPORT_APP_ID: &str = "launcher";
/// Batas atas bukti kerja yang mau dikerjakan aplikasi (2^26 hash, beberapa detik).
const REPORT_MAX_POW_BITS: u32 = 26;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_bytes(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).to_vec()
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| "sumber acak sistem tidak tersedia".to_string())?;
    Ok(buf)
}

/// Kunci pemasangan: 32 byte acak di folder data bersama. Hanya dipakai untuk
/// menandatangani laporan, supaya penerima bisa membatasi kiriman per pemasangan.
fn install_signing_key() -> Result<ed25519_dalek::SigningKey, String> {
    let path = shared_data_dir().join("install.key");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let text = text.trim();
        if text.len() == 64 {
            let mut seed = [0u8; 32];
            let parsed = (0..32).all(|i| {
                u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map(|b| seed[i] = b).is_ok()
            });
            if parsed {
                return Ok(ed25519_dalek::SigningKey::from_bytes(&seed));
            }
        }
    }
    let seed = random_bytes::<32>()?;
    std::fs::create_dir_all(shared_data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, hex(&seed)).map_err(|e| e.to_string())?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

/// Jawaban server hanya dipercaya bila tanda tangannya cocok dengan kunci yang ditanam.
fn verify_server_reply(context: &str, status: u16, body: &str, signature_b64: &str) -> Result<(), String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let bad = || "jawaban penerima tidak bertanda tangan sah".to_string();
    let key: [u8; 32] = b64
        .decode(REPORT_SERVER_KEY_B64)
        .ok()
        .and_then(|k| k.try_into().ok())
        .ok_or_else(bad)?;
    let sig: [u8; 64] = b64
        .decode(signature_b64.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or_else(bad)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| bad())?;
    let message = format!("simpul-response-v2\n{context}\n{status}\n{body}");
    key.verify_strict(message.as_bytes(), &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| bad())
}

fn leading_zero_bits(bytes: &[u8]) -> u32 {
    let mut bits = 0;
    for b in bytes {
        if *b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

fn solve_proof_of_work(challenge: &str, bits: u32) -> Result<String, String> {
    if bits > REPORT_MAX_POW_BITS {
        return Err("penerima meminta bukti kerja yang tidak wajar".to_string());
    }
    for nonce in 0u64..(1u64 << 34) {
        let digest = sha256_bytes(format!("{challenge}:{nonce}").as_bytes());
        if leading_zero_bits(&digest) >= bits {
            return Ok(nonce.to_string());
        }
    }
    Err("bukti kerja tidak selesai".to_string())
}

/// Satu permintaan POST yang jawabannya wajib bertanda tangan server.
fn signed_post(
    client: &reqwest::blocking::Client,
    path: &str,
    context: &str,
    headers: &[(&str, String)],
    body: String,
) -> Result<(u16, serde_json::Value), String> {
    let mut request = client.post(format!("{REPORT_BASE}{path}")).body(body);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let response = request
        .send()
        .map_err(|_| "tidak bisa terhubung ke penerima laporan".to_string())?;
    let status = response.status().as_u16();
    let signature = response
        .headers()
        .get("x-simpul-server-sig")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let text = response.text().unwrap_or_default();
    verify_server_reply(context, status, &text, &signature)?;
    Ok((status, serde_json::from_str(&text).unwrap_or_default()))
}

fn reply_error(status: u16, value: &serde_json::Value) -> String {
    value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("penerima menjawab {status}"), str::to_string)
}

/// Kirim satu laporan. Hanya dipanggil saat pengguna menekan "Kirim laporan"; isinya
/// sama dengan yang ditampilkan di jendela laporan. Mengembalikan nomor laporan.
pub fn send_report(description: &str, log: Option<&str>) -> Result<u64, String> {
    use base64::Engine;
    use ed25519_dalek::Signer;
    let b64 = base64::engine::general_purpose::STANDARD;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("simpul-launcher/{LAUNCHER_VERSION}"))
        .build()
        .map_err(|e| e.to_string())?;

    // 1. Tantangan sekali pakai.
    let nonce = hex(&random_bytes::<16>()?);
    let (status, value) = signed_post(
        &client,
        "/v2/challenge",
        &nonce,
        &[("x-simpul-nonce", nonce.clone())],
        String::new(),
    )?;
    if status != 200 {
        return Err(reply_error(status, &value));
    }
    let challenge = value
        .get("challenge")
        .and_then(serde_json::Value::as_str)
        .ok_or("penerima tidak memberi tantangan")?
        .to_string();
    let bits = value.get("bits").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;

    // 2. Bukti kerja.
    let pow = solve_proof_of_work(&challenge, bits)?;

    // 3. Isi dan tanda tangan kunci pemasangan.
    let mut payload = serde_json::json!({
        "app": REPORT_APP_ID,
        "version": LAUNCHER_VERSION,
        "os": os_label(),
        "kind": "bug",
        "description": description.trim(),
    });
    if let Some(log) = log.filter(|l| !l.trim().is_empty()) {
        // Baris log dipotong supaya tetap berbentuk log aplikasi yang diterima penerima.
        let trimmed: Vec<String> =
            log.lines().map(|l| l.chars().take(300).collect::<String>()).collect();
        payload["log"] = serde_json::Value::String(trimmed.join("\n"));
    }
    let body = payload.to_string();
    let key = install_signing_key()?;
    let message = format!(
        "simpul-report-v2\n{challenge}\n{pow}\n{}",
        hex(&sha256_bytes(body.as_bytes()))
    );
    let signature = key.sign(message.as_bytes());
    let (status, value) = signed_post(
        &client,
        "/v2/report",
        &challenge,
        &[
            ("content-type", "application/json".to_string()),
            ("x-simpul-challenge", challenge.clone()),
            ("x-simpul-pow", pow),
            ("x-simpul-key", b64.encode(key.verifying_key().to_bytes())),
            ("x-simpul-sig", b64.encode(signature.to_bytes())),
        ],
        body,
    )?;
    if status == 200 && value.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(value.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0))
    } else {
        Err(reply_error(status, &value))
    }
}

/// Buka tautan (mis. `mailto:`) lewat pengendali bawaan sistem operasi.
pub fn open_with_system(link: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let spawned = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", link])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = Command::new("open").arg(link).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let spawned = Command::new("xdg-open").arg(link).spawn();
    spawned.map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- tema bersama

/// Tema dibagi ke semua aplikasi Simpul lewat `theme.json` di folder data bersama.
pub fn save_shared_theme(theme: &impl serde::Serialize, reduce_transparency: bool) {
    let dir = shared_data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let value = serde_json::json!({ "theme": theme, "reduce_transparency": reduce_transparency });
    let _ = std::fs::write(dir.join("theme.json"), value.to_string());
}

pub fn load_shared_theme<T: serde::de::DeserializeOwned>() -> Option<(T, bool)> {
    let raw = std::fs::read_to_string(shared_data_dir().join("theme.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let theme = serde_json::from_value::<T>(value.get("theme")?.clone()).ok()?;
    let reduce = value
        .get("reduce_transparency")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Some((theme, reduce))
}

/// Batas isi laporan supaya tautan `mailto:` tetap diterima aplikasi email.
pub const BUG_REPORT_MAX_CHARS: usize = 1200;

/// Isi email laporan bug. Hanya memuat yang ditampilkan ke pengguna sebelum dikirim.
pub fn bug_report_body(app_name: &str, version: &str, description: &str) -> String {
    let trimmed: String = description.trim().chars().take(BUG_REPORT_MAX_CHARS).collect();
    format!(
        "Aplikasi: {app_name}\nVersi: {version}\nSistem: {}\n\nYang terjadi:\n{trimmed}\n",
        os_label()
    )
}

/// Tautan `mailto:` yang membuka aplikasi email pengguna dengan laporan siap kirim.
pub fn bug_report_mailto(app_name: &str, version: &str, description: &str) -> String {
    let subject = format!("[Bug] {app_name} v{version}");
    format!(
        "mailto:{CONTACT_EMAIL}?subject={}&body={}",
        percent_encode(&subject),
        percent_encode(&bug_report_body(app_name, version, description))
    )
}

// ---------------------------------------------------------------- log diagnostik (opt-in)
//
// Mati secara bawaan. Kalau pengguna mengizinkan, kejadian ringkas dicatat ke file lokal.
// Log TIDAK dikirim otomatis: hanya ikut laporan bug kalau pengguna mencentangnya, dan
// isinya ditampilkan dulu sebelum email dibuka.

const LOG_MAX_BYTES: u64 = 256 * 1024;
/// 0 = belum dibaca dari disk, 1 = mati, 2 = hidup.
static DIAGNOSTICS: AtomicU8 = AtomicU8::new(0);

fn privacy_file() -> PathBuf {
    shared_data_dir().join("privacy.json")
}

pub fn log_file() -> PathBuf {
    shared_data_dir().join("logs").join("launcher.log")
}

pub fn diagnostics_enabled() -> bool {
    match DIAGNOSTICS.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::fs::read_to_string(privacy_file())
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v.get("diagnostics").and_then(serde_json::Value::as_bool))
                .unwrap_or(false);
            DIAGNOSTICS.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Simpan pilihan pengguna. Mematikan izin juga menghapus log yang sudah ada.
pub fn set_diagnostics(on: bool) -> Result<(), String> {
    let path = privacy_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::json!({ "diagnostics": on }).to_string();
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    DIAGNOSTICS.store(if on { 2 } else { 1 }, Ordering::Relaxed);
    if !on {
        let _ = std::fs::remove_file(log_file());
        let _ = std::fs::remove_file(log_file().with_extension("log.old"));
    }
    Ok(())
}

/// Ganti folder pengguna dengan `~` supaya nama akun tidak ikut tercatat.
fn scrub(text: &str) -> String {
    let mut out = text.replace(['\r', '\n'], " ");
    for var in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(var) {
            if home.len() > 3 {
                out = out.replace(&home, "~");
                out = out.replace(&home.replace('\\', "/"), "~");
            }
        }
    }
    out
}

/// Catat satu kejadian. Tidak melakukan apa-apa kalau pengguna belum mengizinkan.
pub fn log_event(event: &str) {
    if !diagnostics_enabled() {
        return;
    }
    let path = log_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let line = format!("{secs} v{LAUNCHER_VERSION} {}\n", scrub(event));
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Baris-baris terakhir log, dibatasi `max_chars`, untuk ditampilkan dan dilampirkan.
pub fn log_tail(max_lines: usize, max_chars: usize) -> String {
    let Ok(raw) = std::fs::read_to_string(log_file()) else {
        return String::new();
    };
    let mut picked: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for line in raw.lines().rev().take(max_lines) {
        if used + line.len() + 1 > max_chars {
            break;
        }
        used += line.len() + 1;
        picked.push(line);
    }
    let mut out = String::new();
    for line in picked.into_iter().rev() {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_maps_to_manifest_id() {
        assert_eq!(manifest_id_for("simpulsync-4"), "sync");
        assert_eq!(launcher_id_for("sync"), "simpulsync-4");
        assert_eq!(manifest_id_for("video"), "video");
        assert_eq!(launcher_id_for(manifest_id_for("design")), "design");
    }

    #[test]
    fn minimum_version_blocks_only_older_builds() {
        assert_eq!(version_below("1.3.0", "1.2.9").as_deref(), Some("1.3.0"));
        assert_eq!(version_below("1.3.0", "1.3.0"), None);
        assert_eq!(version_below("1.3.0", "2.0.0"), None);
        // Versi yang tidak terbaca tidak boleh mengunci pengguna.
        assert_eq!(version_below("abc", "1.0.0"), None);
        assert_eq!(version_below("1.3.0", "abc"), None);
    }

    #[test]
    fn forged_server_reply_is_rejected() {
        // Tanda tangan sembarang, kosong, atau untuk isi lain tidak boleh diterima.
        let fake = "A".repeat(86) + "==";
        assert!(verify_server_reply("x", 200, "{\"ok\":true}", &fake).is_err());
        assert!(verify_server_reply("x", 200, "{\"ok\":true}", "").is_err());
    }

    #[test]
    fn proof_of_work_meets_requested_bits() {
        let nonce = solve_proof_of_work("tantangan-uji", 10).unwrap();
        let digest = sha256_bytes(format!("tantangan-uji:{nonce}").as_bytes());
        assert!(leading_zero_bits(&digest) >= 10);
        assert!(solve_proof_of_work("x", 40).is_err());
    }

    #[test]
    fn bug_report_link_is_encoded_and_bounded() {
        let long = "a b&c\n".repeat(2000);
        let link = bug_report_mailto("Simpul Creative Launcher", "1.0.0", &long);
        assert!(link.starts_with("mailto:simpulcreative.apps@gmail.com?subject=%5BBug%5D"));
        assert!(!link.contains(' ') && !link.contains('\n'));
        assert!(!link[link.find("body=").unwrap()..].contains('&'));
        assert!(link.len() < 6000);
    }

    #[test]
    fn scrub_removes_home_and_newlines() {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        if home.len() > 3 {
            let line = scrub(&format!("gagal buka {home}/x.exe\nbaris dua"));
            assert!(!line.contains(&home));
            assert!(!line.contains('\n'));
        }
    }

    #[test]
    fn unknown_app_may_run() {
        let snap = GuardSnapshot::default();
        assert!(snap.app_may_run("apa-saja"));
        assert!(snap.app_update("apa-saja").is_none());
    }
}
