//! Runtime for JVM bytecode translated to Rust by `jvm2rs`.
//!
//! Objects are reference counted (`Rc`) and live on one thread: a translated
//! program (a robot) runs on a thread of its own, and its static fields are
//! thread locals, so several copies can run side by side without sharing
//! anything, as robots loaded by separate class loaders do in Robocode.
//!
//! Classes are `Class` statics with method tables (`vtable`, one slot per
//! overridable method, inherited slots first) and interface tables
//! (`itable`), so code translated separately (the Java library once, each
//! robot on its own) can call into each other.

pub mod fmt;
pub mod n;
pub mod reflect;
pub mod robot;
pub mod spec;
pub mod thread;

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

/// A Java reference: `None` is `null`.
pub type Ref = Option<Rc<Obj>>;

/// A thrown `java.lang.Throwable`.
pub struct Throw(pub Rc<Obj>);

pub type JResult<T> = Result<T, Throw>;

/// A function pointer in a method table, cast to the method's own signature
/// at the call site. Null for abstract methods.
#[derive(Clone, Copy)]
pub struct Fp(pub *const ());

// Method tables only hold function pointers, which any thread may call.
unsafe impl Sync for Fp {}
unsafe impl Send for Fp {}

/// What an array holds.
#[derive(Clone, Copy)]
pub enum Component {
    /// Not an array.
    None,
    /// A primitive type, by descriptor letter (`I`, `D`, ...).
    Prim(u8),
    Ref(&'static Class),
}

pub struct Class {
    /// Internal name: `java/lang/String`, `[I`, `[Ljava/lang/Object;`.
    pub name: &'static str,
    pub sup: Option<&'static Class>,
    /// Every interface it implements (including superinterfaces), not those
    /// of its superclasses.
    pub ifaces: &'static [&'static Class],
    pub interface: bool,
    /// Instance fields: primitives (as bits) and references, superclasses'
    /// first.
    pub prims: u32,
    pub refs: u32,
    pub vtable: &'static [Fp],
    /// Per interface it implements (also through superclasses), its methods
    /// in the interface's declaration order.
    pub itable: &'static [(&'static Class, &'static [Fp])],
    /// Makes the extra storage of native classes (strings, builders).
    pub ext: Option<fn() -> Ext>,
    pub component: Component,
    /// Enum classes: their `values()`, for `Enum.valueOf`.
    pub values: Option<fn() -> JResult<Ref>>,
    /// Fields and methods declared here, for reflection (`reflect`).
    pub fields: &'static [reflect::FieldMeta],
    pub methods: &'static [reflect::MethodMeta],
}

#[allow(non_upper_case_globals)]
pub mod classes {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/classes.rs"));
}
pub use classes::*;

pub struct Obj {
    pub class: &'static Class,
    pub prims: Box<[Cell<u64>]>,
    pub refs: Box<[RefCell<Ref>]>,
    pub ext: Ext,
}

/// Storage beyond fields: arrays and the native classes' state.
pub enum Ext {
    None,
    /// `byte[]` and `boolean[]`.
    Bytes(RefCell<Vec<i8>>),
    Chars(RefCell<Vec<u16>>),
    Shorts(RefCell<Vec<i16>>),
    Ints(RefCell<Vec<i32>>),
    Longs(RefCell<Vec<i64>>),
    Floats(RefCell<Vec<f32>>),
    Doubles(RefCell<Vec<f64>>),
    Refs(RefCell<Vec<Ref>>),
    /// `java.lang.String`: UTF-16, set once by the constructor.
    Str(OnceCell<Box<[u16]>>),
    /// `StringBuilder`, `StringBuffer`.
    Buf(RefCell<Vec<u16>>),
    /// `java.lang.Class`.
    Class(&'static Class),
    Any(RefCell<Box<dyn std::any::Any>>),
}

// ---------------------------------------------------------------- objects

/// A new instance with every field zero or null (`new`).
pub fn alloc(c: &'static Class) -> Ref {
    Some(Rc::new(Obj {
        class: c,
        prims: (0..c.prims).map(|_| Cell::new(0)).collect(),
        refs: (0..c.refs).map(|_| RefCell::new(None)).collect(),
        ext: c.ext.map_or(Ext::None, |f| f()),
    }))
}

/// Null check.
#[inline]
pub fn nn(r: &Ref) -> JResult<&Rc<Obj>> {
    r.as_ref().ok_or_else(npe)
}

#[inline]
pub fn same(a: &Ref, b: &Ref) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

pub fn identity_hash(o: &Rc<Obj>) -> i32 {
    let a = Rc::as_ptr(o) as usize as u64;
    let h = a.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    ((h >> 33) as i32) & 0x7fff_ffff
}

// Instance fields. Primitives are kept as bits: ints sign-extended, floats
// and doubles by their IEEE bits.

#[inline]
pub fn gf_i(r: &Ref, i: usize) -> JResult<i32> {
    Ok(nn(r)?.prims[i].get() as i32)
}
#[inline]
pub fn gf_j(r: &Ref, i: usize) -> JResult<i64> {
    Ok(nn(r)?.prims[i].get() as i64)
}
#[inline]
pub fn gf_f(r: &Ref, i: usize) -> JResult<f32> {
    Ok(f32::from_bits(nn(r)?.prims[i].get() as u32))
}
#[inline]
pub fn gf_d(r: &Ref, i: usize) -> JResult<f64> {
    Ok(f64::from_bits(nn(r)?.prims[i].get()))
}
#[inline]
pub fn gf_a(r: &Ref, i: usize) -> JResult<Ref> {
    Ok(nn(r)?.refs[i].borrow().clone())
}
#[inline]
pub fn pf_i(r: &Ref, i: usize, v: i32) -> JResult<()> {
    nn(r)?.prims[i].set(v as i64 as u64);
    Ok(())
}
#[inline]
pub fn pf_j(r: &Ref, i: usize, v: i64) -> JResult<()> {
    nn(r)?.prims[i].set(v as u64);
    Ok(())
}
#[inline]
pub fn pf_f(r: &Ref, i: usize, v: f32) -> JResult<()> {
    nn(r)?.prims[i].set(u64::from(v.to_bits()));
    Ok(())
}
#[inline]
pub fn pf_d(r: &Ref, i: usize, v: f64) -> JResult<()> {
    nn(r)?.prims[i].set(v.to_bits());
    Ok(())
}
#[inline]
pub fn pf_a(r: &Ref, i: usize, v: Ref) -> JResult<()> {
    *nn(r)?.refs[i].borrow_mut() = v;
    Ok(())
}

// ---------------------------------------------------------------- arrays

macro_rules! prim_array_classes {
    ($($id:ident $code:literal $name:literal),*) => {
        $(pub static $id: Class = Class {
            name: $name,
            sup: Some(&C_java_lang_Object),
            ifaces: &[&C_java_lang_Cloneable, &C_java_io_Serializable],
            interface: false,
            prims: 0,
            refs: 0,
            vtable: &OBJECT_VTABLE,
            itable: &[],
            ext: None,
            component: Component::Prim($code),
            values: None,
            fields: &[],
            methods: &[],
        };)*
        /// The class of arrays of a primitive type (descriptor letter).
        pub fn prim_array_class(code: u8) -> &'static Class {
            match code {
                $($code => &$id,)*
                _ => panic!("no primitive type {}", code as char),
            }
        }
    };
}

/// The class object of `int` (`Integer.TYPE`): a class with no instances.
pub static PRIM_INT: Class = Class {
    name: "int",
    sup: None,
    ifaces: &[],
    interface: false,
    prims: 0,
    refs: 0,
    vtable: &[],
    itable: &[],
    ext: None,
    component: Component::None,
    values: None,
    fields: &[],
    methods: &[],
};

pub static OBJECT_VTABLE: [Fp; 5] = [
    Fp(n::object::equals as *const ()),
    Fp(n::object::hash_code as *const ()),
    Fp(n::object::to_string as *const ()),
    Fp(n::object::clone as *const ()),
    Fp(n::object::finalize as *const ()),
];

prim_array_classes!(
    ARRAY_Z b'Z' "[Z", ARRAY_B b'B' "[B", ARRAY_C b'C' "[C", ARRAY_S b'S' "[S",
    ARRAY_I b'I' "[I", ARRAY_J b'J' "[J", ARRAY_F b'F' "[F", ARRAY_D b'D' "[D"
);

/// What every array implements.
pub static ARRAY_IFACES: [&Class; 2] = [&C_java_lang_Cloneable, &C_java_io_Serializable];

static REF_ARRAY_CLASSES: Mutex<Vec<(usize, &'static Class)>> = Mutex::new(Vec::new());

/// The class of arrays of `elem` (made once, then shared by all threads).
pub fn array_class(elem: &'static Class) -> &'static Class {
    let key = elem as *const Class as usize;
    let mut t = REF_ARRAY_CLASSES.lock().unwrap();
    if let Some(&(_, c)) = t.iter().find(|(k, _)| *k == key) {
        return c;
    }
    let name = if elem.name.starts_with('[') {
        format!("[{}", elem.name)
    } else {
        format!("[L{};", elem.name)
    };
    let c: &'static Class = Box::leak(Box::new(Class {
        name: Box::leak(name.into_boxed_str()),
        sup: Some(&C_java_lang_Object),
        ifaces: &ARRAY_IFACES,
        interface: false,
        prims: 0,
        refs: 0,
        vtable: &OBJECT_VTABLE,
        itable: &[],
        ext: None,
        component: Component::Ref(elem),
        values: None,
        fields: &[],
        methods: &[],
    }));
    t.push((key, c));
    c
}

fn array_obj(c: &'static Class, ext: Ext) -> Ref {
    Some(Rc::new(Obj {
        class: c,
        prims: Box::new([]),
        refs: Box::new([]),
        ext,
    }))
}

/// `newarray`: an array of a primitive type, all zeros.
pub fn newarray(code: u8, n: i32) -> JResult<Ref> {
    if n < 0 {
        return Err(throw(
            &C_java_lang_NegativeArraySizeException,
            &n.to_string(),
        ));
    }
    let n = n as usize;
    let ext = match code {
        b'Z' | b'B' => Ext::Bytes(RefCell::new(vec![0; n])),
        b'C' => Ext::Chars(RefCell::new(vec![0; n])),
        b'S' => Ext::Shorts(RefCell::new(vec![0; n])),
        b'I' => Ext::Ints(RefCell::new(vec![0; n])),
        b'J' => Ext::Longs(RefCell::new(vec![0; n])),
        b'F' => Ext::Floats(RefCell::new(vec![0.0; n])),
        b'D' => Ext::Doubles(RefCell::new(vec![0.0; n])),
        _ => panic!("no primitive type {}", code as char),
    };
    Ok(array_obj(prim_array_class(code), ext))
}

/// `anewarray`: an array of references, all null.
pub fn anewarray(elem: &'static Class, n: i32) -> JResult<Ref> {
    if n < 0 {
        return Err(throw(
            &C_java_lang_NegativeArraySizeException,
            &n.to_string(),
        ));
    }
    Ok(array_obj(
        array_class(elem),
        Ext::Refs(RefCell::new(vec![None; n as usize])),
    ))
}

/// An array whose class is `c` (any array class).
pub fn new_array_of(c: &'static Class, n: i32) -> JResult<Ref> {
    match c.component {
        Component::Prim(code) => newarray(code, n),
        Component::Ref(e) => anewarray(e, n),
        Component::None => panic!("{} is not an array class", c.name),
    }
}

/// `multianewarray`: `c` is the array class, `dims` the leading lengths.
pub fn multianewarray(c: &'static Class, dims: &[i32]) -> JResult<Ref> {
    if let Some(&n) = dims.iter().find(|&&n| n < 0) {
        return Err(throw(
            &C_java_lang_NegativeArraySizeException,
            &n.to_string(),
        ));
    }
    let a = new_array_of(c, dims[0])?;
    if dims.len() > 1
        && let Component::Ref(e) = c.component
        && let Ext::Refs(v) = &nn(&a)?.ext
    {
        for slot in v.borrow_mut().iter_mut() {
            *slot = multianewarray(e, &dims[1..])?;
        }
    }
    Ok(a)
}

pub fn alen(r: &Ref) -> JResult<i32> {
    let o = nn(r)?;
    Ok(match &o.ext {
        Ext::Bytes(v) => v.borrow().len(),
        Ext::Chars(v) => v.borrow().len(),
        Ext::Shorts(v) => v.borrow().len(),
        Ext::Ints(v) => v.borrow().len(),
        Ext::Longs(v) => v.borrow().len(),
        Ext::Floats(v) => v.borrow().len(),
        Ext::Doubles(v) => v.borrow().len(),
        Ext::Refs(v) => v.borrow().len(),
        _ => panic!("arraylength on {}", o.class.name),
    } as i32)
}

fn aioobe(i: i32, len: usize) -> Throw {
    throw(
        &C_java_lang_ArrayIndexOutOfBoundsException,
        &format!("Index {i} out of bounds for length {len}"),
    )
}

macro_rules! array_access {
    ($load:ident $store:ident $variant:ident $t:ty) => {
        #[inline]
        pub fn $load(r: &Ref, i: i32) -> JResult<$t> {
            match &nn(r)?.ext {
                Ext::$variant(v) => {
                    let v = v.borrow();
                    v.get(i as usize).cloned().ok_or_else(|| aioobe(i, v.len()))
                }
                _ => panic!("array type mismatch"),
            }
        }
        #[inline]
        pub fn $store(r: &Ref, i: i32, x: $t) -> JResult<()> {
            match &nn(r)?.ext {
                Ext::$variant(v) => {
                    let mut v = v.borrow_mut();
                    let n = v.len();
                    *v.get_mut(i as usize).ok_or_else(|| aioobe(i, n))? = x;
                    Ok(())
                }
                _ => panic!("array type mismatch"),
            }
        }
    };
}

array_access!(ald_b ast_b Bytes i8);
array_access!(ald_c ast_c Chars u16);
array_access!(ald_s ast_s Shorts i16);
array_access!(ald_i ast_i Ints i32);
array_access!(ald_j ast_j Longs i64);
array_access!(ald_f ast_f Floats f32);
array_access!(ald_d ast_d Doubles f64);

#[inline]
pub fn ald_a(r: &Ref, i: i32) -> JResult<Ref> {
    match &nn(r)?.ext {
        Ext::Refs(v) => {
            let v = v.borrow();
            v.get(i as usize).cloned().ok_or_else(|| aioobe(i, v.len()))
        }
        _ => panic!("array type mismatch"),
    }
}

/// `aastore`, with the store check (the value must fit the array's class).
#[inline]
pub fn ast_a(r: &Ref, i: i32, x: Ref) -> JResult<()> {
    let o = nn(r)?;
    if let (Some(x), Component::Ref(e)) = (&x, o.class.component)
        && !is_assignable(x.class, e)
    {
        return Err(throw(&C_java_lang_ArrayStoreException, x.class.name));
    }
    match &o.ext {
        Ext::Refs(v) => {
            let mut v = v.borrow_mut();
            let n = v.len();
            *v.get_mut(i as usize).ok_or_else(|| aioobe(i, n))? = x;
            Ok(())
        }
        _ => panic!("array type mismatch"),
    }
}

/// A shallow copy (`Object.clone` of arrays and `Cloneable` objects).
pub fn shallow_clone(o: &Rc<Obj>) -> Rc<Obj> {
    let ext = match &o.ext {
        Ext::None => Ext::None,
        Ext::Bytes(v) => Ext::Bytes(v.clone()),
        Ext::Chars(v) => Ext::Chars(v.clone()),
        Ext::Shorts(v) => Ext::Shorts(v.clone()),
        Ext::Ints(v) => Ext::Ints(v.clone()),
        Ext::Longs(v) => Ext::Longs(v.clone()),
        Ext::Floats(v) => Ext::Floats(v.clone()),
        Ext::Doubles(v) => Ext::Doubles(v.clone()),
        Ext::Refs(v) => Ext::Refs(v.clone()),
        Ext::Str(s) => Ext::Str(s.clone()),
        Ext::Buf(v) => Ext::Buf(v.clone()),
        Ext::Class(c) => Ext::Class(c),
        Ext::Any(_) => panic!("cannot clone {}", o.class.name),
    };
    Rc::new(Obj {
        class: o.class,
        prims: o.prims.clone(),
        refs: o.refs.clone(),
        ext,
    })
}

// ---------------------------------------------------------------- types

/// Whether instances of `from` are instances of `to`.
pub fn is_assignable(from: &'static Class, to: &'static Class) -> bool {
    // Every reference type is an Object, interfaces and arrays included.
    if std::ptr::eq(to, &C_java_lang_Object) {
        return true;
    }
    let mut c = Some(from);
    while let Some(k) = c {
        if std::ptr::eq(k, to) || k.ifaces.iter().any(|i| std::ptr::eq(*i, to)) {
            return true;
        }
        c = k.sup;
    }
    match (from.component, to.component) {
        (Component::Ref(f), Component::Ref(t)) => is_assignable(f, t),
        _ => false,
    }
}

#[inline]
pub fn instance_of(r: &Ref, c: &'static Class) -> bool {
    r.as_ref().is_some_and(|o| is_assignable(o.class, c))
}

#[inline]
pub fn checkcast(r: &Ref, c: &'static Class) -> JResult<()> {
    match r {
        Some(o) if !is_assignable(o.class, c) => Err(throw(
            &C_java_lang_ClassCastException,
            &format!(
                "class {} cannot be cast to class {}",
                o.class.name.replace('/', "."),
                c.name.replace('/', ".")
            ),
        )),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------- calls

/// The method in slot `slot` of an object's class.
#[inline]
pub fn vslot(o: &Rc<Obj>, slot: usize) -> JResult<*const ()> {
    match o.class.vtable.get(slot).map(|f| f.0) {
        Some(p) if !p.is_null() => Ok(p),
        _ => Err(throw(&C_java_lang_AbstractMethodError, o.class.name)),
    }
}

/// Method `idx` of interface `iface` on an object.
#[inline]
pub fn islot(o: &Rc<Obj>, iface: &'static Class, idx: usize) -> JResult<*const ()> {
    for (i, t) in o.class.itable {
        if std::ptr::eq(*i, iface) {
            let p = t[idx].0;
            return if p.is_null() {
                Err(throw(&C_java_lang_AbstractMethodError, o.class.name))
            } else {
                Ok(p)
            };
        }
    }
    Err(throw(
        &C_java_lang_IncompatibleClassChangeError,
        &format!("{} does not implement {}", o.class.name, iface.name),
    ))
}

/// Method `idx` of the interface named `iface` on an object (for the
/// runtime's own calls into translated interfaces).
pub fn islot_by_name(o: &Rc<Obj>, iface: &str, idx: usize) -> JResult<*const ()> {
    for (i, t) in o.class.itable {
        if i.name == iface {
            let p = t[idx].0;
            if !p.is_null() {
                return Ok(p);
            }
        }
    }
    Err(throw(
        &C_java_lang_IncompatibleClassChangeError,
        &format!("{} does not implement {iface}", o.class.name),
    ))
}

/// `toString()` of any object, by its method table.
pub fn call_to_string(o: &Rc<Obj>) -> JResult<Ref> {
    let f: fn(Ref) -> JResult<Ref> = unsafe { std::mem::transmute(vslot(o, 2)?) };
    f(Some(o.clone()))
}

pub fn call_hash_code(o: &Rc<Obj>) -> JResult<i32> {
    let f: fn(Ref) -> JResult<i32> = unsafe { std::mem::transmute(vslot(o, 1)?) };
    f(Some(o.clone()))
}

pub fn call_equals(o: &Rc<Obj>, other: Ref) -> JResult<bool> {
    let f: fn(Ref, Ref) -> JResult<i32> = unsafe { std::mem::transmute(vslot(o, 0)?) };
    Ok(f(Some(o.clone()), other)? != 0)
}

/// A method that could not be translated: throws when reached.
pub fn unsupported(what: &str) -> Throw {
    throw(&C_java_lang_UnsatisfiedLinkError, what)
}

// ---------------------------------------------------------------- exceptions

/// A new throwable of class `c` with a message.
pub fn throw(c: &'static Class, msg: &str) -> Throw {
    let o = alloc(c).unwrap();
    *o.refs[0].borrow_mut() = jstr(msg);
    Throw(o)
}

pub fn npe() -> Throw {
    Throw(alloc(&C_java_lang_NullPointerException).unwrap())
}

pub fn arith() -> Throw {
    throw(&C_java_lang_ArithmeticException, "/ by zero")
}

#[inline]
pub fn idiv(a: i32, b: i32) -> JResult<i32> {
    if b == 0 {
        Err(arith())
    } else {
        Ok(a.wrapping_div(b))
    }
}
#[inline]
pub fn irem(a: i32, b: i32) -> JResult<i32> {
    if b == 0 {
        Err(arith())
    } else {
        Ok(a.wrapping_rem(b))
    }
}
#[inline]
pub fn ldiv(a: i64, b: i64) -> JResult<i64> {
    if b == 0 {
        Err(arith())
    } else {
        Ok(a.wrapping_div(b))
    }
}
#[inline]
pub fn lrem(a: i64, b: i64) -> JResult<i64> {
    if b == 0 {
        Err(arith())
    } else {
        Ok(a.wrapping_rem(b))
    }
}
#[inline]
pub fn lcmp(a: i64, b: i64) -> i32 {
    (a > b) as i32 - (a < b) as i32
}
#[inline]
pub fn fcmpl(a: f32, b: f32) -> i32 {
    if a > b {
        1
    } else if a == b {
        0
    } else {
        -1
    }
}
#[inline]
pub fn fcmpg(a: f32, b: f32) -> i32 {
    if a < b {
        -1
    } else if a == b {
        0
    } else {
        1
    }
}
#[inline]
pub fn dcmpl(a: f64, b: f64) -> i32 {
    if a > b {
        1
    } else if a == b {
        0
    } else {
        -1
    }
}
#[inline]
pub fn dcmpg(a: f64, b: f64) -> i32 {
    if a < b {
        -1
    } else if a == b {
        0
    } else {
        1
    }
}

/// A readable description of a throwable: `java.lang.Foo: message`.
pub fn describe(t: &Throw) -> String {
    let name = t.0.class.name.replace('/', ".");
    match t.0.refs.first().map(|m| m.borrow().clone()) {
        Some(Some(m)) => format!("{name}: {}", rust_string(&m)),
        _ => name,
    }
}

// ---------------------------------------------------------------- strings

pub fn new_string(chars: Vec<u16>) -> Ref {
    Some(Rc::new(Obj {
        class: &C_java_lang_String,
        prims: Box::new([]),
        refs: Box::new([]),
        ext: Ext::Str(OnceCell::from(chars.into_boxed_slice())),
    }))
}

/// A Java string from a Rust one.
pub fn jstr(s: &str) -> Ref {
    new_string(s.encode_utf16().collect())
}

/// The characters of a string (an empty slice for anything else).
pub fn chars(o: &Obj) -> &[u16] {
    match &o.ext {
        Ext::Str(s) => s.get().map_or(&[], |s| s),
        _ => &[],
    }
}

pub fn rust_string(o: &Obj) -> String {
    String::from_utf16_lossy(chars(o))
}

thread_local! {
    static INTERNED: RefCell<HashMap<Box<[u16]>, Rc<Obj>>> = RefCell::new(HashMap::new());
    static LITERALS: RefCell<HashMap<usize, Rc<Obj>>> = RefCell::new(HashMap::new());
    static CLASS_OBJECTS: RefCell<HashMap<usize, Rc<Obj>>> = RefCell::new(HashMap::new());
}

/// The interned string with these characters.
pub fn intern(s: &[u16]) -> Rc<Obj> {
    INTERNED.with(|t| {
        t.borrow_mut()
            .entry(s.into())
            .or_insert_with(|| new_string(s.to_vec()).unwrap())
            .clone()
    })
}

/// A string literal (`ldc`): interned, so equal literals are the same object.
#[inline]
pub fn lit(s: &'static [u16]) -> Ref {
    let key = s.as_ptr() as usize;
    Some(LITERALS.with(|t| {
        if let Some(o) = t.borrow().get(&key) {
            return o.clone();
        }
        let o = intern(s);
        t.borrow_mut().insert(key, o.clone());
        o
    }))
}

static MISSING_CLASSES: Mutex<Vec<&'static Class>> = Mutex::new(Vec::new());

/// A stand-in for a class the translation left out, so that `X.class`
/// still names something (it has no instances, so comparisons and type
/// checks with it fail as they should).
pub fn missing_class(name: &'static str) -> &'static Class {
    let mut t = MISSING_CLASSES.lock().unwrap();
    if let Some(c) = t.iter().find(|c| c.name == name) {
        return c;
    }
    let c: &'static Class = Box::leak(Box::new(Class {
        name,
        sup: Some(&C_java_lang_Object),
        ifaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        vtable: &OBJECT_VTABLE,
        itable: &[],
        ext: None,
        component: Component::None,
        values: None,
        fields: &[],
        methods: &[],
    }));
    t.push(c);
    c
}

/// The `java.lang.Class` object of a class (`ldc` of a class, `getClass`).
pub fn class_object(c: &'static Class) -> Ref {
    let key = c as *const Class as usize;
    Some(CLASS_OBJECTS.with(|t| {
        t.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                Rc::new(Obj {
                    class: &C_java_lang_Class,
                    prims: Box::new([]),
                    refs: Box::new([]),
                    ext: Ext::Class(c),
                })
            })
            .clone()
    }))
}

// ---------------------------------------------------------------- output

thread_local! {
    static PRINT: Cell<bool> = const { Cell::new(true) };
}

/// Whether `System.out` and `System.err` print (to the process's stdout and
/// stderr) on this thread. Robots print a lot; battles turn it off.
pub fn set_printing(on: bool) {
    PRINT.with(|p| p.set(on));
}

pub fn printing() -> bool {
    PRINT.with(|p| p.get())
}

/// Runs a translated `main(String[])` as the main thread (and the threads
/// it starts) and reports an uncaught exception as the JVM does. Returns the
/// exit status.
pub fn run_main(main: fn(Ref) -> JResult<()>) -> i32 {
    let failed = thread::run_main_thread(Box::new(move || {
        let args = anewarray(&C_java_lang_String, 0)?;
        main(args)
    }));
    match failed {
        None => 0,
        Some(t) => {
            use std::io::Write;
            let _ = std::io::stdout().flush();
            eprintln!("Exception in thread \"main\" {}", describe(&t));
            1
        }
    }
}

// ---------------------------------------------------------------- lambdas

/// Appends `String.valueOf(r)` (string concatenation).
pub fn concat_any(b: &mut Vec<u16>, r: &Ref) -> JResult<()> {
    b.extend(n::string::value_of_any(r)?);
    Ok(())
}

pub fn concat_str(b: &mut Vec<u16>, s: &str) {
    b.extend(s.encode_utf16());
}

fn unbox_bits(r: &Ref) -> JResult<(&'static str, u64)> {
    let o = nn(r)?;
    Ok((o.class.name, o.prims.first().map_or(0, |c| c.get())))
}

/// The primitive inside a box, converted as a lambda's arguments are.
pub fn unbox_i(r: &Ref) -> JResult<i32> {
    let (c, b) = unbox_bits(r)?;
    Ok(match c {
        "java/lang/Boolean" => (b != 0) as i32,
        "java/lang/Character" => b as u16 as i32,
        _ => b as i32,
    })
}

pub fn unbox_j(r: &Ref) -> JResult<i64> {
    let (c, b) = unbox_bits(r)?;
    Ok(match c {
        "java/lang/Long" => b as i64,
        "java/lang/Character" => i64::from(b as u16),
        _ => i64::from(b as i32),
    })
}

pub fn unbox_d(r: &Ref) -> JResult<f64> {
    let (c, b) = unbox_bits(r)?;
    Ok(match c {
        "java/lang/Double" => f64::from_bits(b),
        "java/lang/Float" => f64::from(f32::from_bits(b as u32)),
        "java/lang/Long" => b as i64 as f64,
        "java/lang/Character" => f64::from(b as u16),
        _ => f64::from(b as i32),
    })
}

pub fn unbox_f(r: &Ref) -> JResult<f32> {
    let (c, b) = unbox_bits(r)?;
    Ok(match c {
        "java/lang/Float" => f32::from_bits(b as u32),
        "java/lang/Double" => f64::from_bits(b) as f32,
        "java/lang/Long" => b as i64 as f32,
        _ => b as i32 as f32,
    })
}

/// Boxes a primitive by its descriptor letter (bytes and shorts as
/// integers: their boxes are not modelled).
pub fn box_i(code: u8, v: i32) -> JResult<Ref> {
    match code {
        b'Z' => n::boxes::boolean::value_of(v),
        b'C' => n::boxes::character::value_of(v),
        _ => n::boxes::integer::value_of(v),
    }
}

pub fn box_j(v: i64) -> JResult<Ref> {
    n::boxes::long::value_of(v)
}

pub fn box_f(v: f32) -> JResult<Ref> {
    n::boxes::float::value_of(v)
}

pub fn box_d(v: f64) -> JResult<Ref> {
    n::boxes::double::value_of(v)
}
