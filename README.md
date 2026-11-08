# PwaMote

Le téléphone comme télécommande du PC, façon Wiimote.

- **Visée au gyroscope** : on pointe avec le téléphone, le curseur suit. Insensible à la manière de tenir l'appareil.
- **Trackpad** et **défilement à deux doigts** en complément, au choix dans les réglages.
- **Clic gauche / clic droit** sur deux grandes zones, avec retour haptique.
- **Clavier déplié** avec Ctrl, Maj, Alt, Win, Échap, flèches… Les modificateurs sont « collants ».
- **Loupe intelligente** autour du curseur ou de la zone de saisie.
- **Actions programmables**, façon Stream Deck : des pages de boutons définies sur le PC, qui simulent des touches (y compris pour les extensions du navigateur), tapent du texte, lancent des programmes ou des scripts, ouvrent des adresses. Une action peut demander des valeurs au téléphone avant de partir : liste, curseur, nombre, texte, interrupteur.
- **Appairage en un scan** (QR code), reconnexion automatique, réglages mémorisés et synchronisés entre PC et téléphones.

Cible actuelle : serveur **Windows 11**, client **iPhone** (Safari, installable sur l'écran d'accueil). Voir [docs/architecture.md](docs/architecture.md).

## Démarrer

Prérequis : Rust stable, Node 22, WebView2 (déjà présent sur Windows 11).

```powershell
cd client;     npm ci; npm run build; cd ..     # la PWA, embarquée dans le serveur
cd desktop-ui; npm ci; npm run build; cd ..     # l'interface de la fenêtre
# Sous Windows, `scripts/build.ps1` fait tout. Penser à relancer `npm run build` dans client/ avant
# chaque `cargo build` : la PWA est embarquée dans le binaire, sinon le téléphone reçoit l'ancienne version.
cargo run --release -p pwamote-desktop          # l'application de bureau
```

L'application s'ouvre avec le **QR code** et l'état du serveur. L'onglet **Actions** édite le panneau d'actions du téléphone (pages, grille, étapes, champs) et permet d'essayer une action sur le PC. Elle vit ensuite dans la **barre système** : fermer la fenêtre la réduit, un clic sur l'icône la rouvre, le menu de l'icône active ou arrête le serveur et quitte. Dans la fenêtre : adresse encodée dans le QR (IPv4, nom `.local` ou saisie libre), lancement avec Windows, démarrage réduit, port, liste des appareils associés.

L'installeur Windows (`.exe`, installation par utilisateur) est produit par la CI : artefact `pwamote-installer-windows`.

Mode ligne de commande, sans fenêtre : `cargo run --release -p pwamote-server`, puis `https://localhost:8443/admin/pair` pour le QR code.

### Première fois sur iPhone : faire confiance au PC

Le gyroscope n'est disponible qu'en HTTPS. Le serveur crée sa propre autorité de certification ; il faut l'installer une fois (environ 30 secondes) :

1. Sur la page d'appairage, ouvrez **Profil de confiance iOS** depuis le téléphone, autorisez le téléchargement.
2. **Réglages › Profil téléchargé › Installer**.
3. **Réglages › Général › Informations › Réglages des certificats** : activez la confiance pour *PwaMote Local CA*.
4. Scannez le QR code, puis touchez **Activer le gyroscope**. Sur iOS, l'autorisation se demande depuis un geste.

Ajoutez ensuite la page à l'écran d'accueil (Partager › Sur l'écran d'accueil) pour l'avoir en plein écran.

## Développer

```bash
cargo test --workspace --exclude pwamote-desktop   # cœur, serveur, bout en bout (génère aussi client/src/generated)
cd client && npm test                               # Vitest de la PWA
cd desktop-ui && npm test                           # Vitest de la fenêtre
cd client && npm run dev          # client seul, avec proxy vers le serveur local
```

`pwamote-desktop` (Tauri) se compile sous Windows, où la CI le teste ; sous Linux il demande WebKitGTK, d'où l'exclusion ci-dessus. Le serveur compile sur Linux/macOS avec des adaptateurs qui se contentent de journaliser : on peut y développer l'interface sans Windows.

Développement guidé par les tests : toute logique vit dans `pwamote-core` (sans OS) ou dans `client/src/lib` (sans DOM), testée avant l'adaptateur qui l'utilise.

## Workflow

Trunk-based development : branches courtes, intégration fréquente dans `main`, CI verte obligatoire (`fmt`, `clippy -D warnings`, tests Rust sur Windows, tests et build du client, types TypeScript à jour).

## État

| | |
|---|---|
| Protocole, réglages, appairage, pointeur gyroscopique, loupe (logique) | fait, testé |
| Serveur HTTPS, WebSocket, mDNS, injection `SendInput` | fait, testé de bout en bout (SendInput compilé, à valider sur Windows) |
| PWA : appairage, gyro, trackpad, clics, clavier, réglages | fait, vérifié dans un navigateur émulant un iPhone |
| Loupe Windows (API Magnification) | écrite, à valider sur Windows |
| Caret via UI Automation (navigateurs, Electron) | à faire |
| Boutons de volume du téléphone | impossible en PWA, voir l'architecture |
| Application de bureau : fenêtre, barre système, démarrage avec Windows, choix de l'adresse du QR | écrite, interface vérifiée dans un navigateur, à valider sur Windows |
| Actions programmables : modèle, validation, protocole, panneau du téléphone, éditeur | fait, testé (bout en bout avec faux système) ; panneau et éditeur vérifiés dans un navigateur |
| Actions : lancement de programmes, ouverture (`ShellExecuteW`), touches avec code de balayage | écrit, compilé pour Windows, à valider sur Windows |
