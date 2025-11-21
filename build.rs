// Precompile syntect syntax and theme sets into a single bincode blob to reduce first-load latency.
use std::{env, fs, path::PathBuf};
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Allow opt-out with UE_NO_PRECOMPILE=1
    if env::var("UE_NO_PRECOMPILE").ok().as_deref() == Some("1") { return; }
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
    // Optionally load custom syntaxes present at build time (non-fatal if missing)
    if let Ok(home) = env::var("HOME") {
        let custom = PathBuf::from(home).join(".ue").join("syntax");
        if custom.exists() { let _ = builder.add_from_folder(&custom, false); }
    }
    let ss = builder.build();
    let theme_set = ThemeSet::load_defaults();
    // Pick representative theme names so we can later choose one
    let themes: Vec<(String, syntect::highlighting::Theme)> = theme_set.themes.iter().map(|(n,t)| (n.clone(), t.clone())).collect();
    let blob = bincode::serialize(&(ss, themes)).expect("serialize syntect assets");
    let dest = out_dir.join("syntect_assets.bin");
    fs::write(&dest, blob).expect("write asset blob");
    println!("cargo:rustc-env=UE_PRECOMPILED_SYNTECT={}", dest.display());
}
