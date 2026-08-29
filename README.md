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
