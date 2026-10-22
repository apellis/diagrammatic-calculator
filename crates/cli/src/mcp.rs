//! `diagcalc mcp`: the operations as an MCP server over stdio (JSON-RPC 2.0, one message per
//! line). Each operation is a tool named after it with `.` replaced by `_`; its input schema is
//! the operation's request schema with the shared definitions inlined.

use diagcalc_core::ops;
use serde_json::{Map, Value, json};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

fn tool_name(op: &str) -> String {
    op.replace('.', "_")
}

/// Replaces `$ref`s into the shared definitions by the definitions themselves.
fn inline(v: &Value, common: &Value) -> Value {
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get("$ref").and_then(Value::as_str) {
                let name = r.rsplit('/').next().unwrap_or_default();
                return inline(&common["$defs"][name], common);
            }
            Value::Object(
                o.iter()
                    .map(|(k, x)| (k.clone(), inline(x, common)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| inline(x, common)).collect()),
        x => x.clone(),
    }
}

/// One object schema: the request's `allOf` parts merged into its own properties.
fn input_schema(schema: &Value, common: &Value) -> Value {
    let req = inline(&schema["$defs"]["request"], common);
    let mut props = Map::new();
    let mut required: Vec<Value> = Vec::new();
    let parts = req["allOf"].as_array().cloned().unwrap_or_default();
    for part in parts.iter().chain(std::iter::once(&req)) {
        if let Some(p) = part["properties"].as_object() {
            for (k, x) in p {
                props.insert(k.clone(), x.clone());
            }
        }
        for r in part["required"].as_array().into_iter().flatten() {
            if !required.contains(r) {
                required.push(r.clone());
            }
        }
    }
    json!({ "type": "object", "properties": props, "required": required })
}

fn tools() -> Value {
    let common: Value = serde_json::from_str(ops::COMMON_SCHEMA).expect("common schema");
    let list: Vec<Value> = ops::SCHEMAS
        .iter()
        .map(|(op, text)| {
            let schema: Value = serde_json::from_str(text).expect("schema");
            json!({
                "name": tool_name(op),
                "description": schema["title"],
                "inputSchema": input_schema(&schema, &common),
            })
        })
        .collect();
    json!({ "tools": list })
}

fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "diagcalc", "version": diagcalc_core::VERSION },
            "instructions": "Exact calculations in diagrammatic algebra (KLR algebras and \
                relatives). Every tool takes a context line such as \
                \"klr type=A2 nu=2i+j ring=QQ\" and terms in the calculator's text format, \
                read top to bottom: `x_1 psi_1 e(ij)` is a crossing with a dot above it. \
                `diagcalc skill` prints the full guide: term format, relation names, signs.",
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            match ops::OPERATIONS.iter().find(|op| tool_name(op) == name) {
                Some(op) => {
                    let args = msg["params"]
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let out = ops::call(op, &args);
                    let is_error = out["status"] == "error";
                    Ok(json!({
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&out).unwrap() }],
                        "structuredContent": out,
                        "isError": is_error,
                    }))
                }
                None => Err((-32602, format!("unknown tool '{name}'"))),
            }
        }
        m if m.starts_with("notifications/") => return None,
        m => Err((-32601, format!("method '{m}' not found"))),
    };
    let id = id?;
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

/// Serves until stdin closes.
pub fn serve() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": e.to_string() }
            })),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_calls_tools() {
        let r = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], "diagcalc");
        let r = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), ops::OPERATIONS.len());
        for t in tools {
            let s = t["inputSchema"].to_string();
            assert!(!s.contains("$ref"), "{s}");
            assert!(
                t["name"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            );
        }
        let simplify = tools.iter().find(|t| t["name"] == "simplify").unwrap();
        assert_eq!(
            simplify["inputSchema"]["required"],
            json!(["context", "term"])
        );
        let r = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"simplify","arguments":{"context":"nilhecke nu=2","term":"x_1 psi_1"}}}))
        .unwrap();
        assert_eq!(
            r["result"]["structuredContent"]["normal_form"],
            "1 + psi_1 x_2"
        );
        assert_eq!(r["result"]["isError"], false);
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let r = handle(&json!({"jsonrpc":"2.0","id":4,"method":"nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }
}
