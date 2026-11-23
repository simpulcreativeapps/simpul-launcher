# Simpul Creative Launcher

Desktop launcher untuk ekosistem software produksi **Simpul** (**SimpulGram**, **SimpulSync 4**, **Capr**, **SimpulDocs**, **SimpulFlow**, **SimpulStudio**, **SimpulCanvas**), dibangun menggunakan **Rust + `eframe`/`egui`** dengan banner splash/loading screen serta logo resmi **Simpul — Creative Apps Launcher**, dan terintegrasi langsung dengan **CLI**, **Antigravity (`agy`)**, serta **Claude (`claude`)**.

## Fitur Utama

1. **Katalog Layak Deploy (Strict Release Gate — Bukan Debug)**
   - Secara otomatis memverifikasi profil artefak (`Release` vs `Debug`) dan digest **SHA-256** sebelum mengimpor atau menginstal aplikasi.
   - Menolak artefak `target/debug` secara default agar hanya software siap produksi (*layak deploy*) seperti **SimpulGram** dan **SimpulSync 4** yang masuk ke lingkungan kerja.
2. **Integrasi Penuh CLI, Antigravity (`agy`), dan Claude (`claude`)**
   - **Claude** dan **Antigravity (`agy`)** dapat mengimpor manifest (`simpul-app.json`) maupun installer/binary (`.msi`, `.exe`, `.zip`) dan langsung menginstalnya ke folder aplikasi Simpul melalui perintah CLI.
   - Jendela desktop memantau perubahan `simpul-registry.json` secara *live* sehingga aplikasi yang diinstal oleh Claude atau `agy` dari terminal langsung muncul seketika di UI.
3. **Antarmuka Native Cepat (`1120x760`)**
   - Mode tampilan **Grid** dan **Table**, filter **Deploy Ready**, **Installed**, **Updates**, dan **Claude & agy**, pencarian instan, laci detail aplikasi (*App Details*), serta **CLI • agy • Claude Bridge Hub**.

## Perintah CLI (Untuk Terminal, Claude & Antigravity `agy`)

```bash
# Buka jendela desktop Simpul Creative Launcher
cargo run

# Cek status launcher, lokasi registry, serta deteksi binary claude & agy (format JSON)
cargo run -- status --json

# Tampilkan daftar software layak deploy (Release)
cargo run -- list --deploy-only

# Claude mengimpor sekaligus menginstal aplikasi baru dari manifest
cargo run -- import ./examples/simpul-app.json --agent claude --install --json

# Antigravity (agy) menginstal binary release langsung ke launcher
cargo run -- install ./examples/simpul-app.json --agent agy --json

# Cetak panduan konfigurasi & template manifest untuk Claude dan agy
cargo run -- agent-config

# Tangkap screenshot otomatis tanpa mengambil fokus jendela (120 frame)
cargo run -- --screenshot ./shots/now.png
```

## Tentang, donasi, dan privasi

- **Tentang**: tombol *Tentang* di header menampilkan versi, pembuat, tautan source code, dan kredit.
- **Donasi**: tombol *Donasi* membuka halaman Saweria di browser setelah konfirmasi. Donasi bersifat sukarela; semua aplikasi tetap gratis. Pengguna yang menyatakan sudah berdonasi mendapat lencana "You are awesome" di layar pembuka. Penanda ini disimpan lokal dan tidak diverifikasi.
- **Laporkan bug**: membuka aplikasi email dengan laporan yang sudah diisi. Tidak ada yang terkirim sebelum pengguna menekan Kirim.
- **Log diagnostik**: mati secara bawaan. Kalau diizinkan, kejadian ringkas dicatat ke file lokal tanpa nama akun, dan hanya ikut laporan bug kalau pengguna mencentangnya. Mematikan izin menghapus log.
- **Tanpa telemetri**: akses internet hanya untuk mengambil manifest bertanda tangan dari `simpul-manifest`, dan atas tindakan pengguna: mengunduh installer yang sha256-nya dicek, serta mengirim laporan bug saat tombol **Kirim laporan** ditekan. Laporan berisi nama aplikasi, versi, sistem operasi, dan tulisan pengguna; log hanya ikut bila dicentang. Tidak ada nama, email, atau alamat IP yang disimpan. Laporan ditandatangani dengan kunci pemasangan acak (`install.key`, dibuat di komputer pengguna dan tidak memuat identitas) supaya penerima bisa menolak kiriman palsu dan membatasi jumlahnya; jawaban penerima juga harus bertanda tangan sah.
- **Akun opsional**: masuk dengan Google hanya bila pengguna memilihnya. Yang disimpan di layanan `simpul-akun`: email dan nama akun Google, kunci publik pemasangan, dan daftar sertifikat. Tidak ada kata sandi. Akun bisa dihapus dari jendela Akun. Sesi tersimpan di `account.json` pada folder data bersama dan hanya berlaku dari pemasangan yang membuatnya.

## Status dan pembaruan

Launcher membaca [`simpul-manifest`](https://github.com/simpulcreativeapps/simpul-manifest) lewat [`simpul-guard`](https://github.com/simpulcreativeapps/simpul-guard): hijau berjalan normal, kuning menampilkan pemberitahuan, merah menampilkan hitung mundur lalu menonaktifkan aplikasi. File kerja pengguna tidak pernah dihapus.

## Kredit dan lisensi

Simpul Creative Launcher dibuat oleh [Simpul Project](https://simpulproject.com) dan merupakan karya turunan dari [ArtCraft Launcher](https://github.com/storytold/craft-launcher) (MIT atau Apache-2.0, © 2026 ArtCraft Team dan kontributor). Lihat [`NOTICE`](NOTICE) dan folder [`licenses/`](licenses/). Produk ini tidak berafiliasi dengan ArtCraft Team.

Kode Simpul Creative Launcher dilisensikan di bawah [MIT](LICENSE-MIT) atau [Apache-2.0](LICENSE-APACHE), sesuai pilihan Anda. Nama dan logo Simpul milik Simpul Project dan tidak termasuk lisensi kode.
