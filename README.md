# PwaMote

Le téléphone comme télécommande du PC, façon Wiimote.

**Site : [pwamote.app](https://pwamote.app)** · [Téléchargements](https://github.com/wardensfx/pwamote/releases/latest)

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

Chaque release propose aussi `PwaMote_<version>_x64_portable.exe` : la même application, sans installation. Elle ne s'extrait nulle part et tourne depuis l'endroit où on la pose ; ses données (réglages, appareils, certificats) vont dans `%APPDATA%\PwaMote`, comme la version installée, avec qui elle les partage. Elle ne crée ni raccourci ni entrée de désinstallation ; « Lancer avec Windows » pointe vers l'emplacement actuel du fichier.

Aucune console ne s'ouvre : le journal est écrit dans `%APPDATA%\PwaMote\logs\pwamote.log` (mis de côté en `pwamote.old.log` au-delà de 2 Mo). En ligne de commande :

```powershell
.\PwaMote.exe --console    # serveur seul, journal dans la console, Ctrl+C pour arrêter
.\PwaMote.exe --help       # options ; --version pour la version
```

Le programme étant une application fenêtrée, PowerShell et cmd rendent la main tout de suite et la sortie s'affiche par-dessus l'invite. Pour qu'ils attendent : `.\PwaMote.exe --console | Out-Host` (PowerShell) ou `start /wait PwaMote.exe --console` (cmd). Lancé par un raccourci, `--console` ouvre sa propre fenêtre.

Pour le développement, le serveur seul existe aussi en binaire console : `cargo run --release -p pwamote-server`, puis `https://localhost:8443/admin/pair` pour le QR code.

### Sans internet ou en mode local : faire confiance au PC

Le gyroscope n'est disponible qu'en HTTPS. Le serveur crée sa propre autorité de certification ; il faut l'installer une fois (environ 30 secondes) :

1. Sur la page d'appairage, ouvrez **Profil de confiance iOS** depuis le téléphone, autorisez le téléchargement.
2. **Réglages › Profil téléchargé › Installer**.
3. **Réglages › Général › Informations › Réglages des certificats** : activez la confiance pour *PwaMote Local CA*.
4. Scannez le QR code, puis touchez **Activer le gyroscope**. Sur iOS, l'autorisation se demande depuis un geste.

Ajoutez ensuite la page à l'écran d'accueil (Partager › Sur l'écran d'accueil) pour l'avoir en plein écran.

### HTTPS automatique (par défaut)

Rien à faire : au premier lancement avec internet, PwaMote donne au PC un nom `xxxx.d.pwamote.app` qui pointe vers son IP locale, obtient un vrai certificat Let's Encrypt et le renouvelle seul. Le QR code utilise alors ce nom, et l'iPhone n'a **aucun profil à installer**. Sans internet, le certificat local (et le profil ci-dessus) prend le relais. Domaine personnel ou mode local seul : fenêtre PwaMote › **HTTPS de l'iPhone**. Détails dans [docs/architecture.md](docs/architecture.md), service DNS dans [relay/](relay/README.md).

### Le téléphone ne trouve pas le PC

Si la fenêtre indique « En ligne » mais que le téléphone n'arrive pas à se connecter, ouvrez `https://<IP du PC>:8443/healthz` dans Safari. Si la page ne se charge pas, le pare-feu Windows bloque probablement l'exécutable (fréquent après une recompilation : `target\release\pwamote-desktop.exe` est un nouveau programme pour Windows). Au lancement, PwaMote vérifie le pare-feu sans droits administrateur et ne demande une confirmation administrateur que si la règle manque ou si une règle de blocage existe. Il crée alors une règle entrante limitée au réseau local pour cet exécutable, en TCP (HTTPS) et UDP (annonce mDNS). Cette demande revient donc une fois par nouvel exécutable, pas à chaque lancement. Côté téléphone, Réglages › Appareil permet toujours de réessayer, de rescanner un QR code ou de recharger la dernière version de l'app.

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

Trunk-based development : commits directs sur `main`, CI verte obligatoire (`fmt`, `clippy -D warnings`, tests Rust sur Windows, tests et build du client, types TypeScript à jour). Messages au format Conventional Commits, d'où est tiré le changelog ; une release se publie en poussant un tag `v1.2.3`. Voir [CONTRIBUTING.md](CONTRIBUTING.md).

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
| Gyro continu : la surface centrale fait clic gauche, appui maintenu et molette | fait, testé, vérifié dans un navigateur émulant un iPhone |
| Image collée dans le champ du clavier → collée sur le PC (presse-papiers + Ctrl+V), route `/paste` | fait, testé de bout en bout ; presse-papiers validé par Yann sur iPhone + Windows, Ctrl+V automatique à valider |

## Licence

PwaMote (application, télécommande et service DNS `relay/`) est distribué sous licence
[GNU AGPL v3 ou ultérieure](LICENSE) (`AGPL-3.0-or-later`). Vous pouvez l'utiliser, l'étudier,
le modifier et le redistribuer ; toute version modifiée que vous distribuez, ou que vous
proposez comme service sur un réseau (un relay, par exemple), doit publier son code source
sous la même licence.
