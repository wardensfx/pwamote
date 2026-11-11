# Service DNS PwaMote (`dns.pwamote.app`)

Worker Cloudflare qui donne à chaque PC un nom `<id>.d.pwamote.app` (12 caractères aléatoires), comme `plex.direct`. Le PC y publie son **adresse locale** et le défi DNS-01 de Let's Encrypt ; il demande lui-même son certificat. Le service ne voit ni clé privée ni trafic.

| Route | Rôle |
|---|---|
| `POST /v1/register` | inscription : `{ id, secret, domain }` (10 par heure et par adresse) |
| `PUT /v1/address` `{ ip }` | enregistrement A (réponse : `{ domain, ip }`, le nom actuel du PC), **réseaux privés uniquement** (10/8, 172.16/12, 192.168/16, 100.64/10) |
| `PUT /v1/challenge` `{ value }` | TXT `_acme-challenge.<id>.d.pwamote.app` |
| `DELETE /v1/challenge` | retire le TXT |
| `GET /v1/health` | état |

Authentification : `Authorization: Bearer <id>.<secret>` ; seule l'empreinte SHA-256 du secret est conservée (KV). Un ménage hebdomadaire supprime les PC muets depuis 180 jours.

## Mise en service (une fois), depuis le tableau de bord Cloudflare

1. Cloudflare › My Profile › API Tokens › **Create Token** › Custom : zone **pwamote.app** uniquement, droits `Zone · Zone · Read` et `Zone · DNS · Edit`. Copier le jeton.
2. **Workers & Pages › Create › Import a repository** : choisir `wardensfx/pwamote`, branche `main`.
   - Nom du Worker : `pwamote-relay`
   - **Root directory : `relay`**
   - Build command : *(vide)* · Deploy command : `npx wrangler deploy`
3. Une fois créé : Worker › **Settings › Variables and Secrets › Add** : type **Secret**, nom `CF_API_TOKEN`, valeur = le jeton. Puis relancer le déploiement (Deployments › Retry).
4. Vérifier : `https://dns.pwamote.app/v1/health` répond `{"ok":true}`.

Le premier déploiement crée tout seul le stockage KV `DEVICES` et le domaine `dns.pwamote.app` (la zone est dans le même compte). Les déploiements suivants se font à chaque push sur `main`. Le secret n'est jamais dans le dépôt.

En ligne de commande, c'est équivalent : `npm ci && npx wrangler login && npx wrangler secret put CF_API_TOKEN && npx wrangler deploy`.

## Pourquoi `d.pwamote.app`

Les PC sont nommés sous `d.pwamote.app`, pas directement sous `pwamote.app`. Seul `d.pwamote.app` sera inscrit à la [Public Suffix List](https://publicsuffix.org/) : chaque PC y devient un site distinct pour les navigateurs (cookies et contrôles same-site isolés entre PC de personnes différentes), sans toucher au site `pwamote.app` ni aux services (`dns.`, `admin.`). Réglable avec la variable `DEVICE_SUFFIX` (par défaut `d.<zone>`).

Les PC inscrits avant ce changement (`<id>.pwamote.app`) gardent leur identifiant : à leur prochaine publication d'adresse, le service crée le nouveau nom, retire l'ancien et renvoie le nouveau nom, que l'application adopte (nouveau certificat, nouveau QR code).

## Limite Let's Encrypt

Let's Encrypt accepte 50 nouveaux certificats par semaine pour un même domaine enregistré (`pwamote.app`) ; les renouvellements ne comptent pas. Largement suffisant au début. Pour une diffusion plus large : formulaire de hausse de limite de Let's Encrypt. L'inscription de `d.pwamote.app` à la Public Suffix List donnera aussi à chaque PC sa propre limite, mais ce n'est pas son but (la PSL refuse les demandes faites pour contourner une limite).
