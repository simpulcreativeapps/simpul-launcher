//! Mewarnai bilah judul jendela Windows mengikuti tema.
//!
//! Hanya Windows 11 yang mendukung warna bebas; di Windows 10 bilah judul hanya berganti
//! gelap/terang, dan di sistem lain fungsi ini tidak melakukan apa-apa.

#[cfg(windows)]
mod imp {
    use core::ffi::c_void;
    use std::sync::Mutex;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowExW(
            parent: *mut c_void,
            after: *mut c_void,
            class: *const u16,
            title: *const u16,
        ) -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attribute: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;

    static LAST: Mutex<Option<([u8; 3], [u8; 3], bool)>> = Mutex::new(None);

    /// Jendela utama milik proses ini (kelas jendela yang dipakai eframe/winit).
    fn own_window() -> *mut c_void {
        let class: Vec<u16> = "Window Class".encode_utf16().chain(std::iter::once(0)).collect();
        let mut hwnd = core::ptr::null_mut();
        // SAFETY: semua penunjuk sah selama panggilan; hasil hanya dibandingkan dan diteruskan.
        unsafe {
            let me = GetCurrentProcessId();
            loop {
                hwnd = FindWindowExW(core::ptr::null_mut(), hwnd, class.as_ptr(), core::ptr::null());
                if hwnd.is_null() {
                    return hwnd;
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == me {
                    return hwnd;
                }
            }
        }
    }

    pub fn tint(caption: [u8; 3], text: [u8; 3], dark: bool) {
        let wanted = (caption, text, dark);
        let Ok(mut last) = LAST.lock() else { return };
        if *last == Some(wanted) {
            return;
        }
        let hwnd = own_window();
        if hwnd.is_null() {
            return; // jendela belum ada; dicoba lagi di frame berikutnya
        }
        let colorref = |c: [u8; 3]| u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16);
        let set = |attribute: u32, value: u32| {
            // SAFETY: `value` hidup selama panggilan dan ukurannya 4 byte seperti yang dilaporkan.
            unsafe {
                DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast(), 4);
            }
        };
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(dark));
        set(DWMWA_CAPTION_COLOR, colorref(caption));
        set(DWMWA_TEXT_COLOR, colorref(text));
        *last = Some(wanted);
    }
}

#[cfg(windows)]
pub use imp::tint;

#[cfg(not(windows))]
pub fn tint(_caption: [u8; 3], _text: [u8; 3], _dark: bool) {}
