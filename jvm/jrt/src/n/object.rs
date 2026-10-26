//! `java.lang.Object`.
use crate::*;

pub fn init(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(same(&this, &o) as i32)
}

pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(identity_hash(nn(&this)?))
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let o = nn(&this)?;
    let h = call_hash_code(o)?;
    Ok(jstr(&format!("{}@{:x}", o.class.name.replace('/', "."), h)))
}

pub fn clone(this: Ref) -> JResult<Ref> {
    let o = nn(&this)?;
    if !matches!(o.class.component, Component::None)
        || is_assignable(o.class, &C_java_lang_Cloneable)
    {
        Ok(Some(shallow_clone(o)))
    } else {
        Err(throw(
            &C_java_lang_CloneNotSupportedException,
            &o.class.name.replace('/', "."),
        ))
    }
}

pub fn finalize(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn get_class(this: Ref) -> JResult<Ref> {
    Ok(class_object(nn(&this)?.class))
}

pub fn nop(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn wait_j(_this: Ref, _ms: i64) -> JResult<()> {
    Ok(())
}
