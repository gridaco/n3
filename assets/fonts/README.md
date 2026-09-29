# Interface fonts

- [Inter](inter/README.md): upright variable TTF for N3 interface text.
- [Lucide](lucide/README.md): dedicated icon font and centralized codepoints.

Each directory contains the font, license, and pinned provenance. Inter adds one
879,708-byte payload; regular and semibold weights share its bytes. egui's default
monospace and glyph fallbacks are retained.

TODO: Fonts used as design materials in documents are a separate future feature,
which should include OS-installed fonts. Defer its design and implementation;
the bundled interface fonts are not a document-font system.
