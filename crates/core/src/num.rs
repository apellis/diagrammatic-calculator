//! Integers and rationals that stay in machine words while they fit.
//!
//! Coefficients in the engine are almost always small, and `BigInt` allocates for every nonzero
//! value. `Int` keeps an `i64` and moves to a `BigInt` only on overflow; `Rat` is a pair of them.
//! Both are kept normalized (a `BigInt` only when the value does not fit in `i64`; a rational in
//! lowest terms with positive denominator), so equal values have equal representations and the
//! derived `Eq` and `Hash` are correct.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
use std::fmt;

/// An integer of any size.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Int {
    Small(i64),
    Big(BigInt),
}

impl Int {
    pub const ZERO: Int = Int::Small(0);
    pub const ONE: Int = Int::Small(1);

    fn norm(b: BigInt) -> Int {
        match b.to_i64() {
            Some(s) => Int::Small(s),
            None => Int::Big(b),
        }
    }

    pub fn to_bigint(&self) -> BigInt {
        match self {
            Int::Small(s) => BigInt::from(*s),
            Int::Big(b) => b.clone(),
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        match self {
            Int::Small(s) => Some(*s),
            Int::Big(_) => None,
        }
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Int::Small(0))
    }

    pub fn is_one(&self) -> bool {
        matches!(self, Int::Small(1))
    }

    pub fn is_negative(&self) -> bool {
        match self {
            Int::Small(s) => *s < 0,
            Int::Big(b) => b.is_negative(),
        }
    }

    pub fn signum(&self) -> i32 {
        match self {
            Int::Small(s) => s.signum() as i32,
            Int::Big(b) => {
                if b.is_negative() {
                    -1
                } else {
                    1
                }
            }
        }
    }

    pub fn abs(&self) -> Int {
        match self {
            Int::Small(s) => s
                .checked_abs()
                .map_or_else(|| Int::Big(BigInt::from(*s).abs()), Int::Small),
            Int::Big(b) => Int::Big(b.abs()),
        }
    }

    pub fn add(&self, o: &Int) -> Int {
        if let (Int::Small(a), Int::Small(b)) = (self, o)
            && let Some(c) = a.checked_add(*b)
        {
            return Int::Small(c);
        }
        Int::norm(self.to_bigint() + o.to_bigint())
    }

    pub fn sub(&self, o: &Int) -> Int {
        if let (Int::Small(a), Int::Small(b)) = (self, o)
            && let Some(c) = a.checked_sub(*b)
        {
            return Int::Small(c);
        }
        Int::norm(self.to_bigint() - o.to_bigint())
    }

    pub fn mul(&self, o: &Int) -> Int {
        if let (Int::Small(a), Int::Small(b)) = (self, o)
            && let Some(c) = a.checked_mul(*b)
        {
            return Int::Small(c);
        }
        Int::norm(self.to_bigint() * o.to_bigint())
    }

    pub fn neg(&self) -> Int {
        match self {
            Int::Small(s) => s
                .checked_neg()
                .map_or_else(|| Int::Big(-BigInt::from(*s)), Int::Small),
            Int::Big(b) => Int::norm(-b),
        }
    }

    /// Truncated division with remainder (the remainder has the sign of `self`); `o ≠ 0`.
    pub fn div_rem(&self, o: &Int) -> (Int, Int) {
        if let (Int::Small(a), Int::Small(b)) = (self, o)
            && let (Some(q), Some(r)) = (a.checked_div(*b), a.checked_rem(*b))
        {
            return (Int::Small(q), Int::Small(r));
        }
        let (q, r) = self.to_bigint().div_rem(&o.to_bigint());
        (Int::norm(q), Int::norm(r))
    }

    /// Floor division and the nonnegative remainder modulo `|o|`; `o ≠ 0`.
    pub fn div_mod_floor(&self, o: &Int) -> (Int, Int) {
        if let (Int::Small(a), Int::Small(b)) = (self, o)
            && let (Some(q), Some(r)) = (a.checked_div_euclid(*b), a.checked_rem_euclid(*b))
        {
            return (Int::Small(q), Int::Small(r));
        }
        let (q, r) = self.to_bigint().div_mod_floor(&o.to_bigint());
        (Int::norm(q), Int::norm(r))
    }

    pub fn gcd(&self, o: &Int) -> Int {
        if let (Int::Small(a), Int::Small(b)) = (self, o) {
            let (mut x, mut y) = (a.unsigned_abs(), b.unsigned_abs());
            while y != 0 {
                (x, y) = (y, x % y);
            }
            return i64::try_from(x).map_or_else(|_| Int::Big(BigInt::from(x)), Int::Small);
        }
        Int::norm(self.to_bigint().gcd(&o.to_bigint()))
    }

    /// The residue in `0..p`.
    pub fn rem_u64(&self, p: u64) -> u64 {
        match self {
            Int::Small(s) => (*s as i128).rem_euclid(p as i128) as u64,
            Int::Big(b) => b.mod_floor(&BigInt::from(p)).to_u64().unwrap(),
        }
    }
}

impl From<i64> for Int {
    fn from(s: i64) -> Int {
        Int::Small(s)
    }
}

impl From<BigInt> for Int {
    fn from(b: BigInt) -> Int {
        Int::norm(b)
    }
}

impl From<&BigInt> for Int {
    fn from(b: &BigInt) -> Int {
        match b.to_i64() {
            Some(s) => Int::Small(s),
            None => Int::Big(b.clone()),
        }
    }
}

impl PartialOrd for Int {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Int {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Int::Small(a), Int::Small(b)) => a.cmp(b),
            _ => self.to_bigint().cmp(&other.to_bigint()),
        }
    }
}

impl fmt::Display for Int {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Int::Small(s) => write!(f, "{s}"),
            Int::Big(b) => write!(f, "{b}"),
        }
    }
}

impl fmt::Debug for Int {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A rational number in lowest terms with positive denominator.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Rat {
    num: Int,
    den: Int,
}

impl Rat {
    pub const ZERO: Rat = Rat {
        num: Int::ZERO,
        den: Int::ONE,
    };
    pub const ONE: Rat = Rat {
        num: Int::ONE,
        den: Int::ONE,
    };

    /// `n / d` in lowest terms; `d ≠ 0`.
    pub fn new(n: Int, d: Int) -> Rat {
        assert!(!d.is_zero(), "zero denominator");
        if n.is_zero() {
            return Rat::ZERO;
        }
        let g = n.gcd(&d);
        let (mut n, mut d) = if g.is_one() {
            (n, d)
        } else {
            (n.div_rem(&g).0, d.div_rem(&g).0)
        };
        if d.is_negative() {
            n = n.neg();
            d = d.neg();
        }
        Rat { num: n, den: d }
    }

    pub fn from_int(n: Int) -> Rat {
        Rat {
            num: n,
            den: Int::ONE,
        }
    }

    pub fn numer(&self) -> &Int {
        &self.num
    }

    pub fn denom(&self) -> &Int {
        &self.den
    }

    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    pub fn is_integer(&self) -> bool {
        self.den.is_one()
    }

    pub fn is_negative(&self) -> bool {
        self.num.is_negative()
    }

    pub fn add(&self, o: &Rat) -> Rat {
        if self.den.is_one() && o.den.is_one() {
            return Rat::from_int(self.num.add(&o.num));
        }
        if self.den == o.den {
            return Rat::new(self.num.add(&o.num), self.den.clone());
        }
        Rat::new(
            self.num.mul(&o.den).add(&o.num.mul(&self.den)),
            self.den.mul(&o.den),
        )
    }

    pub fn neg(&self) -> Rat {
        Rat {
            num: self.num.neg(),
            den: self.den.clone(),
        }
    }

    pub fn sub(&self, o: &Rat) -> Rat {
        self.add(&o.neg())
    }

    pub fn mul(&self, o: &Rat) -> Rat {
        if self.den.is_one() && o.den.is_one() {
            return Rat::from_int(self.num.mul(&o.num));
        }
        Rat::new(self.num.mul(&o.num), self.den.mul(&o.den))
    }

    pub fn recip(&self) -> Option<Rat> {
        (!self.is_zero()).then(|| Rat::new(self.den.clone(), self.num.clone()))
    }
}

impl fmt::Display for Rat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.den.is_one() {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

impl fmt::Debug for Rat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl Zero for Int {
    fn zero() -> Int {
        Int::ZERO
    }
    fn is_zero(&self) -> bool {
        Int::is_zero(self)
    }
}

impl std::ops::Add for Int {
    type Output = Int;
    fn add(self, o: Int) -> Int {
        Int::add(&self, &o)
    }
}

impl One for Int {
    fn one() -> Int {
        Int::ONE
    }
}

impl std::ops::Mul for Int {
    type Output = Int;
    fn mul(self, o: Int) -> Int {
        Int::mul(&self, &o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_moves_to_big_and_back() {
        let m = Int::from(i64::MAX);
        let s = m.add(&Int::ONE);
        assert!(matches!(s, Int::Big(_)));
        assert_eq!(s.sub(&Int::ONE), m);
        assert!(matches!(s.sub(&Int::ONE), Int::Small(_)));
        let p = m.mul(&m);
        assert_eq!(p.div_rem(&m), (m.clone(), Int::ZERO));
        assert_eq!(
            Int::from(i64::MIN).neg().to_bigint(),
            -BigInt::from(i64::MIN)
        );
        assert_eq!(Int::from(i64::MIN).abs().to_string(), "9223372036854775808");
    }

    #[test]
    fn rationals_are_normalized() {
        let a = Rat::new(Int::from(6), Int::from(-4));
        assert_eq!(a.to_string(), "-3/2");
        let b = Rat::new(Int::from(1), Int::from(2));
        assert_eq!(a.add(&b).to_string(), "-1");
        assert!(a.add(&b).is_integer());
        assert_eq!(a.mul(&b).to_string(), "-3/4");
        assert_eq!(a.recip().unwrap().to_string(), "-2/3");
        assert_eq!(Rat::new(Int::ZERO, Int::from(-5)), Rat::ZERO);
    }

    #[test]
    fn floor_division() {
        let (q, r) = Int::from(-7).div_mod_floor(&Int::from(3));
        assert_eq!((q, r), (Int::from(-3), Int::from(2)));
        assert_eq!(Int::from(-7).rem_u64(5), 3);
    }
}
