# Compile les deux interfaces avant le serveur : la PWA est embarquée dans le binaire, donc un
# `cargo build` seul resservirait l'ancienne version au téléphone.
$ErrorActionPreference = "Stop"
Push-Location client;     npm ci; npm run build; Pop-Location
Push-Location desktop-ui; npm ci; npm run build; Pop-Location
cargo build --release -p pwamote-desktop
