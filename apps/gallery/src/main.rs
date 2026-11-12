//! inkpad widget gallery: every widget, on five tabs.

use inkpad::prelude::*;
use inkpad::ui::{assets, clock, icons};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Home,
    Tasks,
    Widgets,
    Fonts,
    Media,
}

const TABS: [(Tab, &str, &str); 5] = [
    (Tab::Home, "house", "Home"),
    (Tab::Tasks, "list-todo", "Tasks"),
    (Tab::Widgets, "settings", "Widgets"),
    (Tab::Fonts, "type", "Fonts"),
    (Tab::Media, "image", "Media"),
];

#[derive(Clone, Debug)]
enum Msg {
    Tab(Tab),
    Toggle(usize),
    Page(usize),
    Hold(usize),
    AddTask,
    Added(Option<String>),
    Dialog(bool),
    Sync,
    Synced(u32),
    Switch(usize),
    IconPage(usize),
}

struct Gallery {
    tab: Tab,
    tasks: Vec<(String, bool)>,
    page: usize,
    icon_page: usize,
    switches: [bool; 2],
    dialog: bool,
    syncing: bool,
    syncs: u32,
    serif: Option<FamilyId>,
}

impl Gallery {
    fn new() -> Self {
        let mut tasks: Vec<(String, bool)> = [
            "Comprar pão, café e frutas para a semana",
            "Reply to the landlord about the lease renewal before Friday",
            "Dentist · Tue 19 Nov 14:30",
            "Read chapter 4 of “Predictably Irrational”",
            "Pay electricity bill",
            "Call mom",
            "Renew passport (photos first!)",
            "Water the plants",
            "Back up the Kindle",
            "Plan the weekend trip to the mountains",
            "Fix the bike's rear brake",
            "Send invoice #42",
            "Book flights for December",
            "Try the new bread recipe",
            "Clean the inbox",
            "Write the inkpad README",
            "Buy a birthday present for Ana",
            "Cancel unused subscriptions",
        ]
        .iter()
        .map(|s| (s.to_string(), false))
        .collect();
        tasks[3].1 = true;
        tasks[4].1 = true;
        Gallery { tab: Tab::Home, tasks, page: 0, icon_page: 0, switches: [true, false], dialog: false, syncing: false, syncs: 0, serif: None }
    }

    fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.1).count()
    }

    fn tab_bar(&self) -> El<Msg> {
        let tabs = TABS
            .iter()
            .map(|&(t, ic, label)| {
                let sel = t == self.tab;
                let fg = if sel { WHITE } else { BLACK };
                let item = col(vec![icon(ic, 26).color(fg), text(label).size(15).bold().color(fg).center_text()]).gap(2).align(Align::Center).pad(0, 8).fill_w().on_tap(Msg::Tab(t));
                if sel { item.bg(BLACK).radius(14.0) } else { item }
            })
            .collect();
        col(vec![rule(2, BLACK), row(tabs).gap(6).pad(10, 8)])
    }

    fn home(&self, env: &Env) -> El<Msg> {
        let now = env.now;
        let next = self.tasks.iter().find(|t| !t.1).map(|t| t.0.clone()).unwrap_or_else(|| "nothing left!".into());
        col(vec![
            text(now.hm()).size(120).bold().center_text(),
            text(now.long_date()).size(26).center_text().color(60),
            empty().h(16),
            card(col(vec![
                row(vec![icon("list-todo", 28), text("Tasks").size(24).bold().fill_w(), text(format!("{} open", self.open_count())).size(20).color(70)]).gap(12),
                text(format!("Next: {next}")).size(21).max_lines(2),
            ])
            .gap(10))
            .on_tap(Msg::Tab(Tab::Tasks)),
            card(col(vec![
                row(vec![icon("calendar", 28), text("Today").size(24).bold().fill_w()]).gap(12),
                text("No calendar connected yet").size(20).color(90),
            ])
            .gap(10)),
            card(row(vec![icon("sparkles", 28), text("Ask Claude…").size(22).color(90).fill_w(), icon("chevron-right", 26)]).gap(12)),
        ])
        .gap(16)
        .pad(28, 24)
    }

    fn tasks(&self) -> El<Msg> {
        let items: Vec<El<Msg>> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, (title, done))| {
                row(vec![checkbox(*done), text(title.clone()).size(22).color(if *done { 120 } else { BLACK }).max_lines(2).fill_w()])
                    .gap(16)
                    .pad(28, 14)
                    .on_tap(Msg::Toggle(i))
                    .on_hold(Msg::Hold(i))
            })
            .collect();
        let sub = format!("{} open · hold a task to read it", self.open_count());
        col(vec![title_bar("Tasks", Some(&sub), vec![icon_button("plus", Msg::AddTask)]), paged(items, self.page, Msg::Page)])
    }

    fn widgets(&self) -> El<Msg> {
        let sw = |i: usize, label: &str, hint: &str| {
            row(vec![col(vec![text(label).size(22).bold(), text(hint).size(17).color(90)]).gap(2).fill_w(), toggle(self.switches[i])]).gap(12).pad(0, 6).on_tap(Msg::Switch(i))
        };
        let sync_label = if self.syncing { "Syncing…".to_string() } else { format!("Synced {} times", self.syncs) };
        let base = col(vec![
            title_bar("Widgets", Some("Buttons, switches, dialogs"), vec![icon_button("refresh-cw", Msg::Sync)]),
            col(vec![
                row(vec![button("Add task", Msg::AddTask), button_outline("Dialog", Msg::Dialog(true))]).gap(12),
                row(vec![button_icon("refresh-cw", "Sync", false, Msg::Sync), text(sync_label).size(18).color(80).fill_w()]).gap(16),
                divider(),
                sw(0, "Sync on wake", "Turn Wi-Fi on briefly when opened"),
                sw(1, "Sleep screen", "Show tasks while the Kindle sleeps"),
                divider(),
                text("Progress").size(18).bold().color(60),
                progress(self.open_count() as f32 / self.tasks.len() as f32, 22),
                row(vec![chip("e-ink"), chip("Rust"), chip("offline-first"), chip("PT-BR")]).gap(8),
            ])
            .gap(18)
            .pad(28, 18),
        ]);
        if self.dialog {
            dialog(
                base,
                "Delete all done tasks?",
                "This is a demo dialog: it blocks taps underneath until you choose.",
                vec![button_outline("Cancel", Msg::Dialog(false)), button("Delete", Msg::Dialog(false))],
            )
        } else {
            base
        }
    }

    fn fonts(&self) -> El<Msg> {
        let para = "Era uma vez uma pequena tela de papel eletrônico que queria ser mais do que um livro. Ação, ênfase, coração.";
        let mut kids = vec![
            title_bar("Fonts", Some("Bundled sans + any font from disk"), vec![]),
        ];
        let mut body = vec![
            text("Atkinson Hyperlegible").size(18).bold().color(80),
            text(para).size(22),
            text("Bold 28 · Regular 18").size(28).bold(),
        ];
        match self.serif {
            Some(f) => {
                body.push(divider());
                body.push(text("Bookerly (loaded from disk)").size(18).bold().color(80));
                body.push(text(para).size(24).family(f).line_spacing(1.1));
                body.push(text("Bold, and italic too.").size(24).family(f).bold());
                body.push(text("A quiet italic line for emphasis.").size(24).family(f).italic());
            }
            None => body.push(text("Put Bookerly-Regular.ttf in assets/fonts (PC) or /mnt/us/fonts (Kindle) to see a serif here.").size(19).color(90)),
        }
        kids.push(col(body).gap(14).pad(28, 18));
        col(kids)
    }

    fn media(&self) -> El<Msg> {
        let photo = assets::image("images/mountains.png", 544, 272, true).map(|i| image(i)).unwrap_or_else(|| text("images/mountains.png not found").size(18));
        let cells: Vec<El<Msg>> = icons::NAMES
            .iter()
            .map(|n| col(vec![icon(n, 34), text(*n).size(12).color(80).center_text().max_lines(1)]).gap(4).align(Align::Center).w(88).pad(0, 6))
            .collect();
        let mut grid_rows = Vec::new();
        let mut it = cells.into_iter();
        loop {
            let chunk: Vec<El<Msg>> = it.by_ref().take(6).collect();
            if chunk.is_empty() {
                break;
            }
            grid_rows.push(row(chunk).gap(2));
        }
        col(vec![
            title_bar("Media", Some("PNG + SVG, dithered to 16 grays"), vec![]),
            col(vec![photo]).pad(28, 14),
            paged(grid_rows, self.icon_page, Msg::IconPage).pad(10, 0),
        ])
    }
}

impl App for Gallery {
    type Msg = Msg;

    fn setup(&mut self, fonts: &mut Fonts, _ctx: &mut Ctx<Msg>) {
        self.serif = assets::load_font(fonts, "Bookerly", "Bookerly");
    }

    fn view(&self, env: &Env) -> El<Msg> {
        let page = match self.tab {
            Tab::Home => self.home(env),
            Tab::Tasks => self.tasks(),
            Tab::Widgets => self.widgets(),
            Tab::Fonts => self.fonts(),
            Tab::Media => self.media(),
        };
        col(vec![page.fill_h(), self.tab_bar()])
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Tab(t) => {
                if t != self.tab {
                    self.tab = t;
                    ctx.full_refresh();
                }
            }
            Msg::Toggle(i) => self.tasks[i].1 = !self.tasks[i].1,
            Msg::Page(p) => self.page = p,
            Msg::IconPage(p) => self.icon_page = p,
            Msg::Hold(i) => ctx.toast(self.tasks[i].0.clone()),
            Msg::AddTask => ctx.prompt("New task", "", Msg::Added),
            Msg::Added(Some(t)) if !t.trim().is_empty() => {
                self.tasks.insert(0, (t.trim().to_string(), false));
                self.page = 0;
                self.tab = Tab::Tasks;
                ctx.toast("Task added");
            }
            Msg::Added(_) => {}
            Msg::Dialog(open) => self.dialog = open,
            Msg::Sync => {
                if !self.syncing {
                    self.syncing = true;
                    let n = self.syncs + 1;
                    ctx.spawn(move || {
                        std::thread::sleep(Duration::from_millis(1200)); // pretend network
                        Msg::Synced(n)
                    });
                }
            }
            Msg::Synced(n) => {
                self.syncing = false;
                self.syncs = n;
                ctx.toast(format!("Synced at {}", clock::now().hm()));
            }
            Msg::Switch(i) => self.switches[i] = !self.switches[i],
        }
    }
}

fn main() {
    if let Err(e) = inkpad::run(Gallery::new()) {
        eprintln!("gallery: {e}");
        std::process::exit(1);
    }
}
