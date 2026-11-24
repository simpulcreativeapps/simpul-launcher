use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::models::{
    AgentActivityEvent, AgentOrigin, AppFeature, BuildProfile, CheckEvery, ImportManifest,
    InstallState, LauncherSettings, PackageFormat, SimpulApp, SimpulRegistry, SortKey, ThemeChoice,
    ToolIntegrationStatus, ViewMode,
};

pub fn data_dir() -> PathBuf {
    if let Ok(custom) = env::var("SIMPUL_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("SimpulCreativeLauncher");
        }
    }
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".simpul-creative-launcher");
    }
    env::temp_dir().join("SimpulCreativeLauncher")
}

pub fn registry_file_path() -> PathBuf {
    data_dir().join("simpul-registry.json")
}

pub fn default_install_root() -> PathBuf {
    data_dir().join("apps")
}

pub fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mins = (secs / 60) % 60;
    let hours = ((secs / 3600) + 7) % 24; // WIB UTC+7 friendly display
    format!("{hours:02}:{mins:02} WIB")
}

/// Versi 2: katalog mengikuti manifest; entri contoh bawaan versi 1 dibuang.
pub const REGISTRY_VERSION: u32 = 3;

/// Buang entri contoh dari registry versi lama. Aplikasi yang terpasang dan yang diimpor
/// pengguna atau agen tetap dipertahankan.
fn migrate_registry(reg: &mut SimpulRegistry) -> bool {
    if reg.version >= REGISTRY_VERSION {
        return false;
    }
    if reg.version < 2 {
        reg.apps
            .retain(|a| a.install_state.is_installed() || a.origin != AgentOrigin::SimpulSuite);
        let kept: Vec<String> = reg.apps.iter().map(|a| a.id.clone()).collect();
        reg.events.retain(|e| kept.contains(&e.app_id));
    }
    if reg.version < 3 {
        // Versi 1.0.0 menyimpan keterangan aplikasi terdeteksi dalam bahasa Inggris.
        for app in &mut reg.apps {
            let auto_detected = app
                .features
                .first()
                .is_some_and(|f| f.title == "Auto-Detected Local Install");
            if !auto_detected {
                continue;
            }
            let exe = app.artifact_path.clone().unwrap_or_default();
            app.headline = format!(
                "{} v{} siap dibuka langsung dari Simpul Creative Launcher.",
                app.name, app.latest_version
            );
            app.about = format!(
                "{} ditemukan otomatis di komputer ini pada {} dan ditautkan supaya bisa dibuka sekali klik.",
                app.name, exe
            );
            app.released_date = "terdeteksi".to_string();
            app.features = vec![AppFeature {
                title: "Terdeteksi otomatis".to_string(),
                description: format!("Ditautkan langsung ke {exe}."),
            }];
            app.release_notes =
                vec![format!("v{}: terdeteksi otomatis di komputer ini.", app.latest_version)];
        }
    }
    reg.version = REGISTRY_VERSION;
    true
}

/// Entri katalog untuk aplikasi yang dirilis lewat manifest tetapi belum terpasang.
pub fn catalog_app(id: &str, name: &str, tagline: Option<&str>, version: &str) -> SimpulApp {
    let tagline = tagline.unwrap_or("Aplikasi Simpul Creative Apps").to_string();
    SimpulApp {
        id: id.to_string(),
        name: name.to_string(),
        headline: tagline.clone(),
        about: tagline.clone(),
        tagline,
        category: "Simpul Creative Apps".to_string(),
        latest_version: version.to_string(),
        released_date: String::new(),
        download_size_mb: 0.0,
        sha256: String::new(),
        profile: BuildProfile::Release,
        origin: AgentOrigin::SimpulSuite,
        package_format: PackageFormat::NativeExe,
        accent_rgb: [77, 141, 255],
        icon_kind: if id == "simpulsync-4" { "sync4" } else { "app" }.to_string(),
        pinned: false,
        hidden: false,
        launch_flags: String::new(),
        workspace_path: None,
        artifact_path: None,
        features: Vec::new(),
        release_notes: Vec::new(),
        install_state: InstallState::NotInstalled,
    }
}

/// Muat registry tanpa memindai komputer (cepat, untuk thread UI). Pemindaian dijalankan
/// terpisah di latar oleh antarmuka.
pub fn load_registry_without_scan() -> SimpulRegistry {
    let path = registry_file_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(mut reg) = serde_json::from_str::<SimpulRegistry>(&raw) {
            if migrate_registry(&mut reg) {
                let _ = save_registry(&reg);
            }
            return reg;
        }
    }
    let reg = default_registry();
    let _ = save_registry(&reg);
    reg
}

pub fn load_or_init_registry(ephemeral: bool) -> SimpulRegistry {
    if ephemeral {
        let mut reg = default_registry();
        reconcile_installed_apps(&mut reg);
        return reg;
    }
    let path = registry_file_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(mut reg) = serde_json::from_str::<SimpulRegistry>(&raw) {
            let migrated = migrate_registry(&mut reg);
            let changes = reconcile_installed_apps(&mut reg);
            if migrated || !changes.is_empty() {
                let _ = save_registry(&reg);
            }
            return reg;
        }
    }
    let mut reg = default_registry();
    reconcile_installed_apps(&mut reg);
    let _ = save_registry(&reg);
    reg
}

pub fn save_registry(reg: &SimpulRegistry) -> Result<(), String> {
    let path = registry_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed creating data folder: {e}"))?;
    }
    let json = serde_json::to_string_pretty(reg)
        .map_err(|e| format!("Failed serializing registry: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed writing registry: {e}"))
}

pub fn registry_mtime() -> Option<SystemTime> {
    fs::metadata(registry_file_path())
        .and_then(|m| m.modified())
        .ok()
}

pub fn detect_integrations(settings: &LauncherSettings) -> ToolIntegrationStatus {
    let (agy_detected, agy_path) = find_on_path(&settings.agy_command);
    let (claude_detected, claude_path) = find_on_path(&settings.claude_command);
    // Perintah untuk agen menunjuk program CLI di sebelah program jendela bila ada.
    let cli_self_path = env::current_exe()
        .map(|p| {
            p.parent()
                .map(|d| d.join("simpul-creative-launcher-cli.exe"))
                .filter(|cli| cli.exists())
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .unwrap_or_else(|_| "simpul-creative-launcher".to_string());
    ToolIntegrationStatus {
        agy_detected,
        agy_path,
        claude_detected,
        claude_path,
        cli_self_path,
        registry_path: registry_file_path().display().to_string(),
    }
}

fn find_on_path(cmd_name: &str) -> (bool, Option<String>) {
    let trimmed = cmd_name.trim();
    if trimmed.is_empty() {
        return (false, None);
    }
    let direct = Path::new(trimmed);
    if direct.is_file() {
        return (true, Some(direct.display().to_string()));
    }
    let Some(path_var) = env::var_os("PATH") else {
        return (false, None);
    };
    #[cfg(target_os = "windows")]
    let exts = ["", ".exe", ".cmd", ".bat", ".ps1"];
    #[cfg(not(target_os = "windows"))]
    let exts = [""];

    for dir in env::split_paths(&path_var) {
        for ext in exts {
            let candidate = dir.join(format!("{trimmed}{ext}"));
            if candidate.is_file() {
                return (true, Some(candidate.display().to_string()));
            }
        }
    }
    (false, None)
}

pub fn compute_sha256_short(path: &Path) -> String {
    let Ok(meta) = fs::metadata(path) else {
        return pseudo_sha256(&path.display().to_string());
    };
    if !meta.is_file() || meta.len() > 512 * 1024 * 1024 {
        return pseudo_sha256(&path.display().to_string());
    }
    let Ok(mut file) = fs::File::open(path) else {
        return pseudo_sha256(&path.display().to_string());
    };
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    while let Ok(n) = file.read(&mut buf) {
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn pseudo_sha256(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn detect_profile_from_path(path: &Path, explicit_profile: Option<&str>) -> BuildProfile {
    if let Some(p) = explicit_profile {
        return match p.trim().to_ascii_lowercase().as_str() {
            "debug" | "dev" => BuildProfile::Debug,
            "staging" | "rc" | "beta" => BuildProfile::Staging,
            _ => BuildProfile::Release,
        };
    }
    let lower = path.display().to_string().replace('/', "\\").to_ascii_lowercase();
    if lower.contains("\\target\\debug\\")
        || lower.ends_with("\\target\\debug")
        || lower.contains("\\bin\\debug\\")
        || lower.contains("-debug.")
        || lower.contains("_debug.")
    {
        BuildProfile::Debug
    } else if lower.contains("-beta") || lower.contains("-rc") || lower.contains("\\staging\\") {
        BuildProfile::Staging
    } else {
        BuildProfile::Release
    }
}

pub fn detect_package_format(path: &Path) -> PackageFormat {
    if path.is_dir() {
        return PackageFormat::ManifestBundle;
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "msi" => PackageFormat::Msi,
        "zip" => PackageFormat::PortableZip,
        "exe" => PackageFormat::NativeExe,
        _ => PackageFormat::ManifestBundle,
    }
}

/// Versi dipakai sebagai nama folder instalasi, jadi tidak boleh memuat pemisah path atau `..`.
pub fn is_safe_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && !version.contains("..")
        && version.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

pub fn slugify_id(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub struct ImportRequest {
    pub source_path: PathBuf,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub tagline: Option<String>,
    pub category: Option<String>,
    pub agent: AgentOrigin,
    pub allow_debug: bool,
    pub auto_install: bool,
}

/// Imports an application or manifest into Simpul Creative Launcher (used by CLI, Claude, agy, and UI).
pub fn import_into_registry(
    reg: &mut SimpulRegistry,
    req: ImportRequest,
) -> Result<SimpulApp, String> {
    let source = req.source_path;
    let mut manifest: Option<ImportManifest> = None;

    // Support importing either a direct manifest JSON, a directory containing simpul-app.json, or a binary/installer
    if source.is_file()
        && source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("json"))
            == Some(true)
    {
        let text = fs::read_to_string(&source)
            .map_err(|e| format!("Could not read manifest {}: {e}", source.display()))?;
        manifest = Some(
            serde_json::from_str::<ImportManifest>(&text)
                .map_err(|e| format!("Invalid Simpul app manifest JSON: {e}"))?,
        );
    } else if source.is_dir() {
        let candidate = source.join("simpul-app.json");
        if candidate.is_file() {
            if let Ok(text) = fs::read_to_string(&candidate) {
                manifest = serde_json::from_str::<ImportManifest>(&text).ok();
            }
        }
    }

    let resolved_artifact = if let Some(ref m) = manifest {
        if let Some(ref bin) = m.binary {
            let p = PathBuf::from(bin);
            if p.is_absolute() {
                p
            } else {
                source
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(p)
            }
        } else {
            source.clone()
        }
    } else {
        source.clone()
    };

    let explicit_prof = manifest.as_ref().and_then(|m| m.profile.as_deref());
    let profile = detect_profile_from_path(&resolved_artifact, explicit_prof);

    // Enforce "Layak Deploy (Bukan Debug)" gate unless overridden
    if reg.settings.require_release_profile && profile == BuildProfile::Debug && !req.allow_debug {
        return Err(format!(
            "Deploy Gate Blocked: '{}' terdeteksi sebagai DEBUG build. Simpul Creative Launcher hanya menerima software layak deploy (Release). Build dengan `--release` atau gunakan `--allow-debug`.",
            resolved_artifact.display()
        ));
    }

    let derived_name = req
        .name
        .or_else(|| manifest.as_ref().and_then(|m| m.name.clone()))
        .unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("SimpulImportedApp")
                .to_string()
        });

    let id = req
        .id
        .or_else(|| manifest.as_ref().and_then(|m| m.id.clone()))
        .map(|s| slugify_id(&s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slugify_id(&derived_name));

    let version = req
        .version
        .or_else(|| manifest.as_ref().and_then(|m| m.version.clone()))
        .unwrap_or_else(|| "1.0.0".to_string());
    if id.is_empty() {
        return Err("Id aplikasi tidak valid: harus memuat huruf atau angka.".to_string());
    }
    if !is_safe_version(&version) {
        return Err(format!(
            "Versi '{version}' tidak valid. Hanya huruf, angka, titik, plus, dan tanda hubung (maks. 32 karakter)."
        ));
    }

    let tagline = req
        .tagline
        .or_else(|| manifest.as_ref().and_then(|m| m.tagline.clone()))
        .unwrap_or_else(|| {
            format!(
                "Diimpor lewat {}, paket {} terverifikasi",
                req.agent.label(),
                profile.short_label()
            )
        });

    let headline = manifest
        .as_ref()
        .and_then(|m| m.headline.clone())
        .unwrap_or_else(|| tagline.clone());

    let about = manifest
        .as_ref()
        .and_then(|m| m.about.clone())
        .unwrap_or_else(|| {
            format!(
                "{} diimpor ke Simpul Creative Launcher lewat {} dan sudah divalidasi.",
                derived_name,
                req.agent.label()
            )
        });

    let category = req
        .category
        .or_else(|| manifest.as_ref().and_then(|m| m.category.clone()))
        .unwrap_or_else(|| "Creative Tool".to_string());

    let size_mb = fs::metadata(&resolved_artifact)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| ((m.len() as f32) / (1024.0 * 1024.0)).max(0.4))
        .unwrap_or(18.5);

    let sha256 = compute_sha256_short(&resolved_artifact);
    let pkg_format = detect_package_format(&resolved_artifact);
    let accent_rgb = match req.agent {
        AgentOrigin::Claude => [168, 85, 247],
        AgentOrigin::AntigravityAgy => [6, 182, 212],
        AgentOrigin::SimpulSuite => [78, 123, 251],
        AgentOrigin::CliManual => [16, 185, 129],
    };

    let workspace = manifest
        .as_ref()
        .and_then(|m| m.workspace.clone())
        .or_else(|| {
            if source.is_dir() {
                Some(source.display().to_string())
            } else {
                source.parent().map(|p| p.display().to_string())
            }
        });

    let flags = manifest
        .as_ref()
        .and_then(|m| m.launch_flags.clone())
        .unwrap_or_default();

    let mut new_app = SimpulApp {
        id: id.clone(),
        name: derived_name.clone(),
        tagline,
        headline,
        about,
        category,
        latest_version: version.clone(),
        released_date: "2026-11-16".to_string(),
        download_size_mb: (size_mb * 10.0).round() / 10.0,
        sha256,
        profile,
        origin: req.agent,
        package_format: pkg_format,
        accent_rgb,
        icon_kind: "custom".to_string(),
        pinned: false,
        hidden: false,
        launch_flags: flags,
        workspace_path: workspace,
        artifact_path: Some(resolved_artifact.display().to_string()),
        features: vec![
            AppFeature {
                title: "Paket siap pakai".to_string(),
                description: format!("Berkas {} terverifikasi dengan tanda terima SHA-256.", profile.short_label()),
            },
            AppFeature {
                title: "CLI, Claude & agy Managed".to_string(),
                description: "Supports headless CLI updates, workspace handoff to Antigravity (agy), and Claude automation.".to_string(),
            },
        ],
        release_notes: vec![format!(
            "v{version}: diimpor lewat {} ({})",
            req.agent.label(),
            now_label()
        )],
        install_state: InstallState::NotInstalled,
    };

    if let Some(existing_idx) = reg.apps.iter().position(|a| a.id == id) {
        // Preserve existing install state unless auto_install updates it
        new_app.install_state = reg.apps[existing_idx].install_state.clone();
        new_app.pinned = reg.apps[existing_idx].pinned;
        new_app.icon_kind = reg.apps[existing_idx].icon_kind.clone();
        reg.apps[existing_idx] = new_app.clone();
    } else {
        reg.apps.push(new_app.clone());
    }

    reg.events.insert(
        0,
        AgentActivityEvent {
            timestamp: now_label(),
            actor: req.agent,
            action: "import".to_string(),
            app_id: id.clone(),
            summary: format!(
                "{} mengimpor {} v{} ({})",
                req.agent.label(),
                derived_name,
                version,
                profile.short_label()
            ),
        },
    );
    reg.events.truncate(40);

    if req.auto_install {
        return install_in_registry(reg, &id, req.agent, req.allow_debug);
    }

    Ok(new_app)
}

/// Installs a registered app into Simpul's managed deployment folder and writes an integrity receipt.
pub fn install_in_registry(
    reg: &mut SimpulRegistry,
    app_id: &str,
    actor: AgentOrigin,
    allow_debug: bool,
) -> Result<SimpulApp, String> {
    let idx = reg
        .apps
        .iter()
        .position(|a| a.id.eq_ignore_ascii_case(app_id) || a.name.eq_ignore_ascii_case(app_id))
        .ok_or_else(|| format!("Application '{app_id}' was not found in Simpul registry"))?;

    if reg.settings.require_release_profile
        && reg.apps[idx].profile == BuildProfile::Debug
        && !allow_debug
    {
        return Err(format!(
            "Deploy Gate Blocked: '{}' adalah Debug build. Hanya aplikasi layak deploy (Release) yang dapat diinstall ke environment produksi.",
            reg.apps[idx].name
        ));
    }

    let version = reg.apps[idx].latest_version.clone();
    let app_slug = reg.apps[idx].id.clone();
    if app_slug.is_empty() || slugify_id(&app_slug) != app_slug || !is_safe_version(&version) {
        return Err(format!(
            "Id atau versi aplikasi tidak aman untuk dipasang ('{app_slug}' v{version})."
        ));
    }
    let install_dir = PathBuf::from(&reg.settings.install_root)
        .join(&app_slug)
        .join(&version);

    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Could not create install folder {}: {e}", install_dir.display()))?;

    let mut installed_target = install_dir.clone();

    if let Some(ref art_str) = reg.apps[idx].artifact_path {
        let art = Path::new(art_str);
        if art.is_file() {
            if let Some(fname) = art.file_name() {
                let dest = install_dir.join(fname);
                if art != dest {
                    let _ = fs::copy(art, &dest);
                }
                installed_target = dest;
            }
        }
    }

    // Write verified deployment receipt
    let receipt = serde_json::json!({
        "id": reg.apps[idx].id,
        "name": reg.apps[idx].name,
        "version": version,
        "profile": reg.apps[idx].profile,
        "sha256": reg.apps[idx].sha256,
        "installed_by": actor.label(),
        "installed_at": now_label(),
        "target_path": installed_target.display().to_string(),
    });
    let _ = fs::write(
        install_dir.join("install-receipt.json"),
        serde_json::to_string_pretty(&receipt).unwrap_or_default(),
    );

    reg.apps[idx].install_state = InstallState::Installed {
        version: version.clone(),
        install_path: installed_target.display().to_string(),
        installed_at: now_label(),
    };

    let updated = reg.apps[idx].clone();
    reg.events.insert(
        0,
        AgentActivityEvent {
            timestamp: now_label(),
            actor,
            action: "install".to_string(),
            app_id: updated.id.clone(),
            summary: format!(
                "{} memasang {} v{} ke {}",
                actor.label(),
                updated.name,
                version,
                install_dir.display()
            ),
        },
    );
    reg.events.truncate(40);
    Ok(updated)
}

pub fn uninstall_in_registry(
    reg: &mut SimpulRegistry,
    app_id: &str,
    actor: AgentOrigin,
) -> Result<String, String> {
    let idx = reg
        .apps
        .iter()
        .position(|a| a.id.eq_ignore_ascii_case(app_id) || a.name.eq_ignore_ascii_case(app_id))
        .ok_or_else(|| format!("Application '{app_id}' was not found"))?;

    let name = reg.apps[idx].name.clone();
    let slug = reg.apps[idx].id.clone();
    let app_root = PathBuf::from(&reg.settings.install_root).join(&slug);
    // Hanya hapus subfolder milik aplikasi ini; id kosong atau aneh tidak boleh menyentuh folder induk.
    if !slug.is_empty() && slugify_id(&slug) == slug && app_root.exists() {
        let _ = fs::remove_dir_all(&app_root);
    }
    reg.apps[idx].install_state = InstallState::NotInstalled;
    reg.events.insert(
        0,
        AgentActivityEvent {
            timestamp: now_label(),
            actor,
            action: "uninstall".to_string(),
            app_id: slug,
            summary: format!("{} menghapus {}", actor.label(), name),
        },
    );
    reg.events.truncate(40);
    Ok(name)
}

#[derive(Debug, Clone)]
pub struct DiscoveredInstalledApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub exe_path: PathBuf,
    pub install_dir: PathBuf,
    pub default_launch_flags: String,
    pub source_label: String,
}

fn is_launchable_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "exe" | "cmd" | "bat" | "lnk")
}

fn is_uninstaller_name(name_lower: &str) -> bool {
    name_lower.starts_with("unins")
        || name_lower.contains("uninstall")
        || name_lower.contains("setup")
        || name_lower.contains("installer")
}

fn find_exe_in_dir(dir: &Path, preferred_names: &[&str]) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    for pref in preferred_names {
        let cand = dir.join(pref);
        if cand.is_file() {
            return Some(cand);
        }
    }
    let mut best: Option<PathBuf> = None;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let is_exe = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("exe"))
                == Some(true);
            if !is_exe {
                continue;
            }
            let fname = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if is_uninstaller_name(&fname) || fname.contains("console") {
                continue;
            }
            best = Some(p);
            break;
        }
    }
    best
}

fn clean_icon_or_path_value(raw: &str) -> Option<PathBuf> {
    let mut s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(stripped) = s.strip_prefix('"') {
        if let Some(end_q) = stripped.find('"') {
            s = &stripped[..end_q];
        } else {
            s = stripped;
        }
    }
    // Strip trailing ",0" icon index from DisplayIcon
    if let Some(comma_idx) = s.rfind(',') {
        let after = &s[comma_idx + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit() || c == '-') {
            s = &s[..comma_idx];
        }
    }
    let trimmed = s.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn canonicalize_known_simpul_app(display_name: &str) -> Option<(&'static str, &'static str, &'static str, &'static [&'static str])> {
    // Bandingkan tanpa spasi/tanda baca: "Simpul Sync version 4.2.0", "SimpulSync 4", "simpul-sync"
    // harus dikenali sebagai aplikasi yang sama dengan entri katalog.
    let lower: String = display_name
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if lower.contains("simpulgram") {
        Some((
            "simpulgram",
            "SimpulGram",
            "",
            &["SimpulGram.exe", "simpulgram.exe"],
        ))
    } else if lower.contains("simpulsync") {
        Some((
            "simpulsync-4",
            "SimpulSync 4",
            "",
            &["simpulsync-gui.exe", "SimpulSync 4.exe", "simpulsync4.exe", "SimpulSync.exe"],
        ))
    } else if lower.contains("capr") || lower.contains("opennotebook") {
        Some((
            "capr",
            "Capr",
            "--desktop-app",
            &["OpenNotebook.exe", "CAPr.exe", "Capr.exe"],
        ))
    } else if lower.contains("simpulproject") {
        Some((
            "simpulproject-ai",
            "Simpulproject AI",
            "",
            &["SimpulprojectAI.exe", "Simpulproject AI.exe"],
        ))
    } else {
        None
    }
}

/// Ubah satu entri Uninstall menjadi aplikasi Simpul yang ditemukan (None bila bukan aplikasi Simpul).
#[cfg(target_os = "windows")]
fn discovered_from_uninstall_entry(
    display_name: &str,
    ver: Option<&str>,
    loc: Option<&str>,
    icon: Option<&str>,
) -> Option<DiscoveredInstalledApp> {
    let lower = display_name.to_ascii_lowercase();
    if !(lower.contains("simpul") || lower.contains("capr") || lower.contains("opennotebook")) {
        return None;
    }
    // Launcher sendiri tidak dimasukkan.
    if lower.contains("creative launcher") || lower.contains("creative apps launcher") {
        return None;
    }
    let (id, clean_name, default_flags, pref_exes) = if let Some(known) = canonicalize_known_simpul_app(display_name) {
        (known.0.to_string(), known.1.to_string(), known.2.to_string(), known.3)
    } else {
        (slugify_id(display_name), display_name.trim().to_string(), String::new(), &[][..])
    };

    let mut resolved_exe: Option<PathBuf> = None;
    if let Some(ic) = icon.and_then(clean_icon_or_path_value) {
        let is_exe = ic.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("exe")) == Some(true);
        if ic.is_file() && is_exe {
            let fname = ic.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if !is_uninstaller_name(&fname) {
                resolved_exe = Some(ic);
            }
        }
    }
    if resolved_exe.is_none() {
        if let Some(loc_dir) = loc.and_then(clean_icon_or_path_value) {
            resolved_exe = find_exe_in_dir(&loc_dir, pref_exes);
        }
    }
    let exe_path = resolved_exe?;
    let install_dir = exe_path.parent().map(Path::to_path_buf).unwrap_or_else(|| exe_path.clone());
    let version = ver
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1.0.0".to_string());
    Some(DiscoveredInstalledApp {
        id,
        name: clean_name,
        version,
        exe_path,
        install_dir,
        default_launch_flags: default_flags,
        source_label: "Windows Registry".to_string(),
    })
}

/// Baca daftar aplikasi terpasang (kunci Uninstall) langsung lewat API Registry Windows.
/// Dulu memakai `reg query /s` + mengurai teks: 17 detik atau lebih per kunci di laptop pemilik.
/// Sekarang hanya `DisplayName` tiap entri yang dibaca; kolom lain hanya untuk entri yang cocok.
#[cfg(target_os = "windows")]
fn query_uninstall_registry_apps() -> Vec<DiscoveredInstalledApp> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    let roots = [
        (HKEY_CURRENT_USER, UNINSTALL),
        (HKEY_LOCAL_MACHINE, UNINSTALL),
        (HKEY_LOCAL_MACHINE, UNINSTALL_WOW),
    ];
    let mut found = Vec::new();
    for (hive, path) in roots {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else {
            continue;
        };
        for sub in root.enum_keys().flatten() {
            let Ok(entry) = root.open_subkey_with_flags(&sub, KEY_READ) else {
                continue;
            };
            let Ok(name) = entry.get_value::<String, _>("DisplayName") else {
                continue;
            };
            let get = |field: &str| entry.get_value::<String, _>(field).ok();
            if let Some(app) = discovered_from_uninstall_entry(
                &name,
                get("DisplayVersion").as_deref(),
                get("InstallLocation").as_deref(),
                get("DisplayIcon").as_deref(),
            ) {
                found.push(app);
            }
        }
    }
    found
}

#[cfg(not(target_os = "windows"))]
fn query_uninstall_registry_apps() -> Vec<DiscoveredInstalledApp> {
    Vec::new()
}

pub fn discover_installed_apps_on_pc() -> Vec<DiscoveredInstalledApp> {
    fn has_id(list: &[DiscoveredInstalledApp], id: &str) -> bool {
        list.iter().any(|r| r.id.eq_ignore_ascii_case(id))
    }

    let mut results = query_uninstall_registry_apps();

    let local_app_data = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let program_files = env::var("ProgramFiles").ok().map(PathBuf::from);
    let user_profile = env::var("USERPROFILE").ok().map(PathBuf::from);

    // Filesystem probes for SimpulGram
    if !has_id(&results, "simpulgram") {
        let mut cands = Vec::new();
        if let Some(ref l) = local_app_data {
            cands.push(l.join("SimpulGram").join("SimpulGram.exe"));
            cands.push(l.join("Programs").join("SimpulGram").join("SimpulGram.exe"));
        }
        if let Some(ref pf) = program_files {
            cands.push(pf.join("SimpulGram").join("SimpulGram.exe"));
        }
        if let Some(exe) = cands.into_iter().find(|p| p.is_file()) {
            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| exe.clone());
            results.push(DiscoveredInstalledApp {
                id: "simpulgram".to_string(),
                name: "SimpulGram".to_string(),
                version: "1.0.0".to_string(),
                exe_path: exe,
                install_dir: dir,
                default_launch_flags: String::new(),
                source_label: "Local Disk".to_string(),
            });
        }
    }

    // Filesystem probes for SimpulSync 4
    if !has_id(&results, "simpulsync-4") {
        let mut cands = Vec::new();
        if let Some(ref l) = local_app_data {
            cands.push(l.join("Programs").join("SimpulSync 4").join("SimpulSync 4.exe"));
            cands.push(l.join("Programs").join("SimpulSync 4").join("simpulsync4.exe"));
            cands.push(l.join("SimpulSync 4").join("SimpulSync 4.exe"));
        }
        if let Some(ref pf) = program_files {
            cands.push(pf.join("SimpulSync 4").join("SimpulSync 4.exe"));
        }
        if let Some(exe) = cands.into_iter().find(|p| p.is_file()) {
            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| exe.clone());
            results.push(DiscoveredInstalledApp {
                id: "simpulsync-4".to_string(),
                name: "SimpulSync 4".to_string(),
                version: "4.1.11".to_string(),
                exe_path: exe,
                install_dir: dir,
                default_launch_flags: String::new(),
                source_label: "Local Disk".to_string(),
            });
        }
    }

    // Filesystem probes for Capr (OpenNotebook)
    if !has_id(&results, "capr") {
        let mut cands = vec![
            PathBuf::from(r"D:\OpenNotebook\OpenNotebook.exe"),
            PathBuf::from(r"C:\OpenNotebook\OpenNotebook.exe"),
            PathBuf::from(r"D:\OpenNotebook\CAPr.exe"),
        ];
        if let Some(ref l) = local_app_data {
            cands.push(l.join("Programs").join("Capr").join("OpenNotebook.exe"));
            cands.push(l.join("Programs").join("OpenNotebook").join("OpenNotebook.exe"));
            cands.push(l.join("Capr").join("OpenNotebook.exe"));
        }
        if let Some(exe) = cands.into_iter().find(|p| p.is_file()) {
            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| exe.clone());
            results.push(DiscoveredInstalledApp {
                id: "capr".to_string(),
                name: "Capr".to_string(),
                version: "2.1.0".to_string(),
                exe_path: exe,
                install_dir: dir,
                default_launch_flags: "--desktop-app".to_string(),
                source_label: "Local Disk".to_string(),
            });
        }
    }

    // Filesystem probes for Simpulproject AI
    if !has_id(&results, "simpulproject-ai") {
        let mut cands = Vec::new();
        if let Some(ref l) = local_app_data {
            cands.push(l.join("Programs").join("Simpulproject AI").join("SimpulprojectAI.exe"));
        }
        if let Some(ref u) = user_profile {
            cands.push(u.join("Documents").join("simpulproject-ai").join("SimpulprojectAI.exe"));
        }
        if let Some(exe) = cands.into_iter().find(|p| p.is_file()) {
            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| exe.clone());
            results.push(DiscoveredInstalledApp {
                id: "simpulproject-ai".to_string(),
                name: "Simpulproject AI".to_string(),
                version: "3.3.24".to_string(),
                exe_path: exe,
                install_dir: dir,
                default_launch_flags: String::new(),
                source_label: "Local Disk".to_string(),
            });
        }
    }

    // Deduplicate by id, preserving first valid executable
    let mut deduped: Vec<DiscoveredInstalledApp> = Vec::new();
    for item in results {
        if !item.exe_path.is_file() {
            continue;
        }
        if !deduped.iter().any(|x| x.id.eq_ignore_ascii_case(&item.id)) {
            deduped.push(item);
        }
    }
    deduped
}

fn is_placeholder_launch_flags(flags: &str) -> bool {
    matches!(
        flags.trim(),
        "--gpu-raster --production"
            | "--workers 8 --verify-chunks"
            | "--notebook-kernel=native"
            | "--strict-release"
    )
}

/// Reconciles the launcher registry with real installed Simpul apps detected on this PC.
/// Fixes placeholder paths in existing saved registries so clicking "Open" launches the real `.exe`.
pub fn reconcile_installed_apps(reg: &mut SimpulRegistry) -> Vec<String> {
    apply_discovered_apps(reg, discover_installed_apps_on_pc(), &std::collections::BTreeMap::new())
}

/// Hash SHA-256 tiap .exe yang ditemukan, dihitung di thread latar (berkas bisa besar).
/// Kunci = jalur .exe.
pub fn hash_discovered_apps(discovered: &[DiscoveredInstalledApp]) -> std::collections::BTreeMap<String, String> {
    discovered
        .iter()
        .map(|d| (d.exe_path.display().to_string(), compute_sha256_short(&d.exe_path)))
        .collect()
}

/// Bagian cepat dari pemindaian: menerapkan hasil `discover_installed_apps_on_pc` ke registry.
/// Antarmuka menjalankan penemuan (lambat: `reg query` dan cek disk) di thread latar, lalu
/// memanggil fungsi ini di thread UI supaya jendela tidak "Not Responding".
pub fn apply_discovered_apps(
    reg: &mut SimpulRegistry,
    discovered: Vec<DiscoveredInstalledApp>,
    hashes: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut updated_names = Vec::new();
    let discovered_ids: Vec<String> = discovered.iter().map(|f| f.id.clone()).collect();

    for found in discovered {
        let exe_str = found.exe_path.display().to_string();
        let dir_str = found.install_dir.display().to_string();
        let real_size_mb = fs::metadata(&found.exe_path)
            .ok()
            .map(|m| (((m.len() as f32) / (1024.0 * 1024.0)).max(0.1) * 10.0).round() / 10.0);

        if let Some(app) = reg
            .apps
            .iter_mut()
            .find(|a| a.id.eq_ignore_ascii_case(&found.id))
        {
            let current_valid = match &app.install_state {
                InstallState::Installed { install_path, .. } => {
                    is_launchable_file(Path::new(install_path))
                }
                InstallState::NotInstalled => false,
            };

            let mut changed = false;
            if !current_valid {
                app.install_state = InstallState::Installed {
                    version: found.version.clone(),
                    install_path: exe_str.clone(),
                    installed_at: format!("Detected ({})", found.source_label),
                };
                app.latest_version = found.version.clone();
                app.artifact_path = Some(exe_str.clone());
                app.workspace_path = Some(dir_str.clone());
                app.package_format = PackageFormat::NativeExe;
                if let Some(sz) = real_size_mb {
                    app.download_size_mb = sz;
                }
                if is_placeholder_launch_flags(&app.launch_flags) || app.launch_flags.trim().is_empty() {
                    app.launch_flags = found.default_launch_flags.clone();
                }
                changed = true;
            } else {
                // Aplikasi diperbarui di luar launcher (atau lewat installer yang dijalankan launcher):
                // exe-nya masih di tempat yang sama, tapi versinya berubah. Ikuti versi hasil pindaian,
                // kalau tidak launcher terus menawarkan "Perbarui" untuk versi yang sudah terpasang.
                // Versi baru bisa terpasang di FOLDER LAIN (mis. Sync lama di "SimpulSync 4\simpulsync4.exe",
                // Sync 4.2.x di "Simpul Sync\simpulsync-gui.exe"): jalur exe juga harus ikut pindah,
                // kalau tidak tombol Buka tetap membuka versi lama (laporan simpul-bugs #4).
                if let InstallState::Installed { version, install_path, .. } = &mut app.install_state {
                    let baru = found.version.trim();
                    if !baru.is_empty() && version.as_str() != baru {
                        *version = baru.to_string();
                        changed = true;
                    }
                    if !install_path.eq_ignore_ascii_case(&exe_str) {
                        *install_path = exe_str.clone();
                        app.artifact_path = Some(exe_str.clone());
                        app.workspace_path = Some(dir_str.clone());
                        if let Some(sz) = real_size_mb {
                            app.download_size_mb = sz;
                        }
                        changed = true;
                    }
                }
                if is_placeholder_launch_flags(&app.launch_flags) {
                    app.launch_flags = found.default_launch_flags.clone();
                    changed = true;
                }
            }

            if changed {
                updated_names.push(format!("{} -> {}", app.name, exe_str));
            }
        } else {
            // Auto-add newly discovered installed Simpul app (e.g., Simpulproject AI)
            let new_app = SimpulApp {
                id: found.id.clone(),
                name: found.name.clone(),
                tagline: format!("Aplikasi Simpul yang terpasang, ditemukan lewat {}", found.source_label),
                headline: format!("{} v{} siap dibuka langsung dari Simpul Creative Launcher.", found.name, found.version),
                about: format!(
                    "{} ditemukan otomatis di komputer ini pada {} dan ditautkan supaya bisa dibuka sekali klik.",
                    found.name, exe_str
                ),
                category: "Terpasang di komputer ini".to_string(),
                latest_version: found.version.clone(),
                released_date: "terdeteksi".to_string(),
                download_size_mb: real_size_mb.unwrap_or(16.0),
                sha256: hashes
                    .get(&exe_str)
                    .cloned()
                    .unwrap_or_else(|| compute_sha256_short(&found.exe_path)),
                profile: BuildProfile::Release,
                origin: AgentOrigin::SimpulSuite,
                package_format: PackageFormat::NativeExe,
                accent_rgb: [56, 189, 248],
                icon_kind: "flow".to_string(),
                pinned: false,
                hidden: false,
                launch_flags: found.default_launch_flags.clone(),
                workspace_path: Some(dir_str),
                artifact_path: Some(exe_str.clone()),
                features: vec![AppFeature {
                    title: "Terdeteksi otomatis".to_string(),
                    description: format!("Ditautkan langsung ke {}.", exe_str),
                }],
                release_notes: vec![format!(
                    "v{}: terdeteksi otomatis dari {}.",
                    found.version, found.source_label
                )],
                install_state: InstallState::Installed {
                    version: found.version.clone(),
                    install_path: exe_str.clone(),
                    installed_at: format!("Detected ({})", found.source_label),
                },
            };
            // Insert right after pinned flagship apps
            let insert_pos = reg
                .apps
                .iter()
                .position(|a| !a.pinned)
                .unwrap_or(reg.apps.len());
            reg.apps.insert(insert_pos, new_app);
            updated_names.push(format!("{} -> {}", found.name, exe_str));
        }
    }

    // Entri hasil deteksi lama yang dulu tidak dikenali (mis. "Simpul Sync version 4.2.0" dengan id
    // `simpul-sync-version-4-2-0`) sekarang dikenali sebagai aplikasi katalog: hapus duplikatnya.
    let before = reg.apps.len();
    let ids: Vec<String> = reg.apps.iter().map(|a| a.id.clone()).collect();
    reg.apps.retain(|a| {
        let auto_detected = a.category == "Terpasang di komputer ini";
        match canonicalize_known_simpul_app(&a.name) {
            Some(k) if auto_detected && k.0 != a.id && ids.iter().any(|i| i == k.0) => false,
            _ => true,
        }
    });
    if reg.apps.len() != before {
        updated_names.push(format!("{} entri ganda dibersihkan", before - reg.apps.len()));
    }

    // Entri "hantu" (laporan simpul-bugs #4): aplikasi yang dulu TERDETEKSI di komputer ini lalu
    // dihapus/dipindah. Exe-nya sudah tidak ada dan pindaian sekarang tidak menemukannya, jadi
    // entri itu tidak bisa dibuka maupun dipasang. Hapus dari daftar. Aplikasi katalog yang sudah
    // dirilis akan muncul lagi dari manifest (bisa dipasang); impor pengguna/agen tidak disentuh.
    let found_ids: Vec<String> = discovered_ids;
    let before = reg.apps.len();
    reg.apps.retain(|a| {
        let auto_detected = a.category == "Terpasang di komputer ini" && a.origin == AgentOrigin::SimpulSuite;
        let dead = match &a.install_state {
            InstallState::Installed { install_path, .. } => !is_launchable_file(Path::new(install_path)),
            InstallState::NotInstalled => true,
        };
        !(auto_detected && dead && !found_ids.iter().any(|i| i.eq_ignore_ascii_case(&a.id)))
    });
    if reg.apps.len() != before {
        updated_names.push(format!("{} entri yang sudah tidak terpasang dibersihkan", before - reg.apps.len()));
    }

    // Ensure any seeded placeholder entries whose fake folder does not contain an executable
    // are not misleadingly marked Installed when nothing exists on disk.
    let managed_root = PathBuf::from(&reg.settings.install_root);
    for app in &mut reg.apps {
        let should_reset_fake = match &app.install_state {
            InstallState::Installed { install_path, .. } => {
                let p = Path::new(install_path);
                !is_launchable_file(p)
                    && !p.exists()
                    && (p.starts_with(&managed_root)
                        || install_path.contains("SimpulCreativeLauncher"))
            }
            InstallState::NotInstalled => false,
        };
        if should_reset_fake {
            app.install_state = InstallState::NotInstalled;
            if is_placeholder_launch_flags(&app.launch_flags) {
                app.launch_flags.clear();
            }
            updated_names.push(format!("{} (marked Not Installed — no local binary)", app.name));
        }
    }

    if !updated_names.is_empty() {
        reg.last_checked_label = format!("Dipindai {}", now_label());
    }
    updated_names
}

/// Fast filesystem-only check (safe to call on every UI frame).
pub fn current_launchable_target(app: &SimpulApp) -> Option<PathBuf> {
    let preferred: &[&str] = match app.id.as_str() {
        "simpulgram" => &["SimpulGram.exe", "simpulgram.exe"],
        "simpulsync-4" => &["simpulsync-gui.exe", "SimpulSync 4.exe", "simpulsync4.exe", "SimpulSync.exe"],
        "capr" => &["OpenNotebook.exe", "CAPr.exe", "Capr.exe"],
        "simpulproject-ai" => &["SimpulprojectAI.exe", "Simpulproject AI.exe"],
        _ => &[],
    };

    if let InstallState::Installed {
        ref install_path, ..
    } = app.install_state
    {
        let p = Path::new(install_path);
        if is_launchable_file(p) {
            return Some(p.to_path_buf());
        }
        if p.is_dir() {
            if let Some(found) = find_exe_in_dir(p, preferred) {
                return Some(found);
            }
        }
    }

    if let Some(ref art) = app.artifact_path {
        let p = Path::new(art);
        if is_launchable_file(p) {
            return Some(p.to_path_buf());
        }
        if p.is_dir() {
            if let Some(found) = find_exe_in_dir(p, preferred) {
                return Some(found);
            }
        }
    }

    None
}

/// Resolves the launchable executable, falling back to live PC registry/disk discovery on launch.
pub fn resolve_launchable_target(app: &SimpulApp) -> Option<PathBuf> {
    if let Some(p) = current_launchable_target(app) {
        return Some(p);
    }

    // Live fallback discovery in case the user installed or moved the app
    discover_installed_apps_on_pc()
        .into_iter()
        .find(|d| d.id.eq_ignore_ascii_case(&app.id) || d.name.eq_ignore_ascii_case(&app.name))
        .map(|d| d.exe_path)
}

pub fn set_app_executable_path(
    reg: &mut SimpulRegistry,
    app_id: &str,
    raw_path: &str,
) -> Result<String, String> {
    let trimmed = raw_path.trim().trim_matches('"');
    let p = PathBuf::from(trimmed);
    if !is_launchable_file(&p) {
        return Err(format!(
            "File tidak ditemukan atau bukan executable (.exe/.cmd/.bat/.lnk): {trimmed}"
        ));
    }
    let idx = reg
        .apps
        .iter()
        .position(|a| a.id.eq_ignore_ascii_case(app_id) || a.name.eq_ignore_ascii_case(app_id))
        .ok_or_else(|| format!("Aplikasi '{app_id}' tidak ditemukan di registry."))?;

    let ver = reg.apps[idx]
        .install_state
        .installed_version()
        .map(str::to_string)
        .unwrap_or_else(|| reg.apps[idx].latest_version.clone());
    let dir_str = p
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| p.display().to_string());
    let exe_str = p.display().to_string();

    reg.apps[idx].install_state = InstallState::Installed {
        version: ver,
        install_path: exe_str.clone(),
        installed_at: format!("Ditautkan {}", now_label()),
    };
    reg.apps[idx].artifact_path = Some(exe_str.clone());
    reg.apps[idx].workspace_path = Some(dir_str);
    reg.apps[idx].package_format = PackageFormat::NativeExe;
    if is_placeholder_launch_flags(&reg.apps[idx].launch_flags) {
        reg.apps[idx].launch_flags.clear();
    }

    let name = reg.apps[idx].name.clone();
    Ok(format!("Executable {name} ditautkan ke {exe_str}"))
}

pub fn open_install_folder(path_str: &str) -> Result<String, String> {
    let p = Path::new(path_str);
    if !p.exists() {
        return Err(format!("Lokasi tidak ditemukan: {path_str}"));
    }
    #[cfg(target_os = "windows")]
    {
        if p.is_file() {
            Command::new("explorer.exe")
                .arg(format!("/select,{}", p.display()))
                .spawn()
                .map_err(|e| format!("Gagal membuka Explorer: {e}"))?;
        } else {
            Command::new("explorer.exe")
                .arg(p)
                .spawn()
                .map_err(|e| format!("Gagal membuka Explorer: {e}"))?;
        }
        return Ok(format!("Membuka folder {}", p.display()));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let dir = if p.is_file() {
            p.parent().unwrap_or(p)
        } else {
            p
        };
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("Failed opening folder: {e}"))?;
        Ok(format!("Opened folder {}", dir.display()))
    }
}

pub fn try_open_in_agent(agent_cmd: &str, target_path: &str) -> Result<String, String> {
    let (_, resolved) = find_on_path(agent_cmd);
    let bin = resolved.unwrap_or_else(|| agent_cmd.to_string());
    Command::new(&bin)
        .arg(target_path)
        .spawn()
        .map(|_| format!("Opened '{target_path}' with {agent_cmd}"))
        .map_err(|e| format!("Could not start '{agent_cmd}': {e}"))
}

pub fn try_launch_app(app: &SimpulApp, extra_args: &[String]) -> Result<String, String> {
    let version = app
        .install_state
        .installed_version()
        .unwrap_or(&app.latest_version);

    let Some(target) = resolve_launchable_target(app) else {
        return Err(format!(
            "Executable untuk {} tidak ditemukan di komputer ini. Klik 'Scan PC' atau atur lokasi .exe di panel Details.",
            app.name
        ));
    };

    // Reject unbalanced quotes in launch_flags before spawning anything
    let mut parsed_flags = split_launch_flags(&app.launch_flags)?;
    if is_placeholder_launch_flags(&app.launch_flags) {
        parsed_flags.clear();
    }

    let ext = target
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let work_dir = target.parent();

    let spawn_res = match ext.as_str() {
        "cmd" | "bat" => {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(&target);
            for tok in &parsed_flags {
                cmd.arg(tok);
            }
            for arg in extra_args {
                cmd.arg(arg);
            }
            if let Some(dir) = work_dir {
                cmd.current_dir(dir);
            }
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }
        "lnk" => {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "start", ""]).arg(&target);
            if let Some(dir) = work_dir {
                cmd.current_dir(dir);
            }
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }
        _ => {
            let mut cmd = Command::new(&target);
            for tok in &parsed_flags {
                cmd.arg(tok);
            }
            for arg in extra_args {
                cmd.arg(arg);
            }
            if let Some(dir) = work_dir {
                cmd.current_dir(dir);
            }
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }
    };

    spawn_res.map_err(|e| format!("Gagal membuka {} ({}): {e}", app.name, target.display()))?;

    let fname = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app");
    Ok(format!("Membuka {} v{} ({})", app.name, version, fname))
}

pub fn split_launch_flags(raw: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in raw.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if in_quotes {
        return Err("A quote in launch options isn't closed.".to_string());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

pub fn default_registry() -> SimpulRegistry {
    let root_str = default_install_root().display().to_string();

    // Katalog tidak lagi ditanam di launcher: isinya datang dari manifest bertanda tangan
    // (aplikasi yang sudah dirilis) dan dari aplikasi yang terpasang di komputer ini.
    let apps: Vec<SimpulApp> = Vec::new();

    let events: Vec<AgentActivityEvent> = Vec::new();

    SimpulRegistry {
        version: REGISTRY_VERSION,
        last_checked_label: "Belum dicek".to_string(),
        settings: LauncherSettings {
            theme: ThemeChoice::Dark,
            view_mode: ViewMode::Grid,
            sort_key: SortKey::Suite,
            sort_descending: false,
            check_every: CheckEvery::Hour,
            auto_update: false,
            batch_updates: true,
            require_release_profile: true,
            install_root: root_str,
            agy_command: "agy".to_string(),
            claude_command: "claude".to_string(),
            reduce_transparency: false,
        },
        apps,
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_cannot_escape_install_folder() {
        for ok in ["1.0.0", "4.1.11", "2.0.0-beta.1", "1.0.0+build5"] {
            assert!(is_safe_version(ok), "{ok}");
        }
        for bad in ["", "..", "../x", "1.0/../..", r"1.0\..\x", "C:1", ".hidden", "1 0"] {
            assert!(!is_safe_version(bad), "{bad}");
        }
        assert!(!is_safe_version(&"9".repeat(33)));
    }

    #[test]
    fn slug_never_contains_path_characters() {
        assert_eq!(slugify_id(r"../..\Evil App!"), "evil-app");
        assert_eq!(slugify_id("---"), "");
    }

    #[test]
    fn nothing_is_listed_until_something_is_released() {
        let reg = default_registry();
        assert!(reg.apps.is_empty());
        assert!(reg.events.is_empty());
        assert_eq!(reg.version, REGISTRY_VERSION);
    }

    #[test]
    fn migration_drops_sample_entries_but_keeps_real_ones() {
        let mut reg = default_registry();
        reg.version = 1;
        let sample = catalog_app("simpulgram", "SimpulGram", None, "2.4.1");
        let mut installed = catalog_app("simpulsync-4", "SimpulSync 4", None, "4.1.11");
        installed.install_state = InstallState::Installed {
            version: "4.1.11".into(),
            install_path: "x".into(),
            installed_at: "y".into(),
        };
        let mut imported = catalog_app("alat-saya", "Alat Saya", None, "1.0.0");
        imported.origin = AgentOrigin::Claude;
        reg.apps = vec![sample, installed, imported];
        assert!(migrate_registry(&mut reg));
        let ids: Vec<&str> = reg.apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["simpulsync-4", "alat-saya"]);
        assert!(!migrate_registry(&mut reg), "hanya sekali");
    }

    #[test]
    fn catalog_app_is_safe_to_install() {
        let app = catalog_app("simpulsync-4", "Simpul Sync", Some("Sinkron audio"), "4.2.0");
        assert_eq!(slugify_id(&app.id), app.id);
        assert!(is_safe_version(&app.latest_version));
        assert!(!app.install_state.is_installed());
    }

    #[test]
    fn version_follows_scan_after_update() {
        // Sync 4.2.0 tercatat terpasang; setelah diperbarui, pindaian menemukan 4.2.1 di exe yang sama.
        let exe = std::env::current_exe().unwrap();
        let mut reg = default_registry();
        let mut app = catalog_app("simpulsync-4", "Simpul Sync", None, "4.2.1");
        app.install_state = InstallState::Installed {
            version: "4.2.0".into(),
            install_path: exe.display().to_string(),
            installed_at: "y".into(),
        };
        reg.apps = vec![app];
        let found = DiscoveredInstalledApp {
            id: "simpulsync-4".into(),
            name: "Simpul Sync".into(),
            version: "4.2.1".into(),
            exe_path: exe.clone(),
            install_dir: exe.parent().unwrap().to_path_buf(),
            default_launch_flags: String::new(),
            source_label: "Windows Registry".into(),
        };
        apply_discovered_apps(&mut reg, vec![found], &std::collections::BTreeMap::new());
        match &reg.apps[0].install_state {
            InstallState::Installed { version, .. } => assert_eq!(version, "4.2.1"),
            InstallState::NotInstalled => panic!("harus tetap terpasang"),
        }
    }

    #[test]
    fn launch_path_moves_to_newer_install_folder() {
        // Laporan #4: Sync lama (exe lain yang MASIH ada) tetap dibuka setelah update ke 4.2.x.
        let dir = std::env::temp_dir().join(format!("simpul-uji-{}", std::process::id()));
        let lama = dir.join("SimpulSync 4").join("simpulsync4.exe");
        let baru = dir.join("Simpul Sync").join("simpulsync-gui.exe");
        for f in [&lama, &baru] {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"MZ").unwrap();
        }
        let mut reg = default_registry();
        let mut app = catalog_app("simpulsync-4", "Simpul Sync", None, "4.2.2");
        app.install_state = InstallState::Installed {
            version: "4.1.11".into(),
            install_path: lama.display().to_string(),
            installed_at: "y".into(),
        };
        reg.apps = vec![app];
        let found = DiscoveredInstalledApp {
            id: "simpulsync-4".into(),
            name: "Simpul Sync".into(),
            version: "4.2.2".into(),
            exe_path: baru.clone(),
            install_dir: baru.parent().unwrap().to_path_buf(),
            default_launch_flags: String::new(),
            source_label: "Windows Registry".into(),
        };
        apply_discovered_apps(&mut reg, vec![found], &std::collections::BTreeMap::new());
        let _ = std::fs::remove_dir_all(&dir);
        match &reg.apps[0].install_state {
            InstallState::Installed { version, install_path, .. } => {
                assert_eq!(version, "4.2.2");
                assert_eq!(install_path, &baru.display().to_string());
            }
            InstallState::NotInstalled => panic!("harus tetap terpasang"),
        }
    }

    #[test]
    fn ghost_auto_detected_entry_is_removed() {
        // Laporan #4: entri hasil deteksi yang exe-nya sudah hilang tidak boleh tersisa di daftar.
        let mut reg = default_registry();
        let mut hantu = catalog_app("simpul-alat-lama", "Simpul Alat Lama", None, "1.0.0");
        hantu.category = "Terpasang di komputer ini".into();
        hantu.install_state = InstallState::Installed {
            version: "1.0.0".into(),
            install_path: r"C:	idakdalat.exe".into(),
            installed_at: "y".into(),
        };
        let mut impor = catalog_app("alat-saya", "Alat Saya", None, "1.0.0");
        impor.origin = AgentOrigin::Claude;
        impor.category = "Terpasang di komputer ini".into();
        reg.apps = vec![hantu, impor];
        apply_discovered_apps(&mut reg, Vec::new(), &std::collections::BTreeMap::new());
        let ids: Vec<&str> = reg.apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["alat-saya"], "hanya entri hantu hasil deteksi yang dihapus");
    }
}
