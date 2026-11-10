# KINETIX

> [Read in English](README.md)

**KINETIX** est un casse-briques moderne haute vélocité écrit en Rust, avec une physique déterministe, une campagne de 800 secteurs répartis sur 8 mondes, des moteurs de rendu commutables à chaud et une architecture multi-plateforme (WebAssembly, Windows, Linux, Android).

## Architecture

- [`crates/kinetix-core`](crates/kinetix-core) : Moteur physique déterministe en Rust pur, détection de collision par sous-étapes, synergies de bonus, configuration JSON et catalogue i18n (`en` / `fr`).
- [`crates/kinetix-app`](crates/kinetix-app) : Client graphique multi-plateforme accéléré matériellement (`Macroquad` WebGL / Desktop / Android), thèmes visuels commutables en temps réel, systèmes de particules et d'ondes de choc.
- [`crates/kinetix-levelforge`](crates/kinetix-levelforge) : Générateur de campagne sans interface, validateur structurel et simulateur de bot déterministe (profils `good` et `casual`).
- [`assets/`](assets) : Configuration externalisée (`game_config.json`, `themes.json`, `levels.json`, `satisfaction.json`, `satisfaction_fine.json`) et catalogues de traduction (`locales/en.json`, `locales/fr.json`).

## Mondes de la campagne (800 secteurs)

La campagne couvre 800 secteurs répartis sur 8 mondes de 100 secteurs chacun, avec un boss intermédiaire au secteur 50 de chaque monde et un boss de fin de monde au secteur 100 de chaque monde :

1. **Monde 1 : Grille d'Ignition** (Secteurs 1-100) - inclut les 15 secteurs historiques du prologue suivis des secteurs thermiques générés.
2. **Monde 2 : Distorsion Quantique** (Secteurs 101-200) - silhouettes fantômes (`G`) et couloirs de phase.
3. **Monde 3 : Forge Omega** (Secteurs 201-300) - tapis roulants mobiles (`M`) et bastions d'acier (`#`).
4. **Monde 4 : Matrice Cryo** (Secteurs 301-400) - raccourcis de portails appairés (`T`) et réseaux givrés.
5. **Monde 5 : Citadelle Solaire** (Secteurs 401-500) - baies de propulseurs (`B`) et chaînes explosives (`E`).
6. **Monde 6 : Labyrinthe Prisme** (Secteurs 501-600) - boucliers cristallins régénérants (`R`) et facettes prismatiques.
7. **Monde 7 : Horizon du Vide** (Secteurs 601-700) - synthèse multi-mécaniques dans des arènes asymétriques.
8. **Monde 8 : Trône de Singularité** (Secteurs 701-800) - épreuve finale culminant face à `APEX ARCHON [FINAL BOSS]` au secteur 800.

Les secteurs générés sont composés, pas éparpillés. Chacun est un motif dessiné à la main (48 icônes, glyphes, mandalas, damiers, diagonales, flèches, escaliers et spirales) portant une mécanique entière : flancs d'acier, cages explosives, mèches en dominos, bandes fantômes, voies de tapis roulant, paires de portails, propulseurs ou boucliers régénérants. Environ 40 % des secteurs ordinaires sont symétriques par construction, les autres sont des décalages asymétriques structurés, et le dernier secteur de chaque bloc qui ne se termine pas sur un boss est une pièce maîtresse. Les mécaniques débutent dans l'ordre (briques fantômes au monde 2, briques mobiles au 3, portails au 4, propulseurs au 5, briques régénérantes au 6). `levelforge simulate` vérifie que le taux de réussite du bot expert baisse d'environ 48 % au monde 1 à environ 37 % au monde 8.

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

## Menu et modes de jeu

Le jeu s'ouvre sur un menu utilisable à la souris, au clavier ou au toucher :

- **Campagne** : choisir un chapitre (monde), puis n'importe quel secteur débloqué. La progression et le meilleur score sont sauvegardés.
- **Mode satisfaction** : des plateaux détente, sans mort subite et sans progression sauvegardée. Cinq plateaux à chambres d'acier sur la grille habituelle de 15 colonnes, puis trois plateaux à mini-briques (`RINGED PLANET`, `TIDE`, `SPIRAL`) : environ 1 500 petites briques qui dessinent une image dans un cadre d'acier à une seule entrée, pour que la balle y entre et vide le champ de l'intérieur.
- **Éditeur** : peindre un plateau de 15 x 10, le tester, l'exporter en code de partage, ou appuyer sur `S` pour l'enregistrer dans **Mes niveaux** (jusqu'à 50 niveaux), d'où il peut être rejoué ou supprimé.
- **Options** : langue, rendu, effets sonores et musique, tous mémorisés.
- **Pause** (`P` ou `Échap`) : reprendre, recommencer, options ou retour au menu.

### Niveaux à grille fine

Une définition de niveau peut porter un objet `cell` facultatif, par exemple `"cell": { "h": 13, "gap": 2 }`, qui fixe la hauteur des rangées et l'écart entre les briques en pixels. Le nombre de colonnes vient toujours de la longueur des rangées. Sans `cell`, un niveau garde les rangées de 26 px et les écarts de 5 px d'origine : la campagne ne change pas. Les plateaux fins n'acceptent que des cases vides (`0`), des briques de couleur (`1`-`9`) et de l'acier (`#`), et sont vérifiés par `validate_fine_level` dans les tests. L'éditeur et les codes de partage restent sur la grille de 15 x 10.

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

# Valider et simuler le pack de niveaux de la campagne
cargo run --release -p kinetix-levelforge -- validate
cargo run --release -p kinetix-levelforge -- simulate

# Compiler et servir la version WebAssembly sur http://localhost:8080
.\scripts\Build-Web.ps1 -Serve
```

## Son, sauvegarde et migration de la campagne

Le son n'est disponible que sur le web pour l'instant : les versions natives sont silencieuses. La progression (meilleur score, secteurs débloqués, thème, langue, réglages du son) est sauvegardée sur toutes les plateformes : dans le stockage du navigateur sur le web (encodage du secteur débloqué sur 16 bits), et dans un fichier `progress.json` sur les versions natives :

- Windows : `%APPDATA%\Kinetix\progress.json`
- macOS : `$HOME/Library/Application Support/Kinetix/progress.json`
- Linux et autres : `$XDG_DATA_HOME/kinetix/progress.json`, ou `$HOME/.local/share/kinetix/progress.json` si `XDG_DATA_HOME` n'est pas défini

Un fichier absent repart des valeurs par défaut. Un fichier illisible (corrompu, ou écrit par une version plus récente) est conservé sous le nom `progress.json.bak` avant la première sauvegarde de la session, et le jeu repart des valeurs par défaut. Si aucun dossier de données utilisateur n'est trouvé, la progression n'est pas sauvegardée. Supprimez le fichier pour réinitialiser votre progression.

Les niveaux enregistrés depuis l'éditeur sont rangés à côté, dans `library.json` sur les versions natives (avec la même règle `.bak`), et dans une clé du stockage du navigateur sur le web.

**Compatibilité des sauvegardes et recalibrage des boss historiques** : Les 15 secteurs d'origine (1-15) conservent exactement leurs grilles de briques et leurs drapeaux de boss au sein du **Monde 1 : Grille d'Ignition** (Secteurs 1-100). Les boss historiques des secteurs 5 (`IGNITION WARDEN`), 10 (`CHRONO ARCHON`) et 15 (`OMEGA EMPEROR`) sont recalibrés sur le palier du Monde 1 (`PV = 18`). Les sauvegardes existantes ayant débloqué un secteur entre 1 et 15 conservent ce même secteur débloqué dans le Monde 1 ; les Mondes 2 (Secteurs 101-200) et 3 (Secteurs 201-300) commençant désormais aux secteurs 101 et 201, les sauvegardes qui n'avaient atteint que les secteurs 6 à 15 verront les Mondes 2 et 3 verrouillés jusqu'à ce que les secteurs 101 et 201 soient atteints.

## Licences

- Code : Apache License 2.0 ([`LICENSE`](LICENSE)) - Copyright 2026 Julien Bombled.
- `vendor/macroquad` embarqué : macroquad 0.4.16 (MIT OR Apache-2.0) avec le correctif amont du débordement de son atlas de glyphes en WebAssembly, appliqué par `[patch.crates-io]` en attendant une version corrigée. Voir [`vendor/macroquad/PATCH.md`](vendor/macroquad/PATCH.md).
- Police de l'interface : Barlow, sous licence SIL Open Font License 1.1 ([`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
