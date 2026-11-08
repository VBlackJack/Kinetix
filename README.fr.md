# KINETIX

> [Read in English](README.md)

**KINETIX** est un casse-briques moderne haute vélocité écrit en Rust, conçu pour dépasser les références du genre (*DX-Ball 2*, *Shatter*, *Ricochet*) avec une physique déterministe, des moteurs de rendu commutables à chaud et une architecture multi-plateforme (WebAssembly, Windows, Linux, Android).

## Architecture

- [`crates/kinetix-core`](crates/kinetix-core) : Moteur physique déterministe en Rust pur, détection de collision par sous-étapes, synergies de bonus, configuration JSON et catalogue i18n (`en` / `fr`).
- [`crates/kinetix-app`](crates/kinetix-app) : Client graphique multi-plateforme accéléré matériellement (`Macroquad` WebGL / Desktop / Android), thèmes visuels commutables en temps réel, systèmes de particules et d'ondes de choc.
- [`assets/`](assets) : Configuration externalisée (`game_config.json`, `themes.json`, `levels.json`) et catalogues de traduction (`locales/en.json`, `locales/fr.json`).

## Moteurs de rendu visuel (commutables à chaud via `F1`)

Le jeu embarque 8 moteurs de rendu, parcourus avec `F1` :

1. **Cyber-Neon** : Lueur synthwave.
2. **Forge Industrielle** : Métal lourd.
3. **Classique 98** : Arcade biseautée.
4. **Matrice Hologramme** : Fil de fer.
5. **Prisme Cristallin** : Gemmes taillées.
6. **Magma Solaire** : Noyau volcanique.
7. **Cryo Arctique** : Verre givré.
8. **Soleil Synth** : Horizon outrun.

## Prérequis

- Rust stable.
- Windows : la chaîne d'outils MSVC (`rustup default stable-msvc`). La chaîne GNU échoue à l'édition de liens en natif.
- Version web : `rustup target add wasm32-unknown-unknown`.
- Python, uniquement pour `-Serve`.

## Démarrage rapide (PowerShell)

```powershell
# Lancer la version native Desktop
cargo run -p kinetix-app

# Lancer la suite de tests
cargo test

# Compiler et servir la version WebAssembly sur http://localhost:8080
.\scripts\Build-Web.ps1 -Serve
```

## Son et sauvegarde

Le son et la sauvegarde de la progression ne sont disponibles que sur le web pour l'instant. Les versions natives sont silencieuses et ne conservent pas la progression.

## Licences

- Code : Apache License 2.0 ([`LICENSE`](LICENSE)) - Copyright 2026 Julien Bombled.
- Police de l'interface : Barlow, sous licence SIL Open Font License 1.1 ([`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
