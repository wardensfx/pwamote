# Architecture

PwaMote transforme un téléphone en télécommande de PC, façon Wiimote : on **vise** avec le gyroscope, on clique avec deux boutons, on tape du texte avec le clavier du téléphone.

## Vue d'ensemble

```mermaid
flowchart LR
  subgraph Phone["Téléphone (PWA, Safari/Chrome)"]
    direction TB
    UI["Interface Solid<br/>pad · clics · clavier · réglages"]
    MOT["MotionSource<br/>devicemotion → repère W3C"]
    PAD["PadGestures<br/>embrayage · trackpad · défilement"]
    SYNC["SettingsSync<br/>modifs locales rejouées"]
    CONN["Connection<br/>WebSocket + reconnexion"]
    UI --> PAD --> CONN
    MOT --> CONN
    UI --> SYNC --> CONN
  end

  subgraph PC["PC Windows 11 (pwamote-server)"]
    direction TB
    WEB["axum + rustls<br/>HTTPS · WebSocket · PWA embarquée"]
    SES["Session<br/>machine à états par téléphone"]
    CORE["pwamote-core<br/>pointeur · loupe · réglages · appairage"]
    IN["InputSink<br/>SendInput"]
    MAG["MagnifierSink<br/>API Magnification"]
    ST["AppState<br/>réglages + appareils (JSON)"]
    WEB --> SES --> CORE
    SES --> IN
    SES --> MAG
    SES <--> ST
  end

  CONN <== "wss://pwamote-pc.local:8443/ws<br/>JSON (contrôle) + binaire (capteurs)" ==> WEB
  MDNS(("mDNS<br/>_pwamote._tcp")) -.-> Phone
  WEB -.annonce.-> MDNS
```

Le serveur est découpé en **cœur pur** (`pwamote-core`, testé sans OS) et **adaptateurs** (`InputSink`, `MagnifierSink`) : les tests de session tournent avec de faux adaptateurs, Windows n'est impliqué que dans la dernière couche.

## Application de bureau

L'application Windows est une coque **Tauri 2** (WebView2) autour du même serveur. La fenêtre (`desktop-ui/`, Solid) parle à Rust par des commandes ; le **superviseur** démarre, arrête et redémarre l'écoute à chaud.

```mermaid
flowchart LR
  subgraph Desktop["pwamote-desktop (Tauri)"]
    direction TB
    WIN["Fenêtre Solid<br/>état · QR · adresse · appareils · options"]
    TRAY["Icône de la barre système<br/>Ouvrir · Serveur actif · Quitter"]
    CMD["Commandes IPC<br/>get_status · set_config · set_enabled<br/>new_pairing · list_devices · revoke_device"]
    PLUG["Plugins<br/>instance unique · lancement avec Windows"]
    WIN <--> CMD
    TRAY --> CMD
  end
  subgraph Lib["pwamote-server (bibliothèque)"]
    direction TB
    SUP["Supervisor<br/>start · stop · set_config · pairing"]
    CFG["AppConfig<br/>port · adresse · démarrage · tray"]
    ST["AppState<br/>réglages · appareils"]
    NET["axum + rustls · mDNS"]
    SUP --> NET
    SUP --> CFG
    SUP --> ST
  end
  CMD --> SUP
  CLI["pwamote-server (CLI)"] --> SUP
```

### Cycle de vie de la fenêtre

```mermaid
stateDiagram-v2
  [*] --> Demarrage
  Demarrage --> FenetreOuverte: démarrer réduit = non
  Demarrage --> Reduite: démarrer réduit = oui
  FenetreOuverte --> Reduite: fermer (réduire à la fermeture = oui)
  FenetreOuverte --> [*]: fermer (réduire à la fermeture = non)
  Reduite --> FenetreOuverte: clic sur l'icône, « Ouvrir », ou 2e lancement
  Reduite --> [*]: menu « Quitter »
  FenetreOuverte --> [*]: menu « Quitter »
  note right of Reduite: le serveur continue d'écouter
```

### Serveur : activer, arrêter, changer d'adresse

```mermaid
flowchart TD
  A["set_config / set_enabled"] --> B{"serveur actif ?"}
  B -- non --> C["arrêt gracieux (300 ms)<br/>les WebSocket sont coupées"]
  B -- oui --> D{"port ou adresse changés ?"}
  D -- non --> E["rien à redémarrer<br/>(options de fenêtre, démarrage)"]
  D -- oui --> F["arrêt puis démarrage"]
  F --> G["certificat réémis<br/>pour l'adresse choisie"]
  G --> H["écoute sur 0.0.0.0:port"]
  H -- échec --> I["erreur claire dans la fenêtre<br/>ex. port déjà utilisé"]
  H -- ok --> J["origine = https://adresse:port<br/>nouveau QR code"]
  C --> K["coche du menu et info-bulle mises à jour"]
  J --> K
```

L'adresse encodée dans le QR est **au choix** : IPv4 recommandée (fonctionne partout), autre IPv4, nom `.local` (stable si l'IP change, mais dépend du mDNS), ou nom saisi (VPN, DNS interne). Le certificat couvre toujours l'adresse choisie. Les appareils restent associés par origine : changer d'adresse oblige un téléphone à se ré-appairer, car le navigateur isole ses données par origine.

## Dépôt

```mermaid
flowchart TD
  R["pwamote/"] --> C["crates/pwamote-core<br/>protocole · réglages · appairage<br/>pointeur · loupe"]
  R --> S["crates/pwamote-server<br/>web · session · TLS · mDNS · plateforme"]
  R --> CL["client/<br/>PWA Vite + Solid + TypeScript"]
  R --> DE["crates/pwamote-desktop<br/>coque Tauri : fenêtre, tray, démarrage"]
  R --> DU["desktop-ui/<br/>interface de la fenêtre (Solid)"]
  DU -- "npm run build" --> DUD["desktop-ui/dist"]
  DUD --> DE
  R --> D["docs/"]
  C -- "cargo test génère (ts-rs)" --> G["client/src/generated/*.ts"]
  G --> CL
  CL -- "npm run build" --> DIST["client/dist"]
  DIST -- "rust-embed" --> S
```

Les types du protocole sont définis **une seule fois**, en Rust. `cargo test` régénère `client/src/generated`, et la CI échoue si ces fichiers ne sont pas à jour.

## Appairage

Un code à usage unique (5 min) est encodé dans un QR affiché sur le PC, à l'adresse `https://localhost:8443/admin/pair` (réservée à la machine). Seul l'empreinte SHA-256 du jeton d'appareil est stockée.

```mermaid
sequenceDiagram
  autonumber
  actor U as Utilisateur
  participant PC as Page /admin/pair
  participant P as PWA (téléphone)
  participant S as Serveur

  U->>PC: ouvre la page sur le PC
  PC->>S: nouveau code (usage unique, 5 min)
  S-->>PC: QR → https://pwamote-pc.local:8443/#/pair/CODE
  U->>P: scanne le QR (appareil photo)
  P->>S: WebSocket, {t: "pair", code, device_name, protocol}
  alt code valide
    S-->>P: {t: "paired", device_id, token}
    S-->>P: {t: "welcome", réglages}
    Note over P: jeton enregistré, code retiré de l'URL
  else code expiré, inconnu ou déjà utilisé
    S-->>P: {t: "error", code: "pairing_code_invalid"} + fermeture
  end

  Note over P,S: Connexions suivantes
  P->>S: {t: "hello", token, protocol}
  S-->>P: {t: "welcome", réglages}
```

## HTTPS local

Safari n'expose le gyroscope et ne permet l'installation hors-ligne qu'en HTTPS. Le serveur crée sa propre autorité de certification une seule fois, puis émet un certificat pour `pwamote-<machine>.local` et les adresses IP du moment à chaque démarrage.

```mermaid
flowchart TD
  A["Premier lancement"] --> B["Création de la CA locale<br/>%APPDATA%\\PwaMote\\ca-*.pem"]
  B --> C["Chaque démarrage : certificat serveur<br/>nom mDNS + IP · 397 jours"]
  C --> D["Le téléphone ouvre /pwamote.mobileconfig"]
  D --> E["Réglages iOS › Profil téléchargé › Installer"]
  E --> F["Réglages › Général › Informations › Réglages des certificats<br/>activer la confiance totale pour « PwaMote Local CA »"]
  F --> G["HTTPS de confiance : gyroscope, PWA installable, hors-ligne"]
```

## Du gyroscope au curseur

Le moteur (`pointer.rs`) ne dépend pas de la façon dont on tient le téléphone : la vitesse angulaire est projetée sur la **verticale réelle** (gravité) et sur l'axe horizontal du téléphone.

```mermaid
flowchart LR
  S["Échantillon capteur<br/>gyro °/s · accéléromètre · dt"] --> G["Gravité<br/>filtre passe-bas"]
  S --> W["ω"]
  G --> P["Projection<br/>lacet = ω·verticale<br/>tangage = ω·droite"]
  W --> P
  P --> F["Filtre One Euro<br/>lisse au repos, réactif en mouvement"]
  F --> Z["Zone morte douce<br/>absorbe le tremblement"]
  Z --> A["Accélération<br/>gestes rapides = plus loin"]
  A --> K["× sensibilité × dt"]
  K --> Q["Accumulateur sub-pixel"]
  Q --> M["SendInput MOUSEEVENTF_MOVE"]
```

### Côté téléphone : d'où viennent ω et la gravité

Les navigateurs ne s'accordent pas sur l'ordre des axes de `rotationRate`. Le client calcule donc ω à partir de `deviceorientation` (alpha, beta, gamma, définis sans ambiguïté par le W3C) : l'orientation devient un quaternion, et la rotation entre deux événements, exprimée dans le repère du téléphone, donne la vitesse angulaire. La gravité vient du même calcul, ce qui garantit qu'elle reste cohérente avec ω. `rotationRate` (avec calibration des axes) ne sert que de repli si aucun événement d'orientation n'arrive.

```mermaid
flowchart LR
  O["deviceorientation<br/>alpha · beta · gamma"] --> Q["Quaternion q"]
  Q --> D["Δq = q₀⁻¹ · q₁"]
  D --> W["ω dans le repère du téléphone"]
  Q --> U["Haut dans le repère du téléphone<br/>Ry(−γ)·Rx(−β)·ez"]
  W --> T["Trame de mouvement"]
  U --> T
  R["devicemotion + calibration des axes"] -. "repli après 1,5 s sans orientation" .-> T
```

### Embrayage

```mermaid
stateDiagram-v2
  [*] --> Libre
  Libre --> Vise: pouce posé (mode « gyro pouce »)
  Vise --> Libre: pouce levé
  Libre --> Vise: toujours (mode « gyro continu »)
  state "Trackpad" as T
  Libre --> T: mode « trackpad »
  Vise --> Vise: échantillon → déplacement
  note right of Libre: le filtre et l'accumulateur sont remis à zéro à chaque transition
```

## Réglages synchronisés

Le **serveur est la source de vérité**. Les modifications du téléphone sont des fonctions rejouées par-dessus la dernière version reçue : si deux téléphones changent deux réglages différents en même temps, les deux changements sont conservés.

```mermaid
sequenceDiagram
  participant A as Téléphone A
  participant S as Serveur
  participant B as Téléphone B
  participant D as settings.json

  A->>A: edit(invert_y) — appliqué tout de suite
  B->>B: edit(zoom ×4) — appliqué tout de suite
  A->>S: update_settings(base_rev 0, A)
  B->>S: update_settings(base_rev 0, B)
  S->>D: rev 1 enregistrée
  S-->>A: settings rev 1
  S-->>B: error settings_conflict + settings rev 1
  Note over B: rejoue « zoom ×4 » sur rev 1
  B->>S: update_settings(base_rev 1, A+B)
  S->>D: rev 2 enregistrée
  S-->>A: settings rev 2
  S-->>B: settings rev 2
```

Les réglages sont validés et ramenés dans des plages sûres côté serveur (`Settings::sanitized`) : valeurs non finies remplacées par les valeurs par défaut.

## Protocole

| Direction | Canal | Contenu |
|---|---|---|
| téléphone → PC | JSON | `hello`, `pair`, `button`, `key_tap`, `key`, `modifier`, `text`, `clutch`, `recenter`, `magnifier`, `update_settings`, `ping` |
| téléphone → PC | binaire | `0x01` mouvement (7 × f32), `0x02` trackpad (2 × f32), `0x03` défilement (2 × f32), `0x04` accélération linéaire pour l'option « déplacements du téléphone » (4 × f32), little-endian |
| PC → téléphone | JSON | `paired`, `welcome`, `settings`, `magnifier_state`, `error`, `pong` |

Garanties de sécurité côté serveur :

- aucune commande n'est exécutée avant `hello` ou `pair` réussi ;
- un clic, une touche ou un modificateur maintenu est **relâché** quand le téléphone se déconnecte ;
- les doubles « appui » sont ignorés (seules les vraies transitions partent) ;
- taille de message limitée à 64 Ko, trames binaires invalides ignorées.

## Loupe intelligente

```mermaid
flowchart TD
  O["Observation à chaque image<br/>écran · curseur · caret"] --> F{"Mode"}
  F -- curseur --> PC1["point = curseur"]
  F -- saisie --> PC2["point = caret"]
  F -- intelligent --> T{"caret a bougé<br/>depuis moins de 1,5 s ?"}
  T -- oui, on tape --> PC2
  T -- non --> PC1
  PC1 --> V["Zone visible<br/>suit seulement près des bords (marge 22 %)"]
  PC2 --> V
  V --> Z["Zoom animé vers la cible<br/>entrée et sortie douces"]
  Z --> MG["MagSetFullscreenTransform"]
```

Limites connues : le caret n'est lisible que pour les applications Win32 classiques ; navigateurs et applications Electron retombent sur le curseur (UI Automation est prévue pour couvrir ces cas). La loupe agit sur l'écran principal.

## Choix techniques

| Sujet | Choix | Pourquoi |
|---|---|---|
| Client | PWA, Vite, Solid, TypeScript | iPhone sans compilation native, multiplateforme, 42 Ko de JS |
| Serveur | Rust, tokio, axum, rustls (ring) | binaire unique, faible latence, pas de dépendance C à compiler |
| Types partagés | `ts-rs` | une seule définition du protocole, vérifiée par la CI |
| Découverte | mDNS (`mdns-sd`) + QR | un navigateur ne peut pas scanner le réseau ; le nom `.local` reste stable si l'IP change |
| Injection | `SendInput` (crate `windows`) | API officielle, texte en Unicode (accents, emoji) |
| Tests | `cargo test`, Vitest, Playwright | cœur et sessions sans OS, client avec faux WebSocket |
