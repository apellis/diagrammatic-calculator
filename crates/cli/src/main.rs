//! `diagcalc`: the command-line surface of the calculator.

mod mcp;

use diagcalc_core::builtin;
use diagcalc_core::ops::{self, Request};
use diagcalc_core::workspace::Workspace;
use serde_json::Value;
use std::process::ExitCode;

const USAGE: &str = "usage: diagcalc <command> [options]

commands:
  simplify -c CONTEXT TERM          normal form of a term
  check -c CONTEXT NAME TERM...     equal, zero, idempotent, orthogonal, homogeneous
  draw -c CONTEXT [--format tikz|svg] TERM
  run FILE.dcw                      evaluate every cell of a workspace
  fmt FILE.dcw [--write]            print a workspace in canonical form, numbering new cells
  check-presentation NAME|FILE [-c CONTEXT]
  presentations                     list the shipped presentations
  match -c CONTEXT TERM             relations that apply, each with a preview of the result
  apply -c CONTEXT TERM             apply one relation (--relation, --match) or --everywhere
  interchange -c CONTEXT TERM --layer N
  slide -c CONTEXT TERM --layer N --dir up|down
  straighten -c CONTEXT TERM        rewrite to the basis, step by step
  chain -c CONTEXT LHS RHS          rewrite chains proving LHS = RHS
  export-lean -c CONTEXT LHS [RHS] --name NAME
  lean-chains FILE.toml [--check]   write (or check) the Lean files for a list of chains
  call OPERATION                    run an operation on a JSON request read from stdin
  mcp                               serve the operations over MCP (stdio)
  skill                             print the guide for AI agents (skill/SKILL.md)
  version

options:
  -c, --context CONTEXT   e.g. \"klr type=A2 nu=2i+j ring=QQ\"
  --pres FILE             a presentation file not shipped with the calculator
  --strategy NAME         a representation declared by the presentation
  --json                  print the operation's JSON result
  -o FILE                 write the output to FILE

rewriting options:
  --summand N             the summand to work on (default 1)
  --relation NAME         only this relation
  --match N               which match to apply (default 1)
  --everywhere            apply the relation at every disjoint match
  --rect L1-L2,S1-S2      only matches inside these layers and strands
  --layer N --dir up|down the layer to interchange or slide, and the direction
  --max-steps N           a cap on the number of steps";

/// The guide for AI agents, shared with the web app's AI panel.
const SKILL: &str = include_str!("../../../skill/SKILL.md");

#[derive(Default)]
struct Args {
    positional: Vec<String>,
    context: Option<String>,
    pres: Option<String>,
    strategy: Option<String>,
    format: Option<String>,
    output: Option<String>,
    json: bool,
    write: bool,
    check: bool,
    rewrite: serde_json::Map<String, Value>,
}

/// `1-3,2-4` as `{"layers": [1, 3], "strands": [2, 4]}`.
fn rectangle(s: &str) -> Result<Value, String> {
    let bad = || format!("--rect takes L1-L2,S1-S2, not '{s}'");
    let range = |r: &str| -> Result<Value, String> {
        let (a, b) = r.split_once('-').unwrap_or((r, r));
        let n = |x: &str| x.trim().parse::<u64>().map_err(|_| bad());
        Ok(serde_json::json!([n(a)?, n(b)?]))
    };
    let (l, st) = s.split_once(',').ok_or_else(bad)?;
    Ok(serde_json::json!({ "layers": range(l)?, "strands": range(st)? }))
}

fn parse_args(raw: Vec<String>) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = raw.into_iter();
    while let Some(x) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match x.as_str() {
            "-c" | "--context" => a.context = Some(val(&x)?),
            "--pres" => a.pres = Some(val(&x)?),
            "--strategy" => a.strategy = Some(val(&x)?),
            "--format" => a.format = Some(val(&x)?),
            "-o" | "--output" => a.output = Some(val(&x)?),
            "--json" => a.json = true,
            "--write" => a.write = true,
            "--check" => a.check = true,
            "--everywhere" => {
                a.rewrite.insert("everywhere".into(), Value::Bool(true));
            }
            "--summand" | "--match" | "--layer" | "--max-steps" => {
                let v = val(&x)?;
                let n: u64 = v
                    .parse()
                    .map_err(|_| format!("{x} takes a number, not '{v}'"))?;
                let key = x.trim_start_matches("--").replace('-', "_");
                // Matches are numbered from 1 on the command line, from 0 in requests.
                let n = if key == "match" {
                    n.saturating_sub(1)
                } else {
                    n
                };
                a.rewrite.insert(key, n.into());
            }
            "--relation" | "--name" | "--doc" => {
                let v = val(&x)?;
                a.rewrite.insert(x[2..].into(), v.into());
            }
            "--dir" => {
                let v = val(&x)?;
                a.rewrite.insert("direction".into(), v.into());
            }
            "--rect" => {
                let v = rectangle(&val(&x)?)?;
                a.rewrite.insert("rectangle".into(), v);
            }
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => a.positional.push(x),
        }
    }
    Ok(a)
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))
}

fn request(a: &Args) -> Result<Request, String> {
    Ok(Request {
        context: a.context.clone().ok_or("this command needs -c CONTEXT")?,
        presentation: a.pres.as_deref().map(read).transpose()?,
        strategy: a.strategy.clone(),
    })
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn provenance_line(v: &Value) -> String {
    let p = &v["provenance"];
    format!(
        "  [{}: {}; certificate {}: {}]",
        s(&p["strategy"]),
        s(&p["method"]),
        s(&p["certificate"]["status"]),
        s(&p["certificate"]["reference"])
    )
}

/// A solve result on one line: the outcome and the solutions found.
fn solve_line(c: &Value) -> String {
    let assign = |m: &Value| {
        m.as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, v)| format!("{k} = {}", v.as_str().unwrap_or("?")))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    match c["outcome"].as_str().unwrap_or("") {
        "none" => "no solution".into(),
        "unique" => format!("unique solution: {}", assign(&c["solution"])),
        "finite" => {
            let vars: Vec<&str> = c["variables"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let pts: Vec<String> = c["points"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| {
                    let xs: Vec<String> = p
                        .as_array()
                        .into_iter()
                        .flatten()
                        .zip(&vars)
                        .map(|(x, v)| format!("{v} = {}", x.as_str().unwrap_or("?")))
                        .collect();
                    format!("({})", xs.join(", "))
                })
                .collect();
            format!("{} solutions: {}", pts.len(), pts.join(" "))
        }
        "family" => format!(
            "a family of dimension {}",
            c["dimension"].as_u64().unwrap_or(0)
        ),
        "limit_reached" => "limit reached".into(),
        o => o.to_string(),
    }
}

fn verdict(v: &Value) -> String {
    let mut out = if v["holds"] == true {
        "holds".to_string()
    } else {
        "fails".to_string()
    };
    if let Some(w) = v["witness"].as_str() {
        out.push_str(&format!(": {w}"));
    }
    out
}

/// Human-readable form of an operation result.
fn human(cmd: &str, v: &Value) -> String {
    if v["status"] == "error" {
        return format!("error: {}", s(&v["message"]));
    }
    match cmd {
        "simplify" => format!("{}\n{}", s(&v["normal_form"]), provenance_line(v)),
        "check" => format!("{}\n{}", verdict(v), provenance_line(v)),
        "draw" => s(&v["output"]).to_string(),
        "check-presentation" => {
            let mut out = String::new();
            for r in v["reports"].as_array().into_iter().flatten() {
                out.push_str(&format!(
                    "{} in {}: {} passed, {} failed, {} unchecked\n",
                    s(&r["presentation"]),
                    s(&r["context"]),
                    r["passed"],
                    r["failed"],
                    r["unchecked"]
                ));
                for f in r["findings"].as_array().into_iter().flatten() {
                    out.push_str(&format!(
                        "  {} {}: {}\n",
                        s(&f["status"]),
                        s(&f["check"]),
                        s(&f["detail"])
                    ));
                }
            }
            out.trim_end().to_string()
        }
        "run" => {
            let mut out = format!("context {}\n", s(&v["context"]));
            for c in v["cells"].as_array().into_iter().flatten() {
                let line = if c["status"] == "error" {
                    format!("error: {}", s(&c["message"]))
                } else {
                    match s(&c["kind"]) {
                        "def" => format!("{} := {}", s(&c["name"]), s(&c["term"])),
                        "watch" => s(&c["normal_form"]).to_string(),
                        "check" => verdict(c),
                        "solve" => solve_line(c),
                        _ => String::new(),
                    }
                };
                out.push_str(&format!("{}: {line}\n", c["id"].as_str().unwrap_or("-")));
            }
            out.trim_end().to_string()
        }
        "match" => {
            let mut out = String::new();
            for m in v["matches"].as_array().into_iter().flatten() {
                out.push_str(&format!(
                    "{}. {}\n   => {}\n",
                    m["index"].as_u64().unwrap_or(0) + 1,
                    match_line(m),
                    s(&m["preview"])
                ));
            }
            if out.is_empty() {
                "no relation applies".into()
            } else {
                out.trim_end().to_string()
            }
        }
        "interchange" => format!("{}\n  [interchange sign {}]", s(&v["term"]), v["sign"]),
        "export-lean" => s(&v["lean"]).to_string(),
        "apply" | "slide" | "straighten" => {
            let mut out = chain_text(v);
            if cmd == "straighten" {
                out.push_str(&format!(
                    "\n  [basis {}; agrees with the representation: {}]",
                    s(&v["basis"]),
                    v["agrees"]
                ));
            }
            out
        }
        "chain" => {
            let mut out = format!(
                "{}\n",
                if v["equal"] == true {
                    "equal"
                } else {
                    "not equal"
                }
            );
            for c in v["chains"].as_array().into_iter().flatten() {
                out.push_str(&chain_text(c));
                out.push('\n');
            }
            out.trim_end().to_string()
        }
        _ => v.to_string(),
    }
}

fn match_line(m: &Value) -> String {
    let r = &m["rectangle"];
    let sign = if m["sign"] == -1 { ", sign -1" } else { "" };
    format!(
        "{} ({}) at layers {}-{}, strands {}-{}{sign}",
        s(&m["relation"]),
        s(&m["direction"]),
        r["layers"][0],
        r["layers"][1],
        r["strands"][0],
        r["strands"][1]
    )
}

/// A chain as numbered steps, each with the word it rewrites and what it becomes.
fn chain_text(v: &Value) -> String {
    let mut out = format!("{}\n", s(&v["start"]));
    for (i, st) in v["steps"].as_array().into_iter().flatten().enumerate() {
        let results: Vec<&str> = st["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| s(&r["term"]))
            .collect();
        let results = if results.is_empty() {
            "0".to_string()
        } else {
            results.join(" + ")
        };
        out.push_str(&format!(
            "  {}. {}\n     {} -> {}\n",
            i + 1,
            match_line(st),
            s(&st["word"]),
            results
        ));
    }
    out.push_str(&format!("= {}", s(&v["term"])));
    out
}

/// The JSON request of a rewriting command.
fn rewrite_request(a: &Args, terms: &[String]) -> Result<Value, String> {
    let mut m = a.rewrite.clone();
    m.insert(
        "context".into(),
        a.context
            .clone()
            .ok_or("this command needs -c CONTEXT")?
            .into(),
    );
    if let Some(p) = &a.pres {
        m.insert("presentation".into(), read(p)?.into());
    }
    if let Some(st) = &a.strategy {
        m.insert("strategy".into(), st.clone().into());
    }
    let (t, rest) = terms.split_first().ok_or("this command needs a term")?;
    m.insert("term".into(), t.clone().into());
    if let Some(r) = rest.first() {
        m.insert("rhs".into(), r.clone().into());
    }
    Ok(Value::Object(m))
}

/// Writes the Lean files for the chains listed in `path`, next to it; with `check`, only
/// reports whether the files on disk are current.
fn lean_chains(path: &str, check: bool) -> Result<bool, String> {
    let dir = std::path::Path::new(path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let files = diagcalc_core::lean::export_batch(&read(path)?, "DiagcalcChains")
        .map_err(|e| e.to_string())?;
    let mut current = true;
    for (rel, text) in files {
        // the exporter checks every name; this keeps any path it returns inside `dir` regardless
        let inside = std::path::Path::new(&rel)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside {
            return Err(format!(
                "refusing to write {rel}: not a plain relative path"
            ));
        }
        let file = dir.join(&rel);
        if check {
            if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
                eprintln!("{} is out of date", file.display());
                current = false;
            }
        } else {
            if let Some(d) = file.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, text)
                .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
            println!("wrote {}", file.display());
        }
    }
    Ok(current)
}

fn run(raw: Vec<String>) -> Result<bool, String> {
    let a = parse_args(raw)?;
    let Some(cmd) = a.positional.first().cloned() else {
        return Err(USAGE.into());
    };
    let rest = &a.positional[1..];
    let one = |what: &str| -> Result<String, String> {
        match rest {
            [x] => Ok(x.clone()),
            _ => Err(format!("{cmd} takes one {what}")),
        }
    };
    let v = match cmd.as_str() {
        "version" | "--version" => {
            println!("diagcalc {}", diagcalc_core::VERSION);
            return Ok(true);
        }
        "help" | "-h" => {
            println!("{USAGE}");
            return Ok(true);
        }
        "skill" => {
            print!("{}", SKILL);
            return Ok(true);
        }
        "presentations" => {
            for (name, _) in builtin::PRESENTATIONS {
                let p = builtin::presentation(name).map_err(|e| e.to_string())?;
                println!("{name:14} {}", p.title);
            }
            return Ok(true);
        }
        "simplify" => ops::simplify(&request(&a)?, &one("term")?),
        "check" => {
            let (name, terms) = rest
                .split_first()
                .ok_or("check needs a check name and its terms")?;
            ops::check(&request(&a)?, name, terms)
        }
        "draw" => ops::render(
            &request(&a)?,
            &one("term")?,
            a.format.as_deref().unwrap_or("tikz"),
        ),
        "run" => {
            let src = read(&one("workspace file")?)?;
            let pres = a.pres.as_deref().map(read).transpose()?;
            ops::run_workspace(&src, pres.as_deref())
        }
        "fmt" => {
            let path = one("workspace file")?;
            let mut ws = Workspace::parse(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            ws.assign_ids();
            let out = ws.print();
            if a.write {
                std::fs::write(&path, out).map_err(|e| format!("cannot write {path}: {e}"))?;
            } else {
                print!("{out}");
            }
            return Ok(true);
        }
        "check-presentation" => {
            let target = one("presentation name or file")?;
            let text = if target.ends_with(".toml") {
                read(&target)?
            } else {
                target
            };
            ops::presentation_check(&text, a.context.as_deref())
        }
        "match" | "apply" | "interchange" | "slide" | "straighten" | "chain" | "export-lean" => {
            let op = match cmd.as_str() {
                "match" => "relations.match",
                "apply" => "relation.apply",
                "chain" => "chain.find",
                "export-lean" => "export.lean",
                c => c,
            };
            ops::call(op, &rewrite_request(&a, rest)?)
        }
        "call" => {
            let op = one("operation")?;
            let mut input = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
                .map_err(|e| e.to_string())?;
            let req: Value = serde_json::from_str(&input).map_err(|e| format!("request: {e}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::call(&op, &req)).unwrap()
            );
            return Ok(true);
        }
        "mcp" => {
            mcp::serve().map_err(|e| e.to_string())?;
            return Ok(true);
        }
        "lean-chains" => return lean_chains(&one("chain file")?, a.check),
        c => return Err(format!("unknown command '{c}'\n\n{USAGE}")),
    };
    let ok = v["status"] == "ok" && v["ok"] != false && v["holds"] != false;
    let out = if a.json {
        serde_json::to_string_pretty(&v).unwrap()
    } else {
        human(&cmd, &v)
    };
    match &a.output {
        Some(path) => {
            std::fs::write(path, out + "\n").map_err(|e| format!("cannot write {path}: {e}"))?
        }
        None => println!("{out}"),
    }
    Ok(ok)
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
