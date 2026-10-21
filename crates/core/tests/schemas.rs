//! The JSON schemas in `spec/` against the code: every fixture's request and response, and every
//! shipped presentation file.

use diagcalc_core::{builtin, ops};
use jsonschema::{Resource, Validator};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

fn load(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A validator for `$ref` into one of the schemas, with every schema registered.
fn validator(reference: &str) -> Validator {
    let mut opts = jsonschema::options();
    let mut files: Vec<PathBuf> = std::fs::read_dir(spec_dir().join("ops"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_str().unwrap().ends_with(".schema.json"))
        .collect();
    files.push(spec_dir().join("presentation.schema.json"));
    for f in files {
        let schema = load(&f);
        let id = schema["$id"].as_str().unwrap().to_string();
        opts.with_resource(id, Resource::from_contents(schema).unwrap());
    }
    opts.build(&json!({ "$ref": reference })).unwrap()
}

fn assert_valid(v: &Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = v
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path))
        .collect();
    assert!(
        errors.is_empty(),
        "{what}: {}\n{instance:#}",
        errors.join("; ")
    );
}

const BASE: &str = "https://github.com/apellis/diagrammatic-calculator/spec";

#[test]
fn fixtures_match_code_and_schemas() {
    let dir = spec_dir().join("ops/fixtures");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let fx = load(&path);
        let op = fx["op"].as_str().unwrap();
        let schema = format!("{BASE}/ops/{op}.schema.json");
        assert_valid(
            &validator(&format!("{schema}#/$defs/request")),
            &fx["request"],
            &format!("{name} request"),
        );
        let got = ops::call(op, &fx["request"]);
        assert_valid(
            &validator(&format!("{schema}#/$defs/response")),
            &got,
            &format!("{name} response"),
        );
        for (k, want) in fx["expect"].as_object().unwrap() {
            assert_eq!(&got[k], want, "{name}: field {k}\n{got:#}");
        }
        n += 1;
    }
    assert!(n >= 10);
}

#[test]
fn shipped_presentations_match_schema() {
    let v = validator(&format!("{BASE}/presentation.schema.json"));
    for (name, text) in builtin::PRESENTATIONS {
        let as_json: Value =
            serde_json::to_value(toml::from_str::<toml::Value>(text).unwrap()).unwrap();
        assert_valid(&v, &as_json, name);
    }
}

#[test]
fn every_operation_has_a_schema() {
    for op in ops::OPERATIONS {
        assert!(
            spec_dir().join(format!("ops/{op}.schema.json")).exists(),
            "{op}"
        );
    }
}

#[test]
fn schemas_reject_bad_messages() {
    let v = validator(&format!("{BASE}/ops/simplify.schema.json#/$defs/response"));
    assert!(!v.is_valid(&json!({ "status": "ok", "terms": 1 })));
    assert!(!v.is_valid(&json!({ "status": "error", "kind": "oops", "message": "" })));
    let v = validator(&format!("{BASE}/presentation.schema.json"));
    assert!(!v.is_valid(&json!({ "format": "diagcalc-presentation/1", "name": "x", "colours": "cartan", "generators": {}, "extra": 1 })));
}
