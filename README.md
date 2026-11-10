# KINETIX

> [Lire en francais](README.fr.md)

**KINETIX** is a modern, high-velocity brick breaker written in Rust, with deterministic physics, an 800-sector campaign across 8 worlds, hot-swappable visual renderers, and multi-platform targets (WebAssembly, Windows, Linux, Android).

## Architecture

- [`crates/kinetix-core`](crates/kinetix-core): Pure Rust deterministic physics engine, sub-stepped collision detection, power-up synergies, JSON configuration, and i18n catalog (`en` / `fr`).
- [`crates/kinetix-app`](crates/kinetix-app): Cross-platform hardware-accelerated renderer (`Macroquad` WebGL / Desktop / Android), real-time switchable visual themes, particle & shockwave systems.
- [`crates/kinetix-levelforge`](crates/kinetix-levelforge): Headless campaign generator, structural validator, and deterministic bot simulator (`good` and `casual` presets).
- [`assets/`](assets): Externalized configuration (`game_config.json`, `themes.json`, `levels.json`, `satisfaction.json`, `satisfaction_fine.json`) and localization catalogs (`locales/en.json`, `locales/fr.json`).

## Campaign Worlds (800 Sectors)

The campaign spans 800 sectors across 8 worlds of 100 sectors each, with mid-world boss arenas at sector 50 of each world and world-finale boss arenas at sector 100 of each world:

1. **World 1: Ignition Grid** (Sectors 1-100) - includes the 15 legacy prologue sectors and generated thermal reactor sectors.
2. **World 2: Quantum Warp** (Sectors 101-200) - ghost silhouettes (`G`) and phase corridors.
3. **World 3: Omega Forge** (Sectors 201-300) - moving conveyor lanes (`M`) and steel anvil bastions (`#`).
4. **World 4: Cryo Matrix** (Sectors 301-400) - paired portal shortcuts (`T`) and frost lattices.
5. **World 5: Solar Citadel** (Sectors 401-500) - bumper pinball bays (`B`) and explosive caldera chains (`E`).
6. **World 6: Prism Labyrinth** (Sectors 501-600) - regenerating crystal shields (`R`) and prism facets.
7. **World 7: Void Horizon** (Sectors 601-700) - multi-mechanic synthesis across asymmetric void arenas.
8. **World 8: Singularity Throne** (Sectors 701-800) - endgame gauntlet culminating in `APEX ARCHON [FINAL BOSS]` at Sector 800.

Generated sectors are composed, not scattered. Each one is a hand-authored motif (48 icons, glyphs, mandalas, checkerboards, diagonals, arrows, staircases and spirals) carrying one whole mechanic feature: steel flanks, explosive cages, domino fuses, ghost bands, conveyor lanes, portal pairs, bumpers or regenerating shields. About 40% of the regular sectors are mirror-symmetric by construction, the others are structured asymmetric offsets, and the last sector of each block that does not end on a boss is a showcase set piece. Mechanics debut in order (ghost bricks in world 2, moving bricks in 3, portals in 4, bumpers in 5, regenerating bricks in 6). `levelforge simulate` checks that the good-bot clear rate falls from about 48% in world 1 to about 37% in world 8.

## Visual Renderers (Hot-Swappable via `F1`)

The game ships 8 renderers, cycled with `F1`:

1. **Cyber-Neon**: Synthwave glow.
2. **Industrial Forge**: Heavy metal.
3. **Classic 98**: Beveled arcade.
4. **Hologram Matrix**: Wireframe data.
5. **Crystal Prism**: Faceted gems.
6. **Solar Magma**: Volcanic core.
7. **Arctic Cryo**: Frosted glass.
8. **Synth Sunset**: Outrun horizon.

## Menu and Game Modes

The game opens on a menu driven by mouse, keyboard or touch:

- **Campaign**: pick a chapter (world), then any unlocked sector. Progress and the high score are saved.
- **Satisfaction mode**: relaxed boards with no sudden death and no saved progress. Five steel-chamber boards on the regular 15-column grid, then three mini-brick boards (`RINGED PLANET`, `TIDE`, `SPIRAL`): about 1,500 small bricks drawing a picture inside a steel frame with a single entrance, so the ball gets in and clears the field from the inside.
- **Editor**: paint a 15 x 10 board, test it, export it as a share code, or press `S` to save it into **My levels** (up to 50 levels), from where it can be replayed or deleted.
- **Options**: language, renderer, sound effects and music, all remembered.
- **Pause** (`P` or `Esc`): resume, restart, options or back to the menu.

### Fine-grid levels

A level definition may carry an optional `cell` object, for example `"cell": { "h": 13, "gap": 2 }`, which sets the row height and the gap between bricks in pixels. The column count still comes from the row length. Without `cell`, a level keeps the original 26 px rows and 5 px gaps, so the campaign is unchanged. Fine boards accept only empty cells (`0`), coloured bricks (`1`-`9`) and steel (`#`), and are checked by `validate_fine_level` in the test suite. The editor and share codes stay on the 15 x 10 grid.

## Prerequisites

- Rust stable.
- Windows: the MSVC toolchain (`rustup default stable-msvc`). The GNU toolchain fails to link natively.
- Web build: `rustup target add wasm32-unknown-unknown`.
- Python, only for `-Serve`.

## Quick Start (PowerShell)

```powershell
# Run native desktop version
cargo run -p kinetix-app

# Run test suite
cargo test

# Validate and simulate the campaign level pack
cargo run --release -p kinetix-levelforge -- validate
cargo run --release -p kinetix-levelforge -- simulate

# Build and serve WebAssembly version on http://localhost:8080
.\scripts\Build-Web.ps1 -Serve
```

## Sound, Save Data, and Campaign Migration

Sound is web-only for now: native builds are silent. Progress (high score, unlocked sectors, theme, language, sound settings) is saved on every platform: in the browser storage on the web (16-bit sector unlock packing), and in a `progress.json` file on native builds:

- Windows: `%APPDATA%\Kinetix\progress.json`
- macOS: `$HOME/Library/Application Support/Kinetix/progress.json`
- Linux and others: `$XDG_DATA_HOME/kinetix/progress.json`, or `$HOME/.local/share/kinetix/progress.json` when `XDG_DATA_HOME` is unset

A missing file starts from the defaults. A file that cannot be read (corrupt, or written by a newer version) is kept as `progress.json.bak` before the first save of the session, and the game starts from the defaults. If no user data directory can be resolved, progress is not saved. Delete the file to reset your progress.

Levels saved from the editor live next to it in `library.json` on native builds (with the same `.bak` rule), and in one browser storage key on the web.

**Save compatibility and legacy boss re-tiering**: The original 15 sectors (1-15) preserve their exact brick layouts and boss flags inside **World 1: Ignition Grid** (Sectors 1-100). Legacy bosses at sectors 5 (`IGNITION WARDEN`), 10 (`CHRONO ARCHON`), and 15 (`OMEGA EMPEROR`) are re-tiered to World 1 (`HP = 18`). Existing saves that had unlocked any sector in 1-15 keep that exact sector unlocked in World 1; Worlds 2 (Sectors 101-200) and 3 (Sectors 201-300) now start at sectors 101 and 201, so saves that had only reached sectors 6-15 will see Worlds 2 and 3 locked until sector 101 / 201 is reached.

## Licenses

- Code: Apache License 2.0 ([`LICENSE`](LICENSE)) - Copyright 2026 Julien Bombled.
- Bundled `vendor/macroquad`: macroquad 0.4.16 (MIT OR Apache-2.0) with the upstream fix for its glyph atlas overflow on WebAssembly, applied through `[patch.crates-io]` until a fixed release ships. See [`vendor/macroquad/PATCH.md`](vendor/macroquad/PATCH.md).
- UI font: Barlow, under the SIL Open Font License 1.1 ([`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
