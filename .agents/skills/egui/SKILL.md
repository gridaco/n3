---
name: egui
description: Apply N3-specific egui layout, theme, icon, overlay, and guide-capture practices when editing its Rust UI. Not a general egui reference.
---

# N3 egui UI

Use this skill for changes to N3's egui panels, Preferences, theme, icons, or viewport overlays. Inspect the owning UI code, `src/theme.rs`, and the relevant executable guide scenario before editing.

- Start with egui's built-in widgets and light/dark styles. Keep the small shared overrides in `src/theme.rs`; avoid a parallel component or CSS-like system. Apply style changes when the resolved theme or accent changes, rather than rebuilding them every frame.
- Keep layout geometry distinct: a scrollable window's outer frame and scrollbar should reach the window content edge, while spacing around controls belongs inside the scroll area. Do not globally alter window margins to repair one dialog.
- Derive neutral overlay text and surfaces from the active theme. Preserve semantic error, selection, and X/Y/Z axis colors where their identity matters.
- Check egui layer order, clipping, and pointer ownership when changing viewport overlays. A readable overlay must not hide a menu or intercept an unrelated viewport gesture.
- Keep the bundled Lucide font in its dedicated named egui family and its codepoints in `src/ui/lucide.rs`. Icon-only controls still need semantic labels and shortcut tooltips; inspect the resulting guide captures for legibility after changing icons.
- For generated guide images, choose Light or Dark explicitly. Exercise real scroll/input routing and verify that illustrated controls are visible and reachable; a recorded control rectangle alone does not prove that.

Read [observed pitfalls](references/pitfalls.md) when adjusting scrollable dialogs, overlays, or executable UI documentation. Follow the repository's `AGENTS.md` for documentation generation and verification.
