//! `java.util.Random`, with Java's exact algorithm (a 48-bit linear
//! congruential generator), so seeded sequences match the JVM's.
use crate::*;

const MULT: i64 = 0x5_deec_e66d;
const MASK: i64 = (1 << 48) - 1;

thread_local! {
    static UNIQUIFIER: Cell<i64> = const { Cell::new(8_682_522_807_148_012) };
}

fn obj(r: &Ref) -> JResult<&Rc<Obj>> {
    nn(r)
}

pub fn init(this: Ref) -> JResult<()> {
    let u = UNIQUIFIER.with(|c| {
        let v = c.get().wrapping_mul(1_181_783_497_276_652_981);
        c.set(v);
        v
    });
    let t = crate::n::system::nano_time()?
        ^ crate::n::system::current_time_millis()?.wrapping_mul(1_000_003);
    set_seed(this, u ^ t)
}

pub fn init_seed(this: Ref, seed: i64) -> JResult<()> {
    set_seed(this, seed)
}

pub fn set_seed(this: Ref, seed: i64) -> JResult<()> {
    let o = obj(&this)?;
    o.prims[0].set(((seed ^ MULT) & MASK) as u64);
    o.prims[1].set(0);
    Ok(())
}

fn bits(o: &Obj, n: u32) -> i32 {
    let s = ((o.prims[0].get() as i64)
        .wrapping_mul(MULT)
        .wrapping_add(0xb))
        & MASK;
    o.prims[0].set(s as u64);
    (s as u64 >> (48 - n)) as i32
}

pub fn next(this: Ref, n: i32) -> JResult<i32> {
    Ok(bits(obj(&this)?, n as u32))
}

pub fn next_int(this: Ref) -> JResult<i32> {
    Ok(bits(obj(&this)?, 32))
}

pub fn next_int_bound(this: Ref, bound: i32) -> JResult<i32> {
    if bound <= 0 {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            "bound must be positive",
        ));
    }
    let o = obj(&this)?;
    let mut r = bits(o, 31);
    let m = bound - 1;
    if bound & m == 0 {
        return Ok(((i64::from(bound) * i64::from(r)) >> 31) as i32);
    }
    let mut u = r;
    loop {
        r = u % bound;
        if u.wrapping_sub(r).wrapping_add(m) >= 0 {
            return Ok(r);
        }
        u = bits(o, 31);
    }
}

/// Java 17's `nextInt(origin, bound)` (from `RandomSupport`).
pub fn next_int_range(this: Ref, origin: i32, bound: i32) -> JResult<i32> {
    if origin >= bound {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            "bound must be greater than origin",
        ));
    }
    let o = obj(&this)?;
    let mut r = bits(o, 32);
    let n = bound.wrapping_sub(origin);
    let m = n.wrapping_sub(1);
    if n & m == 0 {
        Ok((r & m).wrapping_add(origin))
    } else if n > 0 {
        let mut u = (r as u32 >> 1) as i32;
        loop {
            r = u % n;
            if u.wrapping_add(m).wrapping_sub(r) >= 0 {
                return Ok(r.wrapping_add(origin));
            }
            u = (bits(o, 32) as u32 >> 1) as i32;
        }
    } else {
        while r < origin || r >= bound {
            r = bits(o, 32);
        }
        Ok(r)
    }
}

pub fn next_long(this: Ref) -> JResult<i64> {
    let o = obj(&this)?;
    let hi = i64::from(bits(o, 32)) << 32;
    Ok(hi.wrapping_add(i64::from(bits(o, 32))))
}

pub fn next_boolean(this: Ref) -> JResult<i32> {
    Ok((bits(obj(&this)?, 1) != 0) as i32)
}

pub fn next_float(this: Ref) -> JResult<f32> {
    Ok(bits(obj(&this)?, 24) as f32 / (1 << 24) as f32)
}

pub fn next_double(this: Ref) -> JResult<f64> {
    let o = obj(&this)?;
    let hi = i64::from(bits(o, 26)) << 27;
    Ok((hi + i64::from(bits(o, 27))) as f64 * (1.0 / (1u64 << 53) as f64))
}

pub fn next_gaussian(this: Ref) -> JResult<f64> {
    let o = obj(&this)?;
    if o.prims[1].get() != 0 {
        o.prims[1].set(0);
        return Ok(f64::from_bits(o.prims[2].get()));
    }
    loop {
        let v1 = 2.0 * next_double(this.clone())? - 1.0;
        let v2 = 2.0 * next_double(this.clone())? - 1.0;
        let s = v1 * v1 + v2 * v2;
        if s < 1.0 && s != 0.0 {
            let m = (-2.0 * s.ln() / s).sqrt();
            o.prims[2].set((v2 * m).to_bits());
            o.prims[1].set(1);
            return Ok(v1 * m);
        }
    }
}

pub fn next_bytes(this: Ref, bytes: Ref) -> JResult<()> {
    let o = obj(&this)?;
    let len = alen(&bytes)?;
    let mut i = 0;
    while i < len {
        let mut rnd = bits(o, 32);
        for _ in 0..(len - i).min(4) {
            ast_b(&bytes, i, rnd as i8)?;
            rnd >>= 8;
            i += 1;
        }
    }
    Ok(())
}
