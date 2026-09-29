# N3 logo

`n3-logo-01-crisp.svg` is the editable source used by the interface.
`N3.icns` is the macOS app icon generated from that vector source. After changing
the logo, regenerate the icon on a Mac with `just tools icon`, then review it at small
and large sizes. `just build` copies the checked-in icon into `build/N3.app` and
does not require Swift or icon generation during every build.
