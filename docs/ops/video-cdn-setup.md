# Video CDN Setup — Proxmox + Caddy + Cloudflare Tunnel

Ce guide décrit comment héberger les loops vidéo VJ sur ton Proxmox
et les exposer via ton Cloudflare Tunnel existant.

## Prérequis

- Proxmox avec un Cloudflare Tunnel déjà configuré (cloudflared installé).
- Un sous-domaine disponible (ex: `loops.kushie.dev`).
- Le pack généré dans `cdn-video-loops/` (via `pnpm video-loops:build`).

---

## 1. Créer un conteneur LXC sur Proxmox

Depuis l'interface Proxmox :

- OS : Debian 12 (Bookworm) minimal
- Ressources : 512 Mo RAM, 10 Go disque
- Réseau : bridge `vmbr0`, IP statique (ex: `192.168.1.50`)

Installer Caddy dans le LXC :

```bash
apt-get install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt-get update && apt-get install caddy
```

Créer le répertoire de contenu :

```bash
mkdir -p /srv/video-loops
chown caddy:caddy /srv/video-loops
```

## 2. Configurer Caddy

Remplacer `/etc/caddy/Caddyfile` :

```
:8080 {
    root * /srv/video-loops
    file_server browse
    header Access-Control-Allow-Origin "*"
    header Cache-Control "public, max-age=86400, immutable"
    @opt method OPTIONS
    respond @opt 204
}
```

Redémarrer Caddy :

```bash
systemctl restart caddy
systemctl enable caddy
```

Vérifier en local :

```bash
curl -I http://localhost:8080/manifest.json
# Attendu : HTTP/1.1 200 OK + access-control-allow-origin: *
```

## 3. Router le Cloudflare Tunnel vers le LXC

Sur la machine qui exécute `cloudflared`, éditer la config du tunnel
(ex: `~/.cloudflared/config.yml` ou `/etc/cloudflared/config.yml`) :

```yaml
tunnel: <ton-tunnel-id>
credentials-file: /root/.cloudflared/<ton-tunnel-id>.json

ingress:
  - hostname: loops.kushie.dev
    service: http://192.168.1.50:8080
  - service: http_status:404
```

Ajouter l'entrée DNS dans Cloudflare :

```bash
cloudflared tunnel route dns <ton-tunnel-id> loops.kushie.dev
```

Redémarrer cloudflared :

```bash
systemctl restart cloudflared
```

Vérifier depuis l'extérieur :

```bash
curl -I https://loops.kushie.dev/manifest.json
# Attendu : HTTP/2 200 + access-control-allow-origin: *
```

## 4. Uploader le pack

Depuis la machine de développement, après `pnpm video-loops:build` :

```bash
rsync -av --delete cdn-video-loops/ root@192.168.1.50:/srv/video-loops/
```

Vérifier qu'un clip se charge :

```bash
curl -o /dev/null -s -w "%{http_code}" https://loops.kushie.dev/neon-city-night-01.webm
# Attendu : 200
```

## 5. Configurer l'app OpenDrop

Créer `.env` à partir de `.env.example` :

```bash
cp .env.example .env
```

Éditer `.env` :

```
PUBLIC_VIDEO_CDN=https://loops.kushie.dev
```

Rebuilder l'app :

```bash
pnpm build
# ou pour Electron dev :
pnpm electron:dev
```

Les 50+ clips apparaissent maintenant dans la section Vidéo avec le badge 📦.

## 6. Mise à jour du pack

Pour ajouter de nouveaux clips ou régénérer :

```bash
pnpm video-loops:build            # re-télécharge + retranscode
rsync -av --delete cdn-video-loops/ root@192.168.1.50:/srv/video-loops/
git add static/video-loops/       # committer le set bundlé mis à jour
git commit -m "chore: update bundled video loops"
```

## Troubleshooting

| Symptôme                         | Cause probable                              | Solution                                                       |
| -------------------------------- | ------------------------------------------- | -------------------------------------------------------------- |
| Clips CDN absents, bundlés OK    | CDN inaccessible ou `PUBLIC_VIDEO_CDN` vide | `curl -I https://loops.kushie.dev/manifest.json`               |
| CORS error dans la console       | Header absent sur Caddy                     | Vérifier `/etc/caddy/Caddyfile` + `systemctl restart caddy`    |
| Vidéo se charge mais ne joue pas | Format non supporté par Chromium/Electron   | Vérifier que ffmpeg a bien produit du VP9                      |
| `pnpm video-loops:build` échoue  | ffmpeg absent ou clé API invalide           | `which ffmpeg` ; vérifier `PEXELS_API_KEY` / `PIXABAY_API_KEY` |
