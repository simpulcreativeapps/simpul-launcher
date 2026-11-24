//! Akun Simpul (masuk dengan Google) dan sertifikat pemasangan.
//!
//! Akun bersifat opsional. Tanpa akun semua aplikasi gratis tetap bisa dipakai; bedanya
//! hanya tanda "tanpa sertifikat". Protokolnya ada di repo `simpul-akun`:
//! setiap permintaan ditandatangani kunci pemasangan, setiap jawaban harus bertanda
//! tangan server, dan sertifikat diperiksa secara lokal tanpa jaringan.

use std::path::PathBuf;

use base64::Engine;
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};

use crate::suite::{hex, install_signing_key, sha256_bytes, shared_data_dir, LAUNCHER_VERSION};

const ACCOUNT_BASE: &str = "https://simpul-akun.simpul-lapor.workers.dev";
/// Kunci publik server akun. Dipakai untuk memeriksa jawaban dan sertifikat.
const ACCOUNT_SERVER_KEY_B64: &str = "sIp4wesBmmLXzMlaOoMeLnZt4WHneCqlV3VeXPDdpdM=";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    pub name: String,
    token: String,
}

fn account_file() -> PathBuf {
    shared_data_dir().join("account.json")
}

fn certificate_file(app: &str) -> PathBuf {
    shared_data_dir().join("certificates").join(format!("{app}.json"))
}

/// Akun yang tersimpan di komputer ini (bila pengguna sudah masuk). Token sesi disimpan
/// terenkripsi DPAPI (lihat secure_store.rs); berkas lama yang masih polos dienkripsi ulang.
pub fn load_account() -> Option<Account> {
    let read = crate::secure_store::read(&account_file(), "account").and_then(Result::ok)?;
    let account: Account = serde_json::from_slice(&read.secret).ok()?;
    if read.legacy_plain {
        let _ = save_account(&account);
    }
    Some(account)
}

fn save_account(account: &Account) -> Result<(), String> {
    let text = serde_json::to_string(account).map_err(|e| e.to_string())?;
    crate::secure_store::write(&account_file(), "account", text.as_bytes())
}

fn forget_account() {
    let _ = std::fs::remove_file(account_file());
    let _ = std::fs::remove_dir_all(shared_data_dir().join("certificates"));
}

// ---------------------------------------------------------------- permintaan bertanda tangan

fn server_key() -> Result<ed25519_dalek::VerifyingKey, String> {
    let bytes: [u8; 32] = b64()
        .decode(ACCOUNT_SERVER_KEY_B64)
        .ok()
        .and_then(|k| k.try_into().ok())
        .ok_or("kunci server tidak terbaca")?;
    ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "kunci server tidak sah".to_string())
}

fn verify_server(message: &str, signature_b64: &str) -> Result<(), String> {
    let bad = || "jawaban layanan akun tidak bertanda tangan sah".to_string();
    let sig: [u8; 64] = b64()
        .decode(signature_b64.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or_else(bad)?;
    server_key()?
        .verify_strict(message.as_bytes(), &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| bad())
}

fn call(
    path: &str,
    body: &serde_json::Value,
    token: Option<&str>,
) -> Result<(u16, serde_json::Value), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("simpul-launcher/{LAUNCHER_VERSION}"))
        .build()
        .map_err(|e| e.to_string())?;
    let body = body.to_string();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .to_string();
    let key = install_signing_key()?;
    let message = format!(
        "simpul-akun-v1\n{path}\n{time}\n{}",
        hex(&sha256_bytes(body.as_bytes()))
    );
    let mut request = client
        .post(format!("{ACCOUNT_BASE}{path}"))
        .header("content-type", "application/json")
        .header("x-simpul-key", b64().encode(key.verifying_key().to_bytes()))
        .header("x-simpul-time", &time)
        .header("x-simpul-sig", b64().encode(key.sign(message.as_bytes()).to_bytes()))
        .body(body);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = request
        .send()
        .map_err(|_| "tidak bisa terhubung ke layanan akun".to_string())?;
    let status = response.status().as_u16();
    let signature = response
        .headers()
        .get("x-simpul-server-sig")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let text = response.text().unwrap_or_default();
    // Galat sebelum tanda tangan permintaan diperiksa memakai konteks "-".
    let with_context = format!("simpul-akun-response-v1\n{path}|{time}\n{status}\n{text}");
    let without = format!("simpul-akun-response-v1\n-\n{status}\n{text}");
    verify_server(&with_context, &signature).or_else(|_| verify_server(&without, &signature))?;
    Ok((status, serde_json::from_str(&text).unwrap_or_default()))
}

fn error_of(status: u16, value: &serde_json::Value) -> String {
    value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("layanan akun menjawab {status}"), str::to_string)
}

// ---------------------------------------------------------------- masuk dan keluar

/// Mulai masuk. Mengembalikan (state, tautan yang dibuka di browser pengguna).
pub fn login_start() -> Result<(String, String), String> {
    let (status, value) = call("/v1/login/start", &serde_json::json!({}), None)?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    let state = value.get("state").and_then(serde_json::Value::as_str).unwrap_or_default();
    let url = value.get("url").and_then(serde_json::Value::as_str).unwrap_or_default();
    // Tautan hanya dibuka bila benar-benar menuju layanan akun Simpul.
    if state.is_empty() || !url.starts_with(&format!("{ACCOUNT_BASE}/auth/google?state=")) {
        return Err("layanan akun memberi tautan yang tidak dikenal".to_string());
    }
    Ok((state.to_string(), url.to_string()))
}

/// `Ok(None)` berarti pengguna belum selesai di browser.
pub fn login_poll(state: &str) -> Result<Option<Account>, String> {
    let (status, value) = call("/v1/login/poll", &serde_json::json!({ "state": state }), None)?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    if value.get("pending").and_then(serde_json::Value::as_bool) == Some(true) {
        return Ok(None);
    }
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(serde_json::Value::as_str).unwrap_or_default().to_string()
    };
    let user = value.get("user").cloned().unwrap_or_default();
    let account = Account {
        email: text(&user, "email"),
        name: text(&user, "name"),
        token: text(&value, "token"),
    };
    if account.token.is_empty() {
        return Err("layanan akun tidak memberi sesi".to_string());
    }
    save_account(&account)?;
    Ok(Some(account))
}

pub fn logout(account: &Account) {
    let _ = call("/v1/logout", &serde_json::json!({}), Some(&account.token));
    forget_account();
}

/// Hapus akun di server beserta sesi dan sertifikatnya, lalu hapus salinan lokal.
pub fn delete_account(account: &Account) -> Result<(), String> {
    let (status, value) = call("/v1/account/delete", &serde_json::json!({}), Some(&account.token))?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    forget_account();
    Ok(())
}

// ---------------------------------------------------------------- profil dan persetujuan

/// Profil opsional. Disimpan terenkripsi di server; di komputer ini hanya ada di memori.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub username: String,
    pub gender: String,
    pub whatsapp: String,
    pub consent_email: bool,
    pub consent_whatsapp: bool,
    /// Versi teks persetujuan yang berlaku di server; dikirim balik saat menyimpan.
    pub consent_version: String,
}

/// (nilai, label). Harus sama dengan GENDERS di simpul-akun/src/profil.js.
pub const GENDERS: [(&str, &str); 4] = [
    ("", "Tidak diisi"),
    ("laki-laki", "Laki-laki"),
    ("perempuan", "Perempuan"),
    ("tidak_disebut", "Tidak ingin menyebut"),
];
/// Harus sama dengan CONSENT_TEXT di simpul-akun/src/profil.js.
pub const CONSENT_EMAIL_TEXT: &str = "Boleh kirimi saya penawaran dan ajakan kemitraan dari Simpul lewat email.";
pub const CONSENT_WHATSAPP_TEXT: &str =
    "Boleh kirimi saya penawaran dan ajakan kemitraan dari Simpul lewat WhatsApp.";

fn profile_from(value: &serde_json::Value) -> Profile {
    let user = value.get("user").cloned().unwrap_or_default();
    let text = |k: &str| user.get(k).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let flag = |k: &str| user.get(k).and_then(serde_json::Value::as_bool).unwrap_or(false);
    Profile {
        username: text("username"),
        gender: text("gender"),
        whatsapp: text("whatsapp"),
        consent_email: flag("consent_email"),
        consent_whatsapp: flag("consent_whatsapp"),
        consent_version: text("consent_version"),
    }
}

pub fn fetch_profile(account: &Account) -> Result<Profile, String> {
    let (status, value) = call("/v1/me", &serde_json::json!({}), Some(&account.token))?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    Ok(profile_from(&value))
}

/// Simpan profil. Server memeriksa dan merapikan isian (mis. nomor jadi +62...) lalu
/// mengembalikan profil tersimpan.
pub fn save_profile(account: &Account, profile: &Profile) -> Result<Profile, String> {
    let body = serde_json::json!({
        "username": profile.username.trim(),
        "gender": profile.gender,
        "whatsapp": profile.whatsapp.trim(),
        "consent_email": profile.consent_email,
        "consent_whatsapp": profile.consent_whatsapp,
        "consent_version": profile.consent_version,
    });
    let (status, value) = call("/v1/profile", &body, Some(&account.token))?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    Ok(profile_from(&value))
}

// ---------------------------------------------------------------- sertifikat

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCertificate {
    body: String,
    signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateStatus {
    /// Sertifikat sah untuk aplikasi ini dan untuk pemasangan ini.
    Certified,
    /// Tidak ada sertifikat (mis. tidak dipasang lewat launcher, atau belum masuk).
    None,
    /// Ada berkas sertifikat tetapi tidak sah di sini: salinan dari komputer lain,
    /// diubah, atau untuk aplikasi lain.
    Invalid,
}

/// Minta sertifikat untuk satu aplikasi dan simpan di komputer ini.
pub fn request_certificate(account: &Account, app: &str, version: &str) -> Result<(), String> {
    let (status, value) = call(
        "/v1/certificate",
        &serde_json::json!({ "app": app, "version": version }),
        Some(&account.token),
    )?;
    if status != 200 {
        return Err(error_of(status, &value));
    }
    let cert: StoredCertificate = serde_json::from_value(value.get("certificate").cloned().unwrap_or_default())
        .map_err(|_| "sertifikat tidak terbaca".to_string())?;
    if check_certificate(&cert, app) != CertificateStatus::Certified {
        return Err("sertifikat dari layanan akun tidak sah untuk pemasangan ini".to_string());
    }
    let path = certificate_file(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string(&cert).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn check_certificate(cert: &StoredCertificate, app: &str) -> CertificateStatus {
    if verify_server(&format!("simpul-certificate-v1\n{}", cert.body), &cert.signature).is_err() {
        return CertificateStatus::Invalid;
    }
    let Ok(body) = serde_json::from_str::<serde_json::Value>(&cert.body) else {
        return CertificateStatus::Invalid;
    };
    let field = |k: &str| body.get(k).and_then(serde_json::Value::as_str).unwrap_or_default();
    let Ok(key) = install_signing_key() else {
        return CertificateStatus::Invalid;
    };
    let local = b64().encode(key.verifying_key().to_bytes());
    // Sertifikat hanya berlaku untuk aplikasi yang disebutnya dan pemasangan yang memintanya.
    if field("app") == app && field("install_key") == local {
        CertificateStatus::Certified
    } else {
        CertificateStatus::Invalid
    }
}

/// Periksa sertifikat tersimpan untuk satu aplikasi. Tanpa jaringan.
pub fn certificate_status(app: &str) -> CertificateStatus {
    let Ok(raw) = std::fs::read_to_string(certificate_file(app)) else {
        return CertificateStatus::None;
    };
    match serde_json::from_str::<StoredCertificate>(&raw) {
        Ok(cert) => check_certificate(&cert, app),
        Err(_) => CertificateStatus::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forged_or_foreign_certificate_is_not_accepted() {
        // Tanda tangan karangan.
        let fake = StoredCertificate {
            body: r#"{"v":1,"app":"sync","install_key":"x"}"#.to_string(),
            signature: "A".repeat(86) + "==",
        };
        assert_eq!(check_certificate(&fake, "sync"), CertificateStatus::Invalid);
        // Tanpa berkas: tidak bersertifikat, bukan galat.
        assert_eq!(certificate_status("aplikasi-yang-tidak-ada"), CertificateStatus::None);
    }

    #[test]
    fn profile_reply_is_read_and_missing_fields_are_empty() {
        let v = serde_json::json!({ "ok": true, "user": {
            "email": "a@contoh.test", "username": "rani.art", "gender": "perempuan",
            "whatsapp": "+6281234567890", "consent_email": true, "consent_version": "2026-11-24" } });
        let p = profile_from(&v);
        assert_eq!(p.username, "rani.art");
        assert!(p.consent_email && !p.consent_whatsapp);
        assert_eq!(profile_from(&serde_json::json!({})), Profile::default());
        assert!(GENDERS.iter().any(|(v, _)| *v == p.gender));
    }

    #[test]
    fn unsigned_server_reply_is_rejected() {
        assert!(verify_server("simpul-akun-response-v1\n-\n200\n{}", "").is_err());
        assert!(verify_server("x", &("A".repeat(86) + "==")).is_err());
    }
}
