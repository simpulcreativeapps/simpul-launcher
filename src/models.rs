use serde::{Deserialize, Serialize};

// Verified window & layout constants (see MECHANICS.md & mechanics.json)
pub const DEFAULT_WINDOW_WIDTH: f32 = 1280.0;
pub const DEFAULT_WINDOW_HEIGHT: f32 = 760.0;
pub const MIN_WINDOW_WIDTH: f32 = 1000.0;
pub const MIN_WINDOW_HEIGHT: f32 = 620.0;
pub const SCREENSHOT_WARMUP_FRAMES: u32 = 120;

pub const PRIMARY_BTN_HEIGHT: f32 = 34.0;
pub const BADGE_PILL_HEIGHT: f32 = 24.0;
pub const SECONDARY_BTN_HEIGHT: f32 = 28.0;
pub const CARD_ICON_SIZE: f32 = 92.0;
pub const HERO_ICON_SIZE: f32 = 64.0;
pub const CARD_MIN_WIDTH: f32 = 470.0;
pub const CARD_INNER_PADDING: i8 = 24;
pub const CARD_GRID_SPACING: f32 = 16.0;
pub const CARD_CORNER_RADIUS: f32 = 12.0;
pub const CARD_HOVER_ANIM_SECS: f32 = 0.3;
pub const CARD_HOVER_TINT_FACTOR: f32 = 0.05;
pub const CONTROL_CORNER_RADIUS: f32 = 8.0;
pub const DRAWER_MIN_WIDTH: f32 = 320.0;
pub const DRAWER_MAX_WIDTH: f32 = 480.0;
pub const DRAWER_ANIM_SECS: f32 = 0.22;
pub const TOAST_WIDTH: f32 = 380.0;
pub const TOAST_INFO_DURATION_SECS: f64 = 4.5;
pub const TOAST_ERROR_DURATION_SECS: f64 = 8.0;
pub const TOAST_OFFSET_X: f32 = -20.0;
pub const TOAST_OFFSET_Y: f32 = -46.0;
pub const LIVE_SYNC_POLL_SECS: f64 = 1.5;

pub const FONT_HEADING: f32 = 22.0;
pub const FONT_CARD_TITLE: f32 = 17.0;
pub const FONT_BODY: f32 = 14.0;
pub const FONT_CAPTION: f32 = 12.0;
pub const FONT_MICRO: f32 = 11.0;

// Update feed intervals & two-tier backoff, adapted from ArtCraft Launcher (MIT OR Apache-2.0; see NOTICE)
pub const FEED_RETRY_MIN_BACKOFF_SECS: u64 = 120;
pub const FEED_RETRY_MAX_BACKOFF_SECS: u64 = 3600;
pub const FEED_RETRY_MAX_SHIFT: u32 = 16;
pub const FEED_RETRY_AFTER_CAP_SECS: u64 = 86400;
pub const FEED_JITTER_SPAN: f64 = 0.4;
pub const FEED_JITTER_OFFSET: f64 = -0.2;
pub const FEED_RATE_LIMIT_COOLDOWN_SECS: u64 = 900;
pub const FEED_THROTTLE_MAX_BACKOFF_SECS: u64 = 21600;
pub const FEED_THROTTLE_JITTER_MAX: f64 = 0.5;
pub const CHECK_INTERVAL_HOUR_SECS: u64 = 3600;
pub const CHECK_INTERVAL_FOUR_HOURS_SECS: u64 = 14400;
pub const CHECK_INTERVAL_DAY_SECS: u64 = 86400;

// UI zoom shortcuts (Ctrl+= / Ctrl+- / Ctrl+0)
pub const UI_ZOOM_STEP: f32 = 0.1;
pub const UI_ZOOM_MIN: f32 = 0.5;
pub const UI_ZOOM_MAX: f32 = 3.0;

/// Implements the transient-failure exponential backoff with ±20% jitter.
pub fn compute_feed_backoff_secs(failures: u32, jitter_unit_0_1: f64, retry_after: Option<u64>) -> u64 {
    let shift = failures.saturating_sub(1).min(FEED_RETRY_MAX_SHIFT);
    let base = (FEED_RETRY_MIN_BACKOFF_SECS << shift).min(FEED_RETRY_MAX_BACKOFF_SECS) as f64;
    let jitter = 1.0 + (jitter_unit_0_1.clamp(0.0, 1.0) * FEED_JITTER_SPAN + FEED_JITTER_OFFSET);
    let computed = (base * jitter).round().max(0.0) as u64;
    let header = retry_after
        .filter(|&v| v <= FEED_RETRY_AFTER_CAP_SECS)
        .unwrap_or(0);
    computed.max(header)
}

/// Implements the rate-limit throttle backoff (15m..6h + 0..50% jitter).
#[allow(dead_code)]
pub fn compute_feed_throttle_backoff_secs(
    throttles: u32,
    jitter_unit_0_1: f64,
    retry_after: Option<u64>,
) -> u64 {
    let shift = throttles.saturating_sub(1).min(FEED_RETRY_MAX_SHIFT);
    let base = (FEED_RATE_LIMIT_COOLDOWN_SECS << shift).min(FEED_THROTTLE_MAX_BACKOFF_SECS);
    let effective = match retry_after {
        Some(v) if v <= FEED_RETRY_AFTER_CAP_SECS => base.max(v),
        _ => base,
    };
    let mult = 1.0 + jitter_unit_0_1.clamp(0.0, 1.0) * FEED_THROTTLE_JITTER_MAX;
    ((effective as f64) * mult).round().max(0.0) as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildProfile {
    Release,
    Staging,
    Debug,
}

impl BuildProfile {
    pub fn is_deploy_ready(self) -> bool {
        matches!(self, Self::Release)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Release => "RILIS",
            Self::Staging => "STAGING",
            Self::Debug => "DEBUG",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Release => "Build rilis",
            Self::Staging => "Staging",
            Self::Debug => "Build debug",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentOrigin {
    SimpulSuite,
    Claude,
    AntigravityAgy,
    CliManual,
}

impl AgentOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::SimpulSuite => "Simpul",
            Self::Claude => "Claude AI",
            Self::AntigravityAgy => "Antigravity (agy)",
            Self::CliManual => "Manual",
        }
    }

    pub fn from_arg(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "anthropic" => Self::Claude,
            "agy" | "antigravity" | "antigravity-cli" => Self::AntigravityAgy,
            "suite" | "simpul" => Self::SimpulSuite,
            _ => Self::CliManual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    Msi,
    PortableZip,
    NativeExe,
    ManifestBundle,
}

impl PackageFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Msi => "Installer (.msi)",
            Self::PortableZip => "Portable (.zip)",
            Self::NativeExe => "Program (.exe)",
            Self::ManifestBundle => "Paket aplikasi",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InstallState {
    NotInstalled,
    Installed {
        version: String,
        install_path: String,
        installed_at: String,
    },
}

impl InstallState {
    pub fn is_installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }

    pub fn installed_version(&self) -> Option<&str> {
        match self {
            Self::Installed { version, .. } => Some(version.as_str()),
            Self::NotInstalled => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppFeature {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpulApp {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub headline: String,
    pub about: String,
    pub category: String,
    pub latest_version: String,
    pub released_date: String,
    pub download_size_mb: f32,
    pub sha256: String,
    pub profile: BuildProfile,
    pub origin: AgentOrigin,
    pub package_format: PackageFormat,
    pub accent_rgb: [u8; 3],
    pub icon_kind: String,
    pub pinned: bool,
    pub hidden: bool,
    pub launch_flags: String,
    pub workspace_path: Option<String>,
    pub artifact_path: Option<String>,
    pub features: Vec<AppFeature>,
    pub release_notes: Vec<String>,
    pub install_state: InstallState,
}

impl SimpulApp {
    pub fn has_update(&self) -> bool {
        match &self.install_state {
            InstallState::Installed { version, .. } => version != &self.latest_version,
            InstallState::NotInstalled => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    /// Tema bawaan (gelap).
    Dark,
    /// Tema bawaan (terang).
    Light,
    System,
    /// Material 3 Expressive, biru Simpul (terang).
    ExpressiveBlue,
    /// Material 3 Expressive, hangat dan warna-warni (terang).
    ExpressiveWarm,
    /// Liquid Glass di atas cahaya aurora (gelap).
    GlassAurora,
    /// Liquid Glass di atas grafit netral (gelap).
    GlassGraphite,
}

impl ThemeChoice {
    /// Urutan di menu tema.
    pub const ALL: [ThemeChoice; 6] = [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::ExpressiveBlue,
        ThemeChoice::ExpressiveWarm,
        ThemeChoice::GlassAurora,
        ThemeChoice::GlassGraphite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Default gelap",
            Self::Light => "Default terang",
            Self::System => "Ikuti sistem",
            Self::ExpressiveBlue => "Expressive Biru",
            Self::ExpressiveWarm => "Expressive Ceria",
            Self::GlassAurora => "Liquid Glass Aurora",
            Self::GlassGraphite => "Liquid Glass Grafit",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "dark" => Self::Dark,
            "light" => Self::Light,
            "a" | "expressive-blue" => Self::ExpressiveBlue,
            "b" | "expressive-warm" => Self::ExpressiveWarm,
            "c" | "glass-aurora" => Self::GlassAurora,
            "d" | "glass-graphite" => Self::GlassGraphite,
            _ => return None,
        })
    }

    pub fn is_glass(self) -> bool {
        matches!(self, Self::GlassAurora | Self::GlassGraphite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    Grid,
    Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterTab {
    All,
    DeployReady,
    Installed,
    Updates,
    AgentImported,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortKey {
    Suite,
    Name,
    Status,
    Newest,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Suite => "Urutan bawaan",
            Self::Name => "Nama (A–Z)",
            Self::Status => "Status pemasangan",
            Self::Newest => "Rilis terbaru",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckEvery {
    Hour,
    FourHours,
    Day,
    Never,
}

impl CheckEvery {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "Setiap jam",
            Self::FourHours => "Setiap 4 jam",
            Self::Day => "Sekali sehari",
            Self::Never => "Hanya saat diminta",
        }
    }

    pub fn interval_secs(self) -> Option<u64> {
        match self {
            Self::Hour => Some(CHECK_INTERVAL_HOUR_SECS),
            Self::FourHours => Some(CHECK_INTERVAL_FOUR_HOURS_SECS),
            Self::Day => Some(CHECK_INTERVAL_DAY_SECS),
            Self::Never => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    pub theme: ThemeChoice,
    pub view_mode: ViewMode,
    pub sort_key: SortKey,
    pub sort_descending: bool,
    pub check_every: CheckEvery,
    pub auto_update: bool,
    pub batch_updates: bool,
    pub require_release_profile: bool,
    pub install_root: String,
    pub agy_command: String,
    pub claude_command: String,
    /// Tema Liquid Glass tanpa tembus pandang: panel padat dan tanpa latar cahaya.
    #[serde(default)]
    pub reduce_transparency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivityEvent {
    pub timestamp: String,
    pub actor: AgentOrigin,
    pub action: String,
    pub app_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpulRegistry {
    pub version: u32,
    pub last_checked_label: String,
    pub settings: LauncherSettings,
    pub apps: Vec<SimpulApp>,
    pub events: Vec<AgentActivityEvent>,
}

// Runtime-only queue progress for animated install/update inside the GUI
#[derive(Debug, Clone)]
pub enum QueueStage {
    Queued,
    Downloading { progress: f32 },
    VerifyingChecksum { progress: f32 },
    Installing { progress: f32 },
}

#[derive(Debug, Clone)]
pub struct ActiveInstallTask {
    pub app_id: String,
    pub target_version: String,
    pub total_mb: f32,
    pub stage: QueueStage,
    pub paused: bool,
    pub actor: AgentOrigin,
}

#[derive(Debug, Clone)]
pub struct ToolIntegrationStatus {
    pub agy_detected: bool,
    pub agy_path: Option<String>,
    pub claude_detected: bool,
    pub claude_path: Option<String>,
    pub cli_self_path: String,
    pub registry_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportManifest {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub tagline: Option<String>,
    pub headline: Option<String>,
    pub about: Option<String>,
    pub category: Option<String>,
    pub profile: Option<String>,
    pub binary: Option<String>,
    pub workspace: Option<String>,
    pub launch_flags: Option<String>,
}
