# Calculator for inkpad

An iPhone-style calculator for the Kindle that writes the whole expression as you type, like the Windows calculator search does: `11+3+(4×5.5)`, with the result updating live underneath (`= 36`).

- **( )** is one key. It opens a parenthesis where a number could start and closes one after a number. Any `)` you forget is added when you press **=**.
- **⌫** deletes the last character. Hold it, or tap **AC** (shown after a result), to clear.
- After **=**, an operator carries on from the result (`36×2`), and a digit starts a new calculation.
- **%** works the usual calculator way: `50+10%` is 55, and `50×10%` or `10%` just divide by 100.
- `2(3+1)` and `(1+1)(2+2)` multiply. `5×−3` is fine.
- **Tap the last calculation** (the small gray line) to bring it back and edit it.
- **Tap the expression** to type a calculation on the keyboard.

It's one of [inkpad](../..)'s apps: a separate program in the inkpad workspace, installed into inkpad like an app onto a phone.

## Layout

```
app.json         manifest (id, name, icon)
src/main.rs      the UI: display and keypad
src/expr.rs      typing rules, parser and number formatting (with tests)
scripts/dev.ps1  run in the simulator, install
```

## Develop

From this folder (`apps/calculator`):

```powershell
powershell -File scripts\dev.ps1    # simulator (mouse = finger)
cargo test -p calculator
```

## Install

```powershell
powershell -File scripts\dev.ps1 -Install              # simulator home screen
powershell -File scripts\dev.ps1 -Install -Kindle E:   # Kindle (inkpad/apps/calculator)
```

`kindle\deploy.ps1` (and `deploy-all.ps1`) at the root installs it along with the other apps.

## License

MIT or Apache-2.0, at your option, like the rest of inkpad.
