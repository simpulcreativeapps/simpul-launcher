//! Program CLI (subsistem konsol): keluarannya selalu sampai ke terminal, pipa, atau
//! berkas, dari shell mana pun. Tanpa argumen, ia membuka program jendela.

const CONSOLE_BUILD: bool = true;

include!("entry.rs");
