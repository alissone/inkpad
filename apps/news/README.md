# News for inkpad

An offline-first RSS reader for the Kindle. Download a batch of stories while you have Wi-Fi, then read them anywhere, with pictures, set like a book page.

- **⟳ downloads the news:** a few new stories (5 or 10, in settings) from every feed you follow, with their text and up to 4 pictures each. Wi-Fi is switched on just for that and off again afterwards. The title bar shows the progress, and the ✕ that replaces ⟳ stops after the current feed.
- **The list:** newest first, with the feed, how long ago, a thumbnail or a line of the story, and a 🗑 to delete it without opening it. Read stories turn grey.
- **Reading:** tap a story. It's laid out like an ePub page:
  - The text is set in Bookerly if the Kindle has it (or you put it in `inkpad/assets/fonts`), else the bundled Gentium Book Plus.
  - Pictures are converted to grayscale and dithered for e-ink.
  - Swipe or tap › to turn pages.
  - **←** goes back and **🗑** deletes the story.
- **Feeds (the RSS button):** the feeds you follow, by category, each with how many stories you have and when it was last fetched (or why it failed).
  - Tap a feed to read only its stories.
  - **⤓** fetches up to 10 more from it.
  - **🗑** deletes all of its stories.
- **Deleted stories stay deleted:** fetching again only brings stories you haven't seen.
- **Settings (⚙):** stories per feed, text size, pictures on or off, Wi-Fi off after downloading, and how much space the stories use. There's also a button to delete every story you've read.
- **On the home screen:** News publishes a glance with the number of unread stories and the latest headlines.

What you get depends on each feed. Some send the whole story (Ars Technica, Hackaday, The Verge, GitHub Trending's READMEs). Others only send a headline and a link (Hacker News), and News says so at the end of the page. Videos (YouTube feeds) show the thumbnail and the description.

It's one of [inkpad](../..)'s apps: a separate program in the inkpad workspace, installed into inkpad like an app onto a phone.

## Feeds

The feed list is built in (`src/feeds.rs`) and isn't editable on the Kindle yet. To use your own, put a `news/feeds.json` in inkpad's data folder (`inkpad/data/news/feeds.json` on the Kindle):

```json
[{ "category": "Tech", "name": "Lobsters", "url": "https://lobste.rs/rss", "about": "Computing links" }]
```

## Layout

```
app.json          manifest (id, name, icon)
assets/fonts/     Gentium Book Plus (SIL Open Font License, see OFL.txt)
src/main.rs       the UI: story list, reader, feeds, settings
src/feeds.rs      the feeds you follow
src/feed.rs       RSS 2.0 / RSS 1.0 / Atom parsing, dates
src/html.rs       story HTML to blocks (paragraphs, headings, quotes, code, lists, pictures)
src/fetch.rs      downloading feeds and pictures (background threads, retries)
src/store.rs      what's kept offline in inkpad/data/news/
scripts/dev.ps1   run in the simulator, install
```

Offline storage, in `inkpad/data/news/`:
- `index.json`: the stories, the deleted ones, and the state of each feed.
- `stories/<id>.json`: each story's text.
- `img/`: its pictures as small grayscale PNGs.

## Develop

From this folder (`apps/news`):

```powershell
powershell -File scripts\dev.ps1         # simulator (mouse = finger)
cargo test -p news
..\..\target\release\news.exe --fetch 3         # download 3 new stories per feed without the UI, with a report per feed
```

Set `INKPAD_NEWS_OFFLINE=1` to act as if there were no Wi-Fi.

Downloads go through inkpad's `net`, which uses the system `curl`. Pictures can be JPEG, PNG, SVG or WebP. GIF and AVIF are skipped.

## Install

```powershell
powershell -File scripts\dev.ps1 -Install              # simulator home screen
powershell -File scripts\dev.ps1 -Install -Kindle E:   # Kindle (inkpad/apps/news)
```

`kindle\deploy.ps1` (and `deploy-all.ps1`) at the root installs it along with the other apps.

## License

MIT or Apache-2.0, at your option, like the rest of inkpad. Gentium Book Plus is © SIL International, under the SIL Open Font License 1.1 (`assets/fonts/OFL.txt`).
