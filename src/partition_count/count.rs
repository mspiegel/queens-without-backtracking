//! Count value that stays on the stack while small and promotes to `BigUint`
//! on overflow. Keeps the hashmap entries compact for the bulk of states whose
//! multiplicity fits in `u128`.

use num_bigint::BigUint;
use num_traits::Zero;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Count {
    Small(u128),
    Big(BigUint),
}

impl Count {
    pub const ZERO: Count = Count::Small(0);
    pub const ONE: Count = Count::Small(1);

    pub fn is_zero(&self) -> bool {
        match self {
            Count::Small(x) => *x == 0,
            Count::Big(b) => b.is_zero(),
        }
    }

    pub fn into_big(self) -> BigUint {
        match self {
            Count::Small(x) => BigUint::from(x),
            Count::Big(b) => b,
        }
    }
}

impl From<u64> for Count {
    fn from(x: u64) -> Self {
        Count::Small(x as u128)
    }
}

impl From<u128> for Count {
    fn from(x: u128) -> Self {
        Count::Small(x)
    }
}

pub fn add_into(dest: &mut Count, src: &Count) {
    match (&mut *dest, src) {
        (Count::Small(a), Count::Small(b)) => match a.checked_add(*b) {
            Some(sum) => *a = sum,
            None => {
                let promoted = BigUint::from(*a) + BigUint::from(*b);
                *dest = Count::Big(promoted);
            }
        },
        (Count::Small(a), Count::Big(b)) => {
            let promoted = BigUint::from(*a) + b;
            *dest = Count::Big(promoted);
        }
        (Count::Big(a), Count::Small(b)) => {
            *a += BigUint::from(*b);
        }
        (Count::Big(a), Count::Big(b)) => {
            *a += b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_plus_small_stays_small() {
        let mut a = Count::Small(10);
        add_into(&mut a, &Count::Small(25));
        assert_eq!(a, Count::Small(35));
    }

    #[test]
    fn small_plus_small_promotes_on_overflow() {
        let mut a = Count::Small(u128::MAX - 5);
        add_into(&mut a, &Count::Small(100));
        match a {
            Count::Big(b) => {
                let expected = BigUint::from(u128::MAX - 5) + BigUint::from(100u32);
                assert_eq!(b, expected);
            }
            _ => panic!("expected promotion"),
        }
    }

    #[test]
    fn small_plus_big_promotes() {
        let mut a = Count::Small(7);
        let huge: BigUint = BigUint::from(1u32) << 200;
        add_into(&mut a, &Count::Big(huge.clone()));
        assert_eq!(a, Count::Big(BigUint::from(7u32) + huge));
    }

    #[test]
    fn big_plus_small_stays_big() {
        let mut a = Count::Big(BigUint::from(1u32) << 200);
        add_into(&mut a, &Count::Small(42));
        match a {
            Count::Big(b) => assert_eq!(b, (BigUint::from(1u32) << 200) + BigUint::from(42u32)),
            _ => panic!("expected Big"),
        }
    }

    #[test]
    fn big_plus_big() {
        let mut a = Count::Big(BigUint::from(1u32) << 200);
        add_into(&mut a, &Count::Big(BigUint::from(1u32) << 200));
        match a {
            Count::Big(b) => assert_eq!(b, BigUint::from(1u32) << 201),
            _ => panic!("expected Big"),
        }
    }
}
