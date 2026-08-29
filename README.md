# OpenDrop-Native

Native Rust rewrite of OpenDrop-VJ (see `REQUIREMENTS.md`/`PLAN.md`).

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
