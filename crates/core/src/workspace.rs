//! The workspace file format (`.dcw`, format `diagcalc-workspace/1`).
//!
//! A header line, a `context` line, then one cell per line in order:
//!
//! ```text
//! diagcalc-workspace/1
//! context klr type=A2 nu=2i+j ring=QQ
//!
//! def e := c1 x_1 psi_1 e(iij) + c2 psi_1 x_2 e(iij)  # id=d1
//! watch e e - e  view=diagrams  # id=w2
//! solve e e - e = 0 for c1, c2  # id=s3
//! check idempotent(e)  # id=k4
//! note """Candidate from the A2 search."""  # id=n5
//! ```
//!
//! Terms are stored as syntax trees and printed in canonical form, options (`key=value`) follow
//! the cell, and `# id=…` names the cell. Blank lines and whole-line `#` comments are kept, so
//! printing a parsed file changes only spacing inside cells. Only inputs are stored, never
//! results.

use crate::context::ContextSpec;
use crate::error::{Error, Result};
use crate::text::{self, Ast};

pub const WORKSPACE_FORMAT: &str = "diagcalc-workspace/1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellBody {
    /// `def name := term`
    Def {
        name: String,
        term: Ast,
    },
    /// `watch term`: show the term's normal form, kept up to date.
    Watch {
        term: Ast,
    },
    /// `solve lhs = rhs for c1, c2`
    Solve {
        lhs: Ast,
        rhs: Ast,
        unknowns: Vec<String>,
    },
    /// `check name(arg, …)`: a named yes-or-no property with a witness.
    Check {
        name: String,
        args: Vec<Ast>,
    },
    /// `note """text"""`
    Note {
        text: String,
    },
    /// A whole-line comment, without its `#`.
    Comment(String),
    Blank,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub id: Option<String>,
    pub body: CellBody,
    /// `key=value` options such as `view=diagrams`.
    pub options: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub context: ContextSpec,
    pub cells: Vec<Cell>,
}

impl CellBody {
    /// The letter cell IDs of this kind start with.
    pub fn id_prefix(&self) -> Option<char> {
        match self {
            CellBody::Def { .. } => Some('d'),
            CellBody::Watch { .. } => Some('w'),
            CellBody::Solve { .. } => Some('s'),
            CellBody::Check { .. } => Some('k'),
            CellBody::Note { .. } => Some('n'),
            CellBody::Comment(_) | CellBody::Blank => None,
        }
    }
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_alphabetic()) && cs.all(|c| c.is_alphanumeric() || c == '_')
}

/// Splits trailing `key=value` options off a cell's text.
fn split_options(s: &str) -> (String, Vec<(String, String)>) {
    let mut words: Vec<&str> = s.split_whitespace().collect();
    let mut opts = Vec::new();
    while let Some(w) = words.last() {
        match w.split_once('=') {
            Some((k, v)) if is_ident(k) && !v.is_empty() && !k.contains('_') => {
                opts.push((k.to_string(), v.to_string()));
                words.pop();
            }
            _ => break,
        }
    }
    opts.reverse();
    // keep the body's original spacing up to the first option
    let body = if opts.is_empty() {
        s.trim().to_string()
    } else {
        let n = words.len();
        let mut rest = s.trim_start();
        for _ in 0..n {
            rest = rest.trim_start();
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest = &rest[end..];
        }
        s.trim_start()[..s.trim_start().len() - rest.len()]
            .trim()
            .to_string()
    };
    (body, opts)
}

/// Splits a trailing `# id=…` comment off a line.
fn split_id(line: &str) -> Result<(&str, Option<String>)> {
    if let Some(pos) = line.rfind("# id=") {
        let id = line[pos + 5..].trim();
        if !id.chars().all(|c| c.is_ascii_alphanumeric()) || id.is_empty() {
            return Err(Error::invalid(format!("bad cell id '{id}'")));
        }
        return Ok((&line[..pos], Some(id.to_string())));
    }
    Ok((line, None))
}

fn parse_cell(
    line: &str,
    rest: &mut std::iter::Peekable<impl Iterator<Item = (usize, String)>>,
) -> Result<Cell> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Cell {
            id: None,
            body: CellBody::Blank,
            options: vec![],
        });
    }
    if let Some(c) = trimmed.strip_prefix('#')
        && !c.trim_start().starts_with("id=")
    {
        return Ok(Cell {
            id: None,
            body: CellBody::Comment(c.strip_prefix(' ').unwrap_or(c).to_string()),
            options: vec![],
        });
    }
    let (kw, after) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    if kw == "note" {
        let after = after.trim_start();
        let body = after
            .strip_prefix("\"\"\"")
            .ok_or_else(|| Error::invalid("a note is written note \"\"\"text\"\"\""))?;
        let mut text = String::new();
        let mut cur = body.to_string();
        loop {
            if let Some(end) = cur.find("\"\"\"") {
                text.push_str(&cur[..end]);
                let tail = &cur[end + 3..];
                let (tail, id) = split_id(tail)?;
                let (junk, options) = split_options(tail);
                if !junk.is_empty() {
                    return Err(Error::invalid(format!("unexpected '{junk}' after a note")));
                }
                return Ok(Cell {
                    id,
                    body: CellBody::Note { text },
                    options,
                });
            }
            text.push_str(&cur);
            text.push('\n');
            cur = rest
                .next()
                .map(|(_, l)| l)
                .ok_or_else(|| Error::invalid("unclosed note"))?;
        }
    }
    let (line_body, id) = split_id(after)?;
    let (body_text, options) = split_options(line_body);
    let body = match kw {
        "def" => {
            let (name, term) = body_text
                .split_once(":=")
                .ok_or_else(|| Error::invalid("a definition is written def name := term"))?;
            let name = name.trim();
            if !is_ident(name) {
                return Err(Error::invalid(format!("'{name}' is not a name")));
            }
            CellBody::Def {
                name: name.to_string(),
                term: text::parse(term)?,
            }
        }
        "watch" => CellBody::Watch {
            term: text::parse(&body_text)?,
        },
        "solve" => {
            let (eqn, vars) = body_text
                .rsplit_once(" for ")
                .ok_or_else(|| Error::invalid("solve is written solve lhs = rhs for c1, c2"))?;
            let (lhs, rhs) = eqn
                .split_once('=')
                .ok_or_else(|| Error::invalid("solve needs an equation lhs = rhs"))?;
            let unknowns: Vec<String> = vars.split(',').map(|v| v.trim().to_string()).collect();
            if let Some(bad) = unknowns.iter().find(|v| !is_ident(v)) {
                return Err(Error::invalid(format!("'{bad}' is not an unknown")));
            }
            CellBody::Solve {
                lhs: text::parse(lhs)?,
                rhs: text::parse(rhs)?,
                unknowns,
            }
        }
        "check" => {
            let (name, args) = body_text
                .split_once('(')
                .ok_or_else(|| Error::invalid("a check is written check name(arg, …)"))?;
            let args = args
                .trim_end()
                .strip_suffix(')')
                .ok_or_else(|| Error::invalid("a check's arguments end with ')'"))?;
            let name = name.trim();
            if !is_ident(name) {
                return Err(Error::invalid(format!("'{name}' is not a check")));
            }
            // the final ')' must close the check's own '(', not one inside its arguments
            let depth = args.chars().fold(0i32, |d, c| match c {
                '(' => d + 1,
                ')' => d - 1,
                _ => d,
            });
            if depth > 0 {
                return Err(Error::invalid(format!(
                    "unclosed '{name}(': the last ')' closes a parenthesis inside its arguments"
                )));
            }
            let args = split_top_level(args)
                .iter()
                .map(|a| text::parse(a))
                .collect::<Result<_>>()?;
            CellBody::Check {
                name: name.to_string(),
                args,
            }
        }
        k => return Err(Error::invalid(format!("unknown cell kind '{k}'"))),
    };
    Ok(Cell { id, body, options })
}

/// Splits at commas outside parentheses.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() || !out.is_empty() {
        out.push(&s[start..]);
    }
    out
}

impl Workspace {
    pub fn parse(src: &str) -> Result<Workspace> {
        let mut lines = src.lines().map(String::from).enumerate().peekable();
        let header = lines.next().map(|x| x.1).unwrap_or_default();
        if header.trim() != WORKSPACE_FORMAT {
            return Err(Error::invalid(format!(
                "a workspace starts with the line {WORKSPACE_FORMAT}"
            )));
        }
        let ctx_line = lines.next().map(|x| x.1).unwrap_or_default();
        let ctx = ctx_line
            .trim()
            .strip_prefix("context ")
            .ok_or_else(|| Error::invalid("the second line of a workspace is its context"))?;
        let context = ContextSpec::parse(ctx)?;
        let mut cells = Vec::new();
        while let Some((n, line)) = lines.next() {
            let cell = parse_cell(&line, &mut lines).map_err(|e| Error {
                message: format!("line {}: {}", n + 1, e.message),
                ..e
            })?;
            cells.push(cell);
        }
        while cells.last().is_some_and(|c| c.body == CellBody::Blank) {
            cells.pop();
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &cells {
            if let Some(id) = &c.id
                && !seen.insert(id.clone())
            {
                return Err(Error::invalid(format!("cell id {id} is used twice")));
            }
        }
        Ok(Workspace { context, cells })
    }

    /// Gives every cell without an ID a fresh one: its kind's letter and the next unused number.
    pub fn assign_ids(&mut self) {
        let mut next = 1 + self
            .cells
            .iter()
            .filter_map(|c| c.id.as_ref()?.get(1..)?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        for c in &mut self.cells {
            if c.id.is_none()
                && let Some(p) = c.body.id_prefix()
            {
                c.id = Some(format!("{p}{next}"));
                next += 1;
            }
        }
    }

    pub fn print(&self) -> String {
        let mut s = format!("{WORKSPACE_FORMAT}\ncontext {}\n", self.context.print());
        for c in &self.cells {
            s.push_str(&c.print());
            s.push('\n');
        }
        s
    }
}

impl Cell {
    /// Parses one cell from its text: one line, or several for a note.
    pub fn parse(src: &str) -> Result<Cell> {
        let mut lines = src.lines().map(String::from).enumerate().peekable();
        let first = lines.next().map(|x| x.1).unwrap_or_default();
        let cell = parse_cell(&first, &mut lines)?;
        if let Some((_, extra)) = lines.find(|(_, l)| !l.trim().is_empty()) {
            return Err(Error::invalid(format!(
                "one cell per entry: unexpected '{}'",
                extra.trim()
            )));
        }
        Ok(cell)
    }

    /// The cell in canonical form, with its options and ID, as one entry of a workspace file.
    pub fn print(&self) -> String {
        let c = self;
        let mut line = match &c.body {
            CellBody::Def { name, term } => format!("def {name} := {}", text::print(term)),
            CellBody::Watch { term } => format!("watch {}", text::print(term)),
            CellBody::Solve { lhs, rhs, unknowns } => {
                format!(
                    "solve {} = {} for {}",
                    text::print(lhs),
                    text::print(rhs),
                    unknowns.join(", ")
                )
            }
            CellBody::Check { name, args } => {
                let a: Vec<String> = args.iter().map(text::print).collect();
                format!("check {name}({})", a.join(", "))
            }
            CellBody::Note { text } => format!("note \"\"\"{text}\"\"\""),
            CellBody::Comment(t) => format!("# {t}").trim_end().to_string(),
            CellBody::Blank => String::new(),
        };
        for (k, v) in &c.options {
            line.push_str(&format!("  {k}={v}"));
        }
        if let Some(id) = &c.id {
            line.push_str(&format!("  # id={id}"));
        }
        line
    }

    /// The name a definition introduces.
    pub fn defines(&self) -> Option<&str> {
        match &self.body {
            CellBody::Def { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The bare names (no subscript, no arguments) the cell's terms mention: the definitions and
    /// unknowns it may depend on, along with any scalars of the context.
    pub fn names_used(&self) -> Vec<String> {
        fn walk(a: &Ast, out: &mut std::collections::BTreeSet<String>) {
            match a {
                Ast::Name { name, sub, args } => {
                    if sub.is_none() && args.is_none() {
                        out.insert(name.clone());
                    }
                    for x in args.iter().flatten() {
                        walk(x, out);
                    }
                }
                Ast::Pow(x, _) | Ast::PowExpr(x, _) => walk(x, out),
                Ast::Range { body, .. } => walk(body, out),
                Ast::Comp(xs) | Ast::Tensor(xs) => xs.iter().for_each(|x| walk(x, out)),
                Ast::Sum(xs) => xs.iter().for_each(|(_, x)| walk(x, out)),
                Ast::Int(_) | Ast::Frac(..) | Ast::Imag | Ast::Idem(_) | Ast::Id(_) => {}
            }
        }
        let mut out = std::collections::BTreeSet::new();
        match &self.body {
            CellBody::Def { term, .. } | CellBody::Watch { term } => walk(term, &mut out),
            CellBody::Solve { lhs, rhs, .. } => {
                walk(lhs, &mut out);
                walk(rhs, &mut out);
            }
            CellBody::Check { args, .. } => args.iter().for_each(|a| walk(a, &mut out)),
            CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank => {}
        }
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diagcalc-workspace/1
context klr type=A2 nu=2i+j ring=QQ scalars=c1,c2

# Idempotents in R(2i+j)
def e := c1 x_1 psi_1 e(iij) + c2 psi_1 x_2 e(iij)  # id=d1
watch e e - e  view=diagrams  group=idempotent  # id=w2
solve e e - e = 0 for c1, c2  # id=s3
check idempotent(e)  # id=k4
check equal(psi_1 psi_2 psi_1 e(iji), psi_2 psi_1 psi_2 e(iji) + e(iji))  # id=k5
note \"\"\"Candidate from the A2 search,
step 3.\"\"\"  # id=n6
";

    #[test]
    fn round_trip() {
        let w = Workspace::parse(SAMPLE).unwrap();
        assert_eq!(w.cells.len(), 8);
        assert_eq!(w.print(), SAMPLE);
        assert_eq!(Workspace::parse(&w.print()).unwrap(), w);
    }

    #[test]
    fn canonical_spacing_and_ids() {
        let src = "diagcalc-workspace/1\ncontext nilhecke nu=3\ndef a:=psi_1   x_1 # id=d4\nwatch a*a\n\n";
        let mut w = Workspace::parse(src).unwrap();
        w.assign_ids();
        assert_eq!(
            w.print(),
            "diagcalc-workspace/1\ncontext nilhecke nu=3 ring=QQ\ndef a := psi_1 x_1  # id=d4\nwatch a a  # id=w5\n"
        );
    }

    #[test]
    fn errors_name_the_line() {
        let e = Workspace::parse("diagcalc-workspace/1\ncontext nilhecke\nwatch psi_1 +\n")
            .unwrap_err();
        assert!(e.message.starts_with("line 3"), "{}", e.message);
        assert!(Workspace::parse("context nilhecke\n").is_err());
        // an unclosed check names the check, not the idempotent whose ')' ends the line
        let e = Cell::parse("check idempotent(x_1 e(iji)").unwrap_err();
        assert!(
            e.message.contains("unclosed 'idempotent('"),
            "{}",
            e.message
        );
    }

    #[test]
    fn single_cells() {
        let c = Cell::parse("def e:=c1 (x_1 psi_1)^2 e(iij)+ f  view=diagrams  # id=d3").unwrap();
        assert_eq!(c.defines(), Some("e"));
        assert_eq!(c.names_used(), ["c1", "f"]);
        assert_eq!(
            c.print(),
            "def e := c1 (x_1 psi_1)^2 e(iij) + f  view=diagrams  # id=d3"
        );
        let n = Cell::parse("note \"\"\"two\nlines\"\"\"").unwrap();
        assert_eq!(
            n.body,
            CellBody::Note {
                text: "two\nlines".into()
            }
        );
        assert!(Cell::parse("watch a\nwatch b").is_err());
        let s = Cell::parse("solve a a = a for c1, c2").unwrap();
        assert_eq!(s.names_used(), ["a"]);
    }
}
