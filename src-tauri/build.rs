use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

// The export path compiles the frontend's WGSL through include_str!, which
// never sees vite — so the `//!include` directive gets a second resolver here.
// Flattened shaders go to OUT_DIR rather than into the tree, which keeps
// `cargo build` self-contained: no node, and nothing generated to commit.
//
// The grammar is kept minimal on purpose, since this and
// scripts/wgslInclude.js have to agree: one include per line, a path relative
// to the shaders root, and chunks may not include chunks.

const SHADERS: &str = "../src/lib/gpu/shaders";

fn resolve(source: &str, root: &Path, origin: &str) -> (String, BTreeSet<PathBuf>) {
    let mut chunks = BTreeSet::new();
    let mut out = Vec::new();

    for line in source.lines() {
        let Some(rel) = directive(line) else {
            assert!(
                !line.contains("//!include"),
                "{origin}: malformed include, expected //!include \"path\": {}",
                line.trim()
            );
            out.push(line.to_string());
            continue;
        };

        let chunk = root.join(rel);
        let text = fs::read_to_string(&chunk)
            .unwrap_or_else(|_| panic!("{origin}: no such chunk: {rel}"));
        assert!(
            !text.contains("//!include"),
            "{rel}: a chunk may not include another chunk"
        );

        out.push(text.trim_end_matches('\n').to_string());
        chunks.insert(chunk);
    }

    (out.join("\n"), chunks)
}

/// The quoted path of an `//!include` line, if that is what this line is.
fn directive(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("//!include")?;
    let rest = rest.trim();
    rest.strip_prefix('"')?.strip_suffix('"')
}

fn main() {
    let root = PathBuf::from(SHADERS);
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("shaders");
    fs::create_dir_all(&out).expect("create shader out dir");

    println!("cargo:rerun-if-changed={SHADERS}");

    for entry in fs::read_dir(&root).expect("read shaders dir") {
        let path = entry.expect("shader entry").path();
        if path.extension().is_none_or(|e| e != "wgsl") {
            continue;
        }
        let name = path.file_name().expect("shader name");
        let source = fs::read_to_string(&path).expect("read shader");
        let origin = name.to_string_lossy().to_string();

        let (flat, chunks) = resolve(&source, &root, &origin);
        for chunk in &chunks {
            println!("cargo:rerun-if-changed={}", chunk.display());
        }
        fs::write(out.join(name), flat).expect("write flattened shader");
    }

    tauri_build::build()
}
