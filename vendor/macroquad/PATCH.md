# Local macroquad patch

This directory is macroquad 0.4.16 as published on crates.io (MIT OR Apache-2.0, see the
license files), without its `examples/` and `tests/` directories, `Cargo.toml.orig`,
`Cargo.lock` and `.github/`. The `[[example]]` and `[[test]]` targets were removed from
`Cargo.toml` to match, so its generated header still points to the dropped `Cargo.toml.orig`.
The doctests in `src/texture.rs` reference the dropped examples and are not meant to run here.
One file is replaced: `src/text/atlas.rs` is taken
from upstream `master`, which fixes issue #1054 but is not released yet.

## Why

The glyph atlas packer in 0.4.16 accepts a sprite in the current row when
`cursor_x + width < image.width`, but places it at `cursor_x + GAP`. A glyph that ends within
`GAP` pixels of the right edge then fails the bounds check and doubles the whole atlas instead
of starting a new row. Each regrow repacks the cache in `HashMap` order, which can hit the
same case again. With the glyph set Kinetix preloads, the atlas reached 32768 px, where
`width * height * 4` overflows the 32-bit `usize` of wasm32 and `Image::gen_image_color`
panics at startup.

The upstream fix corrects the row-fit check and repacks tallest-first in a deterministic
order. Upstream's unit tests for both bugs are part of the replaced file.

## Removing it

When a macroquad release after 0.4.16 contains the fix, delete this directory and the
`[patch.crates-io]` entry in the workspace `Cargo.toml`, then run the full gate.
