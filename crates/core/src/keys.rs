//! Rejecting misspelled keys in JSON inputs: an object may only use the keys its format names,
//! and an unknown key is an error that names it and the nearest known key.

use crate::error::{Error, Result};
use serde_json::Value;

/// The edit distance between two keys (insertions, deletions, and substitutions).
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// The known key closest to `key`, when it is close enough to be a likely misspelling: at most
/// two edits, and fewer edits than the shorter key has characters. Ties go to the first listed.
pub fn suggestion<'k>(key: &str, known: &[&'k str]) -> Option<&'k str> {
    known
        .iter()
        .map(|k| (distance(key, k), *k))
        .filter(|(d, k)| *d <= 2 && *d < key.chars().count().min(k.chars().count()))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// Errors when the object `v` has a key outside `known`; `what` names the object ("a complex",
/// "the request"). Values that are not objects are left to the caller.
pub fn check(v: &Value, known: &[&str], what: &str) -> Result<()> {
    let Some(o) = v.as_object() else {
        return Ok(());
    };
    // serde_json keeps keys sorted, so the first unknown key reported is deterministic
    if let Some(k) = o.keys().find(|k| !known.contains(&k.as_str())) {
        let hint = suggestion(k, known)
            .map(|s| format!(" (did you mean '{s}'?)"))
            .unwrap_or_default();
        let list = known
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Error::invalid(format!(
            "{what} has no key '{k}'{hint}; its keys are {list}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn suggestions() {
        let known = ["terms", "differentials", "dimension"];
        assert_eq!(suggestion("diferentials", &known), Some("differentials"));
        assert_eq!(suggestion("term", &known), Some("terms"));
        assert_eq!(suggestion("zzz", &known), None);
        // a one-letter key is not "close" to another one-letter key
        assert_eq!(suggestion("a", &["b"]), None);
    }

    #[test]
    fn unknown_keys_are_named() {
        let e = check(
            &json!({ "terms": [], "diferentials": [] }),
            &["terms", "differentials"],
            "a complex",
        )
        .unwrap_err();
        assert_eq!(
            e.message,
            "a complex has no key 'diferentials' (did you mean 'differentials'?); its keys are \
             'terms', 'differentials'"
        );
        let e = check(&json!({ "xyz": 1 }), &["terms"], "a complex").unwrap_err();
        assert!(!e.message.contains("did you mean"), "{}", e.message);
        assert!(check(&json!({ "terms": [] }), &["terms"], "a complex").is_ok());
        assert!(check(&json!([1]), &["terms"], "a complex").is_ok());
    }
}
