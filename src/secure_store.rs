//! Penyimpanan rahasia lokal (token sesi, kunci pemasangan) yang dienkripsi Windows DPAPI.
//!
//! DPAPI cakupan pengguna: hanya akun Windows yang sama di komputer yang sama yang bisa membuka
//! isinya. Berkas yang disalin ke komputer lain, atau dibaca akun Windows lain, tidak berguna.
//! Setiap jenis rahasia memakai "entropi" sendiri, jadi isi `account.json` tidak bisa ditempel ke
//! `install.key` atau sebaliknya.
//!
//! Format berkas: `dpapi1:<base64>`. Berkas lama yang masih polos tetap terbaca, lalu pemanggil
//! menulis ulang dalam bentuk terenkripsi (migrasi otomatis, isi rahasia tidak berubah).

use base64::Engine;

const PREFIX: &str = "dpapi1:";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn entropy(purpose: &str) -> Vec<u8> {
    format!("simpul-launcher|secure-store-v1|{purpose}").into_bytes()
}

#[cfg(windows)]
fn dpapi(data: &[u8], purpose: &str, protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    let mut ent = entropy(purpose);
    let mut input = data.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_mut_ptr() };
    let ent_blob = CRYPT_INTEGER_BLOB { cbData: ent.len() as u32, pbData: ent.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: semua penunjuk menunjuk ke buffer milik fungsi ini yang hidup selama panggilan;
    // keluaran dialokasikan DPAPI dan dibebaskan dengan LocalFree setelah disalin.
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &in_blob,
                std::ptr::null(),
                &ent_blob,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &in_blob,
                std::ptr::null_mut(),
                &ent_blob,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        }
    };
    if ok == 0 || out.pbData.is_null() {
        return Err(if protect { "rahasia tidak bisa dienkripsi" } else { "rahasia tidak bisa dibuka di akun ini" }.into());
    }
    // SAFETY: DPAPI mengembalikan buffer sepanjang cbData yang valid sampai LocalFree.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe {
        LocalFree(out.pbData.cast());
    }
    input.iter_mut().for_each(|b| *b = 0);
    Ok(bytes)
}

/// Selain Windows: launcher tidak didukung; rahasia tidak disimpan tanpa enkripsi.
#[cfg(not(windows))]
fn dpapi(_data: &[u8], _purpose: &str, _protect: bool) -> Result<Vec<u8>, String> {
    Err("penyimpanan rahasia terenkripsi hanya tersedia di Windows".into())
}

/// Tulis rahasia terenkripsi ke berkas.
pub fn write(path: &std::path::Path, purpose: &str, secret: &[u8]) -> Result<(), String> {
    let sealed = dpapi(secret, purpose, true)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, format!("{PREFIX}{}", b64().encode(sealed))).map_err(|e| e.to_string())
}

/// Hasil baca: isi rahasia dan apakah berkasnya masih format lama (polos) yang perlu ditulis ulang.
pub struct Read {
    pub secret: Vec<u8>,
    pub legacy_plain: bool,
}

/// Baca rahasia. `None` bila berkas tidak ada. Galat bila berkas terenkripsi tapi tidak bisa dibuka
/// (mis. disalin dari akun/komputer lain): pemanggil memperlakukannya sebagai tidak ada.
pub fn read(path: &std::path::Path, purpose: &str) -> Option<Result<Read, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    Some(match text.strip_prefix(PREFIX) {
        Some(body) => b64()
            .decode(body)
            .map_err(|_| "berkas rahasia rusak".to_string())
            .and_then(|sealed| dpapi(&sealed, purpose, false))
            .map(|secret| Read { secret, legacy_plain: false }),
        None => Ok(Read { secret: text.as_bytes().to_vec(), legacy_plain: true }),
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rahasia_terenkripsi_dan_terikat_ke_tujuannya() {
        let dir = std::env::temp_dir().join(format!("simpul-secure-store-{}", std::process::id()));
        let path = dir.join("uji.key");
        write(&path, "uji", b"rahasia-uji-123").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with(PREFIX));
        assert!(!raw.contains("rahasia-uji-123"), "isi tidak boleh polos");
        let back = read(&path, "uji").unwrap().unwrap();
        assert_eq!(back.secret, b"rahasia-uji-123");
        assert!(!back.legacy_plain);
        // Entropi berbeda (tujuan lain) tidak bisa membuka.
        assert!(read(&path, "lain").unwrap().is_err());
        // Berkas lama polos tetap terbaca dan ditandai untuk migrasi.
        std::fs::write(&path, "polos").unwrap();
        let old = read(&path, "uji").unwrap().unwrap();
        assert!(old.legacy_plain);
        assert_eq!(old.secret, b"polos");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
