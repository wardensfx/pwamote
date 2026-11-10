# Service DNS PwaMote (`dns.pwamote.app`)

Worker Cloudflare qui donne à chaque PC un nom `<id>.pwamote.app` (12 caractères aléatoires), comme `plex.direct`. Le PC y publie son **adresse locale** et le défi DNS-01 de Let's Encrypt ; il demande lui-même son certificat. Le service ne voit ni clé privée ni trafic.

| Route | Rôle |
|---|---|
| `POST /v1/register` | inscription : `{ id, secret, domain }` (10 par heure et par adresse) |
| `PUT /v1/address` `{ ip }` | enregistrement A, **réseaux privés uniquement** (10/8, 172.16/12, 192.168/16, 100.64/10) |
| `PUT /v1/challenge` `{ value }` | TXT `_acme-challenge.<id>.pwamote.app` |
| `DELETE /v1/challenge` | retire le TXT |
| `GET /v1/health` | état |

Authentification : `Authorization: Bearer <id>.<secret>` ; seule l'empreinte SHA-256 du secret est conservée (KV). Un ménage hebdomadaire supprime les PC muets depuis 180 jours.

## Mise en service (une fois)

1. Cloudflare › **pwamote.app** › colonne de droite : copier l'**ID de zone** dans `wrangler.toml` (`ZONE_ID`).
2. Cloudflare › Mon profil › Jetons d'API › Créer : modèle « Modifier la zone DNS », zone **pwamote.app** uniquement.
3. Dans ce dossier :
   ```sh
   npm ci
   npx wrangler login
   npx wrangler kv namespace create DEVICES   # copier l'id dans wrangler.toml
   npx wrangler secret put CF_API_TOKEN       # coller le jeton de l'étape 2
   npx wrangler deploy
   ```
4. Vérifier : `https://dns.pwamote.app/v1/health` répond `{"ok":true}`.

Le serveur PwaMote utilise `https://dns.pwamote.app` par défaut (`PWAMOTE_RELAY_URL` pour un autre service).

## Limite Let's Encrypt

Let's Encrypt accepte 50 nouveaux certificats par semaine pour un même domaine enregistré (`pwamote.app`) ; les renouvellements ne comptent pas. Largement suffisant pour un usage personnel. Pour une diffusion large : inscrire `pwamote.app` sur la [Public Suffix List](https://publicsuffix.org/) (chaque PC devient alors son propre domaine) ou demander une hausse de limite à Let's Encrypt.
