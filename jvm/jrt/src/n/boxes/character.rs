//! `java.lang.Character`.
use super::*;

fn get(r: &Ref) -> JResult<i32> {
    Ok(bits(r)? as i32)
}

fn ch(c: i32) -> Option<char> {
    char::from_u32(c as u32 & 0xffff)
}

pub fn init(this: Ref, v: i32) -> JResult<()> {
    nn(&this)?.prims[0].set(v as u16 as u64);
    Ok(())
}

pub fn value_of(v: i32) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Character, v as u16 as u64))
}

pub fn is_digit(c: i32) -> JResult<i32> {
    Ok(ch(c).is_some_and(|c| c.is_ascii_digit() || (c.is_numeric() && !c.is_ascii())) as i32)
}
pub fn is_letter(c: i32) -> JResult<i32> {
    Ok(ch(c).is_some_and(char::is_alphabetic) as i32)
}
pub fn is_letter_or_digit(c: i32) -> JResult<i32> {
    Ok(ch(c).is_some_and(char::is_alphanumeric) as i32)
}
pub fn is_whitespace(c: i32) -> JResult<i32> {
    Ok(ch(c)
        .is_some_and(|c| c.is_whitespace() && c != '\u{a0}' && c != '\u{2007}' && c != '\u{202f}')
        as i32)
}
pub fn is_upper_case(c: i32) -> JResult<i32> {
    Ok(ch(c).is_some_and(char::is_uppercase) as i32)
}
pub fn is_lower_case(c: i32) -> JResult<i32> {
    Ok(ch(c).is_some_and(char::is_lowercase) as i32)
}

pub fn to_upper_case(c: i32) -> JResult<i32> {
    Ok(ch(c).map_or(c, |x| {
        let mut u = x.to_uppercase();
        match (u.next(), u.next()) {
            (Some(y), None) if (y as u32) < 0x10000 => y as i32,
            _ => c,
        }
    }))
}

pub fn to_lower_case(c: i32) -> JResult<i32> {
    Ok(ch(c).map_or(c, |x| {
        let mut l = x.to_lowercase();
        match (l.next(), l.next()) {
            (Some(y), None) if (y as u32) < 0x10000 => y as i32,
            _ => c,
        }
    }))
}

pub fn to_string_static(c: i32) -> JResult<Ref> {
    Ok(new_string(vec![c as u16]))
}
pub fn char_value(this: Ref) -> JResult<i32> {
    get(&this)
}
pub fn hash_code(this: Ref) -> JResult<i32> {
    get(&this)
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok((same_class(&o, &C_java_lang_Character) && get(&o)? == get(&this)?) as i32)
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    Ok(get(&this)? - get(&o)?)
}

pub fn is_space_char(c: i32) -> JResult<i32> {
    Ok(matches!(
        c,
        0x20 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000
    ) as i32)
}
pub fn is_iso_control(c: i32) -> JResult<i32> {
    Ok(matches!(c, 0..=0x1f | 0x7f..=0x9f) as i32)
}
pub fn is_defined(c: i32) -> JResult<i32> {
    Ok(char::from_u32(c as u32).is_some() as i32)
}
pub fn is_ideographic(c: i32) -> JResult<i32> {
    Ok(
        matches!(c, 0x3006 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303a | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x3134f)
            as i32,
    )
}
pub fn is_identifier_ignorable(c: i32) -> JResult<i32> {
    Ok(matches!(c, 0..=8 | 0xe..=0x1b | 0x7f..=0x9f | 0xad | 0x200b..=0x200f | 0xfeff) as i32)
}
pub fn is_java_identifier_start(c: i32) -> JResult<i32> {
    Ok((char::from_u32(c as u32).is_some_and(char::is_alphabetic)
        || c == '$' as i32
        || c == '_' as i32) as i32)
}
pub fn is_java_identifier_part(c: i32) -> JResult<i32> {
    Ok((char::from_u32(c as u32).is_some_and(char::is_alphanumeric)
        || c == '$' as i32
        || c == '_' as i32) as i32)
}
pub fn is_high_surrogate(c: i32) -> JResult<i32> {
    Ok((0xd800..=0xdbff).contains(&c) as i32)
}
pub fn is_low_surrogate(c: i32) -> JResult<i32> {
    Ok((0xdc00..=0xdfff).contains(&c) as i32)
}
pub fn is_surrogate(c: i32) -> JResult<i32> {
    Ok((0xd800..=0xdfff).contains(&c) as i32)
}
pub fn is_supplementary(c: i32) -> JResult<i32> {
    Ok((0x10000..=0x10ffff).contains(&c) as i32)
}
pub fn is_valid_code_point(c: i32) -> JResult<i32> {
    Ok((0..=0x10ffff).contains(&c) as i32)
}
pub fn char_count(c: i32) -> JResult<i32> {
    Ok(if c >= 0x10000 { 2 } else { 1 })
}
pub fn to_code_point(hi: i32, lo: i32) -> JResult<i32> {
    Ok(((hi - 0xd800) << 10) + (lo - 0xdc00) + 0x10000)
}
pub fn no(_c: i32) -> JResult<i32> {
    Ok(0)
}

pub fn code_point_at(s: Ref, i: i32) -> JResult<i32> {
    let v = crate::n::string::seq_chars(&s)?;
    let Some(&hi) = v.get(i as usize) else {
        return Err(throw(
            &C_java_lang_StringIndexOutOfBoundsException,
            &format!("index {i}"),
        ));
    };
    let hi = i32::from(hi);
    if (0xd800..=0xdbff).contains(&hi)
        && let Some(&lo) = v.get(i as usize + 1)
        && (0xdc00..=0xdfff).contains(&i32::from(lo))
    {
        return to_code_point(hi, i32::from(lo));
    }
    Ok(hi)
}

pub fn code_point_before(s: Ref, i: i32) -> JResult<i32> {
    let v = crate::n::string::seq_chars(&s)?;
    if i < 1 || i as usize > v.len() {
        return Err(throw(
            &C_java_lang_StringIndexOutOfBoundsException,
            &format!("index {i}"),
        ));
    }
    let lo = i32::from(v[i as usize - 1]);
    if (0xdc00..=0xdfff).contains(&lo) && i >= 2 {
        let hi = i32::from(v[i as usize - 2]);
        if (0xd800..=0xdbff).contains(&hi) {
            return to_code_point(hi, lo);
        }
    }
    Ok(lo)
}

/// `Character.getType`: the general category, as Java numbers it (an
/// approximation from Rust's character classes).
pub fn get_type(c: i32) -> JResult<i32> {
    let Some(ch) = char::from_u32(c as u32) else {
        return Ok(if (0xd800..=0xdfff).contains(&c) {
            19
        } else {
            0
        });
    };
    Ok(if ch.is_uppercase() {
        1
    } else if ch.is_lowercase() {
        2
    } else if ch.is_alphabetic() {
        5
    } else if ch.is_ascii_digit() || (ch.is_numeric() && ch.is_ascii_digit()) {
        9
    } else if ch.is_numeric() {
        11
    } else if ch == ' ' || c == 0xa0 || c == 0x3000 || (0x2000..=0x200a).contains(&c) {
        12
    } else if c == 0x2028 {
        13
    } else if c == 0x2029 {
        14
    } else if ch.is_control() {
        15
    } else if "_".contains(ch) {
        23
    } else if "-".contains(ch) {
        20
    } else if "([{".contains(ch) {
        21
    } else if ")]}".contains(ch) {
        22
    } else if "+<=>|~".contains(ch) {
        25
    } else if "$".contains(ch) {
        26
    } else if "^`".contains(ch) {
        27
    } else if ch.is_ascii_punctuation() {
        24
    } else {
        0
    })
}

pub fn digit(c: i32, radix: i32) -> JResult<i32> {
    Ok(char::from_u32(c as u32)
        .and_then(|ch| ch.to_digit(radix as u32))
        .map_or(-1, |d| d as i32))
}

pub fn for_digit(d: i32, radix: i32) -> JResult<i32> {
    Ok(std::char::from_digit(d as u32, radix as u32).map_or(0, |c| c as i32))
}

pub fn numeric_value(c: i32) -> JResult<i32> {
    Ok(char::from_u32(c as u32)
        .and_then(|ch| ch.to_digit(36))
        .map_or(-1, |d| d as i32))
}
