//! Serveur seul, sans fenêtre : le binaire `pwamote-server` et `PwaMote.exe --console`.

use crate::bootstrap;

/// Démarre le serveur, affiche l'adresse d'appairage et tourne jusqu'à Ctrl+C.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opened = bootstrap::open(bootstrap::data_dir())?;
    let supervisor = opened.supervisor;
    supervisor.start().await?;
    let status = supervisor.status().await;
    let port = status.port.unwrap_or_default();
    println!("\nPwaMote prêt sur {}", status.origin.unwrap_or_default());
    println!("  Page d'appairage (QR code) : https://localhost:{port}/admin/pair");
    println!("  Ctrl+C pour arrêter.\n");

    tokio::signal::ctrl_c().await?;
    supervisor.stop().await;
    Ok(())
}
