<a href="https://github.com/simpulcreativeapps/simpul-launcher/releases/latest">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/launcher-dark.svg">
    <img src="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/launcher-light.svg" width="100%" alt="Simpul Launcher: pasang, perbarui, dan buka aplikasi Simpul dari satu tempat. Gratis tanpa langganan, open source (MIT atau Apache-2.0), tanpa telemetri, unduhan dicek sha256.">
  </picture>
</a>

<p align="center">
  <a href="https://github.com/simpulcreativeapps/simpul-launcher/releases/latest"><img src="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/btn-download.svg" height="46" alt="Download untuk Windows"></a>
  &nbsp;
  <a href="https://saweria.co/simpuldevs"><picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/btn-donate-dark.svg"><img src="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/btn-donate-light.svg" height="46" alt="Donasi"></picture></a>
  &nbsp;
  <a href="https://simpul-creative-apps.pages.dev"><picture><source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/btn-site-dark.svg"><img src="https://raw.githubusercontent.com/simpulcreativeapps/simpulcreativeapps/main/assets/btn-site-light.svg" height="46" alt="Buka website"></picture></a>
</p>

# Simpul Creative Launcher

Satu pintu untuk semua aplikasi [Simpul Creative Apps](https://github.com/simpulcreativeapps): pasang, buka, dan perbarui dari satu jendela. Gratis, open source, dan dibuat di Indonesia oleh [Simpul Project](https://simpulproject.com).

Kalau launcher ini berguna, beri bintang ⭐ di kanan atas halaman ini. Itu cara termudah membantu orang lain menemukannya.

## Pasang

1. Unduh `Simpul-Creative-Launcher-Setup-<versi>.exe` dari [rilis terbaru](https://github.com/simpulcreativeapps/simpul-launcher/releases/latest).
2. Jalankan installernya. Pemasangan tidak butuh hak admin.
3. Buka **Simpul Creative Launcher** dari menu Start.

Untuk Windows 10/11 64-bit.

> Installer belum ditandatangani secara digital, jadi Windows SmartScreen bisa menampilkan peringatan. Cocokkan sha256 berkas dengan yang tercantum di catatan rilis sebelum menjalankannya:
>
> ```powershell
> Get-FileHash .\Simpul-Creative-Launcher-Setup-<versi>.exe -Algorithm SHA256
> ```

## Yang bisa dilakukan

- **Pasang dan buka aplikasi dari satu tempat.** Daftar aplikasi mengikuti yang sudah dirilis; aplikasi Simpul yang sudah terpasang di komputer ditemukan lewat **Pindai Komputer**.
- **Pembaruan yang diperiksa.** Versi terbaru dibaca dari manifest bertanda tangan, dan setiap installer dicek sha256-nya sebelum dijalankan.
- **Status tiap aplikasi.** Hijau berjalan normal, kuning menampilkan pemberitahuan, merah menonaktifkan aplikasi setelah hitung mundur. File kerja pengguna tidak pernah dihapus.
- **Tema.** Terang, gelap, ikuti sistem, Expressive (Biru, Ceria), dan Liquid Glass (Aurora, Grafit).
- **Akun opsional.** Tanpa akun pun semua berjalan. Masuk dengan Google hanya menambahkan sertifikat pemasangan.
- **CLI.** Memasang, membuka, dan memindai aplikasi juga bisa dari terminal, termasuk oleh agen AI.

## Privasi

- **Tanpa telemetri.** Akses internet hanya untuk mengambil manifest bertanda tangan dari [`simpul-manifest`](https://github.com/simpulcreativeapps/simpul-manifest), dan atas tindakan pengguna: mengunduh installer, serta mengirim laporan bug saat tombol **Kirim laporan** ditekan.
- **Laporan bug** berisi nama aplikasi, versi, sistem operasi, dan tulisan pengguna. Tidak ada nama, email, atau alamat IP yang disimpan. Laporan ditandatangani dengan kunci pemasangan acak (`install.key`, dibuat di komputer pengguna dan tidak memuat identitas) supaya penerima bisa menolak kiriman palsu dan membatasi jumlahnya.
- **Log diagnostik** mati secara bawaan. Kalau diizinkan, kejadian ringkas dicatat ke file lokal tanpa nama akun, dan hanya ikut laporan bug kalau pengguna mencentangnya. Mematikan izin menghapus log.
- **Akun** hanya ada bila pengguna memilih masuk dengan Google. Yang disimpan di layanan `simpul-akun`: email dan nama akun Google, kunci publik pemasangan, dan daftar sertifikat. Tidak ada kata sandi. Akun bisa dihapus dari jendela Akun.
- **Donasi** bersifat sukarela dan membuka halaman Saweria di browser setelah konfirmasi. Pengguna yang menyatakan sudah berdonasi mendapat lencana di layar pembuka; penanda ini disimpan lokal dan tidak diverifikasi.

## CLI

Installer bisa menambahkan CLI ke PATH pengguna:

```bash
simpul-creative-launcher                      # buka jendela desktop
simpul-creative-launcher status --json        # status launcher dan lokasi registry
simpul-creative-launcher list --deploy-only   # aplikasi yang siap dipakai (build rilis)
simpul-creative-launcher rescan               # cari aplikasi Simpul yang sudah terpasang
simpul-creative-launcher import <manifest> --install
simpul-creative-launcher install <id-atau-berkas>
simpul-creative-launcher launch <id>
simpul-creative-launcher uninstall <id>
```

Hanya build rilis yang diterima: artefak `target/debug` atau yang bertanda `"profile": "debug"` ditolak kecuali `--allow-debug` diberikan, dan digest sha256 dicek sebelum impor atau pemasangan. Contoh manifest ada di [`examples/`](examples/).

Agen AI di terminal, misalnya Claude Code atau Antigravity (`agy`), bisa memakai perintah yang sama dengan `--agent <nama>`; aplikasi yang mereka pasang langsung muncul di jendela desktop. Panduannya ada di [`CLAUDE.md`](CLAUDE.md), atau jalankan `simpul-creative-launcher agent-config`.

## Bangun dari kode

Butuh Rust 1.85 atau lebih baru.

```bash
git clone https://github.com/simpulcreativeapps/simpul-launcher.git
cd simpul-launcher
cargo run                # jendela desktop
cargo run -- status      # CLI
cargo test
cargo build --release
```

Launcher dibangun dengan Rust dan `eframe`/`egui`. Status dan pembaruan dibaca dari [`simpul-manifest`](https://github.com/simpulcreativeapps/simpul-manifest) lewat [`simpul-guard`](https://github.com/simpulcreativeapps/simpul-guard). Riwayat perubahan ada di [`CHANGELOG.md`](CHANGELOG.md).

## Ikut terlibat

Laporkan bug atau usulkan fitur lewat [issue](https://github.com/simpulcreativeapps/simpul-launcher/issues), atau kirim perbaikan lewat pull request. Bug juga bisa dilaporkan langsung dari launcher lewat **Laporkan bug**.

Simpul gratis dan akan tetap gratis. Dua hal yang paling membantu: bintang ⭐ di repositori ini, dan [donasi sukarela lewat Saweria](https://saweria.co/simpuldevs).

## Kredit dan lisensi

Simpul Creative Launcher dibuat oleh [Simpul Project](https://simpulproject.com) dan merupakan karya turunan dari [ArtCraft Launcher](https://github.com/storytold/craft-launcher) (MIT atau Apache-2.0, © 2026 ArtCraft Team dan kontributor). Lihat [`NOTICE`](NOTICE) dan folder [`licenses/`](licenses/). Produk ini tidak berafiliasi dengan ArtCraft Team.

Kode Simpul Creative Launcher dilisensikan di bawah [MIT](LICENSE-MIT) atau [Apache-2.0](LICENSE-APACHE), sesuai pilihan Anda. Nama dan logo Simpul milik Simpul Project dan tidak termasuk lisensi kode.
