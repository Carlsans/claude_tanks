//! Reflection over translated classes: each `Class` lists its fields and
//! its methods (with an invoker that takes and returns `Val`s), and a
//! robot's program registers its classes so they can be found by name.
//! The runtime uses it to create robots, to fill objects from JSON, and to
//! drive the bot API; robots get `Class.forName` and friends from it.
use crate::*;

/// A value of any Java type, for calls whose signature is only known at run
/// time.
#[derive(Clone)]
pub enum Val {
    V,
    I(i32),
    J(i64),
    F(f32),
    D(f64),
    A(Ref),
}

impl Val {
    pub fn i(&self) -> i32 {
        match self {
            Val::I(v) => *v,
            Val::J(v) => *v as i32,
            _ => 0,
        }
    }
    pub fn j(&self) -> i64 {
        match self {
            Val::J(v) => *v,
            Val::I(v) => i64::from(*v),
            _ => 0,
        }
    }
    pub fn f(&self) -> f32 {
        match self {
            Val::F(v) => *v,
            Val::D(v) => *v as f32,
            _ => 0.0,
        }
    }
    pub fn d(&self) -> f64 {
        match self {
            Val::D(v) => *v,
            Val::F(v) => f64::from(*v),
            Val::I(v) => f64::from(*v),
            _ => 0.0,
        }
    }
    pub fn a(&self) -> Ref {
        match self {
            Val::A(r) => r.clone(),
            _ => None,
        }
    }
}

pub struct FieldMeta {
    pub name: &'static str,
    pub desc: &'static str,
    /// The generic type, if it has one (`Ljava/util/List<LFoo;>;`).
    pub generic: &'static str,
    pub is_static: bool,
    /// Instance fields: the index into `prims` or `refs` (by `desc`).
    pub index: u32,
    /// Static fields: accessors.
    pub get: Option<fn() -> JResult<Val>>,
    pub set: Option<fn(Val) -> JResult<()>>,
}

pub struct MethodMeta {
    /// Name and descriptor: `add(Ljava/lang/Object;)Z`.
    pub sig: &'static str,
    pub is_static: bool,
    /// Calls it: the receiver first unless static.
    pub call: fn(&[Val]) -> JResult<Val>,
}

thread_local! {
    static CLASSES: RefCell<HashMap<&'static str, &'static Class>> = RefCell::new(HashMap::new());
}

/// Makes a program's classes findable by name on this thread (a robot's
/// thread).
pub fn register(classes: &[&'static Class]) {
    CLASSES.with(|t| {
        let mut t = t.borrow_mut();
        for c in NATIVE_CLASS_LIST.iter().chain(classes) {
            t.insert(c.name, c);
        }
    });
}

/// A class by internal name (`java/util/ArrayList`) or array descriptor.
pub fn class_named(name: &str) -> Option<&'static Class> {
    if let Some(e) = name.strip_prefix('[') {
        return match e.as_bytes().first()? {
            b'L' => class_named(e.strip_prefix('L')?.strip_suffix(';')?).map(array_class),
            b'[' => class_named(e).map(array_class),
            &c => Some(prim_array_class(c)),
        };
    }
    CLASSES.with(|t| t.borrow().get(name).copied())
}

/// A method declared by `c` or inherited (static or not).
pub fn method(c: &'static Class, sig: &str) -> Option<&'static MethodMeta> {
    let mut k = Some(c);
    while let Some(cl) = k {
        if let Some(m) = cl.methods.iter().find(|m| m.sig == sig) {
            return Some(m);
        }
        k = cl.sup;
    }
    None
}

/// A field declared by `c` or inherited.
pub fn field(c: &'static Class, name: &str) -> Option<&'static FieldMeta> {
    let mut k = Some(c);
    while let Some(cl) = k {
        if let Some(f) = cl.fields.iter().find(|f| f.name == name) {
            return Some(f);
        }
        k = cl.sup;
    }
    None
}

/// Every instance field of a class, superclasses' first.
pub fn instance_fields(c: &'static Class) -> Vec<&'static FieldMeta> {
    let mut chain = Vec::new();
    let mut k = Some(c);
    while let Some(cl) = k {
        chain.push(cl);
        k = cl.sup;
    }
    chain
        .iter()
        .rev()
        .flat_map(|cl| cl.fields.iter().filter(|f| !f.is_static))
        .collect()
}

fn no_method(c: &Class, sig: &str) -> Throw {
    throw(&C_java_lang_NoSuchMethodError, &format!("{}.{sig}", c.name))
}

/// Calls a method by signature: virtually on `args[0]` unless static.
pub fn call(c: &'static Class, sig: &str, args: &[Val]) -> JResult<Val> {
    let m = method(c, sig).ok_or_else(|| no_method(c, sig))?;
    (m.call)(args)
}

/// Calls the method an object's class has for `sig` (overrides included).
pub fn call_virtual(obj: &Ref, sig: &str, rest: &[Val]) -> JResult<Val> {
    let o = nn(obj)?;
    let mut args = vec![Val::A(obj.clone())];
    args.extend_from_slice(rest);
    call(o.class, sig, &args)
}

/// `new C(args)` with the constructor of descriptor `desc` (`(I)V`).
pub fn new_instance(c: &'static Class, desc: &str, args: &[Val]) -> JResult<Ref> {
    let o = alloc(c);
    let mut all = vec![Val::A(o.clone())];
    all.extend_from_slice(args);
    call(c, &format!("<init>{desc}"), &all)?;
    Ok(o)
}

/// Reads an instance field (by its metadata) of an object.
pub fn get(o: &Obj, f: &FieldMeta) -> Val {
    let i = f.index as usize;
    match f.desc.as_bytes()[0] {
        b'L' | b'[' => Val::A(o.refs[i].borrow().clone()),
        b'J' => Val::J(o.prims[i].get() as i64),
        b'F' => Val::F(f32::from_bits(o.prims[i].get() as u32)),
        b'D' => Val::D(f64::from_bits(o.prims[i].get())),
        _ => Val::I(o.prims[i].get() as i32),
    }
}

/// Writes an instance field (by its metadata) of an object.
pub fn set(o: &Obj, f: &FieldMeta, v: Val) {
    let i = f.index as usize;
    match f.desc.as_bytes()[0] {
        b'L' | b'[' => *o.refs[i].borrow_mut() = v.a(),
        b'J' => o.prims[i].set(v.j() as u64),
        b'F' => o.prims[i].set(u64::from(v.f().to_bits())),
        b'D' => o.prims[i].set(v.d().to_bits()),
        b'Z' => o.prims[i].set((v.i() & 1) as u64),
        b'B' => o.prims[i].set(v.i() as i8 as i64 as u64),
        b'C' => o.prims[i].set(v.i() as u16 as u64),
        b'S' => o.prims[i].set(v.i() as i16 as i64 as u64),
        _ => o.prims[i].set(v.i() as i64 as u64),
    }
}
