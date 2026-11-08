//! Mode ligne de commande : même serveur que l'application de bureau, sans fenêtre.

use pwamote_server::bootstrap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let opened = bootstrap::open(bootstrap::data_dir())?;
    let supervisor = opened.supervisor;
    supervisor.start().await?;
    let status = supervisor.status().await;
    let port = status.port.unwrap_or_default();
    println!("\nPwaMote prêt sur {}", status.origin.unwrap_or_default());
    println!("  Page d'appairage (QR code) : https://localhost:{port}/admin/pair\n");

    tokio::signal::ctrl_c().await?;
    supervisor.stop().await;
    Ok(())
}
