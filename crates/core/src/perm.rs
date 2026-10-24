//! Permutations of strand positions, reduced words, and the inversion order of a word.

/// A permutation of `0..n`, as the map `a ↦ w(a)` (the strand at position `a` ends at `w(a)`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Perm(pub Vec<u8>);

impl Perm {
    pub fn identity(n: usize) -> Self {
        Perm((0..n as u8).collect())
    }
    pub fn n(&self) -> usize {
        self.0.len()
    }
    pub fn is_identity(&self) -> bool {
        self.0.iter().enumerate().all(|(a, &b)| a == b as usize)
    }
    /// The simple transposition `s_k` swapping `k` and `k+1` (0-based).
    pub fn simple(n: usize, k: usize) -> Self {
        let mut p = Self::identity(n);
        p.0.swap(k, k + 1);
        p
    }
    /// `self ∘ other`.
    pub fn compose(&self, other: &Perm) -> Perm {
        Perm(other.0.iter().map(|&a| self.0[a as usize]).collect())
    }
    pub fn inverse(&self) -> Perm {
        let mut inv = vec![0u8; self.n()];
        for (a, &b) in self.0.iter().enumerate() {
            inv[b as usize] = a as u8;
        }
        Perm(inv)
    }
    /// `s_k ∘ self`.
    pub fn left_mul_simple(&self, k: usize) -> Perm {
        Perm(
            self.0
                .iter()
                .map(|&b| {
                    if b as usize == k {
                        k as u8 + 1
                    } else if b as usize == k + 1 {
                        k as u8
                    } else {
                        b
                    }
                })
                .collect(),
        )
    }
    /// Number of inversions.
    pub fn length(&self) -> usize {
        let v = &self.0;
        let mut l = 0;
        for a in 0..v.len() {
            for b in a + 1..v.len() {
                if v[a] > v[b] {
                    l += 1;
                }
            }
        }
        l
    }
    /// True if `ℓ(s_k w) < ℓ(w)`.
    pub fn has_left_descent(&self, k: usize) -> bool {
        let inv = self.inverse();
        inv.0[k] > inv.0[k + 1]
    }
    /// The canonical reduced word, topmost letter first: repeatedly take the smallest left
    /// descent. `w = s_{k_1} s_{k_2} ⋯` with `s_{k_1}` applied last.
    pub fn reduced_word(&self) -> Vec<usize> {
        let mut w = self.clone();
        let mut out = Vec::new();
        while !w.is_identity() {
            let k = (0..w.n() - 1).find(|&k| w.has_left_descent(k)).unwrap();
            out.push(k);
            w = w.left_mul_simple(k);
        }
        out
    }
    /// All permutations of `0..n` in lexicographic order.
    pub fn all(n: usize) -> Vec<Perm> {
        let mut out = Vec::new();
        let mut cur: Vec<u8> = (0..n as u8).collect();
        loop {
            out.push(Perm(cur.clone()));
            // next lexicographic permutation
            let Some(i) = (0..n.saturating_sub(1))
                .rev()
                .find(|&i| cur[i] < cur[i + 1])
            else {
                break;
            };
            let j = (i + 1..n).rev().find(|&j| cur[j] > cur[i]).unwrap();
            cur.swap(i, j);
            cur[i + 1..].reverse();
        }
        out
    }
}

/// The inversions created by the letters of a word (topmost letter first), in the order they are
/// created from the bottom, as pairs of starting positions.
pub fn inversion_order(n: usize, word_top_first: &[usize]) -> Vec<(u8, u8)> {
    let mut at: Vec<u8> = (0..n as u8).collect(); // which original strand is at each position
    let mut out = Vec::with_capacity(word_top_first.len());
    for &k in word_top_first.iter().rev() {
        let (a, b) = (at[k], at[k + 1]);
        out.push((a.min(b), a.max(b)));
        at.swap(k, k + 1);
    }
    out
}

/// The sign relating two reduced words of one permutation in the odd nilCoxeter algebra, where
/// distant letters anticommute and braid moves carry no sign: `(-1)` to the number of pairs of
/// disjoint inversions that the two words create in opposite orders.
pub fn odd_word_sign(n: usize, u: &[usize], v: &[usize]) -> bool {
    let a = inversion_order(n, u);
    let b = inversion_order(n, v);
    debug_assert_eq!(a.len(), b.len());
    let pos_b = |p: &(u8, u8)| b.iter().position(|q| q == p).expect("same inversion set");
    let rank: Vec<usize> = a.iter().map(pos_b).collect();
    let mut parity = false;
    for i in 0..a.len() {
        for j in i + 1..a.len() {
            let disjoint =
                a[i].0 != a[j].0 && a[i].0 != a[j].1 && a[i].1 != a[j].0 && a[i].1 != a[j].1;
            if disjoint && rank[i] > rank[j] {
                parity = !parity;
            }
        }
    }
    parity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_words() {
        for n in 1..6 {
            let all = Perm::all(n);
            assert_eq!(all.len(), (1..=n).product::<usize>());
            for w in all {
                let word = w.reduced_word();
                assert_eq!(word.len(), w.length());
                let mut p = Perm::identity(n);
                for &k in word.iter().rev() {
                    p = p.left_mul_simple(k);
                }
                assert_eq!(p, w);
            }
        }
    }

    #[test]
    fn odd_signs() {
        // distant letters anticommute
        assert!(odd_word_sign(4, &[0, 2], &[2, 0]));
        // braid moves carry no sign
        assert!(!odd_word_sign(3, &[0, 1, 0], &[1, 0, 1]));
        // a commutation and a braid
        assert!(odd_word_sign(4, &[2, 0, 1, 0], &[0, 2, 1, 0]));
    }
}
