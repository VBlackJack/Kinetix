# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-10

First public release.

### Added
- Deterministic brick breaker engine in Rust with sub-stepped collisions, power-ups, bosses and sudden death.
- 800-sector campaign across 8 worlds, with boss arenas at sectors 50 and 100 of each world.
- 8 visual renderers, switchable at any time with `F1`.
- Main menu driven by mouse, keyboard or touch: campaign chapters and sectors, options, controls, and a pause menu.
- Satisfaction mode: relaxed boards with no sudden death and no saved progress, including three mini-brick boards of about 1,500 bricks each.
- Optional `cell` field in level definitions for fine brick grids (row height and gap).
- Level editor with share codes, and a saved level library of up to 50 levels.
- English and French interface.
- Saved progress, settings and level library in the browser on the web and in a data folder on desktop.
- Web (WebAssembly) and Windows builds.

### Fixed
- Web startup crash caused by a glyph atlas overflow in macroquad 0.4.16, fixed with the upstream patch in `vendor/macroquad`.
