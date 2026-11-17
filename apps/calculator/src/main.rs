//! Calculator: an iPhone-style keypad that writes the whole expression as you
//! type it (`11+3+(4×5.5)`), with the result updating live underneath.
//!
//! - **( )** is one key: it opens a parenthesis where a number could start
//!   and closes one where a number just ended. Missing `)` are added for you.
//! - **⌫** deletes one character; hold it (or tap **AC**) to clear.
//! - Tap the last calculation to bring it back; tap the expression to type
//!   it on the keyboard.
//!
//! An inkpad app, developed separately from inkpad itself.

mod expr;

use expr::Input;
use inkpad::prelude::*;

#[derive(Clone, Debug)]
enum Msg {
    Digit(char),
    Dot,
    Op(char),
    Percent,
    Paren,
    Back,
    Clear,
    Equals,
    Recall,
    Type,
    Typed(Option<String>),
}

#[derive(Default)]
struct Calculator {
    input: Input,
}

/// Key size and spacing (the panel is 600 px wide).
const KEY: u32 = 96;
const GAP_X: u32 = 20;
const GAP_Y: u32 = 12;

/// Key colors: digits light, functions mid gray, operators black.
const DIGIT_BG: u8 = 225;
const FN_BG: u8 = 170;

fn key(label: El<Msg>, bg: u8, msg: Msg) -> El<Msg> {
    row(vec![label]).justify(Justify::Center).w(KEY).h(KEY).radius(KEY as f32 / 2.0).bg(bg).on_tap(msg)
}

fn digit(d: char) -> El<Msg> {
    key(text(d.to_string()).size(46), DIGIT_BG, Msg::Digit(d))
}

fn func(label: &str, msg: Msg) -> El<Msg> {
    key(text(label).size(34).bold(), FN_BG, msg)
}

fn op(sign: &str, msg: Msg) -> El<Msg> {
    key(text(sign).size(52).bold().color(WHITE), BLACK, msg)
}

/// Split the expression into lines that fit, at a size that shrinks as it
/// grows. Very long ones keep their end visible (that's where you type).
fn expr_lines(s: &str) -> (u16, Vec<String>) {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    for (max, size) in [(10, 84), (13, 66), (17, 52), (22, 40)] {
        if n <= max {
            return (size, vec![s.to_string()]);
        }
    }
    const PER_LINE: usize = 24;
    const MAX_LINES: usize = 3;
    // Break after an operator or "(", never inside a number (unless one
    // number alone is longer than a line).
    let mut lines: Vec<String> = vec![String::new()];
    let mut piece = String::new();
    let flush =|piece: &mut String, lines: &mut Vec<String>| {
        let cur = lines.last_mut().unwrap();
        if !cur.is_empty() && cur.chars().count() + piece.chars().count() > PER_LINE {
            lines.push(String::new());
        }
        for ch in piece.drain(..) {
            if lines.last().unwrap().chars().count() >= PER_LINE {
                lines.push(String::new());
            }
            lines.last_mut().unwrap().push(ch);
        }
    };
    for &ch in &chars {
        piece.push(ch);
        if "+−×÷(".contains(ch) {
            flush(&mut piece, &mut lines);
        }
    }
    flush(&mut piece, &mut lines);
    if lines.len() > MAX_LINES {
        lines.drain(..lines.len() - MAX_LINES);
        let first = lines[0].chars().next().map_or(0, char::len_utf8);
        lines[0].replace_range(..first, "…");
    }
    (36, lines)
}

impl Calculator {
    fn display(&self) -> El<Msg> {
        let i = &self.input;
        let mut kids: Vec<El<Msg>> = vec![spacer()];
        if let Some((e, r)) = &i.last {
            let shown = if i.fresh { format!("{} =", expr::pretty(e)) } else { format!("{} = {}", expr::pretty(e), expr::pretty(r)) };
            kids.push(text(shown).size(24).color(110).max_lines(1).align_text(Align::End).fill_w().pad(0, 6).on_tap(Msg::Recall));
        }
        let shown = if i.expr.is_empty() { "0".to_string() } else { expr::pretty(&i.expr) };
        let (size, lines) = expr_lines(&shown);
        for l in lines {
            let t = text(l).size(size).max_lines(1).align_text(Align::End).fill_w();
            kids.push(if i.fresh { t.bold() } else { t });
        }
        let under = match (i.error, i.live()) {
            (Some(e), _) => text(e.message()).size(26).bold(),
            (None, Some(v)) => text(format!("= {}", expr::pretty(&v))).size(34).color(90),
            (None, None) => empty().h(40),
        };
        kids.push(under.align_text(Align::End).fill_w());
        col(kids).gap(2).pad(30, 6).fill_h().on_tap(Msg::Type)
    }

    fn keypad(&self) -> El<Msg> {
        let i = &self.input;
        let clear = if i.expr.is_empty() || i.fresh {
            func("AC", Msg::Clear)
        } else {
            key(icon("delete", 40), FN_BG, Msg::Back).on_hold(Msg::Clear)
        };
        let r = |keys: Vec<El<Msg>>| row(keys).gap(GAP_X);
        let zero = row(vec![text("0").size(46)]).pad(40, 0).w(KEY * 2 + GAP_X).h(KEY).radius(KEY as f32 / 2.0).bg(DIGIT_BG).on_tap(Msg::Digit('0'));
        col(vec![
            r(vec![clear, func("( )", Msg::Paren), func("%", Msg::Percent), op("÷", Msg::Op('/'))]),
            r(vec![digit('7'), digit('8'), digit('9'), op("×", Msg::Op('*'))]),
            r(vec![digit('4'), digit('5'), digit('6'), op("−", Msg::Op('-'))]),
            r(vec![digit('1'), digit('2'), digit('3'), op("+", Msg::Op('+'))]),
            r(vec![zero, key(text(".").size(46).bold(), DIGIT_BG, Msg::Dot), op("=", Msg::Equals)]),
        ])
        .gap(GAP_Y)
        .align(Align::Center)
    }
}

impl App for Calculator {
    type Msg = Msg;

    fn view(&self, _env: &Env) -> El<Msg> {
        col(vec![self.display(), self.keypad(), empty().h(16)]).fill()
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        let i = &mut self.input;
        match msg {
            Msg::Digit(d) => i.digit(d),
            Msg::Dot => i.dot(),
            Msg::Op(o) => i.op(o),
            Msg::Percent => i.percent(),
            Msg::Paren => i.paren(),
            Msg::Back => i.back(),
            Msg::Clear => i.clear(),
            Msg::Equals => i.equals(),
            Msg::Recall => i.recall(),
            Msg::Type => {
                let current = if i.fresh { String::new() } else { i.expr.clone() };
                ctx.prompt("Type a calculation", &current, Msg::Typed);
            }
            Msg::Typed(Some(s)) => i.set(&s),
            Msg::Typed(None) => {}
        }
    }
}

fn main() {
    if let Err(e) = inkpad::run(Calculator::default()) {
        eprintln!("calculator: {e}");
        std::process::exit(1);
    }
}
