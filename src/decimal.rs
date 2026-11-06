//! Exact base-10 arithmetic for money and other fractional quantities that an
//! `f64` cannot hold.
//!
//! A `Decimal` is a signed integer mantissa and a decimal scale: the value is
//! `mantissa / 10^scale`. Addition, subtraction, multiplication and remainder
//! are exact while the result fits; a result that needs more than
//! [`MAX_SCALE`] fractional digits, or that leaves the `i128` mantissa, is a
//! runtime error rather than a silently rounded value.
//!
//! Division is the one operation that cannot always be exact: `1 / 3` has no
//! finite decimal expansion. It is answered to [`MAX_SCALE`] places, rounded
//! half away from zero, which is stated in SPEC.md and is what
//! `edge_decimal_division_that_does_not_terminate` pins.

use std::cmp::Ordering;
use std::fmt;

/// The most fractional digits a decimal can hold.
///
/// 18 is enough for every currency in use (a 64-bit cent count covers trillions)
/// while keeping `10^18` inside an `i128` with room for whole digits. A text or
/// product that needs more is refused, because rounding it without being asked
/// would be the same silent loss this type exists to prevent.
pub const MAX_SCALE: u32 = 18;

/// An exact base-10 number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    /// A decimal from an integer mantissa and a scale, normalized so that
    /// `Decimal::new(10, 1)` and `Decimal::new(1, 0)` are the same value.
    pub fn new(mantissa: i128, scale: u32) -> Result<Decimal, String> {
        let normalized = Self::normalize(mantissa, scale);
        if normalized.scale > MAX_SCALE {
            return Err(format!(
                "a decimal can have at most {MAX_SCALE} places after the point, but this one has {}",
                normalized.scale
            ));
        }
        Ok(normalized)
    }

    /// The integer zero.
    pub fn zero() -> Decimal {
        Decimal {
            mantissa: 0,
            scale: 0,
        }
    }

    /// Parses ordinary decimal text such as `12`, `-0.75` or `+3.140`.
    ///
    /// Leading and trailing whitespace is ignored. An exponent, a second point
    /// and a scale past [`MAX_SCALE`] are refused by name rather than guessed
    /// at.
    pub fn parse(text: &str) -> Result<Decimal, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("'' is not a decimal: it is empty".to_string());
        }
        if text.contains(['e', 'E']) {
            return Err(format!(
                "'{text}' is not a decimal: an exponent has no exact decimal scale here"
            ));
        }

        let (negative, unsigned) = match text.as_bytes()[0] {
            b'-' => (true, &text[1..]),
            b'+' => (false, &text[1..]),
            _ => (false, text),
        };
        if unsigned.is_empty() {
            return Err(format!("'{text}' is not a decimal"));
        }

        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or("");
        let fraction = parts.next();
        if parts.next().is_some() {
            return Err(format!(
                "'{text}' is not a decimal: it has more than one point"
            ));
        }
        if whole.is_empty() && fraction.is_none_or(str::is_empty) {
            return Err(format!("'{text}' is not a decimal"));
        }
        if !whole.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "'{text}' is not a decimal: only digits, a sign and one point are allowed"
            ));
        }
        let fraction = fraction.unwrap_or("").trim_end_matches('0');
        if !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "'{text}' is not a decimal: only digits, a sign and one point are allowed"
            ));
        }
        let scale = fraction.len() as u32;
        if scale > MAX_SCALE {
            return Err(format!(
                "'{text}' has {scale} places after the point; a decimal can have at most {MAX_SCALE}"
            ));
        }

        let mut mantissa = 0i128;
        for digit in whole.bytes().chain(fraction.bytes()) {
            mantissa = mantissa
                .checked_mul(10)
                .and_then(|value| value.checked_add(i128::from(digit - b'0')))
                .ok_or_else(|| format!("'{text}' is too large to hold as a decimal"))?;
        }
        if negative {
            mantissa = -mantissa;
        }
        Decimal::new(mantissa, scale)
    }

    /// The decimal a finite `f64` rounds to when written as its shortest
    /// round-tripping text.
    ///
    /// This conversion is deliberately named rather than implicit: an `f64`
    /// such as `0.1` is not exactly one tenth, so `decimal.from_number(0.1)` is
    /// a lossy request a program has to make explicitly. Arithmetic that mixes
    /// a decimal with a number is refused, so that loss can never happen by
    /// accident.
    pub fn from_f64(number: f64) -> Result<Decimal, String> {
        if !number.is_finite() {
            return Err(format!("{number} is not a finite number"));
        }
        Decimal::parse(&format!("{number}"))
    }

    /// The `f64` this decimal is closest to, for an explicit conversion back.
    ///
    /// The conversion is lossy when the decimal needs more precision than an
    /// `f64` has or than its shortest text round-trips, which is why it is a
    /// named function and never an implicit coercion.
    pub fn to_f64(self) -> f64 {
        self.mantissa as f64 / 10f64.powi(self.scale as i32)
    }

    /// The decimal as text, without trailing fractional zeros.
    pub fn text(self) -> String {
        self.to_string()
    }

    /// `self + other`, exact.
    pub fn sum_with(self, other: Decimal) -> Result<Decimal, String> {
        let scale = self.scale.max(other.scale);
        let left = self.mantissa_at(scale)?;
        let right = other.mantissa_at(scale)?;
        let mantissa = left
            .checked_add(right)
            .ok_or_else(|| "the decimal sum is too large to hold".to_string())?;
        Decimal::new(mantissa, scale)
    }

    /// `self - other`, exact.
    pub fn difference(self, other: Decimal) -> Result<Decimal, String> {
        let scale = self.scale.max(other.scale);
        let left = self.mantissa_at(scale)?;
        let right = other.mantissa_at(scale)?;
        let mantissa = left
            .checked_sub(right)
            .ok_or_else(|| "the decimal difference is too large to hold".to_string())?;
        Decimal::new(mantissa, scale)
    }

    /// `self * other`, exact while the mantissa fits and the scale is at most
    /// [`MAX_SCALE`].
    pub fn product(self, other: Decimal) -> Result<Decimal, String> {
        let negative = (self.mantissa < 0) != (other.mantissa < 0);
        let mut magnitude =
            Wide::product(self.mantissa.unsigned_abs(), other.mantissa.unsigned_abs());
        let mut scale = self.scale.checked_add(other.scale).ok_or_else(|| {
            "the decimal product needs too many places after the point".to_string()
        })?;
        while scale > 0 {
            let mut reduced = magnitude;
            if reduced.divide_small(10) != 0 {
                break;
            }
            magnitude = reduced;
            scale -= 1;
        }
        if scale > MAX_SCALE {
            return Err(format!(
                "the decimal product needs {scale} places after the point; a decimal can have at most {MAX_SCALE}"
            ));
        }
        let magnitude = magnitude
            .to_u128()
            .ok_or_else(|| "the decimal product is too large to hold".to_string())?;
        let mantissa = mantissa_from_magnitude(
            negative,
            magnitude,
            "the decimal product is too large to hold",
        )?;
        Ok(Decimal { mantissa, scale })
    }

    /// `self / other`, to [`MAX_SCALE`] places rounded half away from zero.
    pub fn quotient(self, other: Decimal) -> Result<Decimal, String> {
        if other.mantissa == 0 {
            return Err("Division by zero".to_string());
        }
        // result = (a.mant / 10^a.scale) / (b.mant / 10^b.scale)
        //        = a.mant * 10^(b.scale - a.scale) / b.mant
        // The result mantissa at `MAX_SCALE` is that ratio times 10^MAX_SCALE.
        // Since `a.scale <= MAX_SCALE`, that exponent is never negative, so
        // only the numerator needs to be wider than an `i128`.
        let exponent = other.scale + MAX_SCALE - self.scale;
        let numerator = Wide::product(self.mantissa.unsigned_abs(), pow10_wide(exponent)?);
        let denominator = other.mantissa.unsigned_abs();
        let (mut mantissa, remainder) = numerator.divide(denominator);
        // Half away from zero: a remainder of exactly half rounds up in
        // magnitude, and anything more does too.
        if remainder.saturating_mul(2) >= denominator {
            mantissa.increment();
        }
        let mut scale = MAX_SCALE;
        while scale > 0 {
            let mut reduced = mantissa;
            if reduced.divide_small(10) != 0 {
                break;
            }
            mantissa = reduced;
            scale -= 1;
        }
        let mantissa = mantissa
            .to_i128()
            .ok_or_else(|| "the decimal quotient is too large to hold".to_string())?;
        let negative = (self.mantissa < 0) != (other.mantissa < 0);
        Decimal::new(if negative { -mantissa } else { mantissa }, scale)
    }

    /// `self % other`, exact.
    pub fn remainder(self, other: Decimal) -> Result<Decimal, String> {
        if other.mantissa == 0 {
            return Err("Modulo by zero".to_string());
        }
        let scale = self.scale.max(other.scale);
        let left = self.mantissa_at(scale)?;
        let right = other.mantissa_at(scale)?;
        // The only overflow `i128` remainder has is `MIN % -1`, whose exact
        // answer is zero.
        let mantissa = left.checked_rem(right).unwrap_or_default();
        Decimal::new(mantissa, scale)
    }

    /// Adds a whole list exactly, cancelling opposite signs before the `i128`
    /// range is checked.
    ///
    /// Folding [`sum_with`](Self::sum_with) over a list can overflow on an
    /// intermediate even when the exact total fits: two `i128::MAX` values
    /// followed by `-i128::MAX` total `i128::MAX`. This accumulates the aligned
    /// mantissas as signed decimal digits, so only the final value has to fit.
    pub fn sum_all(values: &[Decimal]) -> Result<Decimal, String> {
        let scale = values.iter().map(|value| value.scale).max().unwrap_or(0);
        let mut total = SignedDigits::zero();
        for value in values {
            total.add(value.mantissa, (scale - value.scale) as usize);
        }
        total.into_decimal(scale)
    }

    /// Orders two decimals exactly.
    ///
    /// The comparison works from the leading digit and scale rather than
    /// aligning both mantissas to a common scale, so it cannot overflow even
    /// when the values span the whole `i128` range.
    pub fn compare(self, other: Decimal) -> Result<Ordering, String> {
        if self.mantissa == 0 && other.mantissa == 0 {
            return Ok(Ordering::Equal);
        }
        if self.mantissa == 0 {
            return Ok(if other.mantissa < 0 {
                Ordering::Greater
            } else {
                Ordering::Less
            });
        }
        if other.mantissa == 0 {
            return Ok(if self.mantissa < 0 {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let self_negative = self.mantissa < 0;
        let other_negative = other.mantissa < 0;
        if self_negative != other_negative {
            return Ok(if self_negative {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let ordering = compare_unsigned(
            self.mantissa.unsigned_abs(),
            self.scale,
            other.mantissa.unsigned_abs(),
            other.scale,
        );
        Ok(if self_negative {
            ordering.reverse()
        } else {
            ordering
        })
    }

    /// The decimal with the opposite sign.
    pub fn negated(self) -> Result<Decimal, String> {
        Ok(Decimal {
            mantissa: self
                .mantissa
                .checked_neg()
                .ok_or_else(|| "the decimal is too large to negate".to_string())?,
            scale: self.scale,
        })
    }

    /// Whether this decimal is exactly zero.
    pub fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    fn normalize(mut mantissa: i128, mut scale: u32) -> Decimal {
        if mantissa == 0 {
            return Decimal::zero();
        }
        while scale > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            scale -= 1;
        }
        Decimal { mantissa, scale }
    }

    fn mantissa_at(self, scale: u32) -> Result<i128, String> {
        debug_assert!(scale >= self.scale);
        let factor = pow10(scale - self.scale)?;
        self.mantissa
            .checked_mul(factor)
            .ok_or_else(|| "the decimal is too large to hold".to_string())
    }
}

fn compare_unsigned(left: u128, left_scale: u32, right: u128, right_scale: u32) -> Ordering {
    let left_digits = left.to_string();
    let right_digits = right.to_string();
    let left_leading = left_digits.len() as i64 - 1 - i64::from(left_scale);
    let right_leading = right_digits.len() as i64 - 1 - i64::from(right_scale);
    match left_leading.cmp(&right_leading) {
        Ordering::Equal => {
            let width = left_digits.len().max(right_digits.len());
            let left = left_digits.as_bytes();
            let right = right_digits.as_bytes();
            (0..width)
                .map(|index| {
                    left.get(index)
                        .copied()
                        .unwrap_or(b'0')
                        .cmp(&right.get(index).copied().unwrap_or(b'0'))
                })
                .find(|ordering| *ordering != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        }
        other => other,
    }
}

fn pow10(power: u32) -> Result<i128, String> {
    if power > MAX_SCALE {
        return Err(format!(
            "a decimal can have at most {MAX_SCALE} places after the point"
        ));
    }
    10i128
        .checked_pow(power)
        .ok_or_else(|| "the decimal is too large to hold".to_string())
}

fn pow10_wide(power: u32) -> Result<u128, String> {
    10u128
        .checked_pow(power)
        .ok_or_else(|| "the decimal quotient is too large to hold".to_string())
}

#[derive(Clone, Copy)]
struct Wide([u64; 4]);

impl Wide {
    fn product(left: u128, right: u128) -> Wide {
        let left = [left as u64, (left >> 64) as u64];
        let right = [right as u64, (right >> 64) as u64];
        let mut limbs = [0u64; 4];
        for (i, left_limb) in left.iter().enumerate() {
            let mut carry = 0u128;
            for (j, right_limb) in right.iter().enumerate() {
                let index = i + j;
                let value = u128::from(*left_limb) * u128::from(*right_limb)
                    + u128::from(limbs[index])
                    + carry;
                limbs[index] = value as u64;
                carry = value >> 64;
            }
            let mut index = i + right.len();
            while carry != 0 && index < limbs.len() {
                let value = u128::from(limbs[index]) + carry;
                limbs[index] = value as u64;
                carry = value >> 64;
                index += 1;
            }
        }
        Wide(limbs)
    }

    fn divide(self, divisor: u128) -> (Wide, u128) {
        let mut quotient = [0u64; 4];
        let mut remainder = 0u128;
        for bit in (0..256).rev() {
            remainder = (remainder << 1) | u128::from((self.0[bit / 64] >> (bit % 64)) & 1);
            if remainder >= divisor {
                remainder -= divisor;
                quotient[bit / 64] |= 1u64 << (bit % 64);
            }
        }
        (Wide(quotient), remainder)
    }

    fn divide_small(&mut self, divisor: u64) -> u64 {
        let divisor = u128::from(divisor);
        let mut remainder = 0u128;
        for limb in self.0.iter_mut().rev() {
            let value = (remainder << 64) | u128::from(*limb);
            *limb = (value / divisor) as u64;
            remainder = value % divisor;
        }
        remainder as u64
    }

    fn increment(&mut self) {
        for limb in &mut self.0 {
            let (value, overflow) = limb.overflowing_add(1);
            *limb = value;
            if !overflow {
                return;
            }
        }
    }

    fn to_u128(self) -> Option<u128> {
        if self.0[2] != 0 || self.0[3] != 0 {
            return None;
        }
        Some(u128::from(self.0[0]) | (u128::from(self.0[1]) << 64))
    }

    fn to_i128(self) -> Option<i128> {
        if self.0[2] != 0 || self.0[3] != 0 {
            return None;
        }
        let value = u128::from(self.0[0]) | (u128::from(self.0[1]) << 64);
        i128::try_from(value).ok()
    }
}

/// A signed integer in base ten, least-significant digit first.
///
/// It exists only for [`Decimal::sum_all`]: a list can cancel to a value that
/// fits even when an intermediate partial sum does not, so the accumulator is
/// not limited to an `i128`.
struct SignedDigits {
    negative: bool,
    digits: Vec<u8>,
}

impl SignedDigits {
    fn zero() -> SignedDigits {
        SignedDigits {
            negative: false,
            digits: Vec::new(),
        }
    }

    fn add(&mut self, mantissa: i128, trailing_zeros: usize) {
        if mantissa == 0 {
            return;
        }
        let negative = mantissa < 0;
        let mut magnitude = mantissa.unsigned_abs();
        let mut other = Vec::new();
        while magnitude > 0 {
            other.push((magnitude % 10) as u8);
            magnitude /= 10;
        }
        if trailing_zeros > 0 {
            let mut shifted = vec![0u8; trailing_zeros];
            shifted.extend_from_slice(&other);
            other = shifted;
        }
        if self.digits.is_empty() {
            self.negative = negative;
            self.digits = other;
            return;
        }
        if self.negative == negative {
            self.add_magnitude(&other);
            return;
        }
        match compare_magnitude(&self.digits, &other) {
            Ordering::Greater => self.subtract_magnitude(&other),
            Ordering::Less => {
                self.digits = subtract_magnitude(&other, &self.digits);
                self.negative = negative;
            }
            Ordering::Equal => {
                self.digits.clear();
                self.negative = false;
            }
        }
    }

    fn add_magnitude(&mut self, other: &[u8]) {
        let mut carry = 0u8;
        for index in 0..self.digits.len().max(other.len()) {
            let sum = self.digits.get(index).copied().unwrap_or(0)
                + other.get(index).copied().unwrap_or(0)
                + carry;
            let digit = sum % 10;
            if index < self.digits.len() {
                self.digits[index] = digit;
            } else {
                self.digits.push(digit);
            }
            carry = sum / 10;
        }
        if carry > 0 {
            self.digits.push(carry);
        }
    }

    fn subtract_magnitude(&mut self, other: &[u8]) {
        let mut borrow = 0i8;
        for index in 0..self.digits.len() {
            let mut difference = i8::try_from(self.digits[index]).unwrap()
                - i8::try_from(other.get(index).copied().unwrap_or(0)).unwrap()
                - borrow;
            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            self.digits[index] = difference as u8;
        }
        trim(&mut self.digits);
    }

    fn into_decimal(mut self, mut scale: u32) -> Result<Decimal, String> {
        if self.digits.is_empty() {
            return Ok(Decimal::zero());
        }
        while scale > 0 && self.digits.first() == Some(&0) {
            self.digits.remove(0);
            scale -= 1;
        }
        if self.digits.is_empty() {
            return Ok(Decimal::zero());
        }
        if scale > MAX_SCALE {
            return Err(format!(
                "the decimal sum needs {scale} places after the point; a decimal can have at most {MAX_SCALE}"
            ));
        }
        let mut magnitude = 0u128;
        for digit in self.digits.iter().rev() {
            magnitude = magnitude
                .checked_mul(10)
                .and_then(|value| value.checked_add(u128::from(*digit)))
                .ok_or_else(|| "the decimal sum is too large to hold".to_string())?;
        }
        let mantissa = mantissa_from_magnitude(
            self.negative,
            magnitude,
            "the decimal sum is too large to hold",
        )?;
        Decimal::new(mantissa, scale)
    }
}

fn mantissa_from_magnitude(negative: bool, magnitude: u128, message: &str) -> Result<i128, String> {
    if negative {
        if magnitude == i128::MAX as u128 + 1 {
            Ok(i128::MIN)
        } else if magnitude <= i128::MAX as u128 {
            Ok(-(magnitude as i128))
        } else {
            Err(message.to_string())
        }
    } else if magnitude <= i128::MAX as u128 {
        Ok(magnitude as i128)
    } else {
        Err(message.to_string())
    }
}

fn compare_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        other => other,
    }
}

fn subtract_magnitude(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut difference = larger.to_vec();
    let mut borrow = 0i8;
    for (index, digit) in difference.iter_mut().enumerate() {
        let mut value = i8::try_from(*digit).unwrap()
            - i8::try_from(smaller.get(index).copied().unwrap_or(0)).unwrap()
            - borrow;
        if value < 0 {
            value += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        *digit = value as u8;
    }
    trim(&mut difference);
    difference
}

fn trim(digits: &mut Vec<u8>) {
    while digits.last() == Some(&0) {
        digits.pop();
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let negative = self.mantissa < 0;
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        if scale == 0 {
            return write!(f, "{}{}", if negative { "-" } else { "" }, digits);
        }
        let (whole, fraction) = if digits.len() <= scale {
            let mut fraction = "0".repeat(scale - digits.len());
            fraction.push_str(&digits);
            ("0".to_string(), fraction)
        } else {
            let split = digits.len() - scale;
            (digits[..split].to_string(), digits[split..].to_string())
        };
        write!(
            f,
            "{}{}.{}",
            if negative { "-" } else { "" },
            whole,
            fraction
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).expect("test decimal should parse")
    }

    #[test]
    fn parsing_normalizes_trailing_zeros() {
        assert_eq!(d("0.10"), d("0.1"));
        assert_eq!(d("+3.1400"), d("3.14"));
        assert_eq!(d("-0.0"), Decimal::zero());
        assert_eq!(d(".5"), d("0.5"));
        assert_eq!(d("5."), d("5"));
        assert_eq!(d("0.0000000000000000010"), d("0.000000000000000001"));
    }

    #[test]
    fn edge_parse_rejects_empty_pointless_and_too_long_text() {
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse("-").is_err());
        assert!(Decimal::parse("1.2.3").is_err());
        assert!(Decimal::parse("1e3").is_err());
        assert!(Decimal::parse("0.0000000000000000001").is_err());
    }

    #[test]
    fn addition_is_exact_where_f64_is_not() {
        assert_eq!(d("0.1").sum_with(d("0.2")).unwrap(), d("0.3"));
        assert_ne!(0.1f64 + 0.2, 0.3);
    }

    #[test]
    fn edge_division_rounds_half_away_from_zero_and_refuses_zero() {
        assert_eq!(d("1").quotient(d("3")).unwrap(), d("0.333333333333333333"));
        assert_eq!(d("2").quotient(d("3")).unwrap(), d("0.666666666666666667"));
        assert_eq!(
            d("-2").quotient(d("3")).unwrap(),
            d("-0.666666666666666667")
        );
        assert_eq!(d("1").quotient(d("2")).unwrap(), d("0.5"));
        assert!(d("1").quotient(d("0")).is_err());
    }

    #[test]
    fn edge_division_of_large_representable_quotients_does_not_overflow() {
        assert_eq!(
            d("0.000000000000001001")
                .quotient(d("0.000000000000000001"))
                .unwrap(),
            d("1001")
        );
        assert_eq!(
            d("1000").quotient(d("0.000000000000000001")).unwrap(),
            d("1000000000000000000000")
        );
        assert_eq!(
            d("123456789012345678")
                .quotient(d("0.000000000000000001"))
                .unwrap(),
            d("123456789012345678000000000000000000")
        );
    }

    #[test]
    fn edge_arithmetic_refuses_overflow_and_scale_overflow() {
        let huge = Decimal::parse("170141183460469231731687303715884105727").unwrap();
        assert!(huge.sum_with(huge).is_err());
        let tiny = d("0.000000000000000001");
        assert!(tiny.product(tiny).is_err());
    }

    #[test]
    fn edge_remainder_is_exact_and_handles_the_minimum() {
        assert_eq!(d("7").remainder(d("3")).unwrap(), d("1"));
        assert_eq!(d("-7").remainder(d("3")).unwrap(), d("-1"));
        assert_eq!(d("7.5").remainder(d("2")).unwrap(), d("1.5"));
        let minimum = d("-85070591730234615865843651857942052864")
            .product(d("2"))
            .expect("the minimum i128 should be representable");
        assert_eq!(minimum.remainder(d("-1")).unwrap(), Decimal::zero());
        assert!(d("1").remainder(d("0")).is_err());
    }

    #[test]
    fn edge_product_normalizes_before_refusing_the_scale() {
        assert_eq!(
            d("0.0000000025").product(d("0.0000000004")).unwrap(),
            d("0.000000000000000001")
        );
        assert_eq!(
            d("17014118346046923173168730371588410572.5")
                .product(d("2"))
                .unwrap(),
            d("34028236692093846346337460743176821145")
        );
        let tiny = d("0.000000000000000001");
        assert!(tiny.product(tiny).is_err());
    }

    #[test]
    fn edge_sum_all_cancels_before_checking_the_mantissa_range() {
        let maximum = d("170141183460469231731687303715884105727");
        assert_eq!(
            Decimal::sum_all(&[maximum, maximum, maximum.negated().unwrap()]).unwrap(),
            maximum
        );
        let tiny = d("0.000000000000000001");
        assert_eq!(
            Decimal::sum_all(&[maximum, maximum.negated().unwrap(), tiny]).unwrap(),
            tiny
        );
        assert_eq!(Decimal::sum_all(&[]).unwrap(), Decimal::zero());
        assert!(Decimal::sum_all(&[maximum, maximum]).is_err());
    }

    #[test]
    fn display_keeps_the_written_scale() {
        assert_eq!(d("0").to_string(), "0");
        assert_eq!(d("-1.20").to_string(), "-1.2");
        assert_eq!(d("0.001").to_string(), "0.001");
        assert_eq!(d("-0.001").to_string(), "-0.001");
    }

    #[test]
    fn compare_is_exact_across_scales() {
        assert_eq!(d("1.0").compare(d("1")).unwrap(), Ordering::Equal);
        assert_eq!(d("1.01").compare(d("1.1")).unwrap(), Ordering::Less);
        assert_eq!(d("-2").compare(d("-1.999")).unwrap(), Ordering::Less);
    }

    #[test]
    fn edge_compare_does_not_overflow_when_scales_differ_widely() {
        let maximum = d("170141183460469231731687303715884105727");
        let tiny = d("0.000000000000000001");
        assert_eq!(maximum.compare(tiny).unwrap(), Ordering::Greater);
        assert_eq!(tiny.compare(maximum).unwrap(), Ordering::Less);
        assert_eq!(
            maximum.negated().unwrap().compare(tiny).unwrap(),
            Ordering::Less
        );
        assert_eq!(
            d("1.000000000000000000").compare(d("1")).unwrap(),
            Ordering::Equal
        );
    }
}
