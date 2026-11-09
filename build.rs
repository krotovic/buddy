//! Embeds the Figma exports in assets/ into the binary: writes a list of
//! every animation and part to OUT_DIR/assets.rs, which src/library.rs
//! includes.

use std::fmt::Write;
use std::path::Path;

fn list(dir: &str, ext: &str) -> Vec<(String, String)> {
    println!("cargo:rerun-if-changed={dir}");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let abs = Path::new(env!("CARGO_MANIFEST_DIR")).join(&p);
            (stem, abs.to_string_lossy().into_owned())
        })
        .collect();
    files.sort();
    files
}

fn main() {
    let mut out = String::new();
    for (name, dir, ext) in [
        ("ANIMATIONS", "assets/animations", "json"),
        ("PARTS", "assets/parts", "svg"),
    ] {
        writeln!(out, "pub static {name}: &[(&str, &str)] = &[").unwrap();
        for (stem, path) in list(dir, ext) {
            writeln!(out, "    ({stem:?}, include_str!({path:?})),").unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    std::fs::write(dest, out).unwrap();
}
