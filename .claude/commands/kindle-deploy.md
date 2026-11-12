---
description: Build Inkpad and deploy it to a USB-mounted Kindle, then walk through the on-device checklist
argument-hint: "[drive letter, default E:] [--no-build]"
---

Deploy Inkpad to the Kindle and verify it on the real panel.

## 1. Pre-flight
- Confirm the Kindle is in USB drive mode: `$Drive\documents` must exist (default drive `E:`; use `$ARGUMENTS` if a drive is given).
- If it is missing, stop and ask the user to connect the Kindle. Do not guess another drive.

## 2. Deploy
From the repo root run:

```
powershell -File kindle\deploy.ps1 [-Drive E:] [-NoBuild]
```

The script builds `gallery`, `gfxdemo` and `touchtest` for `armv7-unknown-linux-musleabi`, then copies the binaries, launcher (`inkpad.sh`, LF endings), shortcuts, images and local fonts (e.g. Bookerly) to the Kindle. Pass `-NoBuild` if `--no-build` was requested.

Report build/copy failures with the actual output. Do not retry blindly.

## 3. Hand off to the user
Tell the user to eject the Kindle and open **"Inkpad gallery"**, then check:

- [ ] The clock shows local time (the one thing the simulator can't prove; uses our own small clock/timezone module, since the date/time crate needs a tool the Rust toolchain here lacks).
- [ ] Bookerly and the icons look right on the real panel.
- [ ] A tapped item flashes quickly.
- [ ] Pages turn cleanly.

Then ask for the results and fix anything that failed.
