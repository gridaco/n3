# N3 egui pitfalls

These are N3-specific checks learned while refining its Preferences and viewport appearance. Extend this file only when a real UI failure reveals a reusable lesson.

## Scrollable Preferences

`egui::Window` frame spacing and `egui::ScrollArea` content spacing affect different boundaries. If the scrollbar appears inside the control padding, inspect the window's inner margin before changing the scrollbar itself. Let the scroll area use the available content width and place control padding _inside_ its scrolling content. Keep any correction local to this window so other dialogs retain their intended margins. Inspect the top, a scrolled middle, and the bottom; the close and settings-file controls must remain reachable.

## Theme changes

N3's `System` preference resolves against the native window appearance; explicit Light and Dark override that resolution. Keep the persisted preference distinct from the resolved theme, and ensure an OS appearance event refreshes both egui style and viewport background. Start from `egui::Visuals::light()` and `dark()`, then change only N3's small shared palette, spacing, typography, borders, and radii. Use the active palette for neutral HUD and popup colors. Error feedback and X/Y/Z axes are semantic, so a neutral theme or user accent should not recolor them indiscriminately.

## Viewport overlays

N3 mixes a wgpu scene with egui panels, `Area`s, and painter layers. A shape painted on the background layer may be covered by a panel or later layer; a foreground area can affect hit testing even if it looks transparent. Inspect layer order, clip rectangles, and pointer routing before changing an overlay's order. Check the same overlay against both light and dark viewport backgrounds and while a menu or Preferences window is open. Keep documentation cursors and annotations visual-only.

## Button hover geometry

In egui 0.36, a `Button` is measured with its frame: `button_style` derives inner margin from button padding, expansion, and stroke width. Changing the stroke between idle and hover can therefore change the widget's measured size and shift neighboring controls. There is no CSS `box-sizing` switch. For fixed toolbars, keep stroke width constant across widget states or use fill-only hover feedback, then check both the button rectangles and their containing toolbar rectangle while hovering each item. `Button::min_size` sets a minimum, not an exact size, and its text may remain left-aligned within the extra space. For an icon-only square button, check the painted glyph as well as the response rectangle; center the glyph explicitly when needed. `Button::fill` and `Button::stroke` override their respective hover effects; set state-specific `WidgetVisuals` when the control needs a different hover fill.

## Executable guide captures

The app defaults to `System`, but guide pixels must not depend on the capture host's current appearance. Set an explicit theme in each theme-specific scenario and review both outputs. Control tracing can record a rectangle for a widget below the visible scroll region; scroll the real UI before interacting with or illustrating it, then check the resulting image. A successful replay or byte match does not establish legibility, scrollbar placement, or reachable controls.

Menu controls can enter the trace before their fade-in is visible. `Session::settle` resolves layout without advancing time; it cannot finish that animation. Advance the explicit scenario clock with `Session::wait` before capturing an opened menu, and inspect the resulting image for full opacity. Do not use wall-clock sleeps or disable application animations just for the guide.
