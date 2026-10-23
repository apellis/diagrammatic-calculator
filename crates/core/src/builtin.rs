//! The presentations shipped with the calculator.

use crate::error::{Error, Result};
use crate::pres::Presentation;

/// Name and file text of every shipped presentation.
pub const PRESENTATIONS: [(&str, &str); 5] = [
    (
        "nilhecke",
        include_str!("../../../presentations/nilhecke.pres.toml"),
    ),
    (
        "odd-nilhecke",
        include_str!("../../../presentations/odd-nilhecke.pres.toml"),
    ),
    ("klr", include_str!("../../../presentations/klr.pres.toml")),
    ("kl", include_str!("../../../presentations/kl.pres.toml")),
    ("be2", include_str!("../../../presentations/be2.pres.toml")),
];

/// A shipped presentation by name.
pub fn presentation(name: &str) -> Result<Presentation> {
    let (_, text) = PRESENTATIONS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| Error::invalid(format!("no built-in presentation '{name}'")))?;
    Presentation::from_toml(text)
}
