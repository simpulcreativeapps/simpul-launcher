# Prinsip desain Simpul Creative Apps

Aturan ini berlaku untuk launcher dan semua aplikasi Simpul, di semua tema.
Butir bertanda **[tes]** dijaga otomatis oleh `cargo test`; sisanya diperiksa saat review.

## 1. Terbaca dulu, gaya kemudian

- **[tes]** Semua teks berkontras minimal 4,5:1 terhadap permukaannya (WCAG 2.2 butir 1.4.3).
  Ini berlaku untuk teks utama, sekunder, redup, pil, dan teks di atas tombol berwarna.
- **[tes]** Di tema Liquid Glass, kontras dihitung terhadap titik tergelap dan terterang
  latar cahaya. Latar diredupkan dan panel memakai kaca gelap; kalau kaca membuat teks
  sulit dibaca, kacanya yang mengalah.
- Jendela dialog selalu berlatar padat.
- Warna tidak pernah menjadi satu-satunya penanda: status selalu disertai teks.

## 2. Satu aksi utama per bagian

- Setiap bagian punya paling banyak satu tombol berisian penuh (aksi utama).
  Aksi lain memakai tombol netral.
- Hijau lime hanya untuk donasi, tanda pembaruan, dan lencana pendukung.
- Merah hanya untuk tindakan yang menghapus dan status dihentikan.

## 3. Konsisten

- Satu bahasa di antarmuka: Bahasa Indonesia. Istilah teknis yang sudah umum
  (CLI, Grid) boleh tetap.
- Kata kerja yang sama untuk tindakan yang sama: Pasang, Perbarui, Buka, Hapus instalasi.
- Bentuk sudut, tinggi tombol, dan jarak diambil dari palet tema, tidak ditulis per tempat.

## 4. Jujur

- Antarmuka tidak menampilkan klaim yang tidak dicek (misalnya "terverifikasi" atau
  "terhubung") kecuali memang baru saja diperiksa.
- Daftar aplikasi hanya memuat yang sudah dirilis atau terpasang.
- Pesan galat menyebut apa yang terjadi dan apa yang bisa dilakukan.

## 5. Mudah dikenai

- **[tes]** Target klik minimal 24 x 24 px (WCAG 2.2 butir 2.5.8); tombol di sini 28 px ke atas.
- Tindakan yang tidak bisa dibatalkan selalu meminta konfirmasi.
- Semua dialog bisa ditutup dengan Esc.

## 6. Hormati pengguna

- Tanpa telemetri. Log hanya atas izin, dan isinya ditampilkan sebelum dikirim.
- Donasi selalu sukarela dan tidak pernah menghalangi pemakaian.

## Sumber

- WCAG 2.2: kontras teks 4,5:1 (1.4.3), kontras komponen 3:1 (1.4.11), ukuran target 24 px (2.5.8).
- Material 3 Expressive: hierarki lewat bentuk, warna, dan ukuran; wadah warna tidak boleh
  mengganggu hierarki.
- Apple Human Interface Guidelines (Materials / Liquid Glass): utamakan keterbacaan,
  tambahkan lapisan peredup di belakang kaca bening.
- Sepuluh heuristik kegunaan Nielsen: konsistensi, kecocokan dengan dunia nyata,
  pencegahan kesalahan, desain minimalis.

## Yang belum memenuhi

- Sebagian teks di laci detail, pengaturan, dan menu urutan masih berbahasa Inggris.
- Kontras garis tepi komponen (3:1) belum diuji otomatis.
- Fokus papan ketik memakai gaya bawaan egui dan belum diperiksa per tema.
- Belum ada pengaturan "kurangi transparansi" untuk tema kaca.
