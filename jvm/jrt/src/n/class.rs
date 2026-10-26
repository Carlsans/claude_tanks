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

pub fn get_component_type(this: Ref) -> JResult<Ref> {
    Ok(match of(&this)?.component {
        Component::Ref(e) => class_object(e),
        // Primitive classes (int.class) are not modelled.
        _ => None,
    })
}

pub fn is_array(this: Ref) -> JResult<i32> {
    Ok(!matches!(of(&this)?.component, Component::None) as i32)
}

pub fn is_interface(this: Ref) -> JResult<i32> {
    Ok(of(&this)?.interface as i32)
}

pub fn get_superclass(this: Ref) -> JResult<Ref> {
    Ok(of(&this)?.sup.and_then(class_object))
}

pub fn is_assignable_from(this: Ref, other: Ref) -> JResult<i32> {
    Ok(is_assignable(of(&other)?, of(&this)?) as i32)
}

/// `java.lang.reflect.Array.newInstance(Class, int)`, for reference types.
pub fn array_new_instance(elem: Ref, n: i32) -> JResult<Ref> {
    anewarray(of(&elem)?, n)
}

pub fn array_get_length(a: Ref) -> JResult<i32> {
    alen(&a)
}

/// No generic type information: an empty array.
pub fn no_types(_this: Ref) -> JResult<Ref> {
    anewarray(&C_java_lang_Object, 0)
}
