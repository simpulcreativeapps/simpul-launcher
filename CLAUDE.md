# Claude & Antigravity (agy) Integration Guide — Simpul Creative Apps Launcher

When asked to import, register, or install an application into **Simpul — Creative Apps Launcher**, use the launcher's built-in CLI commands:

## 1. Import and Install a Deploy-Ready App

```bash
# Claude importing & installing a Simpul app manifest or release binary
cargo run --manifest-path Cargo.toml -- import ./examples/capr-notebook.json --agent claude --install --json

# Antigravity (agy) installing a Release artifact directly
cargo run --manifest-path Cargo.toml -- install ./examples/simpulgram-extension.json --agent agy --json
```

## 2. Deploy-Readiness Rule (Layak Deploy, Bukan Debug)

Simpul Creative Launcher enforces a **Release-Only Deploy Gate** by default:
- Flagship production apps include **SimpulGram**, **SimpulSync 4**, and **Capr** (Simpul Notebook).
- Always build target applications in **Release** mode (e.g. `cargo build --release`, `.msi`, or production `.zip`) before importing/installing them.
- Artifacts inside `target/debug/` or marked `"profile": "debug"` are blocked by default unless `--allow-debug` is explicitly passed for sandbox testing.

## 3. Inspect Catalog & Integration Status

```bash
cargo run --manifest-path Cargo.toml -- list --deploy-only --json
cargo run --manifest-path Cargo.toml -- status --json
```

## Acuan ekosistem (simpul-blueprint)

Repo ini bagian dari ekosistem Simpul Creative Apps. **Acuan tunggalnya ada di repo privat
`simpul-blueprint`** milik tim Simpul (tidak publik).

Sebelum mengubah apa pun:
1. Baca `simpul-blueprint/ATURAN-AGEN.md` dan `simpul-blueprint/HANDOFF.md`.
2. Baca blueprint produk ini di `simpul-blueprint/produk/`.

Setelah selesai:
1. Tambahkan entri di atas `simpul-blueprint/LOG.md` (siapa/model, apa, kenapa, berkas/commit,
   diuji, belum/risiko, berikutnya).
2. Perbarui `simpul-blueprint/HANDOFF.md` dan blueprint produk bila arsitektur atau keputusan berubah.

Jangan pernah menulis nilai rahasia ke repo, log, atau chat. Commit, push, deploy, migrasi
produksi, dan penghapusan data butuh izin pemilik.
