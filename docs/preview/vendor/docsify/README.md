# Docsify local preview runtime

Vendored from the official `docsify@5.0.0` npm package on 2026-09-29.
Only the browser runtime, core theme, and upstream MIT license are included.
No Node.js installation, package manager, CDN, or build step is required at runtime.

- [Upstream](https://github.com/docsifyjs/docsify/tree/v5.0.0)
- [Package archive](https://registry.npmjs.org/docsify/-/docsify-5.0.0.tgz)
- [Manual initialization](https://github.com/docsifyjs/docsify/blob/v5.0.0/docs/quickstart.md#manual-initialization)
- [License](LICENSE)

The archive was checked against the npm registry SHA-512 integrity value:

```text
sha512-F2LBvsG/RkW9VZs91FPceZpJBilSC9EYCid0n1YhnQb3OOw63dk70d1ITPR2RPwxmPPcWFQ+TOtpdImilLWyGw==
```

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `docsify.min.js` | 185,377 | `dd215e90bfb7ba05d88e65e8b050016ff58b84f4426a336e86510e3ef95b1da9` |
| `core.min.css` | 39,925 | `d14c45ade932b1120be91d17d1ddce13d73b7266b9cddec538bb3ab19d1ace8b` |
| `LICENSE` | 1,146 | `65a671d4b5bda281aac624bc6901d8a8c494c15dc3e329760e8652aa0d76519a` |

When updating, replace these files together from a pinned upstream package,
retain its license, refresh this record, and check guide navigation, images,
animation playback, and offline loading in the browser. N3's executable Rust
documentation pipeline remains the authoring and verification source; Docsify
only displays its generated Markdown and WebP output.
