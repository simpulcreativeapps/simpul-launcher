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

/// Penerima laporan Simpul (Cloudflare Worker, repo `simpul-lapor`). Tidak ada rahasia di
/// sini: penerima yang memegang token dan membatasi jumlah kiriman.
pub const REPORT_URL: &str = "https://simpul-lapor.simpul-lapor.workers.dev/v1/report";
const REPORT_APP_ID: &str = "launcher";

/// Kirim satu laporan. Hanya dipanggil saat pengguna menekan "Kirim laporan"; isinya
/// sama dengan yang ditampilkan di jendela laporan. Mengembalikan nomor laporan.
pub fn send_report(description: &str) -> Result<u64, String> {
    let payload = serde_json::json!({
        "app": REPORT_APP_ID,
        "version": LAUNCHER_VERSION,
        "os": os_label(),
        "kind": "bug",
        "description": description.trim(),
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(format!("simpul-launcher/{LAUNCHER_VERSION}"))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(REPORT_URL)
        .header("content-type", "application/json")
        .header("x-simpul-client", "1")
        .body(payload.to_string())
        .send()
        .map_err(|_| "tidak bisa terhubung ke penerima laporan".to_string())?;
    let status = response.status();
    let value: serde_json::Value =
        serde_json::from_str(&response.text().unwrap_or_default()).unwrap_or_default();
    if status.is_success() && value.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(value.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0))
    } else {
        Err(value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| format!("penerima menjawab {}", status.as_u16()), str::to_string))
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
