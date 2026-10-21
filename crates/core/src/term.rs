//! Words and terms: linear combinations of layered words.
//!
//! A word is a source object and a list of layers from bottom to top; a layer places one generator
//! at a strand position, with identities elsewhere. A term is a finite sum of coefficient × word,
//! kept sorted with no zero coefficients. The operations that need the presentation (typing,
//! canonical layering, composition) live on [`crate::context::Context`].

use std::cmp::Ordering;

/// One strand label of an object: a colour, pointing up or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Letter {
    pub colour: u16,
    pub up: bool,
}

impl Letter {
    pub fn up(colour: usize) -> Self {
        Letter {
            colour: colour as u16,
            up: true,
        }
    }
}

pub type Object = Vec<Letter>;

/// One generator placed at a strand position (0-based internally; printed 1-based), with the
/// colours bound to the generator's colour variables.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Layer {
    pub generator: u16,
    pub pos: u16,
    pub binding: Vec<u16>,
}

/// A layered word: a source object and layers from bottom to top.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Word {
    pub source: Object,
    pub layers: Vec<Layer>,
}

impl Word {
    pub fn identity(source: Object) -> Self {
        Word {
            source,
            layers: vec![],
        }
    }
}

/// Words are ordered by source, then by number of layers, then layer by layer from the bottom.
impl Ord for Word {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.layers.len().cmp(&other.layers.len()))
            .then_with(|| self.layers.cmp(&other.layers))
    }
}

impl PartialOrd for Word {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A finite sum of coefficient × word: sorted by word, no zero coefficients.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Term<E> {
    pub terms: Vec<(Word, E)>,
}

impl<E> Term<E> {
    pub fn zero() -> Self {
        Term { terms: vec![] }
    }
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }
    pub fn len(&self) -> usize {
        self.terms.len()
    }
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}
