# Changelog

Semua perubahan pada Simpul Creative Launcher dicatat di sini.
Format mengikuti [Keep a Changelog](https://keepachangelog.com/id-ID/1.1.0/),
dan versi mengikuti [Semantic Versioning](https://semver.org/lang/id/).

## [Belum dirilis]

### Ditambahkan
- Menu **Tema** di header dengan empat tema baru: Expressive Biru, Expressive Ceria,
  Liquid Glass Aurora, dan Liquid Glass Grafit. Tema bawaan gelap dan terang tetap ada.
- Katalog mengikuti manifest bertanda tangan: yang tampil hanya aplikasi yang sudah
  dirilis, yang terpasang di komputer, atau yang diimpor lewat CLI.
- Tampilan kosong "No apps found" bila belum ada aplikasi.
- Jendela **Tentang**: versi, pembuat, tautan source code, kontak, dan kredit.
- **Donasi** lewat Saweria dengan langkah konfirmasi, serta lencana "You are awesome"
  di layar pembuka untuk pengguna yang menyatakan sudah berdonasi.
- **Laporkan bug** lewat aplikasi email, dengan isi laporan ditampilkan sebelum dikirim.
- **Log diagnostik** lokal yang hanya aktif atas izin pengguna dan tidak mencatat nama akun.
- Status hijau, kuning, dan merah dari `simpul-manifest` lewat `simpul-guard`:
  pemberitahuan, hitung mundur, dan layar "dihentikan" dengan tombol Uninstall.
- Pembaruan launcher dan aplikasi diunduh dari manifest, dengan pengecekan sha256.
- Berkas `NOTICE` dan teks lisensi ArtCraft Launcher sebagai karya asal.

### Diubah
- Tombol "Check updates" sekarang benar-benar memeriksa manifest.
- Registry naik ke versi 2; entri contoh dari versi lama dibuang saat launcher dibuka.
- Teks antarmuka tidak lagi menyebut aplikasi yang belum ada.
- Versi launcher dibaca dari satu sumber (`Cargo.toml`).

### Diperbaiki
- Kolom `version` dan `id` pada manifest impor disaring, sehingga instalasi dan
  penghapusan tidak bisa keluar dari folder aplikasi.
- Aplikasi tanpa paket tidak lagi ditandai "terpasang".
- Aplikasi berstatus merah tidak bisa dibuka dari launcher maupun CLI.

### Dihapus
- Katalog contoh bawaan beserta versi dan riwayat aktivitas karangan.
- Tulisan "SHA-256 verified" dan "deploy-verified" yang tidak didukung pengecekan nyata.
- Dua berkas exe dan path komputer pembuat dari source.

## [1.0.0] - 2026-11-19

### Ditambahkan
- Versi internal pertama: katalog, impor dan instalasi lewat CLI untuk Claude dan
  Antigravity, pemindaian aplikasi terpasang, tampilan grid dan tabel, tema gelap dan terang.
