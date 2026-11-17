# Library for inkpad

Your Kindle's books as a cover grid. Tap a book to read it; hold it for details.

- **Covers:** taken from the book itself (EPUB, MOBI, AZW3), or from the Kindle's own thumbnails for Amazon books (KFX). Books without a cover, like most PDFs, get a drawn one with the title and author.
- **Titles and authors:** read from the file. Calibre-style names (`Author/Title - Author.mobi`) and Amazon names (`Title_B089N98YDT_sample.kfx`) are cleaned up, and "Last, First" authors are shown as "First Last".
- **Three views:** grid, list, and folders (to browse `documents/` as it is). Sort by recent, title or author, and search by title or author.
- **Reading:**
  - Books open in **KOReader**. When you close KOReader you're back in the Library, on the same page.
  - **Amazon KFX and DRM books** open in the **Kindle's own reader**, the only one that can read them. That's experimental, and you return to the Kindle home afterwards.
- **Progress:** KOReader's reading progress shows as a bar under each cover and a percentage. "Recent" puts the books you're reading first.
- **Fast after the first start:** the Library remembers what it has already read (`inkpad/data/library/`), so later starts only look at new or changed files. The first start shows "Indexing 12 of 48…" and fills the shelf as it goes.

It's one of [inkpad](../..)'s apps: a separate program in the inkpad workspace, installed into inkpad like an app onto a phone.

## Layout

```
app.json        manifest: id, name, icon (home reads this)
src/main.rs     the UI
src/books.rs    finding books, the index cache, cover thumbnails, KOReader progress
src/meta.rs     metadata from EPUB (OPF), MOBI/AZW3 (EXTH) and file names
src/zip.rs      minimal ZIP reader for EPUBs
scripts/dev.ps1 run in the simulator, screenshots, install
```

## Develop

From this folder (`apps/library`):

```powershell
powershell -File scripts\dev.ps1 -Documents "D:\copy of the Kindle\documents"   # simulator window
powershell -File scripts\dev.ps1 -Shots                                          # screenshots\
cargo test -p library
```

Put test books in `sample-books/` (git-ignored), or point `-Documents` at a folder. A copy of a Kindle's `documents/` folder makes a realistic test library. If the Kindle's `system/` folder sits next to it, its `system/thumbnails` provides covers for Amazon books. In the simulator, tapping a book explains that KOReader only runs on the Kindle.

`library --scan` prints what the indexer finds, which helps when a title or cover looks wrong.

## Install

```powershell
powershell -File scripts\dev.ps1 -Install              # into the simulator's home screen
powershell -File scripts\dev.ps1 -Install -Kindle E:   # onto the Kindle (inkpad/apps/library)
```

## How "open in KOReader" works

An inkpad app can't start KOReader directly, because KOReader needs the screen. So the Library calls `handoff::open_in_koreader(ctx, book, Some("library"))`:

1. That call writes the request to a small file and exits with code 42.
2. inkpad's launcher script gives the screen back and runs `koreader.sh <book>`. KOReader opens the file straight in its reader.
3. When KOReader exits, the launcher starts inkpad again and reopens the Library, without asking for the PIN.

`kindle\deploy.ps1` (and `deploy-all.ps1`) at the root installs it along with the other apps.

## License

MIT or Apache-2.0, at your option, like the rest of inkpad.
