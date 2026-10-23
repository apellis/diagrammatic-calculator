//! Cartan data: a generalized Cartan matrix with a symmetrizer.

use crate::error::{Error, Result};

/// A symmetrizable generalized Cartan matrix `a` with symmetrizer `d`, so that
/// `(α_i, α_j) = d_i a_ij` is symmetric. Colour names label the simple roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cartan {
    pub name: String,
    pub a: Vec<Vec<i64>>,
    pub d: Vec<i64>,
}

impl Cartan {
    pub fn rank(&self) -> usize {
        self.a.len()
    }

    /// The symmetric bilinear form `(α_i, α_j) = d_i a_ij`.
    pub fn form(&self, i: usize, j: usize) -> i64 {
        self.d[i] * self.a[i][j]
    }

    /// Checks the axioms: `a_ii = 2`, `a_ij ≤ 0` for `i ≠ j`, `a_ij = 0 ⇔ a_ji = 0`, `d_i > 0`, and
    /// `d_i a_ij = d_j a_ji`.
    pub fn validate(&self) -> Result<()> {
        let n = self.rank();
        if self.d.len() != n || self.a.iter().any(|r| r.len() != n) {
            return Err(Error::invalid(
                "Cartan matrix and symmetrizer sizes disagree",
            ));
        }
        for i in 0..n {
            if self.a[i][i] != 2 {
                return Err(Error::invalid(format!("a_{i}{i} must be 2")));
            }
            if self.d[i] <= 0 {
                return Err(Error::invalid("symmetrizer entries must be positive"));
            }
            for j in 0..n {
                if i != j {
                    if self.a[i][j] > 0 {
                        return Err(Error::invalid("off-diagonal Cartan entries must be ≤ 0"));
                    }
                    if (self.a[i][j] == 0) != (self.a[j][i] == 0) {
                        return Err(Error::invalid("a_ij = 0 must imply a_ji = 0"));
                    }
                    if self.form(i, j) != self.form(j, i) {
                        return Err(Error::invalid("d_i a_ij must equal d_j a_ji"));
                    }
                }
            }
        }
        Ok(())
    }

    /// A finite or affine type by name: `A1`…`A9`, `B2`…`B9`, `C2`…, `D4`…, `G2`, `F4`, or a
    /// product of these written with `x` (`A1xA1`, block diagonal, colours in order).
    ///
    /// Bourbaki numbering. In type `B_n` the last simple root is short; in `C_n` the last is long;
    /// in `G2` the first is short (`a_12 = -3`, `a_21 = -1`). Here `a_ij = 2(α_i, α_j)/(α_i, α_i)`,
    /// and the symmetrizer is normalized so that short roots have `(α, α) = 2`.
    pub fn of_type(name: &str) -> Result<Cartan> {
        check_type_size(name)?;
        let bad = || {
            Error::invalid(format!(
                "unknown Cartan type {name}; the types are An (n ≥ 1), Bn and Cn (n ≥ 2), Dn (n ≥ 4), \
                 G2, F4, and products such as A1xA1"
            ))
        };
        if name.contains('x') {
            let parts = name
                .split('x')
                .map(Cartan::of_type)
                .collect::<Result<Vec<_>>>()?;
            let n: usize = parts.iter().map(Cartan::rank).sum();
            let (mut a, mut d) = (vec![vec![0i64; n]; n], Vec::with_capacity(n));
            let mut off = 0;
            for p in &parts {
                for (i, row) in p.a.iter().enumerate() {
                    a[off + i][off..off + row.len()].copy_from_slice(row);
                }
                d.extend_from_slice(&p.d);
                off += p.rank();
            }
            let c = Cartan {
                name: name.to_string(),
                a,
                d,
            };
            c.validate()?;
            return Ok(c);
        }
        let (letter, n) = name.split_at(1);
        let n: usize = n.parse().map_err(|_| bad())?;
        let mut a = vec![vec![0i64; n]; n];
        for (i, row) in a.iter_mut().enumerate() {
            row[i] = 2;
        }
        let chain = |a: &mut Vec<Vec<i64>>| {
            for i in 0..n.saturating_sub(1) {
                a[i][i + 1] = -1;
                a[i + 1][i] = -1;
            }
        };
        let mut d = vec![1i64; n];
        match letter {
            "A" if n >= 1 => chain(&mut a),
            "B" if n >= 2 => {
                chain(&mut a);
                a[n - 1][n - 2] = -2; // α_{n-1} long, α_n short
                d = vec![2; n];
                d[n - 1] = 1;
            }
            "C" if n >= 2 => {
                chain(&mut a);
                a[n - 2][n - 1] = -2; // α_n long
                d[n - 1] = 2;
            }
            "D" if n >= 4 => {
                chain(&mut a);
                a[n - 2][n - 1] = 0;
                a[n - 1][n - 2] = 0;
                a[n - 3][n - 1] = -1;
                a[n - 1][n - 3] = -1;
            }
            "G" if n == 2 => {
                a[0][1] = -3;
                a[1][0] = -1;
                d = vec![1, 3];
            }
            "F" if n == 4 => {
                chain(&mut a);
                a[2][1] = -2; // α_1, α_2 long; α_3, α_4 short
                d = vec![2, 2, 1, 1];
            }
            _ => return Err(bad()),
        }
        let c = Cartan {
            name: name.to_string(),
            a,
            d,
        };
        c.validate()?;
        Ok(c)
    }
}

/// The largest rank of a Cartan type (in all, for a product such as `A2xB3`).
pub const MAX_RANK: usize = 32;

/// Refuses a Cartan type whose rank, read from its name, exceeds [`MAX_RANK`], before anything
/// of that size is built. Names that are not of the form letter-and-number pass, for `of_type`
/// to reject.
pub fn check_type_size(name: &str) -> Result<()> {
    let mut total: usize = 0;
    for part in name.split('x') {
        let digits = part.get(1..).unwrap_or("");
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            total = total.saturating_add(digits.parse().unwrap_or(usize::MAX));
        }
    }
    if total > MAX_RANK {
        return Err(Error::limit(format!(
            "the Cartan type {name} has rank above {MAX_RANK}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types() {
        for t in ["A1", "A3", "B2", "B3", "C3", "D4", "G2", "F4"] {
            Cartan::of_type(t).unwrap();
        }
        let g2 = Cartan::of_type("G2").unwrap();
        assert_eq!((g2.form(0, 0), g2.form(1, 1), g2.form(0, 1)), (2, 6, -3));
        let b2 = Cartan::of_type("B2").unwrap();
        assert_eq!((b2.form(0, 0), b2.form(1, 1), b2.form(0, 1)), (4, 2, -2));
        assert!(Cartan::of_type("E5").is_err());
    }
}
