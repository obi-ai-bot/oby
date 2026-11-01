# Oby's icon

Every Oby icon is Sangam's 2D Oby (2026-10-27): his head, white with its eyes cut out and its ears up (`oby-head.png`), rising from the icon's bottom edge into a dark that runs from #585858 at the top to black at the bottom, 89% of the icon wide, its ears' tips 29% of the way down.

```
oby-head.png     the head, the one source
AppIcon.icon/    Icon Composer document: iPhone, iPad, Mac and Apple Watch
exports/         everything else, from the same head
  oby-1024.png   the Electron app's Dock icon while developing: AppIcon.icon as macOS renders it (actool)
  oby-foreground-1024.png   Oby alone, on nothing
  android/       adaptive icon layers (Android and Wear OS) and the Play Store icon
  desktop/       Windows (oby.ico) and Linux (16–512 PNGs)
  web/           favicons and the home-screen icon
compose.swift    turns the head into all of the above
render.mjs       renders the 3D character (the icon's old source, no longer used for it)
```

## The Apple icon

`AppIcon.icon` has one layer, Oby rising from the bottom, on a fill from #585858 to black. The Mac and iPhone targets use it as `AppIcon`; the Watch app will too; the Electron app's packaging too (`apps/electron/electron-builder.yml`). The system draws the shape, Liquid Glass, and the dark, tinted and clear looks from it. Open it in Icon Composer (Xcode › Open Developer Tool) to preview every platform and appearance.

## Make the icons again

When Sangam changes the head, replace `oby-head.png` and rebuild everything:

```bash
swift apps/icon/compose.swift apps/icon/oby-head.png apps/icon
```
