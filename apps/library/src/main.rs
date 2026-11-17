//! Library: the books on your Kindle as a cover grid (or a list, or by
//! folder). Tap a book to read it: KOReader opens it, and closing KOReader
//! brings you back here. Amazon KFX and DRM books open in the Kindle reader.
//!
//! An inkpad app, developed separately from inkpad itself.

mod books;
mod meta;
mod zip;

use books::{Book, Found};
use inkpad::prelude::*;
use inkpad::ui::{assets, handoff};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Grid,
    List,
    Folders,
}

impl View {
    fn key(self) -> &'static str {
        match self {
            View::Grid => "grid",
            View::List => "list",
            View::Folders => "folders",
        }
    }
    fn from_key(s: &str) -> View {
        match s {
            "list" => View::List,
            "folders" => View::Folders,
            _ => View::Grid,
        }
    }
    fn next(self) -> View {
        match self {
            View::Grid => View::List,
            View::List => View::Folders,
            View::Folders => View::Grid,
        }
    }
    fn icon(self) -> &'static str {
        match self {
            View::Grid => "grid-2x2",
            View::List => "list",
            View::Folders => "folder",
        }
    }
}

const SORTS: [&str; 3] = ["Recent", "Title", "Author"];
const BATCH: usize = 8;

#[derive(Clone, Debug)]
enum Msg {
    Listed(Vec<Book>, Vec<Found>),
    Indexed(Vec<Book>, Vec<Found>),
    CycleView,
    Sort(usize),
    Search,
    Searched(Option<String>),
    ClearSearch,
    Page(usize),
    Open(String),
    Details(String),
    CloseDetails,
    Folder(String),
}

struct Library {
    books: Vec<Book>,
    /// (indexed, total) while indexing new files.
    indexing: Option<(usize, usize)>,
    view: View,
    sort: usize,
    query: String,
    folder: String,
    page: usize,
    details: Option<String>,
    prefs: Prefs,
    serif: Option<FamilyId>,
}

impl Library {
    fn new() -> Self {
        let prefs = Prefs::open("library");
        let mut books = books::load_index();
        books.iter_mut().for_each(books::apply_progress);
        Library {
            books,
            indexing: None,
            view: View::from_key(prefs.get_str("view").unwrap_or("grid")),
            sort: prefs.get_i64("sort", 0).clamp(0, 2) as usize,
            query: String::new(),
            folder: prefs.get_string("folder", ""),
            page: prefs.get_i64("page", 0).max(0) as usize,
            details: None,
            prefs,
            serif: None,
        }
    }

    fn book(&self, path: &str) -> Option<&Book> {
        self.books.iter().find(|b| b.path == path)
    }

    /// Books to show, filtered by search and sorted.
    fn visible(&self) -> Vec<&Book> {
        let q = self.query.to_lowercase();
        let mut v: Vec<&Book> = self.books.iter().filter(|b| q.is_empty() || b.title.to_lowercase().contains(&q) || b.author.to_lowercase().contains(&q) || b.file_name().to_lowercase().contains(&q)).collect();
        match self.sort {
            1 => v.sort_by_key(|b| b.title.to_lowercase()),
            2 => v.sort_by_cached_key(|b| (b.author.is_empty(), surname(&b.author), b.author.to_lowercase(), b.title.to_lowercase())),
            _ => v.sort_by(|a, b| (b.last_read, b.mtime).cmp(&(a.last_read, a.mtime))),
        }
        v
    }

    fn set_page(&mut self, p: usize) {
        self.page = p;
        self.prefs.set("page", p as i64);
    }

    // ---- covers ----

    fn generated_cover(&self, b: &Book, w: u32, h: u32) -> El<Msg> {
        if w < 90 {
            return col(vec![spacer(), icon("book-text", w / 2), spacer()]).align(Align::Center).w(w).h(h).border(1.5, BLACK).radius(3.0);
        }
        let band = canvas(w, 22, |p, r| {
            for i in 0..(r.w / 6 + 4) {
                let x = r.x as f32 + i as f32 * 6.0 - 20.0;
                p.line(x, r.y as f32 + r.h as f32, x + 20.0, r.y as f32, 1.5, 90);
            }
        });
        let mut title = text(b.title.clone()).size(17).bold().center_text().max_lines(5);
        if let Some(f) = self.serif {
            title = title.family(f);
        }
        col(vec![band, spacer(), title.pad(8, 0), spacer(), text(b.author.clone()).size(12).color(60).center_text().max_lines(2).pad(6, 0), empty().h(10)])
            .w(w)
            .h(h)
            .border(2.0, BLACK)
            .bg(WHITE)
    }

    fn cover(&self, b: &Book, w: u32, h: u32) -> El<Msg> {
        match b.cover.as_deref().and_then(|p| assets::image(p, w, h, true)) {
            Some(img) => image(img).w(w).h(h).border(1.0, BLACK),
            None => self.generated_cover(b, w, h),
        }
    }

    fn meta_line(b: &Book) -> String {
        let mut parts = vec![b.format.clone()];
        if b.finished {
            parts.push("Finished".into());
        } else if let Some(p) = b.progress {
            parts.push(format!("{:.0}%", p * 100.0));
        }
        if b.sample {
            parts.push("Sample".into());
        }
        if b.kindle_only {
            parts.push("Kindle".into());
        }
        parts.join(" · ")
    }

    fn progress_bar(b: &Book, w: u32) -> El<Msg> {
        let frac = if b.finished { 1.0 } else { b.progress.unwrap_or(0.0) };
        let shown = b.finished || b.progress.is_some();
        canvas(w, 5, move |p, r| {
            if shown {
                p.fill_rect(Rect::new(r.x, r.y + 2, r.w, 1), 160);
                p.fill_rect(Rect::new(r.x, r.y, ((r.w as f32) * frac) as u32, 5), BLACK);
            }
        })
    }

    // ---- views ----

    fn grid_cell(&self, b: &Book) -> El<Msg> {
        let (w, h) = (122, 176);
        col(vec![
            row(vec![spacer(), self.cover(b, w, h), spacer()]),
            row(vec![spacer(), Self::progress_bar(b, w), spacer()]),
            text(b.title.clone()).size(14).bold().max_lines(2).center_text(),
            text(Self::meta_line(b)).size(12).color(90).max_lines(1).center_text(),
        ])
        .gap(5)
        .fill_w()
        .on_tap(Msg::Open(b.path.clone()))
        .on_hold(Msg::Details(b.path.clone()))
    }

    fn list_row(&self, b: &Book) -> El<Msg> {
        let author = if b.author.is_empty() { "Unknown author".to_string() } else { b.author.clone() };
        row(vec![
            self.cover(b, 50, 72),
            col(vec![text(b.title.clone()).size(19).bold().max_lines(2), text(author).size(15).color(70).max_lines(1), text(Self::meta_line(b)).size(13).color(100)]).gap(3).fill_w(),
        ])
        .gap(14)
        .pad(22, 8)
        .on_tap(Msg::Open(b.path.clone()))
        .on_hold(Msg::Details(b.path.clone()))
    }

    fn shelf(&self, list: &[&Book]) -> El<Msg> {
        if list.is_empty() {
            let msg = if !self.query.is_empty() {
                "No books match your search."
            } else if self.indexing.is_some() {
                "Looking for books…"
            } else {
                "No books yet. Copy EPUB, MOBI, AZW3 or PDF files to the Kindle's documents folder."
            };
            return col(vec![spacer(), text(msg).size(20).color(80).center_text().pad(40, 0), spacer()]).fill();
        }
        let items: Vec<El<Msg>> = match self.view {
            View::Grid => list.chunks(3).map(|c| {
                let mut cells: Vec<El<Msg>> = c.iter().map(|b| self.grid_cell(b)).collect();
                while cells.len() < 3 {
                    cells.push(empty().fill_w());
                }
                row(cells).gap(8).align(Align::Start).pad(12, 8)
            }).collect(),
            _ => list.iter().map(|b| self.list_row(b)).collect(),
        };
        paged(items, self.page, Msg::Page)
    }

    fn folders(&self) -> El<Msg> {
        let prefix = if self.folder.is_empty() { String::new() } else { format!("{}/", self.folder) };
        let mut subs: Vec<(String, usize)> = Vec::new();
        for b in &self.books {
            let Some(rest) = (if self.folder.is_empty() { Some(b.folder.as_str()) } else { b.folder.strip_prefix(&prefix) }) else { continue };
            if rest.is_empty() {
                continue;
            }
            let name = rest.split('/').next().unwrap_or(rest).to_string();
            match subs.iter_mut().find(|s| s.0 == name) {
                Some(s) => s.1 += 1,
                None => subs.push((name, 1)),
            }
        }
        subs.sort_by_key(|s| s.0.to_lowercase());
        let mut items: Vec<El<Msg>> = Vec::new();
        if !self.folder.is_empty() {
            let up = self.folder.rsplit_once('/').map(|p| p.0.to_string()).unwrap_or_default();
            items.push(row(vec![icon("arrow-left", 28), text("Back").size(20).bold().fill_w()]).gap(16).pad(24, 12).on_tap(Msg::Folder(up)));
        }
        for (name, n) in subs {
            let path = format!("{prefix}{name}");
            items.push(
                row(vec![icon("folder", 30), text(name).size(20).bold().max_lines(1).fill_w(), text(format!("{n}")).size(16).color(90), icon("chevron-right", 24)])
                    .gap(16)
                    .pad(24, 12)
                    .on_tap(Msg::Folder(path)),
            );
        }
        let mut here: Vec<&Book> = self.books.iter().filter(|b| b.folder == self.folder).collect();
        here.sort_by_key(|b| b.title.to_lowercase());
        items.extend(here.iter().map(|b| self.list_row(b)));
        paged(items, self.page, Msg::Page)
    }

    fn details_card(&self, b: &Book) -> El<Msg> {
        let opens = if b.kindle_only { "Opens in the Kindle reader" } else { "Opens in KOReader" };
        let progress = if b.finished { "Finished".to_string() } else { b.progress.map(|p| format!("{:.0}% read", p * 100.0)).unwrap_or_else(|| "Not started in KOReader".into()) };
        let size = if b.size >= 1 << 20 { format!("{:.1} MB", b.size as f64 / 1048576.0) } else { format!("{} KB", b.size / 1024) };
        let folder = if b.folder.is_empty() { "documents".to_string() } else { format!("documents/{}", b.folder) };
        let mut info = vec![text(b.title.clone()).size(22).bold().max_lines(4)];
        if !b.author.is_empty() {
            info.push(text(b.author.clone()).size(18).color(50));
        }
        info.push(empty().h(6));
        for line in [format!("{} · {size}", b.format), progress, folder] {
            info.push(text(line).size(15).color(80).max_lines(2));
        }
        if b.sample {
            info.push(text("Amazon sample").size(15).color(80));
        }
        info.push(text(opens).size(15).bold());
        let card = col(vec![
            row(vec![self.cover(b, 140, 200), col(info).gap(4).fill_w()]).gap(18).align(Align::Start),
            row(vec![spacer(), button_outline("Close", Msg::CloseDetails), button("Read", Msg::Open(b.path.clone()))]).gap(12),
        ])
        .gap(20)
        .padding(22)
        .bg(WHITE)
        .border(3.0, BLACK)
        .radius(18.0);
        col(vec![spacer(), card, spacer()]).pad(20, 0).block_taps().on_tap(Msg::CloseDetails)
    }

    fn open(&mut self, path: &str, ctx: &mut Ctx<Msg>) {
        let Some(b) = self.book(path).cloned() else { return };
        let file = Path::new(&b.path);
        let r = if b.kindle_only { handoff::open_in_kindle_reader(ctx, file) } else { handoff::open_in_koreader(ctx, Some(file), Some("library")) };
        if let Err(e) = r {
            ctx.toast(format!("{e} ({})", b.file_name()));
        }
    }

    fn start_scan(&mut self, ctx: &mut Ctx<Msg>) {
        let cached = self.books.clone();
        self.indexing = Some((0, 0));
        ctx.spawn(move || {
            let mut found = books::list_files(&books::documents_dir());
            found.sort_by_key(|f| std::cmp::Reverse(f.mtime));
            let (mut known, todo) = books::plan(found, &cached);
            known.iter_mut().for_each(books::apply_progress);
            Msg::Listed(known, todo)
        });
    }

    fn index_batch(&mut self, mut todo: Vec<Found>, ctx: &mut Ctx<Msg>) {
        if todo.is_empty() {
            self.indexing = None;
            books::save_index(&self.books);
            return;
        }
        let rest = todo.split_off(todo.len().min(BATCH));
        ctx.spawn(move || {
            let root = books::documents_dir();
            let thumbs = books::thumbnail_names();
            let done = todo
                .iter()
                .map(|f| {
                    let mut b = books::index_one(f, &root, &thumbs);
                    books::apply_progress(&mut b);
                    b
                })
                .collect();
            Msg::Indexed(done, rest)
        });
    }
}

impl App for Library {
    type Msg = Msg;

    fn setup(&mut self, fonts: &mut Fonts, ctx: &mut Ctx<Msg>) {
        self.serif = assets::load_font(fonts, "Bookerly", "Bookerly");
        self.start_scan(ctx);
    }

    fn view(&self, _env: &Env) -> El<Msg> {
        let list = self.visible();
        let sub = if let Some((done, total)) = self.indexing.filter(|i| i.1 > 0) {
            format!("Indexing {done} of {total} new books…")
        } else if !self.query.is_empty() {
            format!("{} found for “{}”", list.len(), self.query)
        } else if self.view == View::Folders {
            format!("documents/{}", self.folder)
        } else {
            format!("{} books", self.books.len())
        };
        let mut actions = vec![icon_button("search", Msg::Search), icon_button(self.view.icon(), Msg::CycleView)];
        if !self.query.is_empty() {
            actions.insert(0, icon_button("x", Msg::ClearSearch));
        }
        let header = title_bar("Library", Some(&sub), actions);
        let body = match self.view {
            View::Folders if self.query.is_empty() => self.folders(),
            _ => col(vec![row(vec![segmented(&SORTS, self.sort, Msg::Sort).fill_w()]).pad(24, 4), self.shelf(&list)]).gap(6),
        };
        let base = col(vec![header, body.fill_h()]);
        match self.details.as_deref().and_then(|p| self.book(p)) {
            Some(b) => stack(vec![base, self.details_card(b)]),
            None => base,
        }
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Listed(known, todo) => {
                self.books = known;
                self.indexing = Some((0, todo.len()));
                self.index_batch(todo, ctx);
            }
            Msg::Indexed(done, rest) => {
                for b in done {
                    self.books.retain(|x| x.path != b.path);
                    self.books.push(b);
                }
                if let Some((_, total)) = self.indexing {
                    self.indexing = Some((total - rest.len(), total));
                }
                self.index_batch(rest, ctx);
            }
            Msg::CycleView => {
                self.view = self.view.next();
                self.prefs.set("view", self.view.key());
                self.set_page(0);
                ctx.full_refresh();
            }
            Msg::Sort(i) => {
                self.sort = i;
                self.prefs.set("sort", i as i64);
                self.set_page(0);
            }
            Msg::Search => ctx.prompt("Search title or author", &self.query, Msg::Searched),
            Msg::Searched(q) => {
                if let Some(q) = q {
                    self.query = q.trim().to_string();
                    self.set_page(0);
                }
            }
            Msg::ClearSearch => {
                self.query.clear();
                self.set_page(0);
            }
            Msg::Page(p) => self.set_page(p),
            Msg::Open(path) => {
                self.details = None;
                self.open(&path, ctx);
            }
            Msg::Details(path) => self.details = Some(path),
            Msg::CloseDetails => self.details = None,
            Msg::Folder(f) => {
                self.folder = f;
                self.prefs.set("folder", self.folder.clone());
                self.set_page(0);
            }
        }
    }
}

/// Sort key for an author: the last word of the first author ("Abreu").
fn surname(author: &str) -> String {
    author.split(',').next().unwrap_or("").split_whitespace().last().unwrap_or("").to_lowercase()
}

fn main() {
    // `library --scan`: index and print what was found (debugging aid).
    if std::env::args().any(|a| a == "--scan") {
        let root = books::documents_dir();
        let thumbs = books::thumbnail_names();
        for f in books::list_files(&root) {
            let b = books::index_one(&f, &root, &thumbs);
            println!("{:5} {:<5} {:<45.45} | {:<22.22} | cover={} kindle={} sample={}", b.size / 1024, b.format, b.title, b.author, b.cover.is_some(), b.kindle_only, b.sample);
        }
        return;
    }
    if let Err(e) = inkpad::run(Library::new()) {
        eprintln!("library: {e}");
        std::process::exit(1);
    }
}
