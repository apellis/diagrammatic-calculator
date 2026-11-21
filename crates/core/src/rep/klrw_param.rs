//! Splitting a scalar `h` off the coefficients of weighted KLR elements, for complexes over
//! `k[h]/(h^{N+1})` (`crate::deform`, `docs/complexes.md`).

use super::klrw::{Elem, Klrw, Summand};
use crate::deform::ParamSplit;
use crate::error::{Error, Result};
use crate::poly::{Monomial, Poly};
use crate::ring::Ring;

type C<R> = Poly<<R as Ring>::Elem>;

impl<R: Ring> Klrw<R> {
    fn param_var(&self, h: &str) -> Result<usize> {
        self.coeff()
            .names
            .iter()
            .position(|n| n == h)
            .ok_or_else(|| Error::invalid(format!("'{h}' is not a scalar of the context")))
    }
}

impl<R: Ring> ParamSplit for Klrw<R> {
    fn param_split(&self, h: &str, f: &Elem<C<R>>) -> Result<Vec<Elem<C<R>>>> {
        let v = self.param_var(h)?;
        let ring = self.coeff();
        let mut parts: Vec<Vec<Summand<C<R>>>> = Vec::new();
        for s in self.normal_form(f)? {
            // the coefficient, sorted by the power of h
            let mut by_power: Vec<Vec<(Monomial, R::Elem)>> = Vec::new();
            for (m, c) in &s.coeff.terms {
                let e = m.0[v];
                if e < 0 {
                    return Err(Error::invalid(format!(
                        "a coefficient has a negative power of {h}"
                    )));
                }
                let e = e as usize;
                if by_power.len() <= e {
                    by_power.resize_with(e + 1, Vec::new);
                }
                let mut m = m.clone();
                m.0[v] = 0;
                by_power[e].push((m, c.clone()));
            }
            for (e, terms) in by_power.into_iter().enumerate() {
                if terms.is_empty() {
                    continue;
                }
                if parts.len() <= e {
                    parts.resize_with(e + 1, Vec::new);
                }
                parts[e].push(Summand {
                    coeff: ring.normalize(terms),
                    ..s.clone()
                });
            }
        }
        parts
            .into_iter()
            .map(|ss| {
                let mut out = Elem::default();
                for s in &ss {
                    out = self.add(&out, &self.summand_elem(s)?);
                }
                Ok(out)
            })
            .collect()
    }

    fn param_mul(&self, h: &str, j: u32, f: &Elem<C<R>>) -> Result<Elem<C<R>>> {
        let v = self.param_var(h)?;
        let ring = self.coeff();
        let hj = ring.monomial(Monomial::var(ring.nvars(), v, j as i32), ring.base.one());
        Ok(self.scale(&hj, f))
    }
}
