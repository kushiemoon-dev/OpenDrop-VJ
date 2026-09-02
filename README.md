# OpenDrop-Native

Native Rust rewrite of OpenDrop-VJ (see `REQUIREMENTS.md`/`PLAN.md`).

## Prérequis de build : NDI SDK

Compiler ce workspace (`cargo build --workspace`, y compris juste `io` ou
`app`, qui dépend de `io`) nécessite le NDI SDK (headers + libs) présent au
moment du build, pas seulement à l'exécution : `grafton-ndi` (Task 9)
utilise `bindgen` dans son `build.rs`, ce qui en fait une dépendance de
build à part entière, pas un simple `dlopen` runtime comme envisagé au
départ.

Deux fichiers versionnés font le pont avec le packaging Arch de cette
machine : `ndi-sdk-shim/` (symlinks `include`/`lib/x86_64-linux-gnu` vers
les emplacements système du paquet pacman `ndi-sdk`) et `.cargo/config.toml`
(positionne `NDI_SDK_DIR` vers `ndi-sdk-shim` quand le shell ne l'exporte
pas déjà lui-même). Voir le commentaire en tête de `.cargo/config.toml`
pour le détail du contournement.

Sur une autre machine ou avec un autre layout SDK (installeur NewTek
standard, autre distro) : soit exporter son propre `NDI_SDK_DIR` avant de
builder, soit remplacer/supprimer `ndi-sdk-shim/` et l'entrée `[env]` de
`.cargo/config.toml` en conséquence. Le SDK lui-même se télécharge sur
ndi.video.

## Limitation connue : découverte NDI

La découverte réseau NDI (énumération des sources NDI publiées sur le réseau
local dans l'interface utilisateur) dépend d'un daemon Avahi actif sur la
machine hôte. L'AppImage distribué ne peut pas embarquer un daemon système ;
cette limitation est donc acceptée et documentée en Phase 6 plutôt que
corrigée. L'application fonctionnera normalement en son absence, mais
l'énumération automatique des sources NDI ne sera pas disponible; l'accès
direct par URL ou adresse IP restera opérationnel.

*Note :* le prérequis NDI SDK + `libprojectm` dev (décrit ci-dessus) s'applique
à **toute machine de build**, y compris Windows (voir Step 12-13 du plan pour
les instructions spécifiques à chaque plateforme).

## Ableton Link (optionnel, GPL)

Le support Ableton Link (`io::link` / panneau Link) est désactivé par
défaut : il n'est ni compilé, ni lié dans le binaire produit par un
`cargo build` standard.

Raison : ce support repose sur `rusty_link`, un binding Rust vers la
bibliothèque C++ officielle d'Ableton Link, distribuée sous
**GPL-2.0-or-later**. Contrairement à la LGPL, la GPL n'a pas de clause
de lien dynamique permissive: lier `rusty_link`, statique ou dynamique,
oblige (lecture FSF classique) l'ensemble du binaire résultant à devenir
GPL-2.0-or-later. Voir `PLAN.md`, Risque 5, pour l'analyse complète.

Pour l'activer explicitement :

```sh
cargo build --features opendrop-app/link
```

(`opendrop-app` est le nom du paquet du crate binaire, déclaré dans
`app/Cargo.toml`: pas le nom de son répertoire `app/`.)

Un binaire compilé avec cette feature doit être traité comme
**GPL-2.0-or-later dans son ensemble**, et non plus comme le projet
principal (licence par défaut à préciser séparément). En conséquence, il
doit rester **absent de tout binaire empaqueté ou distribué par défaut**
(voir la Phase 6 du plan): la feature `link` n'est destinée qu'à des
builds locaux/optionnels assumant explicitement cette contamination de
licence.

## `ffmpeg` (dépendance runtime)

Deux fonctionnalités passent par un sous-processus `ffmpeg`, qui doit donc
être présent dans le `PATH` à l'exécution (rien n'est lié au build) :

- **sortie v4l2loopback** (`io::v4l2loopback`): le compositeur écrit ses
  frames RGBA dans un device v4l2loopback ;
- **panneau Video** (`io::video_capture`): décodage des clips locaux et
  capture caméra, dans l'autre sens : `ffmpeg` écrit des frames RGBA brutes
  sur son stdout, l'application les téléverse en texture GL.

En l'absence de `ffmpeg`, ces deux panneaux affichent une erreur et le
reste de l'application fonctionne normalement. Les clips vidéo eux-mêmes
ne sont pas fournis : voir `app/assets/video-loops/README.md`.

## Sélecteur de fichier natif (`rfd`)

Les panneaux CloudPresets (`ui::cloud_presets`, bouton Upload) et Video
(`ui::video`, bouton « + Video ») utilisent `rfd` pour ouvrir un sélecteur
de fichier natif. Sur Linux, le backend par défaut
de `rfd` (features `xdg-portal` + `async-std`, pas `gtk3`) passe par
xdg-desktop-portal via D-Bus (`ashpd`): aucune bibliothèque GTK3 requise au
build ni au lien. À l'exécution, ce backend a en revanche besoin d'un
service `xdg-desktop-portal` (+ son implémentation de portail, ex.
`xdg-desktop-portal-gtk`/`-kde`/`-hyprland`) actif sur la session ; en son
absence, le bouton Upload échoue silencieusement à ouvrir un sélecteur
plutôt que de faire échouer le build.
