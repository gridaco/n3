# Inter interface font

N3's proportional interface text uses the unmodified upright variable TTF from
Inter **4.1**. Only this one Inter file is embedded: egui evaluates its `wght`
axis at 400 for regular text and 600 for headings and emphasis. Both registered
weights borrow the same bytes. No static-weight files, italic face, TTC, or web
font duplicates are bundled. The `opsz` axis retains its default 14.

- Font: <https://raw.githubusercontent.com/rsms/inter/v4.1/docs/font-files/InterVariable.ttf>
- Release: <https://github.com/rsms/inter/releases/tag/v4.1>
- License: SIL Open Font License 1.1, copied as [LICENSE.txt](LICENSE.txt)
- Size: 879,708 bytes (about 859 KiB)
- SHA-256: `4989b125924991b90d05b2d16e0e388c48f7d5bb8b30539bbf9c755278d0ccaf`

Registration and weight choices live in `src/ui/typography.rs`; the application
keeps egui's default monospace fonts and glyph fallback chains. Inter is not
added to the monospace family, and Lucide remains an independent icon family.
When updating the font, update this pinned provenance and checksum, run the font
regressions, and regenerate and visually review the guide through `just docs update`.
