//! Program jendela. Di Windows memakai subsistem GUI supaya tidak ada jendela terminal
//! yang ikut terbuka. Untuk terminal dan agen, pakai program CLI (`console.rs`).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

const CONSOLE_BUILD: bool = false;

include!("entry.rs");
