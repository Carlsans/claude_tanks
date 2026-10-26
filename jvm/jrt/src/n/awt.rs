//! The slice of `java.awt` robots and the bridge touch without painting:
//! `Color` in full (robots set their colors), and bare `Font`, `BasicStroke`,
//! `RenderingHints` and `AlphaComposite` so the bridge can build its (unused)
//! painting surface. Color keeps its ARGB value in `prims[0]`.
use crate::*;

fn argb(this: &Ref) -> JResult<i32> {
    Ok(nn(this)?.prims[0].get() as i32)
}

/// `UIManager.getColor`: Swing's default control grey.
pub fn ui_color(_key: Ref) -> JResult<Ref> {
    Ok(new_color(0xffee_eeee_u32 as i32))
}

fn clamp(v: i32) -> JResult<i32> {
    if !(0..=255).contains(&v) {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            "Color parameter outside of expected range",
        ));
    }
    Ok(v)
}

fn new_color(v: i32) -> Ref {
    let c = alloc(&C_java_awt_Color);
    c.as_ref().unwrap().prims[0].set(v as u32 as u64);
    c
}

pub fn init_rgb(this: Ref, r: i32, g: i32, b: i32) -> JResult<()> {
    init_rgba(this, r, g, b, 255)
}

pub fn init_rgba(this: Ref, r: i32, g: i32, b: i32, a: i32) -> JResult<()> {
    let v = (clamp(a)? << 24) | (clamp(r)? << 16) | (clamp(g)? << 8) | clamp(b)?;
    nn(&this)?.prims[0].set(v as u32 as u64);
    Ok(())
}

pub fn init_int(this: Ref, rgb: i32) -> JResult<()> {
    nn(&this)?.prims[0].set((0xff00_0000u32 | rgb as u32) as u64);
    Ok(())
}

pub fn init_int_alpha(this: Ref, v: i32, has_alpha: i32) -> JResult<()> {
    let v = if has_alpha != 0 {
        v as u32
    } else {
        0xff00_0000 | v as u32
    };
    nn(&this)?.prims[0].set(v as u64);
    Ok(())
}

pub fn init_float(this: Ref, r: f32, g: f32, b: f32) -> JResult<()> {
    let c = |x: f32| (x * 255.0 + 0.5) as i32;
    init_rgb(this, c(r), c(g), c(b))
}

pub fn get_rgb(this: Ref) -> JResult<i32> {
    argb(&this)
}
pub fn get_red(this: Ref) -> JResult<i32> {
    Ok((argb(&this)? >> 16) & 0xff)
}
pub fn get_green(this: Ref) -> JResult<i32> {
    Ok((argb(&this)? >> 8) & 0xff)
}
pub fn get_blue(this: Ref) -> JResult<i32> {
    Ok(argb(&this)? & 0xff)
}
pub fn get_alpha(this: Ref) -> JResult<i32> {
    Ok((argb(&this)? >> 24) & 0xff)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(match &o {
        Some(x) if std::ptr::eq(x.class, &C_java_awt_Color) => {
            (argb(&this)? == x.prims[0].get() as i32) as i32
        }
        _ => 0,
    })
}

pub fn hash_code(this: Ref) -> JResult<i32> {
    argb(&this)
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let v = argb(&this)?;
    Ok(jstr(&format!(
        "java.awt.Color[r={},g={},b={}]",
        (v >> 16) & 0xff,
        (v >> 8) & 0xff,
        v & 0xff
    )))
}

/// `brighter()` and `darker()` as in AWT.
pub fn brighter(this: Ref) -> JResult<Ref> {
    let v = argb(&this)?;
    let (mut r, mut g, mut b, a) = (
        (v >> 16) & 0xff,
        (v >> 8) & 0xff,
        v & 0xff,
        (v >> 24) & 0xff,
    );
    let i = (1.0 / (1.0 - 0.7)) as i32;
    if r == 0 && g == 0 && b == 0 {
        return Ok(new_color((a << 24) | (i << 16) | (i << 8) | i));
    }
    if r > 0 && r < i {
        r = i;
    }
    if g > 0 && g < i {
        g = i;
    }
    if b > 0 && b < i {
        b = i;
    }
    let f = |x: i32| ((x as f64 / 0.7) as i32).min(255);
    Ok(new_color((a << 24) | (f(r) << 16) | (f(g) << 8) | f(b)))
}

pub fn darker(this: Ref) -> JResult<Ref> {
    let v = argb(&this)?;
    let f = |x: i32| ((x as f64 * 0.7) as i32).max(0);
    Ok(new_color(
        (v & 0xff00_0000u32 as i32)
            | (f((v >> 16) & 0xff) << 16)
            | (f((v >> 8) & 0xff) << 8)
            | f(v & 0xff),
    ))
}

pub fn decode(s: Ref) -> JResult<Ref> {
    let t = rust_string(nn(&s)?);
    let digits = t
        .trim_start_matches(['#'])
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let v = i32::from_str_radix(digits, 16)
        .map_err(|_| throw(&C_java_lang_NumberFormatException, &t))?;
    Ok(new_color((0xff00_0000u32 | v as u32) as i32))
}

macro_rules! named {
    ($($f:ident $v:literal),*) => { $(pub fn $f() -> JResult<Ref> { Ok(new_color(($v as u32 | 0xff00_0000) as i32)) })* };
}

named!(white 0xffffff, light_gray 0xc0c0c0, gray 0x808080, dark_gray 0x404040, black 0x000000, red 0xff0000, pink 0xffafaf, orange 0xffc800, yellow 0xffff00, green 0x00ff00, magenta 0xff00ff, cyan 0x00ffff, blue 0x0000ff);

pub fn nothing(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn nothing_f(_this: Ref, _w: f32) -> JResult<()> {
    Ok(())
}

pub fn nothing_ref(_this: Ref, _x: Ref) -> JResult<()> {
    Ok(())
}

pub fn font_init(this: Ref, name: Ref, style: i32, size: i32) -> JResult<()> {
    let o = nn(&this)?;
    *o.refs[0].borrow_mut() = name;
    o.prims[0].set(style as u64);
    o.prims[1].set(size as u64);
    Ok(())
}

pub fn font_name(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[0].borrow().clone())
}

pub fn font_style(this: Ref) -> JResult<i32> {
    Ok(nn(&this)?.prims[0].get() as i32)
}

pub fn font_size(this: Ref) -> JResult<i32> {
    Ok(nn(&this)?.prims[1].get() as i32)
}

pub fn hints_put(_this: Ref, _k: Ref, _v: Ref) -> JResult<Ref> {
    Ok(None)
}

pub fn hints_get(_this: Ref, _k: Ref) -> JResult<Ref> {
    Ok(None)
}

/// Rendering-hint keys and values: plain objects, never read.
pub fn hint() -> JResult<Ref> {
    Ok(alloc(&C_java_lang_Object))
}

pub fn alpha_composite(rule: i32) -> JResult<Ref> {
    let c = alloc(&C_java_awt_AlphaComposite);
    c.as_ref().unwrap().prims[0].set(rule as u64);
    Ok(c)
}

pub fn alpha_composite_alpha(rule: i32, _alpha: f32) -> JResult<Ref> {
    alpha_composite(rule)
}
