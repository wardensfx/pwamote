//! Le serveur embarque `client/dist`. Si le client n'a pas été compilé, on
//! crée une page de remplacement pour que `cargo build` fonctionne quand même.

use std::{fs, path::Path};

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../client/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    if !dist.join("index.html").exists() {
        fs::create_dir_all(&dist).expect("création de client/dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>PwaMote</title><p>Client non compilé : lancez <code>npm run build</code> dans client/.",
        )
        .expect("page de remplacement");
    }
}
