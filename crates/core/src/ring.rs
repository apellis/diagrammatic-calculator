//! Coefficient rings.
//!
//! Arithmetic goes through a ring *context* (`ring.add(&a, &b)`) rather than operator overloading
//! on elements, so that rings needing run-time data (the prime of ℤ/p, the variables of a
//! polynomial ring) share one interface. See `docs/design.md`.

use crate::num::{Int, Rat};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::fmt::Debug;
use std::hash::Hash;

/// A commutative ring with identity, given as a context acting on element values.
pub trait Ring: Clone + Debug + PartialEq {
    type Elem: Clone + Eq + Hash + Debug;

    fn zero(&self) -> Self::Elem;
    fn one(&self) -> Self::Elem;
    fn int(&self, n: i64) -> Self::Elem;
    fn is_zero(&self, a: &Self::Elem) -> bool;
    fn add(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem;
    fn neg(&self, a: &Self::Elem) -> Self::Elem;
    fn mul(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem;

    /// `b⁻¹` if `b` is a unit.
    fn inv(&self, b: &Self::Elem) -> Option<Self::Elem>;

    /// `a / b` when `b` divides `a` exactly (for domains, the quotient is unique).
    fn div_exact(&self, a: &Self::Elem, b: &Self::Elem) -> Option<Self::Elem>;

    /// Canonical text form, in the syntax of the text term format.
    fn fmt_elem(&self, a: &Self::Elem) -> String;

    /// True if the canonical text form needs parentheses when multiplied by something else
    /// (a sum such as `1+I`, or a negative number in a non-leading position is handled by the
    /// printer separately).
    fn needs_parens(&self, a: &Self::Elem) -> bool {
        let _ = a;
        false
    }

    /// The characteristic: 0 or a prime.
    fn characteristic(&self) -> u64;

    /// A square root of −1, written `I`, if the ring has a designated one.
    fn sqrt_minus_one(&self) -> Option<Self::Elem> {
        None
    }

    /// The image of a rational number `n/d`, if `d` is invertible.
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<Self::Elem>;

    /// A named element of the ring, such as the variable `q` of ℚ(q).
    fn named(&self, name: &str) -> Option<Self::Elem> {
        let _ = name;
        None
    }

    /// True for fields.
    fn is_field(&self) -> bool {
        false
    }

    /// True for integral domains (every ring offered here is one, but parameter rings with
    /// relations need not be).
    fn is_domain(&self) -> bool {
        true
    }

    fn is_one(&self, a: &Self::Elem) -> bool {
        *a == self.one()
    }
    fn sub(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem {
        self.add(a, &self.neg(b))
    }
    fn add_assign(&self, a: &mut Self::Elem, b: &Self::Elem) {
        *a = self.add(a, b);
    }
    fn pow(&self, a: &Self::Elem, mut e: u32) -> Self::Elem {
        let mut base = a.clone();
        let mut acc = self.one();
        while e > 0 {
            if e & 1 == 1 {
                acc = self.mul(&acc, &base);
            }
            e >>= 1;
            if e > 0 {
                base = self.mul(&base, &base);
            }
        }
        acc
    }
    /// True if the element is `-c` for a "positive-looking" `c`, so the printer can write `- c`.
    fn is_negative(&self, a: &Self::Elem) -> bool {
        let _ = a;
        false
    }
}

/// The integers ℤ.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Integers;

impl Ring for Integers {
    type Elem = Int;
    fn zero(&self) -> Int {
        Int::ZERO
    }
    fn one(&self) -> Int {
        Int::ONE
    }
    fn int(&self, n: i64) -> Int {
        Int::from(n)
    }
    fn is_zero(&self, a: &Int) -> bool {
        a.is_zero()
    }
    fn add(&self, a: &Int, b: &Int) -> Int {
        a.add(b)
    }
    fn neg(&self, a: &Int) -> Int {
        a.neg()
    }
    fn mul(&self, a: &Int, b: &Int) -> Int {
        a.mul(b)
    }
    fn inv(&self, b: &Int) -> Option<Int> {
        b.abs().is_one().then(|| b.clone())
    }
    fn div_exact(&self, a: &Int, b: &Int) -> Option<Int> {
        if b.is_zero() {
            return None;
        }
        let (q, r) = a.div_rem(b);
        r.is_zero().then_some(q)
    }
    fn fmt_elem(&self, a: &Int) -> String {
        a.to_string()
    }
    fn characteristic(&self) -> u64 {
        0
    }
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<Int> {
        self.div_exact(&Int::from(n), &Int::from(d))
    }
    fn is_negative(&self, a: &Int) -> bool {
        a.is_negative()
    }
}

/// The rationals ℚ.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Rationals;

impl Ring for Rationals {
    type Elem = Rat;
    fn zero(&self) -> Rat {
        Rat::ZERO
    }
    fn one(&self) -> Rat {
        Rat::ONE
    }
    fn int(&self, n: i64) -> Rat {
        Rat::from_int(Int::from(n))
    }
    fn is_zero(&self, a: &Rat) -> bool {
        a.is_zero()
    }
    fn add(&self, a: &Rat, b: &Rat) -> Rat {
        a.add(b)
    }
    fn neg(&self, a: &Rat) -> Rat {
        a.neg()
    }
    fn mul(&self, a: &Rat, b: &Rat) -> Rat {
        a.mul(b)
    }
    fn inv(&self, b: &Rat) -> Option<Rat> {
        b.recip()
    }
    fn div_exact(&self, a: &Rat, b: &Rat) -> Option<Rat> {
        b.recip().map(|r| a.mul(&r))
    }
    fn fmt_elem(&self, a: &Rat) -> String {
        a.to_string()
    }
    fn characteristic(&self) -> u64 {
        0
    }
    fn is_field(&self) -> bool {
        true
    }
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<Rat> {
        (!d.is_zero()).then(|| Rat::new(Int::from(n), Int::from(d)))
    }
    fn is_negative(&self, a: &Rat) -> bool {
        a.is_negative()
    }
}

/// The prime field ℤ/p for a prime `p < 2⁶³`. Elements are representatives in `0..p`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimeField {
    p: u64,
}

impl PrimeField {
    /// The field ℤ/p; `None` unless `p` is a prime below 2⁶³.
    pub fn new(p: u64) -> Option<Self> {
        (p < (1u64 << 63) && is_prime_u64(p)).then_some(PrimeField { p })
    }
    pub fn modulus(&self) -> u64 {
        self.p
    }
    fn reduce_i128(&self, n: i128) -> u64 {
        n.rem_euclid(self.p as i128) as u64
    }
    /// Reduces an integer modulo p.
    pub fn from_bigint(&self, n: &BigInt) -> u64 {
        let r = n.mod_floor(&BigInt::from(self.p));
        r.to_u64().expect("residue fits in u64")
    }
}

impl Ring for PrimeField {
    type Elem = u64;
    fn zero(&self) -> u64 {
        0
    }
    fn one(&self) -> u64 {
        1 % self.p
    }
    fn int(&self, n: i64) -> u64 {
        self.reduce_i128(n as i128)
    }
    fn is_zero(&self, a: &u64) -> bool {
        *a == 0
    }
    fn add(&self, a: &u64, b: &u64) -> u64 {
        ((*a as u128 + *b as u128) % self.p as u128) as u64
    }
    fn neg(&self, a: &u64) -> u64 {
        if *a == 0 { 0 } else { self.p - a }
    }
    fn mul(&self, a: &u64, b: &u64) -> u64 {
        ((*a as u128 * *b as u128) % self.p as u128) as u64
    }
    fn inv(&self, b: &u64) -> Option<u64> {
        if *b == 0 {
            return None;
        }
        // Fermat: b^(p-2).
        let mut e = self.p - 2;
        let mut base = *b;
        let mut acc = 1u64;
        while e > 0 {
            if e & 1 == 1 {
                acc = self.mul(&acc, &base);
            }
            base = self.mul(&base, &base);
            e >>= 1;
        }
        Some(acc)
    }
    fn div_exact(&self, a: &u64, b: &u64) -> Option<u64> {
        self.inv(b).map(|bi| self.mul(a, &bi))
    }
    fn fmt_elem(&self, a: &u64) -> String {
        a.to_string()
    }
    fn characteristic(&self) -> u64 {
        self.p
    }
    fn is_field(&self) -> bool {
        true
    }
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<u64> {
        self.div_exact(&self.from_bigint(n), &self.from_bigint(d))
    }
}

/// The Gaussian integers ℤ[√−1], written `a + b I` in the text format.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GaussianIntegers;

/// An element `re + im·√−1` of ℤ[√−1].
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Gaussian {
    pub re: BigInt,
    pub im: BigInt,
}

impl Gaussian {
    pub fn new(re: impl Into<BigInt>, im: impl Into<BigInt>) -> Self {
        Gaussian {
            re: re.into(),
            im: im.into(),
        }
    }
}

impl Ring for GaussianIntegers {
    type Elem = Gaussian;
    fn zero(&self) -> Gaussian {
        Gaussian::new(0, 0)
    }
    fn one(&self) -> Gaussian {
        Gaussian::new(1, 0)
    }
    fn int(&self, n: i64) -> Gaussian {
        Gaussian::new(n, 0)
    }
    fn is_zero(&self, a: &Gaussian) -> bool {
        a.re.is_zero() && a.im.is_zero()
    }
    fn add(&self, a: &Gaussian, b: &Gaussian) -> Gaussian {
        Gaussian {
            re: &a.re + &b.re,
            im: &a.im + &b.im,
        }
    }
    fn neg(&self, a: &Gaussian) -> Gaussian {
        Gaussian {
            re: -&a.re,
            im: -&a.im,
        }
    }
    fn mul(&self, a: &Gaussian, b: &Gaussian) -> Gaussian {
        Gaussian {
            re: &a.re * &b.re - &a.im * &b.im,
            im: &a.re * &b.im + &a.im * &b.re,
        }
    }
    fn inv(&self, b: &Gaussian) -> Option<Gaussian> {
        // Units: ±1, ±I.
        let n = &b.re * &b.re + &b.im * &b.im;
        n.is_one().then(|| Gaussian {
            re: b.re.clone(),
            im: -&b.im,
        })
    }
    fn div_exact(&self, a: &Gaussian, b: &Gaussian) -> Option<Gaussian> {
        let n = &b.re * &b.re + &b.im * &b.im;
        if n.is_zero() {
            return None;
        }
        // a · conj(b) / N(b)
        let re = &a.re * &b.re + &a.im * &b.im;
        let im = &a.im * &b.re - &a.re * &b.im;
        let (qr, rr) = re.div_rem(&n);
        let (qi, ri) = im.div_rem(&n);
        (rr.is_zero() && ri.is_zero()).then_some(Gaussian { re: qr, im: qi })
    }
    fn fmt_elem(&self, a: &Gaussian) -> String {
        let im = |b: &BigInt| -> String {
            if b.is_one() {
                "I".to_string()
            } else if (-b).is_one() {
                "-I".to_string()
            } else {
                format!("{b} I")
            }
        };
        match (a.re.is_zero(), a.im.is_zero()) {
            (_, true) => a.re.to_string(),
            (true, false) => im(&a.im),
            (false, false) => {
                if a.im.is_negative() {
                    format!("{} - {}", a.re, im(&-&a.im))
                } else {
                    format!("{} + {}", a.re, im(&a.im))
                }
            }
        }
    }
    fn needs_parens(&self, a: &Gaussian) -> bool {
        !a.re.is_zero() && !a.im.is_zero()
    }
    fn characteristic(&self) -> u64 {
        0
    }
    fn sqrt_minus_one(&self) -> Option<Gaussian> {
        Some(Gaussian::new(0, 1))
    }
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<Gaussian> {
        self.div_exact(&Gaussian::new(n.clone(), 0), &Gaussian::new(d.clone(), 0))
    }
    fn is_negative(&self, a: &Gaussian) -> bool {
        (a.im.is_zero() && a.re.is_negative()) || (a.re.is_zero() && a.im.is_negative())
    }
}

/// Deterministic Miller–Rabin for 64-bit integers.
pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for p in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    let mulmod = |a: u64, b: u64| ((a as u128 * b as u128) % n as u128) as u64;
    let powmod = |mut a: u64, mut e: u64| {
        let mut r = 1u64;
        while e > 0 {
            if e & 1 == 1 {
                r = mulmod(r, a);
            }
            a = mulmod(a, a);
            e >>= 1;
        }
        r
    };
    let mut d = n - 1;
    let mut s = 0;
    while d.is_multiple_of(2) {
        d /= 2;
        s += 1;
    }
    'witness: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = powmod(a, d);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulmod(x, x);
            if x == n - 1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primes() {
        let small: Vec<u64> = (0..60).filter(|&n| is_prime_u64(n)).collect();
        assert_eq!(
            small,
            vec![
                2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59
            ]
        );
        assert!(is_prime_u64(2_305_843_009_213_693_951)); // 2^61 - 1
        assert!(!is_prime_u64(3_215_031_751)); // strong pseudoprime to bases 2, 3, 5, 7
        assert!(PrimeField::new(4).is_none());
    }

    #[test]
    fn prime_field_inverse() {
        let f = PrimeField::new(101).unwrap();
        for a in 1..101 {
            let ai = f.inv(&a).unwrap();
            assert_eq!(f.mul(&a, &ai), 1);
        }
        assert_eq!(f.int(-1), 100);
    }

    #[test]
    fn gaussian() {
        let z = GaussianIntegers;
        let i = Gaussian::new(0, 1);
        assert_eq!(z.mul(&i, &i), z.int(-1));
        let a = Gaussian::new(3, 4);
        let b = Gaussian::new(1, 2);
        let ab = z.mul(&a, &b);
        assert_eq!(z.div_exact(&ab, &b), Some(a.clone()));
        assert_eq!(z.div_exact(&a, &Gaussian::new(2, 0)), None);
        assert_eq!(z.fmt_elem(&Gaussian::new(1, -1)), "1 - I");
        assert_eq!(z.fmt_elem(&Gaussian::new(0, -2)), "-2 I");
        assert_eq!(z.inv(&i), Some(Gaussian::new(0, -1)));
    }

    #[test]
    fn rationals_print() {
        let q = Rationals;
        let a = q.div_exact(&q.int(-6), &q.int(4)).unwrap();
        assert_eq!(q.fmt_elem(&a), "-3/2");
    }
}
