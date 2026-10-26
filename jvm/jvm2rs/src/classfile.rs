//! Reads JVM class files: the constant pool, fields, methods and their code.
//! Everything a translator needs, nothing more (no annotations, no debug
//! information, no stack maps: types are inferred from the bytecode itself).

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error(m.into()))
}

/// One constant-pool entry. Indices point back into the pool.
#[derive(Clone, Debug)]
pub enum Const {
    /// Slot 0, and the slot after a long or double.
    Unused,
    /// Modified UTF-8, decoded to UTF-16 so string constants stay exact.
    Utf8(Vec<u16>),
    Int(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class(u16),
    String(u16),
    Field {
        class: u16,
        nat: u16,
    },
    Method {
        class: u16,
        nat: u16,
    },
    InterfaceMethod {
        class: u16,
        nat: u16,
    },
    NameAndType {
        name: u16,
        desc: u16,
    },
    MethodHandle {
        kind: u8,
        reference: u16,
    },
    MethodType(u16),
    Dynamic {
        bootstrap: u16,
        nat: u16,
    },
    InvokeDynamic {
        bootstrap: u16,
        nat: u16,
    },
    Module(u16),
    Package(u16),
}

/// A field, method or interface-method reference, resolved to names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MemberRef {
    pub class: String,
    pub name: String,
    pub desc: String,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub access: u16,
    pub name: String,
    pub desc: String,
    /// `ConstantValue` attribute (static finals): a pool index.
    pub constant: Option<u16>,
    /// `Signature` attribute: the generic type (`Ljava/util/List<LFoo;>;`).
    pub signature: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Handler {
    pub start: u16,
    pub end: u16,
    pub handler: u16,
    /// `None` catches everything (`finally`).
    pub catch: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    pub bytes: Vec<u8>,
    pub handlers: Vec<Handler>,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub access: u16,
    pub name: String,
    pub desc: String,
    pub code: Option<Code>,
}

#[derive(Clone, Debug)]
pub struct Bootstrap {
    pub method: u16,
    pub args: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct ClassFile {
    pub major: u16,
    pub access: u16,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub pool: Vec<Const>,
    pub bootstrap: Vec<Bootstrap>,
}

pub mod access {
    pub const PUBLIC: u16 = 0x0001;
    pub const PRIVATE: u16 = 0x0002;
    pub const STATIC: u16 = 0x0008;
    pub const FINAL: u16 = 0x0010;
    pub const SYNCHRONIZED: u16 = 0x0020;
    pub const NATIVE: u16 = 0x0100;
    pub const INTERFACE: u16 = 0x0200;
    pub const ABSTRACT: u16 = 0x0400;
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn u1(&mut self) -> Result<u8> {
        let v = *self
            .b
            .get(self.i)
            .ok_or(Error("truncated class file".into()))?;
        self.i += 1;
        Ok(v)
    }
    fn u2(&mut self) -> Result<u16> {
        Ok(u16::from(self.u1()?) << 8 | u16::from(self.u1()?))
    }
    fn u4(&mut self) -> Result<u32> {
        Ok(u32::from(self.u2()?) << 16 | u32::from(self.u2()?))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .b
            .get(self.i..self.i + n)
            .ok_or(Error("truncated class file".into()))?;
        self.i += n;
        Ok(s)
    }
}

/// Modified UTF-8 (nulls as two bytes, supplementary characters as two
/// surrogates of three bytes each) to UTF-16.
pub fn decode_mutf8(b: &[u8]) -> Vec<u16> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 {
            out.push(u16::from(c));
            i += 1;
        } else if c & 0xe0 == 0xc0 && i + 1 < b.len() {
            out.push(u16::from(c & 0x1f) << 6 | u16::from(b[i + 1] & 0x3f));
            i += 2;
        } else if c & 0xf0 == 0xe0 && i + 2 < b.len() {
            out.push(
                u16::from(c & 0x0f) << 12
                    | u16::from(b[i + 1] & 0x3f) << 6
                    | u16::from(b[i + 2] & 0x3f),
            );
            i += 3;
        } else {
            // Not valid modified UTF-8; keep the byte as a character.
            out.push(u16::from(c));
            i += 1;
        }
    }
    out
}

impl ClassFile {
    pub fn parse(b: &[u8]) -> Result<ClassFile> {
        let mut r = Reader { b, i: 0 };
        if r.u4()? != 0xcafe_babe {
            return err("not a class file");
        }
        let _minor = r.u2()?;
        let major = r.u2()?;
        let n = r.u2()? as usize;
        let mut pool = vec![Const::Unused; n.max(1)];
        let mut k = 1;
        while k < n {
            let tag = r.u1()?;
            pool[k] = match tag {
                1 => {
                    let len = r.u2()? as usize;
                    Const::Utf8(decode_mutf8(r.bytes(len)?))
                }
                3 => Const::Int(r.u4()? as i32),
                4 => Const::Float(f32::from_bits(r.u4()?)),
                5 => Const::Long((u64::from(r.u4()?) << 32 | u64::from(r.u4()?)) as i64),
                6 => Const::Double(f64::from_bits(
                    u64::from(r.u4()?) << 32 | u64::from(r.u4()?),
                )),
                7 => Const::Class(r.u2()?),
                8 => Const::String(r.u2()?),
                9 => Const::Field {
                    class: r.u2()?,
                    nat: r.u2()?,
                },
                10 => Const::Method {
                    class: r.u2()?,
                    nat: r.u2()?,
                },
                11 => Const::InterfaceMethod {
                    class: r.u2()?,
                    nat: r.u2()?,
                },
                12 => Const::NameAndType {
                    name: r.u2()?,
                    desc: r.u2()?,
                },
                15 => Const::MethodHandle {
                    kind: r.u1()?,
                    reference: r.u2()?,
                },
                16 => Const::MethodType(r.u2()?),
                17 => Const::Dynamic {
                    bootstrap: r.u2()?,
                    nat: r.u2()?,
                },
                18 => Const::InvokeDynamic {
                    bootstrap: r.u2()?,
                    nat: r.u2()?,
                },
                19 => Const::Module(r.u2()?),
                20 => Const::Package(r.u2()?),
                t => return err(format!("constant-pool tag {t} at entry {k}")),
            };
            // Longs and doubles take two entries.
            k += if matches!(tag, 5 | 6) { 2 } else { 1 };
        }
        let mut cf = ClassFile {
            major,
            access: 0,
            name: String::new(),
            super_name: None,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            pool,
            bootstrap: Vec::new(),
        };
        cf.access = r.u2()?;
        let this = r.u2()?;
        cf.name = cf.class_name(this)?;
        let sup = r.u2()?;
        cf.super_name = if sup == 0 {
            None
        } else {
            Some(cf.class_name(sup)?)
        };
        for _ in 0..r.u2()? {
            let i = r.u2()?;
            cf.interfaces.push(cf.class_name(i)?);
        }
        for _ in 0..r.u2()? {
            let access = r.u2()?;
            let name = cf.utf8(r.u2()?)?;
            let desc = cf.utf8(r.u2()?)?;
            let mut constant = None;
            let mut signature = None;
            for _ in 0..r.u2()? {
                let an = cf.utf8(r.u2()?)?;
                let len = r.u4()? as usize;
                let body = r.bytes(len)?;
                if len >= 2 {
                    let idx = u16::from(body[0]) << 8 | u16::from(body[1]);
                    match an.as_str() {
                        "ConstantValue" => constant = Some(idx),
                        "Signature" => signature = cf.utf8(idx).ok(),
                        _ => {}
                    }
                }
            }
            cf.fields.push(Field {
                signature,
                access,
                name,
                desc,
                constant,
            });
        }
        for _ in 0..r.u2()? {
            let access = r.u2()?;
            let name = cf.utf8(r.u2()?)?;
            let desc = cf.utf8(r.u2()?)?;
            let mut code = None;
            for _ in 0..r.u2()? {
                let an = cf.utf8(r.u2()?)?;
                let len = r.u4()? as usize;
                let body = r.bytes(len)?;
                if an == "Code" {
                    code = Some(cf.parse_code(body)?);
                }
            }
            cf.methods.push(Method {
                access,
                name,
                desc,
                code,
            });
        }
        for _ in 0..r.u2()? {
            let an = cf.utf8(r.u2()?)?;
            let len = r.u4()? as usize;
            let body = r.bytes(len)?;
            if an == "BootstrapMethods" {
                let mut br = Reader { b: body, i: 0 };
                for _ in 0..br.u2()? {
                    let method = br.u2()?;
                    let args = (0..br.u2()?).map(|_| br.u2()).collect::<Result<_>>()?;
                    cf.bootstrap.push(Bootstrap { method, args });
                }
            }
        }
        Ok(cf)
    }

    fn parse_code(&self, body: &[u8]) -> Result<Code> {
        let mut r = Reader { b: body, i: 0 };
        let max_stack = r.u2()?;
        let max_locals = r.u2()?;
        let len = r.u4()? as usize;
        let bytes = r.bytes(len)?.to_vec();
        let mut handlers = Vec::new();
        for _ in 0..r.u2()? {
            let (start, end, handler, catch) = (r.u2()?, r.u2()?, r.u2()?, r.u2()?);
            let catch = if catch == 0 {
                None
            } else {
                Some(self.class_name(catch)?)
            };
            handlers.push(Handler {
                start,
                end,
                handler,
                catch,
            });
        }
        Ok(Code {
            max_stack,
            max_locals,
            bytes,
            handlers,
        })
    }

    pub fn utf16(&self, i: u16) -> Result<&[u16]> {
        match self.pool.get(i as usize) {
            Some(Const::Utf8(s)) => Ok(s),
            _ => err(format!("{}: entry {i} is not UTF-8", self.name)),
        }
    }

    pub fn utf8(&self, i: u16) -> Result<String> {
        Ok(String::from_utf16_lossy(self.utf16(i)?))
    }

    pub fn class_name(&self, i: u16) -> Result<String> {
        match self.pool.get(i as usize) {
            Some(Const::Class(n)) => self.utf8(*n),
            _ => err(format!("{}: entry {i} is not a class", self.name)),
        }
    }

    pub fn name_and_type(&self, i: u16) -> Result<(String, String)> {
        match self.pool.get(i as usize) {
            Some(Const::NameAndType { name, desc }) => Ok((self.utf8(*name)?, self.utf8(*desc)?)),
            _ => err(format!("{}: entry {i} is not a name and type", self.name)),
        }
    }

    /// A field, method or interface-method reference.
    pub fn member(&self, i: u16) -> Result<MemberRef> {
        match self.pool.get(i as usize) {
            Some(
                Const::Field { class, nat }
                | Const::Method { class, nat }
                | Const::InterfaceMethod { class, nat },
            ) => {
                let (name, desc) = self.name_and_type(*nat)?;
                Ok(MemberRef {
                    class: self.class_name(*class)?,
                    name,
                    desc,
                })
            }
            _ => err(format!(
                "{}: entry {i} is not a member reference",
                self.name
            )),
        }
    }

    pub fn method(&self, name: &str, desc: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.desc == desc)
    }

    pub fn is_interface(&self) -> bool {
        self.access & access::INTERFACE != 0
    }
}
