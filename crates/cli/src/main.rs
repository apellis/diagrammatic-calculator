//! `diagcalc`: the command-line surface of the calculator.

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
  version

options:
  -c, --context CONTEXT   e.g. \"klr type=A2 nu=2i+j ring=QQ\"
  --pres FILE             a presentation file not shipped with the calculator
  --strategy NAME         a representation declared by the presentation
  --json                  print the operation's JSON result
  -o FILE                 write the output to FILE";

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
                        _ => String::new(),
                    }
                };
                out.push_str(&format!("{}: {line}\n", c["id"].as_str().unwrap_or("-")));
            }
            out.trim_end().to_string()
        }
        _ => v.to_string(),
    }
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
