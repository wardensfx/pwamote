use std::{fs, path::Path};

fn main() {
    // L'interface est compilée dans desktop-ui/dist ; sans elle, on met une page de remplacement
    // pour que `cargo build` fonctionne quand même.
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop-ui/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    if !dist.join("index.html").exists() {
        fs::create_dir_all(&dist).expect("création de desktop-ui/dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>PwaMote</title><p>Interface non compilée : lancez <code>npm run build</code> dans desktop-ui/.",
        )
        .expect("page de remplacement");
    }
    tauri_build::build();
}
