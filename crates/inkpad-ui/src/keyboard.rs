//! On-screen keyboard and the full-screen text prompt that uses it.

use crate::el::*;
use crate::widgets::{button, button_outline};
use inkpad_gfx::{BLACK, WHITE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Lower,
    Upper,
    Numbers,
    Accents,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyAct {
    Char(char),
    Back,
    Shift,
    Layer(Layer),
    Enter,
}

pub struct PromptState {
    pub title: String,
    pub text: String,
    pub layer: Layer,
    pub multiline: bool,
}

impl PromptState {
    pub fn new(title: &str, initial: &str, multiline: bool) -> Self {
        PromptState { title: title.into(), text: initial.into(), layer: Layer::Lower, multiline }
    }

    /// Apply a key. Returns true when the user pressed Enter on a
    /// single-line prompt (= submit).
    pub fn key(&mut self, k: &KeyAct) -> bool {
        match k {
            KeyAct::Char(c) => {
                self.text.push(*c);
                if self.layer == Layer::Upper {
                    self.layer = Layer::Lower;
                }
            }
            KeyAct::Back => {
                self.text.pop();
            }
            KeyAct::Shift => self.layer = if self.layer == Layer::Upper { Layer::Lower } else { Layer::Upper },
            KeyAct::Layer(l) => self.layer = *l,
            KeyAct::Enter => {
                if self.multiline {
                    self.text.push('\n');
                } else {
                    return true;
                }
            }
        }
        false
    }
}

const KEY_H: u32 = 62;

fn rows(layer: Layer) -> [&'static str; 3] {
    match layer {
        Layer::Lower => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        Layer::Upper => ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
        Layer::Numbers => ["1234567890", "-/:;()$&@\"", ".,?!'#%*+="],
        Layer::Accents => ["áàâãéêíóôõ", "úüçñÁÉÓÚÇ", "ªº€£¿¡°…–"],
    }
}

fn key<M>(label: El<M>, act: KeyAct, wrap: &dyn Fn(KeyAct) -> M, weight: u16, dark: bool) -> El<M> {
    let k = label.width(Size::Fill(weight)).h(KEY_H - 8).radius(10.0).on_tap(wrap(act));
    if dark { k.bg(BLACK) } else { k.border(2.0, BLACK).bg(WHITE) }
}

fn ch_key<M>(c: char, wrap: &dyn Fn(KeyAct) -> M) -> El<M> {
    key(text(c.to_string()).size(26).center_text().pad(0, 10), KeyAct::Char(c), wrap, 2, false)
}

/// The keyboard element. `wrap` turns key actions into messages.
pub fn keyboard<M>(layer: Layer, wrap: &dyn Fn(KeyAct) -> M) -> El<M> {
    let [r1, r2, r3] = rows(layer);
    let row1 = row(r1.chars().map(|c| ch_key(c, wrap)).collect()).gap(6);
    let mut row2: Vec<El<M>> = Vec::new();
    if r2.chars().count() < 10 {
        row2.push(empty().width(Size::Fill(1)));
    }
    row2.extend(r2.chars().map(|c| ch_key(c, wrap)));
    if r2.chars().count() < 10 {
        row2.push(empty().width(Size::Fill(1)));
    }
    let mut row3: Vec<El<M>> = Vec::new();
    let letters = matches!(layer, Layer::Lower | Layer::Upper);
    if letters {
        let shift_dark = layer == Layer::Upper;
        let ic = icon("arrow-big-up", 28).color(if shift_dark { WHITE } else { BLACK });
        row3.push(key(ic, KeyAct::Shift, wrap, 3, shift_dark));
    }
    row3.extend(r3.chars().map(|c| ch_key(c, wrap)));
    row3.push(key(icon("delete", 28), KeyAct::Back, wrap, if letters { 3 } else { 2 }, false));
    let mode = |label: &str, l: Layer| key(text(label).size(20).bold().center_text().pad(0, 12), KeyAct::Layer(l), wrap, 3, false);
    let row4 = row(vec![
        if layer == Layer::Numbers { mode("abc", Layer::Lower) } else { mode("?123", Layer::Numbers) },
        if layer == Layer::Accents { mode("abc", Layer::Lower) } else { mode("áé", Layer::Accents) },
        key(text("space").size(20).color(90).center_text().pad(0, 12), KeyAct::Char(' '), wrap, 10, false),
        key(icon("corner-down-left", 28).color(WHITE), KeyAct::Enter, wrap, 4, true),
    ])
    .gap(6);
    col(vec![row1, row(row2).gap(6), row(row3).gap(6), row4]).gap(8).pad(8, 10).bg(WHITE).border(0.0, BLACK)
}

/// Full-screen prompt: title, the text so far, buttons, keyboard.
pub fn prompt_view<M>(p: &PromptState, wrap: &dyn Fn(KeyAct) -> M, cancel: M, done: M) -> El<M> {
    let shown = format!("{}|", p.text);
    let field = text(shown).size(26).max_lines(5).padding(16).border(2.5, BLACK).radius(12.0).fill_w();
    let head = col(vec![
        text(p.title.clone()).size(30).bold(),
        field,
        row(vec![spacer(), button_outline("Cancel", cancel), button("Done", done)]).gap(12),
    ])
    .gap(18)
    .pad(24, 20);
    col(vec![head, spacer(), rule(2, BLACK), keyboard(p.layer, wrap)]).bg(WHITE).block_taps().fill()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_shift() {
        let mut p = PromptState::new("t", "", false);
        p.key(&KeyAct::Shift);
        assert_eq!(p.layer, Layer::Upper);
        p.key(&KeyAct::Char('A'));
        assert_eq!(p.layer, Layer::Lower, "shift is one-shot");
        p.key(&KeyAct::Char('ç'));
        p.key(&KeyAct::Back);
        p.key(&KeyAct::Char('b'));
        assert_eq!(p.text, "Ab");
        assert!(p.key(&KeyAct::Enter), "enter submits single-line");
        let mut m = PromptState::new("t", "x", true);
        assert!(!m.key(&KeyAct::Enter));
        assert_eq!(m.text, "x\n");
    }
}
