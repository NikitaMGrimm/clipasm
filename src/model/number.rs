use std::fmt;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

/// One exact, reduced rational number.
///
/// Authored decimals and arithmetic never pass through binary floating point.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Number(BigRational);

impl Number {
    pub(crate) fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
        let invalid = || {
            Diagnostic::new(
                "E_INVALID_NUMBER",
                format!("`{text}` is not a decimal number"),
                span.clone(),
            )
        };
        if text.is_empty() {
            return Err(invalid());
        }
        let (digits, scale) = if let Some((whole, fraction)) = text.split_once('.') {
            if whole.is_empty()
                || fraction.is_empty()
                || fraction.contains('.')
                || !whole.bytes().all(|byte| byte.is_ascii_digit())
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(invalid());
            }
            let mut digits = String::with_capacity(whole.len() + fraction.len());
            digits.push_str(whole);
            digits.push_str(fraction);
            (digits, fraction.len())
        } else {
            if !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            (text.to_owned(), 0)
        };
        let numerator = digits.parse::<BigInt>().map_err(|_| invalid())?;
        let exponent = u32::try_from(scale).map_err(|_| {
            Diagnostic::new(
                "E_NUMBER_TOO_LARGE",
                "number literal has too many decimal places",
                span.clone(),
            )
        })?;
        let denominator = BigInt::from(10_u8).pow(exponent);
        Ok(Self(BigRational::new(numerator, denominator)))
    }

    pub(crate) fn from_integer(value: i64) -> Self {
        Self(BigRational::from_integer(BigInt::from(value)))
    }

    pub(crate) fn from_unsigned_integer(value: u64) -> Self {
        Self(BigRational::from_integer(BigInt::from(value)))
    }

    pub(crate) fn from_ratio(numerator: i64, denominator: i64) -> Self {
        debug_assert_ne!(denominator, 0);
        Self(BigRational::new(
            BigInt::from(numerator),
            BigInt::from(denominator),
        ))
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub(crate) fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    pub(crate) fn is_integer(&self) -> bool {
        self.0.is_integer()
    }

    pub(crate) fn to_i64(&self) -> Option<i64> {
        self.is_integer()
            .then(|| self.0.to_integer().to_i64())
            .flatten()
    }

    pub(crate) fn to_u64(&self) -> Option<u64> {
        self.is_integer()
            .then(|| self.0.to_integer().to_u64())
            .flatten()
    }

    pub(crate) fn numerator(&self) -> &BigInt {
        self.0.numer()
    }

    pub(crate) fn denominator(&self) -> &BigInt {
        self.0.denom()
    }

    pub(crate) fn negated(&self) -> Self {
        Self(-&self.0)
    }

    pub(crate) fn add(&self, other: &Self) -> Self {
        Self(&self.0 + &other.0)
    }

    pub(crate) fn subtract(&self, other: &Self) -> Self {
        Self(&self.0 - &other.0)
    }

    pub(crate) fn multiply(&self, other: &Self) -> Self {
        Self(&self.0 * &other.0)
    }

    pub(crate) fn divide(&self, other: &Self) -> Option<Self> {
        (!other.is_zero()).then(|| Self(&self.0 / &other.0))
    }

    /// Return the unique reduced integer or `numerator/denominator` spelling.
    #[must_use]
    pub fn canonical(&self) -> String {
        if self.is_integer() {
            self.0.to_integer().to_string()
        } else {
            format!("{}/{}", self.numerator(), self.denominator())
        }
    }

    /// Return a finite decimal when exact, otherwise the reduced fraction.
    #[must_use]
    pub fn authored_display(&self) -> String {
        self.finite_decimal().unwrap_or_else(|| self.canonical())
    }

    fn finite_decimal(&self) -> Option<String> {
        let mut denominator = self.denominator().clone();
        let two = BigInt::from(2_u8);
        let five = BigInt::from(5_u8);
        let mut twos = 0_u32;
        let mut fives = 0_u32;
        while (&denominator % &two).is_zero() {
            denominator /= &two;
            twos = twos.checked_add(1)?;
        }
        while (&denominator % &five).is_zero() {
            denominator /= &five;
            fives = fives.checked_add(1)?;
        }
        if denominator != BigInt::one() {
            return None;
        }
        let scale = twos.max(fives);
        let multiplier =
            BigInt::from(2_u8).pow(scale - twos) * BigInt::from(5_u8).pow(scale - fives);
        let scaled = self.numerator() * multiplier;
        let negative = scaled.is_negative();
        let mut digits = scaled.abs().to_string();
        if scale == 0 {
            return Some(if negative {
                format!("-{digits}")
            } else {
                digits
            });
        }
        let scale = usize::try_from(scale).ok()?;
        if digits.len() <= scale {
            digits.insert_str(0, &"0".repeat(scale + 1 - digits.len()));
        }
        let split = digits.len() - scale;
        digits.insert(split, '.');
        if negative {
            digits.insert(0, '-');
        }
        Some(digits)
    }
}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.authored_display())
    }
}

impl Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.canonical())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> SourceSpan {
        SourceSpan::file_start("test.clipasm")
    }

    #[test]
    fn parses_and_reduces_integers_and_decimals_exactly() {
        assert_eq!(Number::parse("12", &span()).unwrap().canonical(), "12");
        assert_eq!(
            Number::parse("1.28712", &span()).unwrap().canonical(),
            "16089/12500"
        );
        assert_eq!(
            Number::parse("0.0800", &span()).unwrap().canonical(),
            "2/25"
        );
    }

    #[test]
    fn displays_finite_values_as_decimals_and_others_as_fractions() {
        assert_eq!(Number::from_ratio(5, 2).authored_display(), "2.5");
        assert_eq!(Number::from_ratio(1, 3).authored_display(), "1/3");
        assert_eq!(Number::from_ratio(-1, 40).authored_display(), "-0.025");
    }
}
