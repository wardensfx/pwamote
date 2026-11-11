# Contribuer

## Messages de commit

Le changelog de chaque release est généré à partir des messages de commit ([Conventional Commits](https://www.conventionalcommits.org/fr/v1.0.0/), configuration dans [`cliff.toml`](cliff.toml)). On écrit en français, au présent, une ligne qui décrit l'effet visible :

```
<type>(<portée facultative>): <description>
```

| Type | Rubrique du changelog | Exemple |
|---|---|---|
| `feat` | ✨ Nouveautés | `feat(client): scan du QR code dans l'app installée` |
| `fix` | 🐛 Corrections | `fix(server): certificat local stable d'un lancement à l'autre` |
| `perf` | ⚡ Performances | `perf(core): filtre du gyroscope sans allocation` |
| `docs` | 📝 Documentation | `docs: schéma du flux de release` |
| `refactor`, `style`, `test`, `build`, `ci`, `chore` | 🔧 Maintenance | `ci: release sur tag de version` |

- Portées courantes : `core`, `server`, `client`, `desktop`, `desktop-ui`, `relay`.
- Un changement incompatible (protocole, réglages, appairage à refaire) prend un `!` : `feat(core)!: nouveau format de réglages`, ou une ligne `BREAKING CHANGE: …` dans le corps. Il apparaît en tête des notes.
- Un commit sans type reste listé, sous « Divers ».
- L'aperçu de la prochaine release est visible dans le résumé de chaque run de CI sur `main` (job `changelog`), ou en local : `npx git-cliff --unreleased`.

## Publier une release

```bash
git tag v1.2.3          # SemVer ; v1.2.3-beta.1 publie une pré-release
git push origin v1.2.3
```

Le workflow [`release.yml`](.github/workflows/release.yml) relance toute la CI avec cette version injectée (Cargo, Tauri, PWA), puis crée la release GitHub avec l'installeur `PwaMote_1.2.3_x64-setup.exe`, la version sans installation `PwaMote_1.2.3_x64_portable.exe`, `SHA256SUMS.txt` et les notes générées depuis le tag précédent. Les numéros de version restent à `0.1.0` dans le dépôt : seul le tag fait foi. Détails dans [l'architecture](docs/architecture.md#releases). Seuls ces binaires signalent les nouvelles versions dans la fenêtre ; une compilation locale ne contacte jamais GitHub.

Les exécutables sont signés (Authenticode, Azure Artifact Signing) dès que les variables et secrets décrits dans [la signature Windows](docs/signature-windows.md) sont configurés ; sinon ils sortent non signés.
