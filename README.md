# KINETIX

> [Lire en francais](README.fr.md)

**KINETIX** is a modern, high-velocity brick breaker written in Rust, designed to surpass classic arcade references (*DX-Ball 2*, *Shatter*, *Ricochet*) with deterministic physics, hot-swappable visual renderers, and multi-platform targets (WebAssembly, Windows, Linux, Android).

## Architecture

- [`crates/kinetix-core`](crates/kinetix-core): Pure Rust deterministic physics engine, sub-stepped collision detection, power-up synergies, JSON configuration, and i18n catalog (`en` / `fr`).
- [`crates/kinetix-app`](crates/kinetix-app): Cross-platform hardware-accelerated renderer (`Macroquad` WebGL / Desktop / Android), real-time switchable visual themes, particle & shockwave systems.
- [`assets/`](assets): Externalized configuration (`game_config.json`, `themes.json`, `levels.json`) and localization catalogs (`locales/en.json`, `locales/fr.json`).

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

# Build and serve WebAssembly version on http://localhost:8080
.\scripts\Build-Web.ps1 -Serve
```

## Sound and Save Data

Sound and progress saving are web-only for now. Native builds are silent and do not persist progress.

## Licenses

- Code: Apache License 2.0 ([`LICENSE`](LICENSE)) - Copyright 2026 Julien Bombled.
- UI font: Barlow, under the SIL Open Font License 1.1 ([`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
