//! `java.lang.Math` and `StrictMath`, with Java's rules where they differ
//! from Rust's (rounding, `signum`, `max`/`min` of NaN and zeros).
use crate::*;

pub fn sin(a: f64) -> JResult<f64> {
    Ok(a.sin())
}
pub fn cos(a: f64) -> JResult<f64> {
    Ok(a.cos())
}
pub fn tan(a: f64) -> JResult<f64> {
    Ok(a.tan())
}
pub fn asin(a: f64) -> JResult<f64> {
    Ok(a.asin())
}
pub fn acos(a: f64) -> JResult<f64> {
    Ok(a.acos())
}
pub fn atan(a: f64) -> JResult<f64> {
    Ok(a.atan())
}
pub fn atan2(y: f64, x: f64) -> JResult<f64> {
    Ok(y.atan2(x))
}
pub fn sinh(a: f64) -> JResult<f64> {
    Ok(a.sinh())
}
pub fn cosh(a: f64) -> JResult<f64> {
    Ok(a.cosh())
}
pub fn tanh(a: f64) -> JResult<f64> {
    Ok(a.tanh())
}
pub fn sqrt(a: f64) -> JResult<f64> {
    Ok(a.sqrt())
}
pub fn cbrt(a: f64) -> JResult<f64> {
    Ok(a.cbrt())
}
pub fn pow(a: f64, b: f64) -> JResult<f64> {
    Ok(a.powf(b))
}
pub fn exp(a: f64) -> JResult<f64> {
    Ok(a.exp())
}
pub fn expm1(a: f64) -> JResult<f64> {
    Ok(a.exp_m1())
}
pub fn log(a: f64) -> JResult<f64> {
    Ok(a.ln())
}
pub fn log10(a: f64) -> JResult<f64> {
    Ok(a.log10())
}
pub fn log1p(a: f64) -> JResult<f64> {
    Ok(a.ln_1p())
}
pub fn hypot(a: f64, b: f64) -> JResult<f64> {
    Ok(a.hypot(b))
}
pub fn floor(a: f64) -> JResult<f64> {
    Ok(a.floor())
}
pub fn ceil(a: f64) -> JResult<f64> {
    Ok(a.ceil())
}
pub fn rint(a: f64) -> JResult<f64> {
    Ok(a.round_ties_even())
}

/// Java 8+ `Math.round(double)`: half up, exact for every input.
pub fn round_d(a: f64) -> JResult<i64> {
    let bits = a.to_bits() as i64;
    let biased = (bits & 0x7ff0_0000_0000_0000) >> 52;
    let shift = (53 - 2 + 1023) - biased;
    if shift & -64 == 0 {
        let mut r = (bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
        if bits < 0 {
            r = -r;
        }
        Ok(((r >> shift) + 1) >> 1)
    } else {
        Ok(a as i64)
    }
}

pub fn round_f(a: f32) -> JResult<i32> {
    let bits = a.to_bits() as i32;
    let biased = (bits & 0x7f80_0000) >> 23;
    let shift = (24 - 2 + 127) - biased;
    if shift & -32 == 0 {
        let mut r = (bits & 0x007f_ffff) | 0x0080_0000;
        if bits < 0 {
            r = -r;
        }
        Ok(((r >> shift) + 1) >> 1)
    } else {
        Ok(a as i32)
    }
}

pub fn abs_i(a: i32) -> JResult<i32> {
    Ok(a.wrapping_abs())
}
pub fn abs_j(a: i64) -> JResult<i64> {
    Ok(a.wrapping_abs())
}
pub fn abs_f(a: f32) -> JResult<f32> {
    Ok(a.abs())
}
pub fn abs_d(a: f64) -> JResult<f64> {
    Ok(a.abs())
}
pub fn max_i(a: i32, b: i32) -> JResult<i32> {
    Ok(a.max(b))
}
pub fn max_j(a: i64, b: i64) -> JResult<i64> {
    Ok(a.max(b))
}
pub fn min_i(a: i32, b: i32) -> JResult<i32> {
    Ok(a.min(b))
}
pub fn min_j(a: i64, b: i64) -> JResult<i64> {
    Ok(a.min(b))
}

pub fn max_d(a: f64, b: f64) -> JResult<f64> {
    Ok(if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        b
    } else if a >= b {
        a
    } else {
        b
    })
}

pub fn min_d(a: f64, b: f64) -> JResult<f64> {
    Ok(if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        b
    } else if a <= b {
        a
    } else {
        b
    })
}

pub fn max_f(a: f32, b: f32) -> JResult<f32> {
    Ok(max_d(f64::from(a), f64::from(b))? as f32)
}

pub fn min_f(a: f32, b: f32) -> JResult<f32> {
    Ok(min_d(f64::from(a), f64::from(b))? as f32)
}

pub fn signum_d(a: f64) -> JResult<f64> {
    Ok(if a.is_nan() || a == 0.0 {
        a
    } else {
        1f64.copysign(a)
    })
}

pub fn signum_f(a: f32) -> JResult<f32> {
    Ok(if a.is_nan() || a == 0.0 {
        a
    } else {
        1f32.copysign(a)
    })
}

/// As Java 9 and later compute them.
pub fn to_radians(a: f64) -> JResult<f64> {
    Ok(a * 0.017453292519943295)
}
pub fn to_degrees(a: f64) -> JResult<f64> {
    Ok(a * 57.29577951308232)
}

pub fn ulp(a: f64) -> JResult<f64> {
    if a.is_nan() {
        return Ok(a);
    }
    if a.is_infinite() {
        return Ok(f64::INFINITY);
    }
    let a = a.abs();
    if a == f64::MAX {
        return Ok(2f64.powi(971));
    }
    Ok(f64::from_bits(a.to_bits() + 1) - a)
}

pub fn ieee_remainder(a: f64, b: f64) -> JResult<f64> {
    Ok(a - (a / b).round_ties_even() * b)
}

pub fn copy_sign(a: f64, b: f64) -> JResult<f64> {
    Ok(a.copysign(b))
}

pub fn floor_div_i(a: i32, b: i32) -> JResult<i32> {
    let q = idiv(a, b)?;
    Ok(if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q - 1
    } else {
        q
    })
}

pub fn floor_mod_i(a: i32, b: i32) -> JResult<i32> {
    Ok(a.wrapping_sub(floor_div_i(a, b)?.wrapping_mul(b)))
}

thread_local! {
    static SEED: Cell<i64> = Cell::new(initial_seed());
}

fn initial_seed() -> i64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64);
    (t ^ 0x5_deec_e66d) & ((1 << 48) - 1)
}

/// Seeds `Math.random()` on this thread (Java's `Random` algorithm).
pub fn set_random_seed(seed: i64) {
    SEED.with(|s| s.set((seed ^ 0x5_deec_e66d) & ((1 << 48) - 1)));
}

fn next(bits: u32) -> i64 {
    SEED.with(|s| {
        let v = (s.get().wrapping_mul(0x5_deec_e66d).wrapping_add(0xb)) & ((1 << 48) - 1);
        s.set(v);
        v >> (48 - bits)
    })
}

pub fn random() -> JResult<f64> {
    Ok(((next(26) << 27) + next(27)) as f64 * (1.0 / (1u64 << 53) as f64))
}

fn overflow(kind: &str) -> Throw {
    throw(
        &C_java_lang_ArithmeticException,
        &format!("{kind} overflow"),
    )
}

pub fn add_exact_i(a: i32, b: i32) -> JResult<i32> {
    a.checked_add(b).ok_or_else(|| overflow("integer"))
}
pub fn add_exact_j(a: i64, b: i64) -> JResult<i64> {
    a.checked_add(b).ok_or_else(|| overflow("long"))
}
pub fn sub_exact_i(a: i32, b: i32) -> JResult<i32> {
    a.checked_sub(b).ok_or_else(|| overflow("integer"))
}
pub fn sub_exact_j(a: i64, b: i64) -> JResult<i64> {
    a.checked_sub(b).ok_or_else(|| overflow("long"))
}
pub fn mul_exact_i(a: i32, b: i32) -> JResult<i32> {
    a.checked_mul(b).ok_or_else(|| overflow("integer"))
}
pub fn mul_exact_j(a: i64, b: i64) -> JResult<i64> {
    a.checked_mul(b).ok_or_else(|| overflow("long"))
}
pub fn to_int_exact(a: i64) -> JResult<i32> {
    i32::try_from(a).map_err(|_| overflow("integer"))
}

pub fn floor_div_j(a: i64, b: i64) -> JResult<i64> {
    let q = ldiv(a, b)?;
    Ok(if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q - 1
    } else {
        q
    })
}

pub fn floor_mod_j(a: i64, b: i64) -> JResult<i64> {
    Ok(a.wrapping_sub(floor_div_j(a, b)?.wrapping_mul(b)))
}
