//! The expression being typed, and its evaluation.
//!
//! The expression is kept as plain ASCII (`11+3+(4*5.5)`) and shown with
//! proper signs (`11+3+(4×5.5)`). Evaluation is forgiving, like a calculator
//! should be while you type: missing `)` are closed for you, `2(3)` and
//! `(2)(3)` multiply, and `%` follows the usual calculator rule:
//! `50+10%` is 55 (10% of 50), while `10%` alone or `50*10%` is just /100.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Syntax,
    DivZero,
    TooBig,
}

impl Error {
    pub fn message(self) -> &'static str {
        match self {
            Error::Syntax => "Incomplete expression",
            Error::DivZero => "Can't divide by zero",
            Error::TooBig => "Number too big",
        }
    }
}

// ---------------------------------------------------------------- evaluate

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    Open,
    Close,
    Pct,
}

fn tokenize(s: &str) -> Result<Vec<Tok>, Error> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_ascii_digit() || ch == '.' {
            let start = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            // Exponent, as in a previous result like 1.5e-7.
            if i < c.len() && c[i] == 'e' {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '-' || c[j] == '+') {
                    j += 1;
                }
                if j < c.len() && c[j].is_ascii_digit() {
                    while j < c.len() && c[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let t: String = c[start..i].iter().collect();
            if t == "." {
                return Err(Error::Syntax);
            }
            out.push(Tok::Num(t.parse().map_err(|_| Error::Syntax)?));
            continue;
        }
        out.push(match ch {
            '+' | '-' | '*' | '/' => Tok::Op(ch),
            '(' => Tok::Open,
            ')' => Tok::Close,
            '%' => Tok::Pct,
            ' ' => {
                i += 1;
                continue;
            }
            _ => return Err(Error::Syntax),
        });
        i += 1;
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<Tok> {
        self.t.get(self.i).copied()
    }

    /// sum := product (('+'|'-') product)*
    fn sum(&mut self) -> Result<f64, Error> {
        let (mut v, _) = self.product()?;
        while let Some(Tok::Op(o @ ('+' | '-'))) = self.peek() {
            self.i += 1;
            let (r, pct) = self.product()?;
            // "a + b%" adds b percent of a.
            let r = if pct { v * r } else { r };
            v = if o == '+' { v + r } else { v - r };
        }
        Ok(v)
    }

    /// product := unary (('*'|'/')? unary)*   (no sign means 2(3) style)
    /// Returns whether it was a lone percentage (for the rule above).
    fn product(&mut self) -> Result<(f64, bool), Error> {
        let (mut v, mut pct) = self.unary()?;
        loop {
            match self.peek() {
                Some(Tok::Op(o @ ('*' | '/'))) => {
                    self.i += 1;
                    let (r, _) = self.unary()?;
                    if o == '/' && r == 0.0 {
                        return Err(Error::DivZero);
                    }
                    v = if o == '*' { v * r } else { v / r };
                }
                Some(Tok::Num(_)) | Some(Tok::Open) => v *= self.unary()?.0,
                _ => break,
            }
            pct = false;
        }
        Ok((v, pct))
    }

    /// unary := ('-'|'+') unary | primary '%'*
    fn unary(&mut self) -> Result<(f64, bool), Error> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.i += 1;
                let (v, p) = self.unary()?;
                return Ok((-v, p));
            }
            Some(Tok::Op('+')) => {
                self.i += 1;
                return self.unary();
            }
            _ => {}
        }
        let mut v = self.primary()?;
        let mut pct = false;
        while self.peek() == Some(Tok::Pct) {
            self.i += 1;
            v /= 100.0;
            pct = true;
        }
        Ok((v, pct))
    }

    /// primary := number | '(' sum ')'?   (a missing ')' at the end is fine)
    fn primary(&mut self) -> Result<f64, Error> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(n)
            }
            Some(Tok::Open) => {
                self.i += 1;
                let v = self.sum()?;
                match self.peek() {
                    Some(Tok::Close) => self.i += 1,
                    None => {}
                    _ => return Err(Error::Syntax),
                }
                Ok(v)
            }
            _ => Err(Error::Syntax),
        }
    }
}

/// Evaluate an ASCII expression.
pub fn eval(s: &str) -> Result<f64, Error> {
    let t = tokenize(s)?;
    if t.is_empty() {
        return Err(Error::Syntax);
    }
    let mut p = Parser { t, i: 0 };
    let v = p.sum()?;
    if p.i != p.t.len() {
        return Err(Error::Syntax);
    }
    if !v.is_finite() || v.abs() >= 1e100 {
        return Err(Error::TooBig);
    }
    Ok(v)
}

/// Evaluate what's typed so far, ignoring a trailing operator or `(`:
/// `11+3+(` previews as 14.
pub fn preview(s: &str) -> Result<f64, Error> {
    eval(s.trim_end_matches(|c| "+-*/(".contains(c)))
}

/// A number as the calculator shows it: up to 12 significant digits, no
/// float noise (0.1+0.2 = 0.3), scientific notation for huge or tiny ones.
pub fn format(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let a = v.abs();
    if a >= 1e15 || a < 1e-9 {
        let s = format!("{:.9e}", v);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        return format!("{}e{}", trim_zeros(m), e);
    }
    let int_digits = a.log10().floor() as i32 + 1;
    let decimals = (12 - int_digits).clamp(0, 12) as usize;
    let s = trim_zeros(&format!("{:.*}", decimals, v));
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// The expression with real math signs, for display.
pub fn pretty(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let c: Vec<char> = s.chars().collect();
    for (i, &ch) in c.iter().enumerate() {
        out.push(match ch {
            '*' => '×',
            '/' => '÷',
            // Keep the minus of an exponent (1e-7) plain.
            '-' if i > 0 && c[i - 1] == 'e' => '-',
            '-' => '−',
            _ => ch,
        });
    }
    out
}

/// Turn typed text (keyboard, or a pasted-looking `11 x 3`) into the ASCII form.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter_map(|ch| match ch {
            '×' | 'x' | 'X' | '*' => Some('*'),
            '÷' | '/' | ':' => Some('/'),
            '−' | '–' | '-' => Some('-'),
            ',' => Some('.'),
            '0'..='9' | '.' | '+' | '(' | ')' | '%' | 'e' => Some(ch),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- typing

fn is_op(c: char) -> bool {
    "+-*/".contains(c)
}

/// What's on the calculator: the expression being typed, and the last
/// finished calculation.
#[derive(Default, Debug)]
pub struct Input {
    /// ASCII expression.
    pub expr: String,
    /// The last calculation: (expression as typed, result).
    pub last: Option<(String, String)>,
    /// `expr` holds a result just computed with "=": a digit starts over,
    /// an operator continues from it.
    pub fresh: bool,
    pub error: Option<Error>,
}

impl Input {
    fn last_char(&self) -> Option<char> {
        self.expr.chars().last()
    }

    /// The digits (and dot) of the number at the end, if any.
    fn tail_number(&self) -> &str {
        let start = self.expr.rfind(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(0, |i| i + 1);
        &self.expr[start..]
    }

    fn open_parens(&self) -> i32 {
        self.expr.chars().map(|c| match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        }).sum()
    }

    fn start_typing(&mut self) {
        self.error = None;
        if self.fresh {
            self.expr.clear();
            self.fresh = false;
        }
    }

    /// After `)`, `%` or a result, a number or `(` multiplies.
    fn times_if_needed(&mut self) {
        if matches!(self.last_char(), Some(')') | Some('%')) {
            self.expr.push('*');
        }
    }

    pub fn digit(&mut self, d: char) {
        self.start_typing();
        self.times_if_needed();
        if self.tail_number() == "0" {
            self.expr.pop(); // 07 -> 7
        }
        self.expr.push(d);
    }

    pub fn dot(&mut self) {
        self.start_typing();
        if self.tail_number().contains('.') {
            return;
        }
        self.times_if_needed();
        if !self.last_char().is_some_and(|c| c.is_ascii_digit()) {
            self.expr.push('0');
        }
        self.expr.push('.');
    }

    pub fn op(&mut self, o: char) {
        self.error = None;
        self.fresh = false; // continue from the result
        match self.last_char() {
            None => {
                if o == '-' {
                    self.expr.push('-');
                } else {
                    self.expr.push('0');
                    self.expr.push(o);
                }
            }
            Some('(') => {
                if o == '-' {
                    self.expr.push('-');
                }
            }
            Some(c) if is_op(c) => {
                // 5× then − : a negative number (5×−3).
                if o == '-' && (c == '*' || c == '/') {
                    self.expr.push('-');
                    return;
                }
                while self.last_char().is_some_and(is_op) {
                    self.expr.pop();
                }
                if self.expr.is_empty() || self.last_char() == Some('(') {
                    if o == '-' {
                        self.expr.push('-');
                    } else if self.expr.is_empty() {
                        self.expr.push('0');
                        self.expr.push(o);
                    }
                } else {
                    self.expr.push(o);
                }
            }
            Some(_) => self.expr.push(o),
        }
    }

    pub fn percent(&mut self) {
        self.error = None;
        if self.last_char().is_some_and(|c| c.is_ascii_digit() || c == ')' || c == '.') {
            self.fresh = false;
            self.expr.push('%');
        }
    }

    /// One key for both parentheses: opens one where a number could start,
    /// closes one where a number just ended (if any are open).
    pub fn paren(&mut self) {
        self.start_typing();
        match self.last_char() {
            None | Some('(') => self.expr.push('('),
            Some(c) if is_op(c) => self.expr.push('('),
            Some(_) if self.open_parens() > 0 => self.expr.push(')'),
            Some(_) => {
                self.expr.push('*');
                self.expr.push('(');
            }
        }
    }

    pub fn back(&mut self) {
        self.error = None;
        if self.fresh {
            self.clear();
        } else {
            self.expr.pop();
        }
    }

    pub fn clear(&mut self) {
        *self = Input::default();
    }

    /// "=": compute, keep the calculation as history and show the result.
    pub fn equals(&mut self) {
        if self.expr.is_empty() || self.fresh {
            return;
        }
        match eval(&self.expr) {
            Ok(v) => {
                let mut shown = self.expr.clone();
                for _ in 0..self.open_parens().max(0) {
                    shown.push(')');
                }
                let r = format(v);
                self.last = Some((shown, r.clone()));
                self.expr = r;
                self.fresh = true;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Bring back the last calculation's expression to edit it.
    pub fn recall(&mut self) {
        if let Some((e, _)) = &self.last {
            self.expr = e.clone();
            self.fresh = false;
            self.error = None;
        }
    }

    /// Replace the whole expression (typed on the keyboard).
    pub fn set(&mut self, s: &str) {
        self.expr = normalize(s);
        self.fresh = false;
        self.error = None;
    }

    /// The live result while typing: only when there's something to compute
    /// (not for a lone number).
    pub fn live(&self) -> Option<String> {
        if self.fresh || !self.expr.chars().skip(1).any(|c| is_op(c) || c == '%' || c == '(') {
            return None;
        }
        preview(&self.expr).ok().map(format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> String {
        eval(s).map(format).unwrap_or_else(|e| format!("{e:?}"))
    }

    #[test]
    fn evaluates() {
        assert_eq!(ev("11+3+(4*5.5)"), "36");
        assert_eq!(ev("2+3*4"), "14");
        assert_eq!(ev("(2+3)*4"), "20");
        assert_eq!(ev("2(3+1)"), "8");
        assert_eq!(ev("(1+1)(2+2)"), "8");
        assert_eq!(ev("-3*-2"), "6");
        assert_eq!(ev("10-2-3"), "5");
        assert_eq!(ev("8/2/2"), "2");
        assert_eq!(ev("0.1+0.2"), "0.3");
        assert_eq!(ev("1/3"), "0.333333333333");
        assert_eq!(ev("2*(3+4"), "14");
        assert_eq!(ev("1/0"), "DivZero");
        assert_eq!(ev("3+"), "Syntax");
        assert_eq!(ev("5."), "5");
        assert_eq!(ev("1.5e-7*2"), "0.0000003");
    }

    #[test]
    fn percent() {
        assert_eq!(ev("50+10%"), "55");
        assert_eq!(ev("50-10%"), "45");
        assert_eq!(ev("50*10%"), "5");
        assert_eq!(ev("10%"), "0.1");
    }

    #[test]
    fn formats() {
        assert_eq!(format(1234567.0), "1234567");
        assert_eq!(format(-0.5), "-0.5");
        assert_eq!(format(1e20), "1e20");
        assert_eq!(format(2.0 / 3.0), "0.666666666667");
        assert_eq!(pretty("4*5.5-2/1e-7"), "4×5.5−2÷1e-7");
    }

    fn typed(keys: &str) -> Input {
        let mut i = Input::default();
        for k in keys.chars() {
            match k {
                '0'..='9' => i.digit(k),
                '.' => i.dot(),
                '+' | '-' | '*' | '/' => i.op(k),
                '%' => i.percent(),
                'p' => i.paren(),
                '<' => i.back(),
                '=' => i.equals(),
                _ => unreachable!(),
            }
        }
        i
    }

    #[test]
    fn typing() {
        assert_eq!(typed("11+3+p4*5.5p").expr, "11+3+(4*5.5)");
        assert_eq!(typed("11+3+p4*5.5").live().as_deref(), Some("36"));
        assert_eq!(typed("5+*").expr, "5*", "operator replaces operator");
        assert_eq!(typed("5*-3").expr, "5*-3", "minus after times is a sign");
        assert_eq!(typed("5*-+").expr, "5+");
        assert_eq!(typed("..5.").expr, "0.5");
        assert_eq!(typed("007").expr, "7");
        assert_eq!(typed("2p3").expr, "2*(3", "paren after a number multiplies");
        assert_eq!(typed("p2+3pp").expr, "(2+3)*(");
        assert_eq!(typed("+2").expr, "0+2");
        assert_eq!(typed("12<").expr, "1");
        let i = typed("2+3=");
        assert_eq!((i.expr.as_str(), i.fresh), ("5", true));
        assert_eq!(i.last, Some(("2+3".into(), "5".into())));
        assert_eq!(typed("2+3=*2").expr, "5*2", "operator continues the result");
        assert_eq!(typed("2+3=7").expr, "7", "digit starts over");
        assert_eq!(typed("p2+3=").last.unwrap().0, "(2+3)", "closes parens");
        assert_eq!(typed("1/0=").error, Some(Error::DivZero));
        assert_eq!(typed("12").live(), None, "a lone number has no preview");
    }
}
