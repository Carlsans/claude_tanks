//! `java.lang.Class`: names and instance checks only (no reflection).
use crate::*;

fn of(r: &Ref) -> JResult<&'static Class> {
    match &nn(r)?.ext {
        Ext::Class(c) => Ok(c),
        _ => panic!("not a class object"),
    }
}

/// `getName`: dotted, also inside array descriptors (`[Ljava.lang.String;`).
pub fn java_name(c: &Class) -> String {
    c.name.replace('/', ".")
}

pub fn get_name(this: Ref) -> JResult<Ref> {
    Ok(jstr(&java_name(of(&this)?)))
}

pub fn get_simple_name(this: Ref) -> JResult<Ref> {
    let c = of(&this)?;
    let base = c.name.rsplit('/').next().unwrap_or(c.name);
    Ok(jstr(base.rsplit('$').next().unwrap_or(base)))
}

pub fn is_instance(this: Ref, o: Ref) -> JResult<i32> {
    Ok(instance_of(&o, of(&this)?) as i32)
}

pub fn no(_this: Ref) -> JResult<i32> {
    Ok(0)
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let c = of(&this)?;
    let kind = if c.interface { "interface " } else { "class " };
    Ok(jstr(&format!("{kind}{}", java_name(c))))
}
