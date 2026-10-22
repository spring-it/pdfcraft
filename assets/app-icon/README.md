# PdfCraft red PDF app icon

The earlier green animal/lion icon is replaced with the **user-provided red PDF document icon** for the dedicated `v0.10.2-wofei.1` custom release.

<img src="pdfcraft-small.svg" alt="PdfCraft red PDF document icon" width="128">

The supplied artwork is the red rounded-square PDF document icon uploaded by the requester. It is the canonical application icon for the 0.10.2 custom release. The existing master artwork and all platform variants use this same supplied icon; the transparent canvas and original artwork are retained without redesign.

## Platform resources

| Path | Used by |
|---|---|
| `pdfcraft-source.webp` | 512×512 optimized copy of the provided artwork for maintainers |
| `pdfcraft.svg`, `pdfcraft-small.svg` | SVG wrappers containing the supplied raster artwork; not the earlier animal drawing |
| `pdfcraft-1024.png`, `pdfcraft.icns` | macOS dock, window and app bundle icon |
| `pdfcraft.ico` | Windows executable resources, desktop and Start menu shortcuts, MSI Add/Remove Programs icon |
| `pdfcraft-document.svg`, `pdfcraft-document.ico` | The same red PDF artwork for Windows PDF document icon resource 2 |
| `hicolor/<size>x<size>/apps/ai.storyteller.pdfcraft.png` | Linux theme PNGs (16–512 px), Windows/Linux in-app runtime icon uses 256 px |
| `hicolor/scalable/apps/ai.storyteller.pdfcraft.svg` | Linux scalable icon |

The Windows executable embeds `pdfcraft.ico` and `pdfcraft-document.ico` in `apps/pdfcraft/build.rs`. The MSI uses the same red icon in its installed desktop shortcut, Start menu shortcut, and Add/Remove Programs entry. The application runtime uses the updated PNG in `apps/pdfcraft/src/main.rs`.

To regenerate the PNG and ICO assets from the embedded SVG source, use `packaging/icons.sh` with `resvg`. On macOS it additionally generates the ICNS. All generated assets and checksums in `ATTRIBUTION.toml` are checked in.

**Rights:** This customized artwork was supplied by the requester. See [LICENSE-source.txt](LICENSE-source.txt); no new open-source license is asserted for the supplied artwork.
