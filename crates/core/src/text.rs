//! The text term format: what people type, agents produce, and logs and the API carry.
//!
//! This module parses text to a syntax tree that keeps what was written, and prints a syntax tree
//! back. Turning a tree into a term needs a context and lives in [`crate::elab`]. The grammar,
//! loosest binding first:
//!
//! ```text
//! sum     := [+|-] tensor ((+|-) tensor)*
//! tensor  := comp ((x) | ⊗) comp)*
//! comp    := factor ([*] factor)*          juxtaposition is composition: a b = a ∘ b
//! factor  := atom [^ int | ^{intexpr}]
//! atom    := int [/ int] | I | name [_sub] [(sum, …)] | e(colours) | 1_obj | (sum)
//!          | sum_{f=lo..hi} comp            a sum over an integer range; empty if hi < lo
//! ```
//!
//! Unicode forms are accepted for `ψ`, `π`, `⊗`, `−`, `·` and `∘`; the printer writes ASCII
//! except for names that were typed in Unicode.

use crate::error::{Error, Result};
use crate::expr::{TermInt, parse_term_int};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ast {
    Int(BigInt),
    Frac(BigInt, BigInt),
    /// The square root of −1 in ℤ[√−1].
    Imag,
    /// A generator, parameter, unknown or definition: `x_1`, `t_ij`, `c1`, `Q_ab(x_1, x_2)`.
    Name {
        name: String,
        sub: Option<String>,
        args: Option<Vec<Ast>>,
    },
    /// An idempotent `e(...)`, with the colours as written.
    Idem(String),
    /// An identity `1_obj`, with the object as written.
    Id(String),
    Pow(Box<Ast>, i64),
    /// A power whose exponent is an integer expression: `x_1^{h(i) - 1 - f}`.
    PowExpr(Box<Ast>, TermInt),
    /// `sum_{var=lo..hi} body`: the sum of `body` for `var` from `lo` to `hi`.
    Range {
        var: String,
        lo: TermInt,
        hi: TermInt,
        body: Box<Ast>,
    },
    /// Composition, top factor first: `a b` is `[a, b]`.
    Comp(Vec<Ast>),
    /// Tensor product, left factor first.
    Tensor(Vec<Ast>),
    /// A sum of signed summands (`true` = negated).
    Sum(Vec<(bool, Ast)>),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(BigInt),
    Name(String),
    /// `_` followed by its subscript text.
    Sub(String),
    /// `^{...}`: an exponent written as an integer expression.
    Sup(String),
    Raw(String),
    Sym(char),
    Tensor,
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic()
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>> {
    let cs: Vec<(usize, char)> = src.char_indices().collect();
    let mut out: Vec<(Tok, usize)> = Vec::new();
    let mut i = 0;
    let at = |i: usize| cs.get(i).map(|x| x.1);
    while i < cs.len() {
        let (off, c) = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            while i < cs.len() && cs[i].1 != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '(' && at(i + 1) == Some('x') && at(i + 2) == Some(')') {
            out.push((Tok::Tensor, off));
            i += 3;
            continue;
        }
        if c == '⊗' {
            out.push((Tok::Tensor, off));
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let mut n = BigInt::zero();
            while let Some(d) = at(i).and_then(|c| c.to_digit(10)) {
                n = n * 10 + d;
                i += 1;
            }
            out.push((Tok::Num(n), off));
            continue;
        }
        if c == '^' && at(i + 1) == Some('{') {
            i += 2;
            let mut raw = String::new();
            loop {
                match at(i) {
                    None => return Err(Error::syntax("unclosed '^{'", off)),
                    Some('}') => {
                        i += 1;
                        break;
                    }
                    Some(ch) => raw.push(ch),
                }
                i += 1;
            }
            out.push((Tok::Sup(raw), off));
            continue;
        }
        if c == '_' {
            i += 1;
            let start = cs.get(i).map(|x| x.0).unwrap_or(src.len());
            if at(i) == Some('{') {
                let mut depth = 0;
                let mut s = String::new();
                loop {
                    match at(i) {
                        None => return Err(Error::syntax("unclosed '{' in subscript", start)),
                        Some('{') => {
                            depth += 1;
                            if depth > 1 {
                                s.push('{');
                            }
                        }
                        Some('}') => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                            s.push('}');
                        }
                        Some(ch) => s.push(ch),
                    }
                    i += 1;
                }
                out.push((Tok::Sub(s), off));
            } else {
                let mut s = String::new();
                while let Some(ch) = at(i) {
                    if ch.is_alphanumeric() || ch == '*' {
                        s.push(ch);
                        i += 1;
                    } else {
                        break;
                    }
                }
                if s.is_empty() {
                    return Err(Error::syntax("expected a subscript after '_'", off));
                }
                out.push((Tok::Sub(s), off));
            }
            continue;
        }
        if is_name_start(c) {
            let mut s = String::new();
            while let Some(ch) = at(i) {
                if is_name_char(ch) {
                    s.push(ch);
                    i += 1;
                } else {
                    break;
                }
            }
            // `e(...)`: the idempotent; keep its contents raw.
            if s == "e" && at(i) == Some('(') {
                i += 1;
                let mut raw = String::new();
                loop {
                    match at(i) {
                        None => return Err(Error::syntax("unclosed 'e('", off)),
                        Some(')') => {
                            i += 1;
                            break;
                        }
                        Some(ch) => raw.push(ch),
                    }
                    i += 1;
                }
                out.push((Tok::Name("e".into()), off));
                out.push((Tok::Raw(raw.trim().to_string()), off));
                continue;
            }
            out.push((Tok::Name(s), off));
            continue;
        }
        let sym = match c {
            '+' | '*' | '/' | '^' | '(' | ')' | ',' | '-' => c,
            '−' => '-',
            '·' | '∘' => '*',
            _ => return Err(Error::syntax(format!("unexpected character '{c}'"), off)),
        };
        out.push((Tok::Sym(sym), off));
        i += 1;
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }
    fn off(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1).unwrap_or(self.end)
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Sym(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: char) -> Result<()> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(Error::syntax(format!("expected '{c}'"), self.off()))
        }
    }

    fn sum(&mut self) -> Result<Ast> {
        let mut items = Vec::new();
        let mut neg = if self.eat('-') {
            true
        } else {
            self.eat('+');
            false
        };
        loop {
            items.push((neg, self.tensor()?));
            if self.eat('+') {
                neg = false;
            } else if self.eat('-') {
                neg = true;
            } else {
                break;
            }
        }
        if items.len() == 1 && !items[0].0 {
            Ok(items.pop().unwrap().1)
        } else {
            Ok(Ast::Sum(items))
        }
    }

    fn tensor(&mut self) -> Result<Ast> {
        let mut items = vec![self.comp()?];
        while self.peek() == Some(&Tok::Tensor) {
            self.pos += 1;
            items.push(self.comp()?);
        }
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(Ast::Tensor(items))
        }
    }

    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Num(_)) | Some(Tok::Name(_)) | Some(Tok::Sym('('))
        )
    }

    fn comp(&mut self) -> Result<Ast> {
        let mut items = vec![self.factor()?];
        loop {
            if self.eat('/') {
                items.push(Ast::Pow(Box::new(self.factor()?), -1));
            } else if self.eat('*') || self.starts_atom() {
                items.push(self.factor()?);
            } else {
                break;
            }
        }
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(Ast::Comp(items))
        }
    }

    fn factor(&mut self) -> Result<Ast> {
        let a = self.atom()?;
        if let Some(Tok::Sup(raw)) = self.peek().cloned() {
            let off = self.off();
            self.pos += 1;
            let e = parse_term_int(&raw).map_err(|e| Error::syntax(e.message, off))?;
            return Ok(match e.constant() {
                Some(n) => Ast::Pow(Box::new(a), n),
                None => Ast::PowExpr(Box::new(a), e),
            });
        }
        if self.eat('^') {
            let braced = self.eat_brace_open();
            let neg = self.eat('-');
            let off = self.off();
            let n = match self.peek().cloned() {
                Some(Tok::Num(n)) => {
                    self.pos += 1;
                    n
                }
                _ => return Err(Error::syntax("expected an integer exponent", off)),
            };
            if braced {
                self.expect_brace_close()?;
            }
            let n: i64 = i64::try_from(&n).map_err(|_| Error::syntax("exponent too large", off))?;
            return Ok(Ast::Pow(Box::new(a), if neg { -n } else { n }));
        }
        Ok(a)
    }

    // `^{...}` arrives as Sym('^') then a Sub? No: braces only appear in subscripts. Exponents in
    // braces are written `^(-1)` or `^-1`.
    fn eat_brace_open(&mut self) -> bool {
        self.eat('(')
    }
    fn expect_brace_close(&mut self) -> Result<()> {
        self.expect(')')
    }

    fn atom(&mut self) -> Result<Ast> {
        let off = self.off();
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                if let Some(Tok::Sub(s)) = self.peek().cloned() {
                    if !n.is_one() {
                        return Err(Error::syntax(
                            "only 1 takes an object subscript (1_obj)",
                            off,
                        ));
                    }
                    self.pos += 1;
                    return Ok(Ast::Id(s));
                }
                if self.peek() == Some(&Tok::Sym('/'))
                    && matches!(self.toks.get(self.pos + 1).map(|t| &t.0), Some(Tok::Num(_)))
                {
                    self.pos += 1;
                    let Some(Tok::Num(d)) = self.peek().cloned() else {
                        unreachable!()
                    };
                    self.pos += 1;
                    if d.is_zero() {
                        return Err(Error::syntax("division by zero", off));
                    }
                    return Ok(Ast::Frac(n, d));
                }
                Ok(Ast::Int(n))
            }
            Some(Tok::Name(name)) => {
                self.pos += 1;
                if name == "e"
                    && let Some(Tok::Raw(r)) = self.peek().cloned()
                {
                    self.pos += 1;
                    return Ok(Ast::Idem(r));
                }
                if name == "I" {
                    return Ok(Ast::Imag);
                }
                let sub = if let Some(Tok::Sub(s)) = self.peek().cloned() {
                    self.pos += 1;
                    Some(s)
                } else {
                    None
                };
                if name == "sum"
                    && let Some(r) = sub.as_deref().filter(|r| r.contains('='))
                {
                    let (var, range) = r.split_once('=').unwrap();
                    let (lo, hi) = range
                        .split_once("..")
                        .ok_or_else(|| Error::syntax("a sum is written sum_{f=lo..hi}", off))?;
                    let var = var.trim().to_string();
                    if var.is_empty() || !var.chars().all(|c| c.is_alphanumeric()) {
                        return Err(Error::syntax("the index of a sum is a name", off));
                    }
                    let parse =
                        |t: &str| parse_term_int(t).map_err(|e| Error::syntax(e.message, off));
                    let (lo, hi) = (parse(lo)?, parse(hi)?);
                    let body = self.comp()?;
                    return Ok(Ast::Range {
                        var,
                        lo,
                        hi,
                        body: Box::new(body),
                    });
                }
                let args = if self.peek() == Some(&Tok::Sym('('))
                    && sub.is_some()
                    && name.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    // Function application, for parameter polynomials such as Q_ab(x_1, x_2).
                    self.pos += 1;
                    let mut args = vec![self.sum()?];
                    while self.eat(',') {
                        args.push(self.sum()?);
                    }
                    self.expect(')')?;
                    Some(args)
                } else {
                    None
                };
                Ok(Ast::Name { name, sub, args })
            }
            Some(Tok::Sym('(')) => {
                self.pos += 1;
                let a = self.sum()?;
                self.expect(')')?;
                Ok(a)
            }
            Some(t) => Err(Error::syntax(format!("unexpected {}", describe(&t)), off)),
            None => Err(Error::syntax("unexpected end of input", off)),
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(n) => format!("number {n}"),
        Tok::Name(n) => format!("'{n}'"),
        Tok::Sub(s) => format!("subscript '_{s}'"),
        Tok::Sup(s) => format!("exponent '^{{{s}}}'"),
        Tok::Raw(_) => "idempotent contents".into(),
        Tok::Sym(c) => format!("'{c}'"),
        Tok::Tensor => "'(x)'".into(),
    }
}

/// Parses one term.
pub fn parse(src: &str) -> Result<Ast> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        pos: 0,
        end: src.len(),
    };
    if p.peek().is_none() {
        return Err(Error::syntax("empty term", 0));
    }
    let a = p.sum()?;
    if p.pos != p.toks.len() {
        return Err(Error::syntax(
            format!("unexpected {}", describe(p.peek().unwrap())),
            p.off(),
        ));
    }
    Ok(a)
}

fn print_sub(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '*') {
        format!("_{s}")
    } else {
        format!("_{{{s}}}")
    }
}

/// Prints a syntax tree; `parse` of the result gives the same tree.
pub fn print(a: &Ast) -> String {
    print_prec(a, 0)
}

// Precedence levels: 0 sum, 1 tensor, 2 comp, 3 factor/atom.
fn print_prec(a: &Ast, prec: u8) -> String {
    let (s, p) = match a {
        Ast::Int(n) => {
            if n.is_negative() {
                (format!("({n})"), 3)
            } else {
                (n.to_string(), 3)
            }
        }
        Ast::Frac(n, d) => (format!("{n}/{d}"), 3),
        Ast::Imag => ("I".into(), 3),
        Ast::Name { name, sub, args } => {
            let mut s = name.clone();
            if let Some(sb) = sub {
                s.push_str(&print_sub(sb));
            }
            if let Some(args) = args {
                let a: Vec<String> = args.iter().map(|x| print_prec(x, 0)).collect();
                s.push_str(&format!("({})", a.join(", ")));
            }
            (s, 3)
        }
        Ast::Idem(r) => (format!("e({r})"), 3),
        Ast::Id(r) => (format!("1{}", print_sub(r)), 3),
        Ast::Pow(b, e) => {
            let bs = print_prec(b, 4);
            let es = if *e < 0 {
                format!("-{}", -e)
            } else {
                e.to_string()
            };
            (format!("{bs}^{es}"), 3)
        }
        Ast::PowExpr(b, e) => (format!("{}^{{{}}}", print_prec(b, 4), e.print()), 3),
        Ast::Range { var, lo, hi, body } => (
            format!(
                "sum_{{{var}={}..{}}} {}",
                lo.print(),
                hi.print(),
                print_prec(body, 2)
            ),
            2,
        ),
        Ast::Comp(items) => {
            let parts: Vec<String> = items.iter().map(|x| print_prec(x, 3)).collect();
            (parts.join(" "), 2)
        }
        Ast::Tensor(items) => {
            let parts: Vec<String> = items.iter().map(|x| print_prec(x, 2)).collect();
            (parts.join(" (x) "), 1)
        }
        Ast::Sum(items) => {
            let mut s = String::new();
            for (k, (neg, x)) in items.iter().enumerate() {
                if k == 0 {
                    if *neg {
                        s.push('-');
                    }
                } else {
                    s.push_str(if *neg { " - " } else { " + " });
                }
                s.push_str(&print_prec(x, 1));
            }
            (s, 0)
        }
    };
    // Factors inside a composition that are themselves compositions need brackets; so does the
    // base of a power unless it is an atom.
    let needs = match (a, prec) {
        (_, 4) => {
            !matches!(
                a,
                Ast::Int(_) | Ast::Imag | Ast::Name { .. } | Ast::Idem(_) | Ast::Id(_)
            ) || matches!(a, Ast::Int(n) if n.is_negative())
        }
        (Ast::Comp(_) | Ast::Range { .. }, 3) => true,
        _ => p < prec,
    };
    if needs && !(matches!(a, Ast::Int(n) if n.is_negative())) {
        format!("({s})")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(s: &str) -> String {
        let a = parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        let p = print(&a);
        let b = parse(&p).unwrap_or_else(|e| panic!("reprint of {s} = {p}: {e}"));
        assert_eq!(a, b, "{s} printed as {p}");
        p
    }

    #[test]
    fn examples_from_the_spec() {
        assert_eq!(
            round("2 x_1^2 psi_1 e(iij) - q^-1 t_ij psi_2 psi_1 e(iji) + c1 (x_1 (x) 1_j)"),
            "2 x_1^2 psi_1 e(iij) - q^-1 t_ij psi_2 psi_1 e(iji) + c1 (x_1 (x) 1_j)"
        );
        assert_eq!(round("x_1 psi_1"), "x_1 psi_1");
        assert_eq!(round("psi_1 x_2 + 1"), "psi_1 x_2 + 1");
        assert_eq!(round("Q_ab(x_1, x_2) e(a b)"), "Q_ab(x_1, x_2) e(a b)");
        assert_eq!(round("e*e - e"), "e e - e");
    }

    #[test]
    fn structure() {
        assert_eq!(
            parse("a b + c").unwrap(),
            Ast::Sum(vec![
                (false, Ast::Comp(vec![name("a"), name("b")])),
                (false, name("c"))
            ])
        );
        assert_eq!(
            parse("(a b) c").unwrap(),
            Ast::Comp(vec![Ast::Comp(vec![name("a"), name("b")]), name("c")])
        );
        assert_eq!(round("(a b) c"), "(a b) c");
        assert_eq!(round("(a + b)^2"), "(a + b)^2");
        assert_eq!(round("-(a - b)"), "-(a - b)");
        assert_eq!(round("3/2 ψ_1 ⊗ x_1"), "3/2 ψ_1 (x) x_1");
        assert_eq!(round("1_{ij*}"), "1_ij*");
        assert_eq!(round("(q + 1)/q^2 psi_1"), "(q + 1) (q^2)^-1 psi_1");
        assert_eq!(round("1/2/(q + 1)"), "1/2 (q + 1)^-1");
        assert_eq!(round("x_1^{h(i) - 1 - f} e(i)"), "x_1^{h(i) - 1 - f} e(i)");
        assert_eq!(round("x_1^{2} e(i)"), "x_1^2 e(i)");
        assert_eq!(
            round("sum_{f=0..h(i)-1} x_1^{f} x_2^{h(i) - 1 - f} + 1"),
            "sum_{f=0..h(i) - 1} x_1^{f} x_2^{h(i) - 1 - f} + 1"
        );
        assert_eq!(
            round("y (sum_{g=0..2} x_1^{g}) z"),
            "y (sum_{g=0..2} x_1^{g}) z"
        );
        assert!(parse("sum_{f=0} x_1").is_err());
        assert!(parse("x_1 +").is_err());
        assert!(parse("2_i").is_err());
    }

    fn name(n: &str) -> Ast {
        Ast::Name {
            name: n.into(),
            sub: None,
            args: None,
        }
    }
}
