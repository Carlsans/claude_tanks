//! `java.lang.Enum`: the ordinal in `prims[0]`, the name in `refs[0]`.
use crate::*;

pub fn init(this: Ref, name: Ref, ordinal: i32) -> JResult<()> {
    let o = nn(&this)?;
    *o.refs[0].borrow_mut() = name;
    o.prims[0].set(ordinal as i64 as u64);
    Ok(())
}

pub fn name(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[0].borrow().clone())
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    name(this)
}

pub fn ordinal(this: Ref) -> JResult<i32> {
    Ok(nn(&this)?.prims[0].get() as i32)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(same(&this, &o) as i32)
}

pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(identity_hash(nn(&this)?))
}

pub fn clone(_this: Ref) -> JResult<Ref> {
    Err(throw(&C_java_lang_CloneNotSupportedException, ""))
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    let (a, b) = (nn(&this)?, nn(&o)?);
    if !std::ptr::eq(declaring(a), declaring(b)) {
        return Err(throw(&C_java_lang_ClassCastException, b.class.name));
    }
    Ok(a.prims[0].get() as i32 - b.prims[0].get() as i32)
}

/// Constants with bodies are subclasses of their enum.
fn declaring(o: &Obj) -> &'static Class {
    match o.class.sup {
        Some(s) if s.name != "java/lang/Enum" => s,
        _ => o.class,
    }
}

pub fn get_declaring_class(this: Ref) -> JResult<Ref> {
    Ok(class_object(declaring(nn(&this)?)))
}

pub fn value_of(class: Ref, name: Ref) -> JResult<Ref> {
    let c = match &nn(&class)?.ext {
        Ext::Class(c) => *c,
        _ => panic!("not a class object"),
    };
    let wanted = crate::n::string::of(&name)?.to_vec();
    let Some(values) = c.values else {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            &format!("{} is not an enum class", c.name),
        ));
    };
    let all = values()?;
    if let Ext::Refs(v) = &nn(&all)?.ext {
        for e in v.borrow().iter().flatten() {
            if let Some(n) = &*e.refs[0].borrow()
                && chars(n) == wanted.as_slice()
            {
                return Ok(Some(e.clone()));
            }
        }
    }
    Err(throw(
        &C_java_lang_IllegalArgumentException,
        &format!(
            "No enum constant {}.{}",
            c.name.replace('/', "."),
            String::from_utf16_lossy(&wanted)
        ),
    ))
}
