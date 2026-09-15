# JetBrainsMono-Subset.ttf

Derived from `public/fonts/jetbrains-mono-latin.woff2` (this repository), which is
JetBrains Mono, Copyright 2020 The JetBrains Mono Project Authors
(https://github.com/JetBrains/JetBrainsMono), licensed under the SIL Open Font
License, Version 1.1 — see `public/fonts/LICENSES.md` for the full license text.

This file is a printable-ASCII-only subset (U+0020-U+007E), produced with
`fonttools` from the already-vendored webfont:

    woff2_decompress jetbrains-mono-latin.woff2
    python3 -c "from fontTools import ttLib; from fontTools.subset import Subsetter, Options; ..."

Monospace, not proportional: chosen deliberately so a KeyFace's character-count
budget (≤8 chars primary / ≤11 chars secondary at 64x64) is a fixed-width layout
computation, not a font-metrics guess. Subsetting keeps this crate's one new
binary asset small (~15 KB) and — combined with never touching a system font —
keeps rendering deterministic, which is what makes the KeyFace content-hash
cache and the render snapshot tests valid.
