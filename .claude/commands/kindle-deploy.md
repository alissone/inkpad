---
description: Build Inkpad and its apps, deploy them to a USB-mounted Kindle, then walk through the on-device checklist
argument-hint: "[drive letter, default E:] [--no-build]"
---

Deploy Inkpad to the Kindle and verify it on the real panel.

## 1. Pre-flight
- Confirm the Kindle is in USB drive mode: `$Drive\documents` must exist (default drive `E:`; use `$ARGUMENTS` if a drive is given).
- If it is missing, stop and ask the user to connect the Kindle. Do not guess another drive.

## 2. Deploy
From the repo root run:

```
powershell -File kindle\deploy-all.ps1 [-Drive E:] [-NoBuild]
```

It runs `kindle\deploy.ps1`:
- builds `home` and every app in `apps/` and `tools/` for `armv7-unknown-linux-musleabi`;
- installs them into `inkpad/apps/<id>/`;
- copies the launcher (`inkpad.sh`, LF endings), the shortcuts, the images, wallpapers and CA certificates, and local fonts (e.g. Bookerly).

After that it installs any apps from other repositories: those given with `-Apps`, and those listed in the git-ignored `kindle\extra-apps.txt`. A missing folder is skipped with a note. Pass `-NoBuild` if `--no-build` was requested.

Report build or copy failures with the actual output. Do not retry blindly.

## 3. Hand off to the user
Tell the user to eject the Kindle and open **Inkpad** from the library, then check:

- [ ] Home shows the local time and the installed apps; the top bar comes back after leaving.
- [ ] Sleep/wake shows the chosen sleep screen; the PIN lock appears on wake if set.
- [ ] Library opens a book in KOReader and comes back to the Library afterwards.
- [ ] News: the refresh button switches Wi-Fi on, downloads, and stories read offline with pictures.
- [ ] Bookerly and the icons look right on the real panel; pages turn cleanly.

Then ask for the results and fix anything that failed.
