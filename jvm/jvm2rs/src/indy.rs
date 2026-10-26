//! `invokedynamic` call sites javac emits: string concatenation (Java 9+,
//! `StringConcatFactory`) and lambdas and method references
//! (`LambdaMetafactory`). Each lambda site becomes a small class of its own
//! that holds the captured values and forwards the interface method to the
//! target, boxing, unboxing and widening as the metafactory would.

use crate::classfile::{ClassFile, Const, MemberRef};

pub struct Lambda {
    /// The functional interface (the call site's return type).
    pub iface: String,
    /// The interface method's name and erased descriptor.
    pub name: String,
    pub sam_desc: String,
    /// Further descriptors the same method answers to (bridges).
    pub bridges: Vec<String>,
    /// Extra interfaces (`Serializable` and other markers).
    pub markers: Vec<String>,
    /// Captured values: the call site's parameters.
    pub captured: Vec<String>,
    /// Method handle kind (5 virtual, 6 static, 7 special, 8 new, 9 interface)
    /// and target.
    pub kind: u8,
    pub target: MemberRef,
}

pub enum Site {
    /// Pieces: literal text or an argument (by position) with its descriptor.
    Concat(Vec<Piece>),
    Lambda(Lambda),
    Unsupported(String),
}

pub enum Piece {
    Text(Vec<u16>),
    Arg(usize, String),
}

fn method_type(cf: &ClassFile, i: u16) -> Option<String> {
    match cf.pool.get(i as usize)? {
        Const::MethodType(d) => cf.utf8(*d).ok(),
        _ => None,
    }
}

fn handle(cf: &ClassFile, i: u16) -> Option<(u8, MemberRef)> {
    match cf.pool.get(i as usize)? {
        Const::MethodHandle { kind, reference } => Some((*kind, cf.member(*reference).ok()?)),
        _ => None,
    }
}

fn constant_text(cf: &ClassFile, i: u16) -> Option<Vec<u16>> {
    Some(match cf.pool.get(i as usize)? {
        Const::String(u) => cf.utf16(*u).ok()?.to_vec(),
        Const::Int(v) => v.to_string().encode_utf16().collect(),
        Const::Long(v) => v.to_string().encode_utf16().collect(),
        Const::Float(v) => jrt::fmt::float_to_string(*v).encode_utf16().collect(),
        Const::Double(v) => jrt::fmt::double_to_string(*v).encode_utf16().collect(),
        _ => return None,
    })
}

/// Reads the call site at pool index `idx`.
pub fn parse(cf: &ClassFile, idx: u16) -> Site {
    let Some(Const::InvokeDynamic { bootstrap, nat }) = cf.pool.get(idx as usize) else {
        return Site::Unsupported("bad invokedynamic".into());
    };
    let Ok((name, desc)) = cf.name_and_type(*nat) else {
        return Site::Unsupported("bad invokedynamic".into());
    };
    let Some(bsm) = cf.bootstrap.get(*bootstrap as usize) else {
        return Site::Unsupported("missing bootstrap method".into());
    };
    let Some((_, factory)) = handle(cf, bsm.method) else {
        return Site::Unsupported("bad bootstrap method".into());
    };
    let (params, ret) = crate::desc::method(&desc);
    match (factory.class.as_str(), factory.name.as_str()) {
        ("java/lang/invoke/StringConcatFactory", "makeConcatWithConstants") => {
            let Some(recipe) = bsm.args.first().and_then(|&a| constant_text(cf, a)) else {
                return Site::Unsupported("string concatenation without a recipe".into());
            };
            let mut pieces = Vec::new();
            let (mut arg, mut constant) = (0, 1);
            let mut text = Vec::new();
            for c in recipe {
                match c {
                    1 | 2 => {
                        if !text.is_empty() {
                            pieces.push(Piece::Text(std::mem::take(&mut text)));
                        }
                        if c == 1 {
                            pieces.push(Piece::Arg(
                                arg,
                                params.get(arg).copied().unwrap_or("I").to_string(),
                            ));
                            arg += 1;
                        } else {
                            let k = bsm.args.get(constant).and_then(|&a| constant_text(cf, a));
                            pieces.push(Piece::Text(k.unwrap_or_default()));
                            constant += 1;
                        }
                    }
                    c => text.push(c),
                }
            }
            if !text.is_empty() {
                pieces.push(Piece::Text(text));
            }
            Site::Concat(pieces)
        }
        ("java/lang/invoke/StringConcatFactory", "makeConcat") => Site::Concat(
            params
                .iter()
                .enumerate()
                .map(|(i, p)| Piece::Arg(i, p.to_string()))
                .collect(),
        ),
        ("java/lang/invoke/LambdaMetafactory", "metafactory" | "altMetafactory") => {
            let (Some(sam), Some((kind, target))) = (
                bsm.args.first().and_then(|&a| method_type(cf, a)),
                bsm.args.get(1).and_then(|&a| handle(cf, a)),
            ) else {
                return Site::Unsupported("bad lambda bootstrap arguments".into());
            };
            let Some(iface) = ret.and_then(crate::desc::class_of) else {
                return Site::Unsupported("lambda of a non-interface type".into());
            };
            let (mut markers, mut bridges) = (Vec::new(), Vec::new());
            if factory.name == "altMetafactory" {
                let int = |k: usize| match bsm.args.get(k).and_then(|&a| cf.pool.get(a as usize)) {
                    Some(Const::Int(v)) => *v,
                    _ => 0,
                };
                let flags = int(3);
                let mut k = 4;
                if flags & 2 != 0 {
                    let n = int(k) as usize;
                    for j in 0..n {
                        if let Some(c) =
                            bsm.args.get(k + 1 + j).and_then(|&a| cf.class_name(a).ok())
                        {
                            markers.push(c);
                        }
                    }
                    k += 1 + n;
                }
                if flags & 4 != 0 {
                    let n = int(k) as usize;
                    for j in 0..n {
                        if let Some(d) = bsm.args.get(k + 1 + j).and_then(|&a| method_type(cf, a)) {
                            bridges.push(d);
                        }
                    }
                }
                if flags & 1 != 0 {
                    markers.push("java/io/Serializable".into());
                }
            }
            Site::Lambda(Lambda {
                iface: iface.to_string(),
                name,
                sam_desc: sam,
                bridges,
                markers,
                captured: params.iter().map(|p| p.to_string()).collect(),
                kind,
                target,
            })
        }
        (c, n) => Site::Unsupported(format!("invokedynamic bootstrap {c}.{n}")),
    }
}
