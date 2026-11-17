//! News: an offline-first RSS reader for the Kindle.
//!
//! - ⟳ downloads a few new stories from every feed (text and pictures), so
//!   they read offline. Wi-Fi is switched on just for that.
//! - Stories read like a book page (Bookerly if the Kindle has it, else the
//!   bundled Gentium Book Plus), with pictures.
//! - Delete a story from the list (🗑) or while reading it; deleted stories
//!   don't come back on the next fetch.
//! - The feeds screen lists what you follow: fetch more from one feed, read
//!   only that feed, or delete all of its stories.
//!
//! An inkpad app, developed separately from inkpad itself.

mod feed;
mod feeds;
mod fetch;
mod html;
mod store;

use feeds::Feed;
use fetch::FeedResult;
use html::Block;
use inkpad::prelude::*;
use inkpad::ui::{assets, clock, glance};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use store::{Index, Story};

#[derive(Clone, Debug)]
enum Msg {
    /// Fetch every feed (`None`) or more from one.
    Fetch(Option<String>),
    StopFetch,
    Online(Result<bool, String>),
    Fetched(FeedResult),
    WifiOff,
    Open(String),
    Back,
    Delete(String),
    ReadPage(usize),
    Page(usize),
    Feeds(bool),
    FeedsPage(usize),
    /// Show only one feed's stories (`None` = all).
    Filter(Option<String>),
    AskDeleteFeed(String),
    DeleteFeed(String),
    DeleteRead,
    CloseDialog,
    Settings(bool),
    PerFeed(usize),
    TextSize(usize),
    Toggle(&'static str),
}

const PER_FEED: &str = "per_feed";
const TEXT_SIZE: &str = "text_size";
const IMAGES: &str = "images";
const WIFI_OFF: &str = "wifi_off_after";
const PER_FEED_CHOICES: [usize; 2] = [5, 10];
/// "Fetch more" from one feed.
const MORE: usize = 10;
const TEXT_SIZES: [u16; 3] = [20, 23, 26];

struct Job {
    queue: VecDeque<(usize, usize)>,
    total: usize,
    done: usize,
    added: usize,
    failed: Vec<String>,
    current: Option<String>,
    turned_on: bool,
    stop: bool,
    /// One feed's "fetch more" (says so when it finds nothing new).
    single: bool,
}

#[derive(Clone)]
enum Dialog {
    DeleteFeed(String),
}

struct News {
    feeds: Vec<Feed>,
    index: Index,
    prefs: Prefs,
    page: usize,
    filter: Option<String>,
    feeds_open: bool,
    feeds_page: usize,
    reading: Option<String>,
    body: Vec<Block>,
    read_page: usize,
    job: Option<Job>,
    dialog: Option<Dialog>,
    settings: bool,
    serif: Option<FamilyId>,
    /// Decoded, dithered pictures. Full-size ones are dropped when you leave
    /// a story; thumbnails stay.
    images: RefCell<HashMap<String, Option<Rc<GrayImage>>>>,
}

fn now() -> i64 {
    clock::unix_now()
}

/// "now", "12 min", "3 h", "Yesterday", "Thu 12 Nov".
fn ago(t: i64) -> String {
    let d = now() - t;
    if d < 90 {
        return "now".into();
    }
    if d < 3600 {
        return format!("{} min", d / 60);
    }
    let off = clock::offset_minutes();
    let (a, b) = (clock::from_unix(t, off), clock::from_unix(now(), off));
    let days = clock::days_from_civil(b.year, b.month, b.day) - clock::days_from_civil(a.year, a.month, a.day);
    match days {
        0 => format!("{} h", d / 3600),
        1 => "Yesterday".into(),
        _ => {
            const WD: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            const MO: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let s = format!("{} {} {}", WD[a.weekday as usize % 7], a.day, MO[(a.month as usize + 11) % 12]);
            if a.year == b.year { s } else { format!("{s} {}", a.year) }
        }
    }
}

/// Split long paragraphs at sentence ends so each piece fits on a page.
fn pieces(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        cur.push_str(word);
        cur.push(' ');
        let len = cur.chars().count();
        let sentence_end = word.ends_with(['.', '!', '?', ':', ';']);
        if (len > max * 2 / 3 && sentence_end) || len > max {
            out.push(cur.trim_end().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim_end().to_string());
    }
    out
}

impl News {
    fn new() -> Self {
        let mut index = Index::load();
        index.sort();
        News {
            feeds: feeds::load(),
            index,
            prefs: Prefs::open("news"),
            page: 0,
            filter: None,
            feeds_open: false,
            feeds_page: 0,
            reading: None,
            body: Vec::new(),
            read_page: 0,
            job: None,
            dialog: None,
            settings: false,
            serif: None,
            images: RefCell::new(HashMap::new()),
        }
    }

    fn per_feed(&self) -> usize {
        self.prefs.get_i64(PER_FEED, 5).max(1) as usize
    }
    fn text_size(&self) -> u16 {
        TEXT_SIZES[(self.prefs.get_i64(TEXT_SIZE, 1).clamp(0, 2)) as usize]
    }
    fn flag(&self, k: &str) -> bool {
        self.prefs.get_bool(k, true)
    }

    fn feed(&self, id: &str) -> Option<&Feed> {
        self.feeds.iter().find(|f| f.id() == id)
    }
    fn feed_name(&self, id: &str) -> String {
        self.feed(id).map(|f| f.name.clone()).unwrap_or_else(|| id.to_string())
    }

    fn picture(&self, file: &str, thumb: bool) -> Option<Rc<GrayImage>> {
        if let Some(hit) = self.images.borrow().get(file) {
            return hit.clone();
        }
        let img = GrayImage::load(&store::img_dir().join(file)).map_err(|e| eprintln!("news: {file}: {e}")).ok().map(|i| {
            // Pictures saved by older versions could be taller.
            let i = if !thumb && i.height > fetch::IMG_MAX_H { i.fit(fetch::IMG_W, fetch::IMG_MAX_H) } else { i };
            Rc::new(i.dither16())
        });
        self.images.borrow_mut().insert(file.to_string(), img.clone());
        img
    }

    fn drop_pictures(&self) {
        self.images.borrow_mut().retain(|k, _| k.ends_with("-t.png"));
    }

    fn serif(&self, t: El<Msg>) -> El<Msg> {
        match self.serif {
            Some(f) => t.family(f),
            None => t,
        }
    }

    fn publish_glance(&self) {
        let unread: Vec<&Story> = self.index.stories.iter().filter(|s| !s.read).collect();
        if self.index.stories.is_empty() {
            glance::remove("news");
            return;
        }
        let note = match unread.len() {
            0 => "All read".to_string(),
            n => format!("{n} unread"),
        };
        let lines = unread.iter().take(8).map(|s| s.title.clone()).collect();
        glance::publish(&glance::Glance { id: "news".into(), title: "News".into(), icon: "newspaper".into(), note, lines, updated: now() });
    }

    fn saved(&mut self) {
        self.index.save();
        self.publish_glance();
    }

    // ---- fetching ----

    fn start_fetch(&mut self, feed: Option<String>, ctx: &mut Ctx<Msg>) {
        if self.job.is_some() {
            ctx.toast("Already downloading");
            return;
        }
        let queue: VecDeque<(usize, usize)> = match &feed {
            None => (0..self.feeds.len()).map(|i| (i, self.per_feed())).collect(),
            Some(id) => self.feeds.iter().position(|f| &f.id() == id).map(|i| (i, MORE)).into_iter().collect(),
        };
        if queue.is_empty() {
            return;
        }
        self.job = Some(Job { total: queue.len(), queue, done: 0, added: 0, failed: Vec::new(), current: None, turned_on: false, stop: false, single: feed.is_some() });
        ctx.spawn(|| Msg::Online(fetch::go_online()));
    }

    fn next_feed(&mut self, ctx: &mut Ctx<Msg>) {
        let images = self.flag(IMAGES);
        let Some(job) = &mut self.job else { return };
        let next = if job.stop { None } else { job.queue.pop_front() };
        let Some((i, want)) = next else {
            self.finish_fetch(ctx);
            return;
        };
        let feed = self.feeds[i].clone();
        job.current = Some(feed.name.clone());
        let known = self.index.known(&feed.id());
        ctx.spawn(move || Msg::Fetched(fetch::fetch_feed(&feed, known, want, images)));
    }

    fn finish_fetch(&mut self, ctx: &mut Ctx<Msg>) {
        let Some(job) = self.job.take() else { return };
        if job.turned_on && self.flag(WIFI_OFF) {
            ctx.spawn(|| {
                inkpad::net::wifi_off();
                Msg::WifiOff
            });
        }
        if !job.single {
            self.index.last_fetch = Some(now());
        }
        let mut msg = match job.added {
            0 if job.single => "Nothing new in this feed".to_string(),
            0 => "No new stories".to_string(),
            1 => "1 new story".to_string(),
            n => format!("{n} new stories"),
        };
        match job.failed.len() {
            0 => {}
            1 if job.total == 1 => msg = format!("Couldn't fetch: {}", self.index.feeds.get(&job.failed[0]).and_then(|s| s.error.clone()).unwrap_or_default()),
            n => msg.push_str(&format!(" · {n} feeds failed (see Feeds)")),
        }
        ctx.toast(msg);
        self.saved();
        ctx.full_refresh();
    }

    fn fetched(&mut self, r: FeedResult, ctx: &mut Ctx<Msg>) {
        let state = self.index.feeds.entry(r.feed.clone()).or_default();
        state.fetched_at = Some(r.at);
        state.error = r.error.clone();
        if let Some(e) = &r.error {
            eprintln!("news: {}: {e}", r.feed);
        }
        let added = r.stories.len();
        // A story deleted while its feed was downloading stays deleted.
        let known = self.index.known(&r.feed);
        self.index.stories.extend(r.stories.into_iter().filter(|s| !known.contains(&s.guid)));
        self.index.sort();
        self.index.save();
        if let Some(job) = &mut self.job {
            job.done += 1;
            job.added += added;
            if r.error.is_some() {
                job.failed.push(r.feed);
            }
        }
        self.next_feed(ctx);
    }

    fn delete(&mut self, id: &str) {
        self.index.delete(id);
        self.saved();
    }

    // ---- views ----

    fn visible(&self) -> Vec<&Story> {
        self.index.stories.iter().filter(|s| self.filter.as_ref().is_none_or(|f| &s.feed == f)).collect()
    }

    fn story_row(&self, s: &Story) -> El<Msg> {
        let mut meta = Vec::new();
        if self.filter.is_none() {
            meta.push(self.feed_name(&s.feed));
        }
        meta.push(ago(s.published));
        let title = self.serif(text(s.title.clone()).size(22).bold().color(if s.read { 120 } else { BLACK }).max_lines(3).line_spacing(1.05));
        let mut info = vec![title, text(meta.join(" · ")).size(15).color(if s.read { 130 } else { 80 }).max_lines(1)];
        if s.thumb.is_none() && !s.summary.is_empty() {
            info.insert(1, self.serif(text(s.summary.clone()).size(17).color(if s.read { 130 } else { 70 }).max_lines(2)));
        }
        let mut kids = vec![col(info).gap(4).fill_w()];
        if let Some(img) = s.thumb.as_ref().and_then(|t| self.picture(t, true)) {
            kids.push(image(img).radius(6.0));
        }
        kids.push(icon_button("trash-2", Msg::Delete(s.id.clone())));
        row(kids).gap(10).pad_edges(Edges { top: 10, right: 8, bottom: 10, left: 26 }).on_tap(Msg::Open(s.id.clone()))
    }

    fn progress_line(&self) -> Option<El<Msg>> {
        let job = self.job.as_ref()?;
        let what = match &job.current {
            Some(n) if job.total > 1 => format!("Downloading {} of {} · {n}", (job.done + 1).min(job.total), job.total),
            Some(n) => format!("Downloading from {n}…"),
            None => "Connecting…".to_string(),
        };
        Some(col(vec![text(what).size(16).color(60).max_lines(1), progress(job.done as f32 / job.total.max(1) as f32, 14)]).gap(6).pad(28, 8))
    }

    fn stories_view(&self) -> El<Msg> {
        let stories = self.visible();
        let unread = stories.iter().filter(|s| !s.read).count();
        let busy = self.job.is_some();
        let fetch_btn = if busy { icon_button("x", Msg::StopFetch) } else { icon_button("refresh-cw", Msg::Fetch(None)) };
        let header = match &self.filter {
            None => {
                let when = self.index.last_fetch.map(|t| format!(" · fetched {}", ago(t))).unwrap_or_default();
                let sub = format!("{} stories · {unread} unread{when}", stories.len());
                title_bar("News", Some(&sub), vec![fetch_btn, icon_button("rss", Msg::Feeds(true)), icon_button("settings", Msg::Settings(true))])
            }
            Some(f) => {
                let sub = format!("{} stories · {unread} unread", stories.len());
                let more = if busy { icon_button("x", Msg::StopFetch) } else { icon_button("download", Msg::Fetch(Some(f.clone()))) };
                title_bar(&self.feed_name(f), Some(&sub), vec![more, icon_button("arrow-left", Msg::Filter(None))])
            }
        };
        let mut top = vec![header];
        if let Some(p) = self.progress_line() {
            top.push(p);
        }
        let body = if stories.is_empty() {
            let (msg, btn) = match (&self.filter, busy) {
                (_, true) => ("Downloading stories…".to_string(), None),
                (None, false) => (format!("No stories yet. Download the latest from your {} feeds to read them offline.", self.feeds.len()), Some(button_icon("download", "Download news", true, Msg::Fetch(None)))),
                (Some(f), false) => ("No stories from this feed.".to_string(), Some(button_icon("download", "Download", true, Msg::Fetch(Some(f.clone()))))),
            };
            let mut kids = vec![spacer(), icon("newspaper", 64), text(msg).size(21).color(70).center_text()];
            kids.extend(btn);
            kids.push(spacer());
            col(kids).gap(18).align(Align::Center).pad(44, 0).fill()
        } else {
            paged(stories.iter().map(|s| self.story_row(s)).collect(), self.page, Msg::Page)
        };
        top.push(body.fill_h());
        col(top).fill()
    }

    fn feeds_view(&self) -> El<Msg> {
        let sub = format!("{} feeds · {} stories", self.feeds.len(), self.index.stories.len());
        let fetch_btn = if self.job.is_some() { icon_button("x", Msg::StopFetch) } else { icon_button("refresh-cw", Msg::Fetch(None)) };
        let header = title_bar("Feeds", Some(&sub), vec![fetch_btn, icon_button("arrow-left", Msg::Feeds(false))]);
        let mut rows: Vec<El<Msg>> = Vec::new();
        let mut category = "";
        for f in &self.feeds {
            if f.category != category {
                category = &f.category;
                rows.push(text(f.category.to_uppercase()).size(14).bold().color(90).pad_edges(Edges { top: 12, right: 28, bottom: 2, left: 28 }));
            }
            let id = f.id();
            let n = self.index.count(&id);
            let st = self.index.feeds.get(&id);
            let status = match (st.and_then(|s| s.error.as_ref()), st.and_then(|s| s.fetched_at)) {
                (Some(e), _) => format!("Couldn't fetch: {e}"),
                (None, Some(t)) => format!("{n} stor{} · fetched {}", if n == 1 { "y" } else { "ies" }, ago(t)),
                (None, None) => if n > 0 { format!("{n} stories") } else { "Not fetched yet".into() },
            };
            let info = col(vec![
                text(f.name.clone()).size(21).bold().max_lines(1),
                text(f.about.clone()).size(15).color(80).max_lines(1),
                text(status).size(14).color(if st.is_some_and(|s| s.error.is_some()) { 0 } else { 110 }).max_lines(1),
            ])
            .gap(2)
            .fill_w()
            .on_tap(Msg::Filter(Some(id.clone())));
            rows.push(row(vec![info, icon_button("download", Msg::Fetch(Some(id.clone()))), icon_button("trash-2", Msg::AskDeleteFeed(id))]).gap(2).pad_edges(Edges { top: 6, right: 8, bottom: 6, left: 28 }));
        }
        let mut top = vec![header];
        if let Some(p) = self.progress_line() {
            top.push(p);
        }
        top.push(paged(rows, self.feeds_page, Msg::FeedsPage).fill_h());
        col(top).bg(WHITE).fill()
    }

    fn reader(&self, s: &Story) -> El<Msg> {
        let fs = self.text_size();
        let bar = col(vec![
            row(vec![icon_button("arrow-left", Msg::Back), text(self.feed_name(&s.feed)).size(17).bold().color(70).max_lines(1).fill_w(), icon_button("trash-2", Msg::Delete(s.id.clone()))]).gap(6).pad(10, 4),
            rule(2, BLACK),
        ]);
        let para = |t: String, size: u16, color: u8| self.serif(text(t).size(size).color(color).line_spacing(1.3));
        let mut kids: Vec<El<Msg>> = Vec::new();
        kids.push(self.serif(text(s.title.clone()).size(fs + 9).bold().line_spacing(1.1)).pad_edges(Edges { top: 18, right: 28, bottom: 6, left: 28 }));
        let mut meta = Vec::new();
        if let Some(a) = &s.author {
            meta.push(a.clone());
        }
        let t = clock::from_unix(s.published, clock::offset_minutes());
        meta.push(format!("{} {} {} {}", &t.weekday_name()[..3], t.day, &t.month_name()[..3], t.hm()));
        kids.push(text(meta.join(" · ")).size(15).color(80).max_lines(2).pad_edges(Edges { top: 0, right: 28, bottom: 14, left: 28 }));
        let mut has_text = false;
        for b in &self.body {
            match b {
                Block::P(t) => {
                    has_text = true;
                    for (i, p) in pieces(t, 420).into_iter().enumerate() {
                        let top = if i == 0 { 8 } else { 0 };
                        kids.push(para(p, fs, 0).pad_edges(Edges { top, right: 28, bottom: 0, left: 28 }));
                    }
                }
                Block::H(t) => {
                    has_text = true;
                    kids.push(self.serif(text(t.clone()).size(fs + 3).bold()).pad_edges(Edges { top: 16, right: 28, bottom: 2, left: 28 }));
                }
                Block::Quote(t) => {
                    has_text = true;
                    for p in pieces(t, 380) {
                        kids.push(row(vec![rule(3, 140).h(24), para(p, fs - 1, 50).italic().fill_w()]).gap(14).align(Align::Stretch).pad_edges(Edges { top: 8, right: 28, bottom: 0, left: 34 }));
                    }
                }
                Block::Li(t) => {
                    has_text = true;
                    for (i, p) in pieces(t, 420).into_iter().enumerate() {
                        let bullet = text(if i == 0 { "•" } else { " " }).size(fs).w(18);
                        kids.push(row(vec![bullet, para(p, fs, 0).fill_w()]).align(Align::Start).pad_edges(Edges { top: 4, right: 28, bottom: 0, left: 34 }));
                    }
                }
                Block::Code(t) => {
                    has_text = true;
                    let lines: Vec<&str> = t.lines().collect();
                    for chunk in lines.chunks(14) {
                        kids.push(text(chunk.join("\n")).size(fs - 5).line_spacing(1.15).pad(14, 10).bg(232).radius(8.0).pad_edges(Edges { top: 8, right: 24, bottom: 0, left: 24 }));
                    }
                }
                Block::Img(f) => {
                    if let Some(img) = self.picture(f, false) {
                        kids.push(row(vec![spacer(), image(img), spacer()]).pad(0, 12));
                    }
                }
                Block::Hr => kids.push(row(vec![spacer(), text("*   *   *").size(fs).color(100), spacer()]).pad(0, 12)),
            }
        }
        if !has_text {
            kids.push(para("This feed only sends the headline. The full story is at the link below.".into(), fs - 2, 80).pad_edges(Edges { top: 12, right: 28, bottom: 0, left: 28 }));
        }
        if !s.link.is_empty() {
            kids.push(text(s.link.clone()).size(14).color(90).max_lines(3).pad_edges(Edges { top: 22, right: 28, bottom: 8, left: 28 }));
        }
        col(vec![bar, paged(kids, self.read_page, Msg::ReadPage).fill_h()]).bg(WHITE).block_taps().fill()
    }

    fn settings_sheet(&self) -> El<Msg> {
        let per = PER_FEED_CHOICES.iter().position(|&n| n == self.per_feed()).unwrap_or(0);
        let size = self.prefs.get_i64(TEXT_SIZE, 1).clamp(0, 2) as usize;
        let mb = store::disk_usage() as f64 / 1_048_576.0;
        let read = self.index.stories.iter().filter(|s| s.read).count();
        let font = match self.serif {
            Some(_) => "Stories use Bookerly when the Kindle has it, else Gentium Book Plus.",
            None => "No book font found; using the system font.",
        };
        let card_ = col(vec![
            text("News settings").size(26).bold(),
            setting_row("Stories per feed", Some("New ones to get from each feed"), segmented(&["5", "10"], per, Msg::PerFeed).w(150)),
            setting_row("Text size", None, segmented(&["S", "M", "L"], size, Msg::TextSize).w(190)),
            setting_row("Download pictures", Some("Up to 4 per story"), toggle(self.flag(IMAGES))).on_tap(Msg::Toggle(IMAGES)),
            setting_row("Turn Wi-Fi off after", Some("Only if News turned it on"), toggle(self.flag(WIFI_OFF))).on_tap(Msg::Toggle(WIFI_OFF)),
            divider(),
            text(format!("{} stories · {mb:.1} MB on the Kindle. {font}", self.index.stories.len())).size(16).color(80),
            row(vec![button_outline(&format!("Delete {read} read"), Msg::DeleteRead), spacer(), button("Close", Msg::Settings(false))]).gap(12),
        ])
        .gap(12)
        .padding(24)
        .bg(WHITE)
        .border(3.0, BLACK)
        .radius(18.0);
        col(vec![spacer(), card_, spacer()]).pad(20, 0).block_taps().on_tap(Msg::Settings(false))
    }
}

impl App for News {
    type Msg = Msg;

    fn setup(&mut self, fonts: &mut Fonts, _ctx: &mut Ctx<Msg>) {
        self.serif = assets::load_font(fonts, "Bookerly", "Bookerly").or_else(|| assets::load_font(fonts, "Gentium Book Plus", "GentiumBookPlus"));
        self.publish_glance();
    }

    fn view(&self, _env: &Env) -> El<Msg> {
        if let Some(s) = self.reading.as_ref().and_then(|id| self.index.get(id)) {
            return self.reader(s);
        }
        let base = if self.feeds_open { self.feeds_view() } else { self.stories_view() };
        if let Some(Dialog::DeleteFeed(id)) = &self.dialog {
            let n = self.index.count(id);
            let body = format!("Delete all {n} stories from {}? Fetching again won't bring them back, only newer ones.", self.feed_name(id));
            return dialog(base, "Delete stories?", &body, vec![button_outline("Cancel", Msg::CloseDialog), button("Delete", Msg::DeleteFeed(id.clone()))]);
        }
        if self.settings {
            return stack(vec![base, self.settings_sheet()]);
        }
        base
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Fetch(feed) => self.start_fetch(feed, ctx),
            Msg::StopFetch => {
                if let Some(j) = &mut self.job {
                    j.stop = true;
                    ctx.toast("Stopping after this feed…");
                }
            }
            Msg::Online(Ok(turned_on)) => {
                if let Some(j) = &mut self.job {
                    j.turned_on = turned_on;
                }
                self.next_feed(ctx);
            }
            Msg::Online(Err(e)) => {
                self.job = None;
                ctx.toast(e);
            }
            Msg::Fetched(r) => self.fetched(r, ctx),
            Msg::WifiOff => {}
            Msg::Open(id) => {
                self.body = store::load_body(&id);
                if let Some(s) = self.index.stories.iter_mut().find(|s| s.id == id) {
                    if !s.read {
                        s.read = true;
                        self.saved();
                    }
                }
                self.reading = Some(id);
                self.read_page = 0;
                ctx.full_refresh();
            }
            Msg::Back => {
                self.reading = None;
                self.body.clear();
                self.drop_pictures();
                ctx.full_refresh();
            }
            Msg::Delete(id) => {
                let in_reader = self.reading.as_deref() == Some(id.as_str());
                self.delete(&id);
                if in_reader {
                    self.reading = None;
                    self.body.clear();
                    self.drop_pictures();
                    ctx.full_refresh();
                }
                ctx.toast("Story deleted");
            }
            Msg::ReadPage(p) => self.read_page = p,
            Msg::Page(p) => self.page = p,
            Msg::Feeds(open) => {
                self.feeds_open = open;
                ctx.full_refresh();
            }
            Msg::FeedsPage(p) => self.feeds_page = p,
            Msg::Filter(f) => {
                self.filter = f;
                self.feeds_open = false;
                self.page = 0;
                ctx.full_refresh();
            }
            Msg::AskDeleteFeed(id) => self.dialog = Some(Dialog::DeleteFeed(id)),
            Msg::DeleteFeed(id) => {
                self.dialog = None;
                let n = self.index.delete_feed(&id);
                self.saved();
                ctx.toast(format!("Deleted {n} stor{}", if n == 1 { "y" } else { "ies" }));
            }
            Msg::DeleteRead => {
                let ids: Vec<String> = self.index.stories.iter().filter(|s| s.read).map(|s| s.id.clone()).collect();
                for id in &ids {
                    self.index.delete(id);
                }
                self.saved();
                ctx.toast(format!("Deleted {} read stor{}", ids.len(), if ids.len() == 1 { "y" } else { "ies" }));
            }
            Msg::CloseDialog => self.dialog = None,
            Msg::Settings(open) => self.settings = open,
            Msg::PerFeed(i) => self.prefs.set(PER_FEED, PER_FEED_CHOICES[i] as i64),
            Msg::TextSize(i) => self.prefs.set(TEXT_SIZE, i as i64),
            Msg::Toggle(k) => {
                let v = !self.flag(k);
                self.prefs.set(k, v);
            }
        }
    }
}

/// `news --fetch [N]`: download without the UI (testing aid).
fn fetch_cli(want: usize) {
    let mut index = Index::load();
    for f in feeds::load() {
        let r = fetch::fetch_feed(&f, index.known(&f.id()), want, true);
        let imgs = r.stories.iter().filter(|s| s.thumb.is_some()).count();
        println!("{:<30} {:>2} new, {imgs} with pictures{}", f.name, r.stories.len(), r.error.as_ref().map(|e| format!("  ERROR: {e}")).unwrap_or_default());
        let st = index.feeds.entry(r.feed.clone()).or_default();
        st.fetched_at = Some(r.at);
        st.error = r.error;
        index.stories.extend(r.stories);
    }
    index.last_fetch = Some(now());
    index.sort();
    index.save();
    println!("{} stories, {:.1} MB", index.stories.len(), store::disk_usage() as f64 / 1_048_576.0);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fetch") {
        fetch_cli(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(5));
        return;
    }
    if let Err(e) = inkpad::run(News::new()) {
        eprintln!("news: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn splits_paragraphs() {
        let text = "One sentence here. ".repeat(60);
        let p = super::pieces(text.trim(), 420);
        assert!(p.len() > 2);
        assert!(p.iter().all(|x| x.chars().count() <= 440));
        assert!(p[0].ends_with('.'));
        assert_eq!(p.join(" "), text.trim());
    }
}
