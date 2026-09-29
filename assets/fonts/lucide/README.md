# Lucide icon font

N3 embeds the user-supplied `lucide.ttf` as a dedicated egui icon font. It is
paired with the `lucide-static` 1.48.0 codepoint reference:

- Font: <https://unpkg.com/lucide-static@1.48.0/font/lucide.ttf>
- Codepoints: <https://unpkg.com/lucide-static@1.48.0/font/lucide.css>
- License: <https://unpkg.com/lucide-static@1.48.0/LICENSE> (copy in `LICENSE`)

SHA-256 of this `lucide.ttf`:
`eaf30a2a0d2ec0cfbf7ab69ed6dc6e39b7e5e2f0255c17856dcb0b73a15584bb`.

When replacing the font, keep the TTF and codepoint reference at the same
version. Update the glyph mapping in `src/ui/lucide.rs` and this provenance,
hash, and license as needed. Then run the UI tests and regenerate and review
the guide captures; glyph codepoints may change between releases.
