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
    Ok(
        ch(c).is_some_and(|c| c.is_ascii_digit() || (c.is_numeric() && !c.is_ascii()))
            as i32,
    )
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
