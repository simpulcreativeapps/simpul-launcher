# Changelog

Semua perubahan pada Simpul Creative Launcher dicatat di sini.
Format mengikuti [Keep a Changelog](https://keepachangelog.com/id-ID/1.1.0/),
dan versi mengikuti [Semantic Versioning](https://semver.org/lang/id/).

## [Belum dirilis]

## [1.5.1] - 2026-11-23

### Ditambahkan
- Bilah judul jendela Windows mengikuti warna tema (Windows 11; di Windows 10 hanya
  berganti gelap atau terang).

### Diperbaiki
- Garis gelap di antara bilah samping dan isi jendela pada tema terang.

## [1.5.0] - 2026-11-23

Tata letak dibangun ulang mengikuti rancangan acuan dari pemilik proyek.

### Ditambahkan
- **Bilah samping**: logo, lalu Beranda, Aplikasi, Pembaruan (dengan jumlah yang
  menunggu), Pengaturan, dan Tentang; di dasarnya kartu ajakan menuju situs Simpul.
- **Bilah atas**: kolom cari besar, pilihan Tema, Donasi, lonceng pembaruan, dan tombol
  akun yang memuat Masuk, Cek pembaruan, CLI & agen, dan Laporkan bug.
- **Banner Beranda** selebar layar dengan judul dua baris, versi launcher, tiga aksi,
  dan tanda Simpul berpendar di sisi kanan.
- Huruf tebal untuk judul dan tombol. Di Windows memakai Segoe UI milik sistem, dibaca
  saat berjalan dan tidak ikut dibagikan.

### Diubah
- Tab filter dan tombol tampilan menjadi bentuk pil setinggi 40 px dengan ikon.
- Kartu aplikasi lebih besar: ikon 92 px, nama tebal, keterangan di samping ikon, serta
  tombol Buka, Detail, dan menu berdampingan.
- Baris status memisahkan versi dan sertifikat; "Tanpa sertifikat" tidak lagi hijau.
- Ukuran jendela bawaan 1280x760 dan ukuran terkecil 1000x620.

### Dihapus
- Header lama dengan tujuh tombol, banner lama bergambar, dan kolom cari kedua di baris
  tab (pencarian sekarang hanya di bilah atas).

## [1.4.0] - 2026-11-23

### Ditambahkan
- **Akun Simpul (opsional)**: tombol Masuk di header dan jendela Akun. Masuk lewat Google
  di browser; launcher tidak pernah melihat kata sandi. Tersedia Keluar dan Hapus akun.
- **Sertifikat pemasangan**: saat masuk, pemasangan launcher didaftarkan; aplikasi yang
  dipasang lewat launcher ikut didaftarkan. Sertifikat bertanda tangan server, terikat ke
  kunci pemasangan komputer ini, dan diperiksa secara lokal.
- Baris status kartu aplikasi menampilkan "Bersertifikat", "Tanpa sertifikat", atau
  "Sertifikat tidak sah" (salinan dari pemasangan lain).

### Diubah
- Keterangan privasi di jendela Tentang dan README menyebut data akun.

## [1.3.0] - 2026-11-23

### Ditambahkan
- **Pembaruan wajib untuk launcher.** Bila manifest bertanda tangan memuat versi launcher
  yang lebih baru, launcher menampilkan layar "Pembaruan wajib", mengunduh installernya,
  memeriksa sha256, lalu memasangnya dengan satu klik. Bila unduhan gagal, pengguna boleh
  lanjut sekali untuk sesi itu, kecuali versinya sudah di bawah `min_version` manifest.
- **Versi launcher minimal per aplikasi** (`min_launcher` di manifest): aplikasi tidak
  bisa dibuka atau dipasang dari launcher yang lebih lama.
- **Versi minimal per aplikasi** (`min_version` di manifest): aplikasi di bawah versi itu
  wajib diperbarui sebelum dibuka.

## [1.2.0] - 2026-11-23

### Keamanan
- Kirim laporan memakai protokol bertanda tangan (penerima `simpul-lapor` v2):
  tantangan sekali pakai, bukti kerja, dan tanda tangan Ed25519 dari kunci pemasangan
  atas tantangan dan isi laporan. Kiriman polos dari alat lain, isi yang diubah di
  jalan, dan kiriman yang diputar ulang ditolak penerima.
- Jawaban penerima wajib bertanda tangan Ed25519 dengan kunci publik yang ditanam di
  aplikasi; jawaban tanpa tanda tangan sah dianggap gagal.
- Sambungan hanya HTTPS dan tidak mengikuti pengalihan.
- Log dikirim sebagai bagian terpisah dan hanya diterima bila berbentuk log aplikasi.

### Ditambahkan
- Kunci pemasangan acak `install.key` di folder data bersama, dibuat saat laporan
  pertama dikirim. Tidak memuat identitas pengguna.

## [1.1.0] - 2026-11-23

### Ditambahkan
- **Kirim laporan** satu klik: laporan bug dikirim langsung ke penerima Simpul
  (`simpul-lapor`) tanpa aplikasi email. Isi yang dikirim tetap ditampilkan lebih dulu,
  dan hasilnya tampil sebagai nomor laporan atau pesan gagal.

### Diubah
- "Kirim lewat email" menjadi pilihan cadangan "Lewat email".
- Keterangan privasi di jendela Tentang dan README menyebut pengiriman laporan ini.

## [1.0.1] - 2026-11-23

Hasil uji langsung pada launcher yang terpasang dari installer 1.0.0.

### Diperbaiki
- Jendela terminal hitam tidak lagi ikut terbuka saat launcher dijalankan dari Start menu.
  Perintah CLI tetap mencetak keluarannya ke terminal pemanggil.
- Layar pembuka tampil sekitar 1,8 detik (sebelumnya 0,8 detik), dan sedikit lebih lama
  untuk pendukung, supaya lencana "You are awesome" sempat terbaca.
- **Kirim lewat email** memakai pengendali email sistem operasi, dan isi laporan selalu
  ikut disalin. Bila tidak ada aplikasi email, pengguna diberi tahu dan tinggal menempel.
- Uninstaller mengeluarkan lagi folder launcher dari PATH pengguna.
- Impor aplikasi memakai versi dari manifest; isian Versi boleh dikosongkan.
- Impor lewat formulir tercatat sebagai "Manual", bukan sebagai agen Claude.
- Tentang, Donasi, dan Laporkan bug tidak lagi terbuka bertumpuk.

### Diubah
- Label "Rilis" pada jenis build menjadi "Build rilis" agar tidak terbaca sebagai
  "sudah dirilis resmi".
- Teks yang masih berbahasa Inggris diterjemahkan: panel Detail, judul kolom Tabel,
  dialog hapus instalasi, pesan impor, pindai, antrean, dan catatan aktivitas.
  Keterangan aplikasi terdeteksi yang tersimpan dari 1.0.0 ikut diperbarui (registry v3).
- Tema yang dipilih disimpan juga ke `theme.json` di folder data bersama, sehingga
  aplikasi Simpul lain memakai tema yang sama, dan sebaliknya.
- `--version` dan teks bantuan CLI mengambil nomor versi dari satu sumber.
- Installer menyertakan `CHANGELOG.md`, `NOTICE`, dan teks lisensi di folder `docs`.

### Ditambahkan
- Program CLI terpisah `simpul-creative-launcher-cli.exe` (subsistem konsol) untuk terminal
  dan agen, supaya keluarannya sampai ke terminal, pipa, atau berkas dari shell mana pun.
  `simpul-creative-launcher` di PATH sekarang menjalankan program ini; perintah yang
  ditampilkan di panel "CLI & agen" juga menunjuk ke sana.

### Ditambahkan
- Pengaturan **Kurangi transparansi**: tema Liquid Glass memakai panel padat tanpa latar cahaya.
- Tes otomatis kontras komponen 3:1 (tepi kontrol, cincin fokus, tombol terpilih).
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
- `design/PRINSIP.md`: prinsip desain yang dipakai, mana yang diuji otomatis, dan yang belum terpenuhi.
- Berkas `NOTICE` dan teks lisensi ArtCraft Launcher sebagai karya asal.

### Diubah
- Seluruh teks antarmuka (laci detail, pengaturan, panel agen, formulir impor, menu urutan)
  kini berbahasa Indonesia.
- Fokus papan ketik dan teks terpilih memakai cincin 2 px dan warna yang lolos 3:1 di semua tema.
- Palet semua tema disusun ulang supaya setiap teks berkontras minimal 4,5:1 (WCAG 2.2),
  dijaga oleh tes otomatis. Tema Liquid Glass memakai kaca gelap di atas latar yang diredupkan.
- Hero disederhanakan: satu judul, satu aksi utama, tanpa label status yang tidak dicek.
- Antarmuka utama memakai Bahasa Indonesia secara konsisten; tombol Splash dihapus dari header.
- Tombol Pasang menjadi aksi utama kartu aplikasi.
- Tombol "Check updates" sekarang benar-benar memeriksa manifest.
- Registry naik ke versi 2; entri contoh dari versi lama dibuang saat launcher dibuka.
- Teks antarmuka tidak lagi menyebut aplikasi yang belum ada.
- Versi launcher dibaca dari satu sumber (`Cargo.toml`).

### Diperbaiki
- Kolom perintah CLI di jendela Pengaturan terpotong di tepi kanan.
- Teks putih pada tombol terpilih dan tombol Pasang tidak terbaca di beberapa tema.
- Kolom `version` dan `id` pada manifest impor disaring, sehingga instalasi dan
  penghapusan tidak bisa keluar dari folder aplikasi.
- Aplikasi tanpa paket tidak lagi ditandai "terpasang".
- Aplikasi berstatus merah tidak bisa dibuka dari launcher maupun CLI.

### Dihapus
- Pilihan pembaruan otomatis di Pengaturan, karena belum berfungsi.
- Tulisan "Verifying SHA-256 checksum" pada antrean pasang.
- Katalog contoh bawaan beserta versi dan riwayat aktivitas karangan.
- Tulisan "SHA-256 verified" dan "deploy-verified" yang tidak didukung pengecekan nyata.
- Dua berkas exe dan path komputer pembuat dari source.

## [1.0.0] - 2026-11-19

### Ditambahkan
- Versi internal pertama: katalog, impor dan instalasi lewat CLI untuk Claude dan
  Antigravity, pemindaian aplikasi terpasang, tampilan grid dan tabel, tema gelap dan terang.
