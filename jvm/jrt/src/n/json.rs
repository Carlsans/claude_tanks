//! JSON to objects and back, by the classes' field tables (`reflect`): the
//! bot API's `JsonConverter` (gson's reflection cannot be translated).
//! Fields map to keys of the same name; lists get their element type from
//! the field's generic signature; an object with a `type` key becomes the
//! subclass of that simple name (the bot API's polymorphic events); enums
//! match their `value` field or their name.
use crate::reflect::{self, Val};
use crate::*;
use serde_json::Value as J;

fn err(msg: &str) -> Throw {
    throw(&C_java_lang_IllegalArgumentException, msg)
}

fn class_of(r: &Ref) -> JResult<&'static Class> {
    match &nn(r)?.ext {
        Ext::Class(c) => Ok(c),
        _ => Err(err("not a class object")),
    }
}

/// The type arguments of a generic signature, as (erased descriptor,
/// generic signature) pairs: `Ljava/util/List<LFoo;>;` gives [(`LFoo;`, ..)].
fn type_args(generic: &str) -> Vec<(String, String)> {
    let Some(open) = generic.find('<') else {
        return Vec::new();
    };
    let inner = &generic[open + 1..generic.rfind('>').unwrap_or(generic.len())];
    let mut out = Vec::new();
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        while i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if i < b.len() && b[i] == b'*' {
            out.push(("Ljava/lang/Object;".to_string(), String::new()));
            i += 1;
            continue;
        }
        let body_start = i;
        let mut depth = 0;
        while i < b.len() {
            match b[i] {
                b'<' => depth += 1,
                b'>' => depth -= 1,
                b';' if depth == 0 => {
                    i += 1;
                    break;
                }
                b'[' | b'L' | b'T' if depth == 0 && i == body_start => {}
                c if depth == 0 && i == body_start && c.is_ascii_uppercase() => {
                    i += 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        let g = &inner[body_start..i];
        let erased = match g.find('<') {
            Some(p) => format!("{};", &g[..p]),
            None => g.to_string(),
        };
        let erased = if erased.starts_with('T') {
            "Ljava/lang/Object;".to_string()
        } else {
            erased
        };
        out.push((erased, g.to_string()));
        let _ = start;
    }
    out
}

fn num(v: &J) -> f64 {
    match v {
        J::Number(n) => n.as_f64().unwrap_or(0.0),
        J::Bool(b) => f64::from(u8::from(*b)),
        J::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn int(v: &J) -> i64 {
    match v {
        J::Number(n) => n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
        _ => num(v) as i64,
    }
}

fn new_of(name: &str) -> JResult<(&'static Class, Ref)> {
    let c = reflect::class_named(name).ok_or_else(|| err(&format!("{name} is not translated")))?;
    Ok((c, reflect::new_instance(c, "()V", &[])?))
}

fn decode(v: &J, desc: &str, generic: &str) -> JResult<Val> {
    let d = desc.as_bytes()[0];
    if v.is_null() {
        return Ok(match d {
            b'L' | b'[' => Val::A(None),
            b'J' => Val::J(0),
            b'F' => Val::F(0.0),
            b'D' => Val::D(0.0),
            _ => Val::I(0),
        });
    }
    Ok(match d {
        b'Z' => Val::I(match v {
            J::Bool(b) => i32::from(*b),
            _ => i32::from(num(v) != 0.0),
        }),
        b'B' | b'C' | b'S' | b'I' => Val::I(int(v) as i32),
        b'J' => Val::J(int(v)),
        b'F' => Val::F(num(v) as f32),
        b'D' => Val::D(num(v)),
        b'[' => {
            let c = reflect::class_named(desc).ok_or_else(|| err(desc))?;
            let items = v.as_array().cloned().unwrap_or_default();
            let a = new_array_of(c, items.len() as i32)?;
            let elem = &desc[1..];
            for (i, x) in items.iter().enumerate() {
                let e = decode(x, elem, "")?;
                let i = i as i32;
                match elem.as_bytes()[0] {
                    b'Z' | b'B' => ast_b(&a, i, e.i() as i8)?,
                    b'C' => ast_c(&a, i, e.i() as u16)?,
                    b'S' => ast_s(&a, i, e.i() as i16)?,
                    b'I' => ast_i(&a, i, e.i())?,
                    b'J' => ast_j(&a, i, e.j())?,
                    b'F' => ast_f(&a, i, e.f())?,
                    b'D' => ast_d(&a, i, e.d())?,
                    _ => ast_a(&a, i, e.a())?,
                }
            }
            Val::A(a)
        }
        _ => Val::A(decode_ref(v, &desc[1..desc.len() - 1], generic)?),
    })
}

fn decode_ref(v: &J, name: &str, generic: &str) -> JResult<Ref> {
    Ok(match name {
        "java/lang/String" | "java/lang/CharSequence" => match v {
            J::String(s) => jstr(s),
            other => jstr(&other.to_string()),
        },
        "java/lang/Integer" | "java/lang/Short" | "java/lang/Byte" => {
            n::boxes::integer::value_of(int(v) as i32)?
        }
        "java/lang/Long" => n::boxes::long::value_of(int(v))?,
        "java/lang/Double" => n::boxes::double::value_of(num(v))?,
        "java/lang/Float" => n::boxes::float::value_of(num(v) as f32)?,
        "java/lang/Boolean" => {
            n::boxes::boolean::value_of(i32::from(v.as_bool().unwrap_or(num(v) != 0.0)))?
        }
        "java/lang/Object" | "java/lang/Number" => match v {
            J::String(s) => jstr(s),
            J::Bool(b) => n::boxes::boolean::value_of(i32::from(*b))?,
            J::Number(n) => n::boxes::double::value_of(n.as_f64().unwrap_or(0.0))?,
            other => jstr(&other.to_string()),
        },
        "java/util/List"
        | "java/util/Collection"
        | "java/util/ArrayList"
        | "java/lang/Iterable" => collection(v, "java/util/ArrayList", generic)?,
        "java/util/Set" | "java/util/HashSet" | "java/util/LinkedHashSet" => {
            collection(v, "java/util/LinkedHashSet", generic)?
        }
        "java/util/Map" | "java/util/HashMap" | "java/util/LinkedHashMap" => {
            let (_, m) = new_of("java/util/LinkedHashMap")?;
            let args = type_args(generic);
            let (vd, vg) = args
                .get(1)
                .cloned()
                .unwrap_or(("Ljava/lang/Object;".into(), String::new()));
            if let Some(o) = v.as_object() {
                for (k, x) in o {
                    let val = decode(x, &vd, &vg)?;
                    reflect::call_virtual(
                        &m,
                        "put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                        &[Val::A(jstr(k)), val],
                    )?;
                }
            }
            m
        }
        _ => {
            let c = reflect::class_named(name)
                .ok_or_else(|| err(&format!("{name} is not translated")))?;
            object(v, c)?
        }
    })
}

fn collection(v: &J, class: &str, generic: &str) -> JResult<Ref> {
    let (_, list) = new_of(class)?;
    let (ed, eg) = type_args(generic)
        .into_iter()
        .next()
        .unwrap_or(("Ljava/lang/Object;".into(), String::new()));
    for x in v.as_array().cloned().unwrap_or_default() {
        let e = decode(&x, &ed, &eg)?;
        reflect::call_virtual(&list, "add(Ljava/lang/Object;)Z", &[e])?;
    }
    Ok(list)
}

fn is_enum(c: &Class) -> bool {
    c.values.is_some() || c.sup.is_some_and(|s| s.name == "java/lang/Enum")
}

thread_local! {
    /// Enum constants by class and JSON text, and classes' instance fields.
    static ENUMS: RefCell<HashMap<(usize, String), Ref>> = RefCell::new(HashMap::new());
    static FIELDS: RefCell<HashMap<usize, Rc<Vec<&'static reflect::FieldMeta>>>> = RefCell::new(HashMap::new());
}

fn fields_of(c: &'static Class) -> Rc<Vec<&'static reflect::FieldMeta>> {
    let key = c as *const Class as usize;
    FIELDS.with(|f| {
        f.borrow_mut()
            .entry(key)
            .or_insert_with(|| Rc::new(reflect::instance_fields(c)))
            .clone()
    })
}

/// The enum constant whose `value` field (or name) is `s` (cached: enum
/// constants never change).
fn enum_constant(c: &'static Class, s: &str) -> JResult<Ref> {
    let key = (c as *const Class as usize, s.to_string());
    if let Some(e) = ENUMS.with(|m| m.borrow().get(&key).cloned()) {
        return Ok(e);
    }
    let e = find_enum_constant(c, s)?;
    ENUMS.with(|m| m.borrow_mut().insert(key, e.clone()));
    Ok(e)
}

fn find_enum_constant(c: &'static Class, s: &str) -> JResult<Ref> {
    let Some(values) = c.values else {
        return Err(err(&format!("{} has no values()", c.name)));
    };
    let all = values()?;
    let value_field =
        reflect::field(c, "value").filter(|f| !f.is_static && f.desc == "Ljava/lang/String;");
    if let Ext::Refs(v) = &nn(&all)?.ext {
        for e in v.borrow().iter().flatten() {
            let text = match value_field {
                Some(f) => reflect::get(e, f).a(),
                None => e.refs[0].borrow().clone(),
            };
            if text.as_ref().is_some_and(|t| rust_string(t) == s) {
                return Ok(Some(e.clone()));
            }
        }
    }
    Ok(None)
}

fn object(v: &J, declared: &'static Class) -> JResult<Ref> {
    if is_enum(declared) {
        return enum_constant(declared, v.as_str().unwrap_or(""));
    }
    let mut c = declared;
    if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
        let pkg = declared.name.rsplit_once('/').map_or("", |(p, _)| p);
        if let Some(sub) = reflect::class_named(&format!("{pkg}/{t}"))
            && is_assignable(sub, declared)
        {
            c = sub;
        }
    }
    let o = if reflect::method(c, "<init>()V").is_some() {
        reflect::new_instance(c, "()V", &[])?
    } else {
        alloc(c)
    };
    let obj = nn(&o)?.clone();
    for f in fields_of(c).iter() {
        if let Some(x) = v.get(f.name) {
            reflect::set(&obj, f, decode(x, f.desc, f.generic)?);
        }
    }
    Ok(o)
}

fn encode(r: &Ref) -> JResult<J> {
    let Some(o) = r else {
        return Ok(J::Null);
    };
    let bits = || o.prims.first().map_or(0, |c| c.get());
    if is_gson_tree(o.class) {
        let text = call_to_string(o)?;
        let t = text.as_ref().map_or(String::new(), |t| rust_string(t));
        return serde_json::from_str(&t).map_err(|e| err(&e.to_string()));
    }
    Ok(match o.class.name {
        "java/lang/String" => J::String(rust_string(o)),
        "java/lang/Integer" => J::from(bits() as i32),
        "java/lang/Long" => J::from(bits() as i64),
        "java/lang/Double" => J::from(f64::from_bits(bits())),
        "java/lang/Float" => J::from(f64::from(f32::from_bits(bits() as u32))),
        "java/lang/Boolean" => J::Bool(bits() != 0),
        "java/lang/Character" => J::String(String::from_utf16_lossy(&[bits() as u16])),
        _ if is_enum(o.class) || o.class.sup.is_some_and(is_enum) => {
            let c = if is_enum(o.class) {
                o.class
            } else {
                o.class.sup.unwrap()
            };
            let text = match reflect::field(c, "value")
                .filter(|f| !f.is_static && f.desc == "Ljava/lang/String;")
            {
                Some(f) => reflect::get(o, f).a(),
                None => o.refs[0].borrow().clone(),
            };
            text.map_or(J::Null, |t| J::String(rust_string(&t)))
        }
        _ => match &o.ext {
            Ext::Refs(v) => J::Array(v.borrow().iter().map(encode).collect::<JResult<_>>()?),
            Ext::Ints(v) => J::from(v.borrow().clone()),
            Ext::Longs(v) => J::from(v.borrow().clone()),
            Ext::Doubles(v) => J::from(v.borrow().clone()),
            Ext::Floats(v) => J::from(v.borrow().iter().map(|x| f64::from(*x)).collect::<Vec<_>>()),
            Ext::Bytes(v) => J::from(v.borrow().iter().map(|x| i32::from(*x)).collect::<Vec<_>>()),
            _ if reflect::class_named("java/util/Collection")
                .is_some_and(|c| is_assignable(o.class, c)) =>
            {
                let a = reflect::call_virtual(r, "toArray()[Ljava/lang/Object;", &[])?.a();
                encode(&a)?
            }
            _ => {
                let mut m = serde_json::Map::new();
                for f in fields_of(o.class).iter() {
                    let v = match f.desc.as_bytes()[0] {
                        b'L' | b'[' => encode(&reflect::get(o, f).a())?,
                        b'Z' => J::Bool(reflect::get(o, f).i() != 0),
                        b'J' => J::from(reflect::get(o, f).j()),
                        b'F' | b'D' => J::from(reflect::get(o, f).d()),
                        _ => J::from(reflect::get(o, f).i()),
                    };
                    if !v.is_null() {
                        m.insert(f.name.to_string(), v);
                    }
                }
                J::Object(m)
            }
        },
    })
}

/// gson's own tree types (`JsonObject`, `JsonElement`...).
fn is_gson_tree(c: &Class) -> bool {
    c.name.starts_with("com/google/gson/Json")
}

/// A gson tree of a JSON value, built through gson's own classes (numbers
/// as Integer, Long or Double).
fn gson_tree(v: &J) -> JResult<Ref> {
    Ok(match v {
        J::Null => new_of("com/google/gson/JsonNull")?.1,
        J::Bool(b) => {
            let c = reflect::class_named("com/google/gson/JsonPrimitive")
                .ok_or_else(|| err("JsonPrimitive"))?;
            reflect::new_instance(
                c,
                "(Ljava/lang/Boolean;)V",
                &[Val::A(n::boxes::boolean::value_of(i32::from(*b))?)],
            )?
        }
        J::Number(x) => {
            let c = reflect::class_named("com/google/gson/JsonPrimitive")
                .ok_or_else(|| err("JsonPrimitive"))?;
            let num = match (x.as_i64(), x.as_f64()) {
                (Some(i), _) if i32::try_from(i).is_ok() => n::boxes::integer::value_of(i as i32)?,
                (Some(i), _) => n::boxes::long::value_of(i)?,
                (None, f) => n::boxes::double::value_of(f.unwrap_or(0.0))?,
            };
            reflect::new_instance(c, "(Ljava/lang/Number;)V", &[Val::A(num)])?
        }
        J::String(t) => {
            let c = reflect::class_named("com/google/gson/JsonPrimitive")
                .ok_or_else(|| err("JsonPrimitive"))?;
            reflect::new_instance(c, "(Ljava/lang/String;)V", &[Val::A(jstr(t))])?
        }
        J::Array(items) => {
            let (_, a) = new_of("com/google/gson/JsonArray")?;
            for x in items {
                let e = gson_tree(x)?;
                reflect::call_virtual(&a, "add(Lcom/google/gson/JsonElement;)V", &[Val::A(e)])?;
            }
            a
        }
        J::Object(m) => {
            let (_, o) = new_of("com/google/gson/JsonObject")?;
            for (k, x) in m {
                let e = gson_tree(x)?;
                reflect::call_virtual(
                    &o,
                    "add(Ljava/lang/String;Lcom/google/gson/JsonElement;)V",
                    &[Val::A(jstr(k)), Val::A(e)],
                )?;
            }
            o
        }
    })
}

/// `JsonConverter.fromJson(String, Class)`.
pub fn from_string(text: Ref, class: Ref) -> JResult<Ref> {
    let s = String::from_utf16_lossy(n::string::of(&text)?);
    let v: J = serde_json::from_str(&s).map_err(|e| err(&e.to_string()))?;
    let c = class_of(&class)?;
    if is_gson_tree(c) {
        return gson_tree(&v);
    }
    object(&v, c)
}

/// `JsonConverter.fromJson(JsonObject, Class)`: from the value `ws_on_text`
/// decoded, or else via the tree's text.
pub fn from_tree(tree: Ref, class: Ref) -> JResult<Ref> {
    if let Some(o) = &tree
        && let Ext::Any(a) = &o.ext
        && let Some(v) = a.borrow().downcast_ref::<J>()
    {
        let c = class_of(&class)?;
        return if is_gson_tree(c) {
            gson_tree(v)
        } else {
            object(v, c)
        };
    }
    let text = call_to_string(nn(&tree)?)?;
    from_string(text, class)
}

/// The bot API's handler for each message type (`WebSocketHandler`).
const HANDLERS: &[(&str, &str)] = &[
    (
        "TickEventForBot",
        "handleTick(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "RoundStartedEvent",
        "handleRoundStarted(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "RoundEndedEventForBot",
        "handleRoundEnded(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "GameStartedEventForBot",
        "handleGameStarted(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "GameEndedEventForBot",
        "handleGameEnded(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "SkippedTurnEvent",
        "handleSkippedTurn(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "ServerHandshake",
        "handleServerHandshake(Lcom/google/gson/JsonObject;)V",
    ),
];

/// `WebSocketHandler.onText`: decodes the message once (no gson tree) and
/// calls the bot API's handler for its type, with a stand-in for the
/// `JsonObject` that `from_tree` reads the value from.
pub fn ws_on_text(this: Ref, _ws: Ref, data: Ref, _last: i32) -> JResult<Ref> {
    let text = String::from_utf16_lossy(&n::string::seq_chars(&data)?);
    let v: J = serde_json::from_str(&text).map_err(|e| err(&e.to_string()))?;
    let ty = v
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let class = nn(&this)?.class;
    if ty == "GameAbortedEvent" {
        reflect::call(class, "handleGameAborted()V", &[Val::A(this)])?;
        return Ok(None);
    }
    if let Some((_, sig)) = HANDLERS.iter().find(|(t, _)| *t == ty) {
        let holder = alloc(&C_jrt_JsonValue);
        if let Some(o) = &holder
            && let Ext::Any(a) = &o.ext
        {
            *a.borrow_mut() = Box::new(v);
        }
        reflect::call(class, sig, &[Val::A(this), Val::A(holder)])?;
    }
    Ok(None)
}

pub fn value_ext() -> Ext {
    Ext::Any(RefCell::new(Box::new(J::Null)))
}

/// `JsonConverter.toJson(Object)`.
pub fn to_json(o: Ref) -> JResult<Ref> {
    Ok(jstr(&encode(&o)?.to_string()))
}
