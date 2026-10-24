//! Small expression languages used in presentation files: integer expressions for degrees and
//! parities of generator schemas, and side conditions of relation schemas.
//!
//! Integer expressions: integers, `+ - *`, parentheses, the symmetric form `(a,b)` of the Cartan
//! datum, the Cartan entry `a_ab`, the symmetrizer `d_a`, the parity `p(a)` of a colour variable,
//! and, in a 2-category, the pairing `h(a)` = ⟨h_a, λ⟩ with a region's weight λ: for a generator's
//! degree the region just right of the generator, for a relation the region right of the whole
//! relation. Conditions: `a = b`, `a != b`, comparisons of integer expressions
//! (`(a,b) = 0`, `a_ab <= -2`), `and`, `or`, `not`, and parentheses.

use crate::error::{Error, Result};

/// What an expression needs to know about colours bound to its variables.
pub trait ColourEnv {
    fn form(&self, a: usize, b: usize) -> Result<i64>;
    fn cartan_entry(&self, a: usize, b: usize) -> Result<i64>;
    fn symmetrizer(&self, a: usize) -> Result<i64>;
    fn parity(&self, a: usize) -> i64;
    /// ⟨h_a, λ⟩ for the weight λ of the region in question; an error where there are no weights.
    fn pairing(&self, _a: usize) -> Result<i64> {
        Err(Error::invalid(
            "h(...) needs region weights: give the context a weight mu=...",
        ))
    }
}

/// An environment whose region is given: the pairings are read from `region` (indexed by colour).
pub struct AtRegion<'a> {
    pub env: &'a dyn ColourEnv,
    pub region: &'a [i64],
}

impl ColourEnv for AtRegion<'_> {
    fn form(&self, a: usize, b: usize) -> Result<i64> {
        self.env.form(a, b)
    }
    fn cartan_entry(&self, a: usize, b: usize) -> Result<i64> {
        self.env.cartan_entry(a, b)
    }
    fn symmetrizer(&self, a: usize) -> Result<i64> {
        self.env.symmetrizer(a)
    }
    fn parity(&self, a: usize) -> i64 {
        self.env.parity(a)
    }
    fn pairing(&self, a: usize) -> Result<i64> {
        self.region
            .get(a)
            .copied()
            .ok_or_else(|| Error::invalid("no region weight for this colour"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntExpr {
    Const(i64),
    Form(usize, usize),
    Entry(usize, usize),
    Sym(usize),
    Par(usize),
    /// ⟨h_a, λ⟩ for the region λ of the expression's environment.
    Pair(usize),
    Neg(Box<IntExpr>),
    Add(Box<IntExpr>, Box<IntExpr>),
    Sub(Box<IntExpr>, Box<IntExpr>),
    Mul(Box<IntExpr>, Box<IntExpr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    True,
    SameColour(usize, usize, bool),
    Compare(IntExpr, Cmp, IntExpr),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
}

impl IntExpr {
    pub fn eval(&self, b: &[usize], env: &dyn ColourEnv) -> Result<i64> {
        Ok(match self {
            IntExpr::Const(n) => *n,
            IntExpr::Form(x, y) => env.form(b[*x], b[*y])?,
            IntExpr::Entry(x, y) => env.cartan_entry(b[*x], b[*y])?,
            IntExpr::Sym(x) => env.symmetrizer(b[*x])?,
            IntExpr::Par(x) => env.parity(b[*x]),
            IntExpr::Pair(x) => env.pairing(b[*x])?,
            IntExpr::Neg(e) => -e.eval(b, env)?,
            IntExpr::Add(x, y) => x.eval(b, env)? + y.eval(b, env)?,
            IntExpr::Sub(x, y) => x.eval(b, env)? - y.eval(b, env)?,
            IntExpr::Mul(x, y) => x.eval(b, env)? * y.eval(b, env)?,
        })
    }

    /// Canonical text, with variables named by `vars`.
    pub fn print(&self, vars: &[String]) -> String {
        self.print_prec(vars, 0)
    }

    fn print_prec(&self, v: &[String], prec: u8) -> String {
        let s = match self {
            IntExpr::Const(n) => return n.to_string(),
            IntExpr::Form(x, y) => return format!("({},{})", v[*x], v[*y]),
            IntExpr::Entry(x, y) => return format!("a_{}{}", v[*x], v[*y]),
            IntExpr::Sym(x) => return format!("d_{}", v[*x]),
            IntExpr::Par(x) => return format!("p({})", v[*x]),
            IntExpr::Pair(x) => return format!("h({})", v[*x]),
            IntExpr::Neg(e) => return format!("-{}", e.print_prec(v, 3)),
            IntExpr::Add(x, y) => (
                format!("{} + {}", x.print_prec(v, 1), y.print_prec(v, 2)),
                1,
            ),
            IntExpr::Sub(x, y) => (
                format!("{} - {}", x.print_prec(v, 1), y.print_prec(v, 2)),
                1,
            ),
            IntExpr::Mul(x, y) => (format!("{} {}", x.print_prec(v, 2), y.print_prec(v, 3)), 2),
        };
        if s.1 < prec {
            format!("({})", s.0)
        } else {
            s.0
        }
    }
}

impl IntExpr {
    pub fn uses_pairing(&self) -> bool {
        match self {
            IntExpr::Pair(_) => true,
            IntExpr::Neg(e) => e.uses_pairing(),
            IntExpr::Add(x, y) | IntExpr::Sub(x, y) | IntExpr::Mul(x, y) => {
                x.uses_pairing() || y.uses_pairing()
            }
            _ => false,
        }
    }

    /// Whether the expression reads a colour's parity `p(a)`.
    pub fn uses_parity(&self) -> bool {
        match self {
            IntExpr::Par(_) => true,
            IntExpr::Neg(e) => e.uses_parity(),
            IntExpr::Add(x, y) | IntExpr::Sub(x, y) | IntExpr::Mul(x, y) => {
                x.uses_parity() || y.uses_parity()
            }
            _ => false,
        }
    }
}

impl Cond {
    pub fn uses_pairing(&self) -> bool {
        match self {
            Cond::Compare(x, _, y) => x.uses_pairing() || y.uses_pairing(),
            Cond::And(x, y) | Cond::Or(x, y) => x.uses_pairing() || y.uses_pairing(),
            Cond::Not(x) => x.uses_pairing(),
            _ => false,
        }
    }

    pub fn eval(&self, b: &[usize], env: &dyn ColourEnv) -> Result<bool> {
        Ok(match self {
            Cond::True => true,
            Cond::SameColour(x, y, eq) => (b[*x] == b[*y]) == *eq,
            Cond::Compare(l, c, r) => {
                let (l, r) = (l.eval(b, env)?, r.eval(b, env)?);
                match c {
                    Cmp::Eq => l == r,
                    Cmp::Ne => l != r,
                    Cmp::Lt => l < r,
                    Cmp::Le => l <= r,
                    Cmp::Gt => l > r,
                    Cmp::Ge => l >= r,
                }
            }
            Cond::And(x, y) => x.eval(b, env)? && y.eval(b, env)?,
            Cond::Or(x, y) => x.eval(b, env)? || y.eval(b, env)?,
            Cond::Not(x) => !x.eval(b, env)?,
        })
    }

    pub fn print(&self, v: &[String]) -> String {
        self.print_prec(v, 0)
    }

    fn print_prec(&self, v: &[String], prec: u8) -> String {
        let (s, p) = match self {
            Cond::True => ("true".to_string(), 3),
            Cond::SameColour(x, y, eq) => (
                format!("{} {} {}", v[*x], if *eq { "=" } else { "!=" }, v[*y]),
                3,
            ),
            Cond::Compare(l, c, r) => {
                let op = match c {
                    Cmp::Eq => "=",
                    Cmp::Ne => "!=",
                    Cmp::Lt => "<",
                    Cmp::Le => "<=",
                    Cmp::Gt => ">",
                    Cmp::Ge => ">=",
                };
                (format!("{} {} {}", l.print(v), op, r.print(v)), 3)
            }
            Cond::Or(x, y) => (
                format!("{} or {}", x.print_prec(v, 0), y.print_prec(v, 1)),
                0,
            ),
            Cond::And(x, y) => (
                format!("{} and {}", x.print_prec(v, 1), y.print_prec(v, 2)),
                1,
            ),
            Cond::Not(x) => (format!("not {}", x.print_prec(v, 3)), 2),
        };
        if p < prec { format!("({s})") } else { s }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Int(i64),
    Ident(String),
    Sym(&'static str),
}

fn lex(s: &str) -> Result<Vec<(Tok, usize)>> {
    let mut out = Vec::new();
    let cs: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < cs.len() {
        let (off, c) = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let mut n = 0i64;
            while i < cs.len() && cs[i].1.is_ascii_digit() {
                n = n * 10 + cs[i].1.to_digit(10).unwrap() as i64;
                i += 1;
            }
            out.push((Tok::Int(n), off));
        } else if c.is_alphabetic() || c == '_' {
            let mut t = String::new();
            while i < cs.len() && (cs[i].1.is_alphanumeric() || cs[i].1 == '_') {
                t.push(cs[i].1);
                i += 1;
            }
            out.push((Tok::Ident(t), off));
        } else {
            let two: String = cs[i..(i + 2).min(cs.len())].iter().map(|x| x.1).collect();
            let sym = match two.as_str() {
                "!=" => Some("!="),
                "<=" => Some("<="),
                ">=" => Some(">="),
                "==" => Some("="),
                _ => None,
            };
            if let Some(s) = sym {
                out.push((Tok::Sym(s), off));
                i += 2;
                continue;
            }
            let s = match c {
                '+' => "+",
                '-' | '−' => "-",
                '*' => "*",
                '(' => "(",
                ')' => ")",
                ',' => ",",
                '=' => "=",
                '<' => "<",
                '>' => ">",
                _ => return Err(Error::syntax(format!("unexpected character '{c}'"), off)),
            };
            out.push((Tok::Sym(s), off));
            i += 1;
        }
    }
    Ok(out)
}

struct P<'a> {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    vars: &'a [String],
    len: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }
    fn off(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1).unwrap_or(self.len)
    }
    fn eat(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Sym(x)) if *x == s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn eat_word(&mut self, w: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(x)) if x == w) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<()> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(Error::syntax(format!("expected '{s}'"), self.off()))
        }
    }
    fn var(&mut self) -> Result<usize> {
        match self.peek().cloned() {
            Some(Tok::Ident(n)) => {
                let i = self.var_index(&n)?;
                self.pos += 1;
                Ok(i)
            }
            _ => Err(Error::syntax("expected a colour variable", self.off())),
        }
    }
    fn var_index(&self, n: &str) -> Result<usize> {
        self.vars
            .iter()
            .position(|v| v == n)
            .ok_or_else(|| Error::syntax(format!("unknown colour variable '{n}'"), self.off()))
    }
    /// Splits `ab` (after `a_` or `d_`) into variable names.
    fn split_vars(&self, s: &str, count: usize) -> Result<Vec<usize>> {
        let mut out = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            let v = self
                .vars
                .iter()
                .filter(|v| rest.starts_with(v.as_str()))
                .max_by_key(|v| v.len())
                .ok_or_else(|| {
                    Error::syntax(format!("cannot read '{s}' as colour variables"), self.off())
                })?;
            out.push(self.var_index(v)?);
            rest = &rest[v.len()..];
        }
        if out.len() != count {
            return Err(Error::syntax(
                format!("'{s}' should name {count} colour variables"),
                self.off(),
            ));
        }
        Ok(out)
    }

    fn int_expr(&mut self) -> Result<IntExpr> {
        let mut e = if self.eat("-") {
            IntExpr::Neg(Box::new(self.int_term()?))
        } else {
            self.int_term()?
        };
        loop {
            if self.eat("+") {
                e = IntExpr::Add(Box::new(e), Box::new(self.int_term()?));
            } else if self.eat("-") {
                e = IntExpr::Sub(Box::new(e), Box::new(self.int_term()?));
            } else {
                return Ok(e);
            }
        }
    }
    fn int_term(&mut self) -> Result<IntExpr> {
        let mut e = self.int_atom()?;
        loop {
            self.eat("*");
            match self.peek() {
                Some(Tok::Int(_)) | Some(Tok::Ident(_)) => {}
                Some(Tok::Sym("(")) => {}
                _ => return Ok(e),
            }
            if matches!(self.peek(), Some(Tok::Ident(w)) if w == "and" || w == "or") {
                return Ok(e);
            }
            e = IntExpr::Mul(Box::new(e), Box::new(self.int_atom()?));
        }
    }
    fn int_atom(&mut self) -> Result<IntExpr> {
        let off = self.off();
        match self.peek().cloned() {
            Some(Tok::Int(n)) => {
                self.pos += 1;
                Ok(IntExpr::Const(n))
            }
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                // (a,b) or a parenthesized expression
                let save = self.pos;
                if let Some(Tok::Ident(n)) = self.peek().cloned()
                    && self.vars.contains(&n)
                    && matches!(
                        self.toks.get(self.pos + 1).map(|t| &t.0),
                        Some(Tok::Sym(","))
                    )
                {
                    let a = self.var()?;
                    self.expect(",")?;
                    let b = self.var()?;
                    self.expect(")")?;
                    return Ok(IntExpr::Form(a, b));
                }
                self.pos = save;
                let e = self.int_expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Ident(w)) => {
                self.pos += 1;
                if w == "p" || w == "h" {
                    self.expect("(")?;
                    let a = self.var()?;
                    self.expect(")")?;
                    return Ok(if w == "p" {
                        IntExpr::Par(a)
                    } else {
                        IntExpr::Pair(a)
                    });
                }
                if let Some(r) = w.strip_prefix("a_") {
                    let v = self.split_vars(r, 2)?;
                    return Ok(IntExpr::Entry(v[0], v[1]));
                }
                if let Some(r) = w.strip_prefix("d_") {
                    let v = self.split_vars(r, 1)?;
                    return Ok(IntExpr::Sym(v[0]));
                }
                Err(Error::syntax(format!("unexpected '{w}'"), off))
            }
            _ => Err(Error::syntax("expected an integer expression", off)),
        }
    }

    fn cond(&mut self) -> Result<Cond> {
        let mut c = self.cond_and()?;
        while self.eat_word("or") {
            c = Cond::Or(Box::new(c), Box::new(self.cond_and()?));
        }
        Ok(c)
    }
    fn cond_and(&mut self) -> Result<Cond> {
        let mut c = self.cond_not()?;
        while self.eat_word("and") {
            c = Cond::And(Box::new(c), Box::new(self.cond_not()?));
        }
        Ok(c)
    }
    fn cond_not(&mut self) -> Result<Cond> {
        if self.eat_word("not") {
            return Ok(Cond::Not(Box::new(self.cond_not()?)));
        }
        if self.eat_word("true") {
            return Ok(Cond::True);
        }
        // colour equality: var (=|!=) var
        if let (Some(Tok::Ident(x)), Some(Tok::Sym(op)), Some(Tok::Ident(y))) = (
            self.toks.get(self.pos).map(|t| &t.0),
            self.toks.get(self.pos + 1).map(|t| &t.0),
            self.toks.get(self.pos + 2).map(|t| &t.0),
        ) && (*op == "=" || *op == "!=")
            && self.vars.contains(x)
            && self.vars.contains(y)
        {
            let eq = *op == "=";
            let (a, b) = (self.var_index(x)?, self.var_index(y)?);
            self.pos += 3;
            return Ok(Cond::SameColour(a, b, eq));
        }
        // parenthesized condition or comparison starting with '('
        if matches!(self.peek(), Some(Tok::Sym("("))) {
            let save = self.pos;
            self.pos += 1;
            if let Ok(c) = self.cond()
                && self.eat(")")
                && !matches!(self.peek(), Some(Tok::Sym(_)))
            {
                return Ok(c);
            }
            self.pos = save;
        }
        let l = self.int_expr()?;
        let op = match self.peek() {
            Some(Tok::Sym("=")) => Cmp::Eq,
            Some(Tok::Sym("!=")) => Cmp::Ne,
            Some(Tok::Sym("<")) => Cmp::Lt,
            Some(Tok::Sym("<=")) => Cmp::Le,
            Some(Tok::Sym(">")) => Cmp::Gt,
            Some(Tok::Sym(">=")) => Cmp::Ge,
            _ => return Err(Error::syntax("expected a comparison", self.off())),
        };
        self.pos += 1;
        let r = self.int_expr()?;
        Ok(Cond::Compare(l, op, r))
    }
}

/// An integer expression inside a term: exponents `x_1^{h(i) - 1 - f}`, the bounds of sums
/// `sum_{f=0..h(i)-1}`, and the labels of bubbles. Names are resolved when the term is
/// elaborated: a name is a summation index or an integer variable of a relation; `h(c)` is the
/// pairing ⟨h_c, μ⟩ with the weight of the rightmost region, `p(c)` the parity, and `a_cd` the
/// Cartan entry ⟨h_c, α_d⟩, for colours or colour variables `c`, `d` (one letter each in `a_cd`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TermInt {
    Const(i64),
    Var(String),
    Pair(String),
    Par(String),
    Entry(String, String),
    Neg(Box<TermInt>),
    Add(Box<TermInt>, Box<TermInt>),
    Sub(Box<TermInt>, Box<TermInt>),
    Mul(Box<TermInt>, Box<TermInt>),
}

/// What a term's integer expression asks of its colours.
#[derive(Clone, Copy, Debug)]
pub enum ColourQuery<'a> {
    /// `h(c)`.
    Pair(&'a str),
    /// `p(c)`.
    Par(&'a str),
    /// `a_cd`.
    Entry(&'a str, &'a str),
}

impl TermInt {
    pub fn eval(
        &self,
        var: &dyn Fn(&str) -> Option<i64>,
        colour: &dyn Fn(ColourQuery) -> Result<i64>,
    ) -> Result<i64> {
        Ok(match self {
            TermInt::Const(n) => *n,
            TermInt::Var(v) => {
                var(v).ok_or_else(|| Error::invalid(format!("'{v}' is not an index in scope")))?
            }
            TermInt::Pair(c) => colour(ColourQuery::Pair(c))?,
            TermInt::Par(c) => colour(ColourQuery::Par(c))?,
            TermInt::Entry(c, d) => colour(ColourQuery::Entry(c, d))?,
            TermInt::Neg(e) => -e.eval(var, colour)?,
            TermInt::Add(x, y) => x.eval(var, colour)? + y.eval(var, colour)?,
            TermInt::Sub(x, y) => x.eval(var, colour)? - y.eval(var, colour)?,
            TermInt::Mul(x, y) => x.eval(var, colour)? * y.eval(var, colour)?,
        })
    }

    /// The constant, if the expression has no names.
    pub fn constant(&self) -> Option<i64> {
        self.eval(&|_| None, &|_| Err(Error::invalid("not constant")))
            .ok()
    }

    pub fn print(&self) -> String {
        self.print_prec(0)
    }

    fn print_prec(&self, prec: u8) -> String {
        let s = match self {
            TermInt::Const(n) if *n < 0 => (n.to_string(), 1),
            TermInt::Const(n) => return n.to_string(),
            TermInt::Var(v) => return v.clone(),
            TermInt::Pair(c) => return format!("h({c})"),
            TermInt::Par(c) => return format!("p({c})"),
            TermInt::Entry(c, d) => return format!("a_{c}{d}"),
            TermInt::Neg(e) => (format!("-{}", e.print_prec(3)), 1),
            TermInt::Add(x, y) => (format!("{} + {}", x.print_prec(1), y.print_prec(2)), 1),
            TermInt::Sub(x, y) => (format!("{} - {}", x.print_prec(1), y.print_prec(2)), 1),
            TermInt::Mul(x, y) => (format!("{}*{}", x.print_prec(2), y.print_prec(3)), 2),
        };
        if s.1 < prec {
            format!("({})", s.0)
        } else {
            s.0
        }
    }
}

impl P<'_> {
    fn term_int(&mut self) -> Result<TermInt> {
        let mut e = if self.eat("-") {
            TermInt::Neg(Box::new(self.term_int_term()?))
        } else {
            self.term_int_term()?
        };
        loop {
            if self.eat("+") {
                e = TermInt::Add(Box::new(e), Box::new(self.term_int_term()?));
            } else if self.eat("-") {
                e = TermInt::Sub(Box::new(e), Box::new(self.term_int_term()?));
            } else {
                return Ok(e);
            }
        }
    }
    fn term_int_term(&mut self) -> Result<TermInt> {
        let mut e = self.term_int_atom()?;
        loop {
            let star = self.eat("*");
            if !star
                && !matches!(
                    self.peek(),
                    Some(Tok::Int(_) | Tok::Ident(_) | Tok::Sym("("))
                )
            {
                return Ok(e);
            }
            e = TermInt::Mul(Box::new(e), Box::new(self.term_int_atom()?));
        }
    }
    fn term_int_atom(&mut self) -> Result<TermInt> {
        let off = self.off();
        match self.peek().cloned() {
            Some(Tok::Int(n)) => {
                self.pos += 1;
                Ok(TermInt::Const(n))
            }
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                let e = self.term_int()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Ident(w)) => {
                self.pos += 1;
                if (w == "h" || w == "p") && self.eat("(") {
                    let c = match self.peek().cloned() {
                        Some(Tok::Ident(c)) => c,
                        _ => {
                            return Err(Error::syntax(
                                format!("expected a colour in {w}(...)"),
                                self.off(),
                            ));
                        }
                    };
                    self.pos += 1;
                    self.expect(")")?;
                    return Ok(if w == "h" {
                        TermInt::Pair(c)
                    } else {
                        TermInt::Par(c)
                    });
                }
                if let Some(r) = w.strip_prefix("a_") {
                    let cs: Vec<char> = r.chars().collect();
                    if cs.len() != 2 {
                        return Err(Error::syntax(
                            format!("'{w}' should name two one-letter colours"),
                            off,
                        ));
                    }
                    return Ok(TermInt::Entry(cs[0].to_string(), cs[1].to_string()));
                }
                Ok(TermInt::Var(w))
            }
            _ => Err(Error::syntax("expected an integer expression", off)),
        }
    }
}

pub fn parse_term_int(s: &str) -> Result<TermInt> {
    let mut p = parser(s, &[])?;
    let e = p.term_int()?;
    if p.pos != p.toks.len() {
        return Err(Error::syntax(
            "unexpected text after the expression",
            p.off(),
        ));
    }
    // a plain negative constant stays a constant
    Ok(match e {
        TermInt::Neg(b) if matches!(*b, TermInt::Const(_)) => {
            TermInt::Const(-b.constant().unwrap())
        }
        e => e,
    })
}

/// The longest integer expression or condition read (an exponent, a sum's bounds, a condition in
/// a presentation): it bounds how deep the recursive parser and the evaluation of what it builds
/// can go.
pub const MAX_EXPR_LEN: usize = 1000;

fn parser<'a>(s: &str, vars: &'a [String]) -> Result<P<'a>> {
    if s.len() > MAX_EXPR_LEN {
        return Err(Error::syntax(
            format!("expression longer than {MAX_EXPR_LEN} characters"),
            MAX_EXPR_LEN,
        ));
    }
    Ok(P {
        toks: lex(s)?,
        pos: 0,
        vars,
        len: s.len(),
    })
}

pub fn parse_int_expr(s: &str, vars: &[String]) -> Result<IntExpr> {
    let mut p = parser(s, vars)?;
    let e = p.int_expr()?;
    if p.pos != p.toks.len() {
        return Err(Error::syntax(
            "unexpected text after the expression",
            p.off(),
        ));
    }
    Ok(e)
}

pub fn parse_cond(s: &str, vars: &[String]) -> Result<Cond> {
    let mut p = parser(s, vars)?;
    let c = p.cond()?;
    if p.pos != p.toks.len() {
        return Err(Error::syntax(
            "unexpected text after the condition",
            p.off(),
        ));
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Env;
    impl ColourEnv for Env {
        fn form(&self, a: usize, b: usize) -> Result<i64> {
            Ok([[2, -1], [-1, 2]][a][b])
        }
        fn cartan_entry(&self, a: usize, b: usize) -> Result<i64> {
            self.form(a, b)
        }
        fn symmetrizer(&self, _: usize) -> Result<i64> {
            Ok(1)
        }
        fn parity(&self, a: usize) -> i64 {
            a as i64
        }
    }

    fn vars() -> Vec<String> {
        vec!["a".into(), "b".into()]
    }

    #[test]
    fn int_exprs() {
        let v = vars();
        for (s, val) in [
            ("-(a,b)", 1),
            ("(a,a)", 2),
            ("2 - (a,b) * 3", 5),
            ("p(a) p(b)", 0),
            ("d_a + a_ab", 0),
        ] {
            let e = parse_int_expr(s, &v).unwrap();
            assert_eq!(e.eval(&[0, 1], &Env).unwrap(), val, "{s}");
            let back = parse_int_expr(&e.print(&v), &v).unwrap();
            assert_eq!(back, e, "{s} printed as {}", e.print(&v));
        }
    }

    #[test]
    fn conditions() {
        let v = vars();
        for (s, val) in [
            ("a != b", true),
            ("a = b", false),
            ("(a,b) = 0", false),
            ("not (a = b) and (a,b) < 0", true),
            ("a = b or a_ab = -1", true),
        ] {
            let c = parse_cond(s, &v).unwrap();
            assert_eq!(c.eval(&[0, 1], &Env).unwrap(), val, "{s}");
            assert_eq!(parse_cond(&c.print(&v), &v).unwrap(), c, "{s}");
        }
    }

    #[test]
    fn term_ints() {
        let colour = |q: ColourQuery| -> Result<i64> {
            let idx = |c: &str| if c == "i" { 0 } else { 1 };
            Ok(match q {
                ColourQuery::Pair(c) => [3, -2][idx(c)],
                ColourQuery::Par(c) => idx(c) as i64,
                ColourQuery::Entry(c, d) => [[2, -1], [-1, 2]][idx(c)][idx(d)],
            })
        };
        let var = |v: &str| (v == "f").then_some(4);
        for (s, val) in [
            ("h(i) - 1 - f", -2),
            ("p(j) p(i) + p(j)", 1),
            ("-a_ij + h(j)", -1),
            ("f*(a_jj - 1)", 4),
        ] {
            let e = parse_term_int(s).unwrap();
            assert_eq!(e.eval(&var, &colour).unwrap(), val, "{s}");
            assert_eq!(
                parse_term_int(&e.print()).unwrap(),
                e,
                "{s} printed as {}",
                e.print()
            );
        }
        assert!(parse_term_int("a_ijk").is_err());
    }
}
