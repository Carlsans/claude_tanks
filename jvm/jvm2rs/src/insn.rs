//! Decodes JVM bytecode into normalized instructions (`iload_2` and
//! `wide iload 2` both become `Load(I, 2)`, branch offsets become targets).

use crate::classfile::{Const, Error, Result};

/// The value kinds of the operand stack and locals. Booleans, bytes, chars
/// and shorts are ints; `R` is a `jsr` return address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum K {
    I,
    J,
    F,
    D,
    A,
    R,
}

impl K {
    /// Category 2 values take two stack slots and two locals.
    pub fn wide(self) -> bool {
        matches!(self, K::J | K::D)
    }
    pub fn letter(self) -> char {
        match self {
            K::I => 'i',
            K::J => 'j',
            K::F => 'f',
            K::D => 'd',
            K::A => 'a',
            K::R => 'r',
        }
    }
}

/// Array element types (the `baload` family).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elem {
    /// `byte` and `boolean`.
    B,
    C,
    S,
    I,
    J,
    F,
    D,
    A,
}

impl Elem {
    pub fn kind(self) -> K {
        match self {
            Elem::B | Elem::C | Elem::S | Elem::I => K::I,
            Elem::J => K::J,
            Elem::F => K::F,
            Elem::D => K::D,
            Elem::A => K::A,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Shl,
    Shr,
    Ushr,
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Ge,
    Gt,
    Le,
}

impl Cond {
    pub fn rust(self) -> &'static str {
        match self {
            Cond::Eq => "==",
            Cond::Ne => "!=",
            Cond::Lt => "<",
            Cond::Ge => ">=",
            Cond::Gt => ">",
            Cond::Le => "<=",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invoke {
    Virtual,
    Special,
    Static,
    Interface,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Insn {
    Nop,
    AconstNull,
    Iconst(i32),
    Lconst(i64),
    Fconst(f32),
    Dconst(f64),
    /// `ldc` of a string, class, method type or dynamic constant: pool index.
    Ldc(u16),
    Load(K, u16),
    Store(K, u16),
    ArrLoad(Elem),
    ArrStore(Elem),
    Pop,
    Pop2,
    Dup,
    DupX1,
    DupX2,
    Dup2,
    Dup2X1,
    Dup2X2,
    Swap,
    Arith(Op, K),
    Iinc(u16, i32),
    /// Conversions: from, to (`i2b`, `i2c`, `i2s` have their own).
    Conv(K, K),
    I2b,
    I2c,
    I2s,
    Lcmp,
    Fcmp {
        nan_greater: bool,
    },
    Dcmp {
        nan_greater: bool,
    },
    /// Compare an int with zero.
    If(Cond, u32),
    IfIcmp(Cond, u32),
    IfAcmp {
        eq: bool,
        target: u32,
    },
    IfNull {
        null: bool,
        target: u32,
    },
    Goto(u32),
    Jsr(u32),
    Ret(u16),
    TableSwitch {
        default: u32,
        low: i32,
        targets: Vec<u32>,
    },
    LookupSwitch {
        default: u32,
        pairs: Vec<(i32, u32)>,
    },
    Return(Option<K>),
    GetStatic(u16),
    PutStatic(u16),
    GetField(u16),
    PutField(u16),
    Invoke(Invoke, u16),
    InvokeDynamic(u16),
    New(u16),
    /// `newarray`: the element's descriptor letter.
    NewArray(u8),
    ANewArray(u16),
    MultiANewArray(u16, u8),
    ArrayLength,
    Athrow,
    CheckCast(u16),
    InstanceOf(u16),
    MonitorEnter,
    MonitorExit,
}

impl Insn {
    /// Whether execution can continue with the next instruction.
    pub fn falls_through(&self) -> bool {
        !matches!(
            self,
            Insn::Goto(_)
                | Insn::Return(_)
                | Insn::Athrow
                | Insn::Ret(_)
                | Insn::TableSwitch { .. }
                | Insn::LookupSwitch { .. }
                | Insn::Jsr(_)
        )
    }

    /// Branch targets (not counting falling through).
    pub fn targets(&self) -> Vec<u32> {
        match self {
            Insn::If(_, t) | Insn::IfIcmp(_, t) | Insn::Goto(t) | Insn::Jsr(t) => vec![*t],
            Insn::IfAcmp { target, .. } | Insn::IfNull { target, .. } => vec![*target],
            Insn::TableSwitch {
                default, targets, ..
            } => {
                let mut v = targets.clone();
                v.push(*default);
                v
            }
            Insn::LookupSwitch { default, pairs } => {
                let mut v: Vec<u32> = pairs.iter().map(|p| p.1).collect();
                v.push(*default);
                v
            }
            _ => Vec::new(),
        }
    }
}

fn kind_of(code: u8) -> K {
    // In each family the order is i, l, f, d, a.
    [K::I, K::J, K::F, K::D, K::A][code as usize]
}

/// Decodes a method's code into (pc, instruction) pairs. `pool` resolves
/// `ldc` constants that are numbers.
pub fn decode(code: &[u8], pool: &[Const]) -> Result<Vec<(u32, Insn)>> {
    let mut out = Vec::new();
    let mut p = 0usize;
    let rd =
        |i: usize| -> Result<u8> { code.get(i).copied().ok_or(Error("truncated code".into())) };
    let u2 = |i: usize| -> Result<u16> { Ok(u16::from(rd(i)?) << 8 | u16::from(rd(i + 1)?)) };
    let i4 =
        |i: usize| -> Result<i32> { Ok((u32::from(u2(i)?) << 16 | u32::from(u2(i + 2)?)) as i32) };
    while p < code.len() {
        let pc = p as u32;
        let op = code[p];
        let rel2 = |o: usize| -> Result<u32> { Ok((pc as i64 + i64::from(u2(o)? as i16)) as u32) };
        let (insn, len) = match op {
            0x00 => (Insn::Nop, 1),
            0x01 => (Insn::AconstNull, 1),
            0x02..=0x08 => (Insn::Iconst(i32::from(op) - 3), 1),
            0x09 | 0x0a => (Insn::Lconst(i64::from(op - 0x09)), 1),
            0x0b..=0x0d => (Insn::Fconst(f32::from(op - 0x0b)), 1),
            0x0e | 0x0f => (Insn::Dconst(f64::from(op - 0x0e)), 1),
            0x10 => (Insn::Iconst(i32::from(rd(p + 1)? as i8)), 2),
            0x11 => (Insn::Iconst(i32::from(u2(p + 1)? as i16)), 3),
            0x12..=0x14 => {
                let (idx, len) = if op == 0x12 {
                    (u16::from(rd(p + 1)?), 2)
                } else {
                    (u2(p + 1)?, 3)
                };
                let insn = match pool.get(idx as usize) {
                    Some(Const::Int(v)) => Insn::Iconst(*v),
                    Some(Const::Float(v)) => Insn::Fconst(*v),
                    Some(Const::Long(v)) => Insn::Lconst(*v),
                    Some(Const::Double(v)) => Insn::Dconst(*v),
                    _ => Insn::Ldc(idx),
                };
                (insn, len)
            }
            0x15..=0x19 => (Insn::Load(kind_of(op - 0x15), u16::from(rd(p + 1)?)), 2),
            0x1a..=0x2d => {
                let k = (op - 0x1a) / 4;
                (Insn::Load(kind_of(k), u16::from((op - 0x1a) % 4)), 1)
            }
            0x2e..=0x35 => (
                Insn::ArrLoad(
                    [
                        Elem::I,
                        Elem::J,
                        Elem::F,
                        Elem::D,
                        Elem::A,
                        Elem::B,
                        Elem::C,
                        Elem::S,
                    ][(op - 0x2e) as usize],
                ),
                1,
            ),
            0x36..=0x3a => (Insn::Store(kind_of(op - 0x36), u16::from(rd(p + 1)?)), 2),
            0x3b..=0x4e => {
                let k = (op - 0x3b) / 4;
                (Insn::Store(kind_of(k), u16::from((op - 0x3b) % 4)), 1)
            }
            0x4f..=0x56 => (
                Insn::ArrStore(
                    [
                        Elem::I,
                        Elem::J,
                        Elem::F,
                        Elem::D,
                        Elem::A,
                        Elem::B,
                        Elem::C,
                        Elem::S,
                    ][(op - 0x4f) as usize],
                ),
                1,
            ),
            0x57 => (Insn::Pop, 1),
            0x58 => (Insn::Pop2, 1),
            0x59 => (Insn::Dup, 1),
            0x5a => (Insn::DupX1, 1),
            0x5b => (Insn::DupX2, 1),
            0x5c => (Insn::Dup2, 1),
            0x5d => (Insn::Dup2X1, 1),
            0x5e => (Insn::Dup2X2, 1),
            0x5f => (Insn::Swap, 1),
            0x60..=0x77 => {
                let ops = [Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Rem, Op::Neg];
                (
                    Insn::Arith(ops[((op - 0x60) / 4) as usize], kind_of((op - 0x60) % 4)),
                    1,
                )
            }
            0x78..=0x83 => {
                let ops = [Op::Shl, Op::Shr, Op::Ushr, Op::And, Op::Or, Op::Xor];
                let k = if (op - 0x78).is_multiple_of(2) {
                    K::I
                } else {
                    K::J
                };
                (Insn::Arith(ops[((op - 0x78) / 2) as usize], k), 1)
            }
            0x84 => (
                Insn::Iinc(u16::from(rd(p + 1)?), i32::from(rd(p + 2)? as i8)),
                3,
            ),
            0x85..=0x90 => {
                let pairs = [
                    (K::I, K::J),
                    (K::I, K::F),
                    (K::I, K::D),
                    (K::J, K::I),
                    (K::J, K::F),
                    (K::J, K::D),
                    (K::F, K::I),
                    (K::F, K::J),
                    (K::F, K::D),
                    (K::D, K::I),
                    (K::D, K::J),
                    (K::D, K::F),
                ];
                let (a, b) = pairs[(op - 0x85) as usize];
                (Insn::Conv(a, b), 1)
            }
            0x91 => (Insn::I2b, 1),
            0x92 => (Insn::I2c, 1),
            0x93 => (Insn::I2s, 1),
            0x94 => (Insn::Lcmp, 1),
            0x95 | 0x96 => (
                Insn::Fcmp {
                    nan_greater: op == 0x96,
                },
                1,
            ),
            0x97 | 0x98 => (
                Insn::Dcmp {
                    nan_greater: op == 0x98,
                },
                1,
            ),
            0x99..=0x9e => {
                let c = [Cond::Eq, Cond::Ne, Cond::Lt, Cond::Ge, Cond::Gt, Cond::Le]
                    [(op - 0x99) as usize];
                (Insn::If(c, rel2(p + 1)?), 3)
            }
            0x9f..=0xa4 => {
                let c = [Cond::Eq, Cond::Ne, Cond::Lt, Cond::Ge, Cond::Gt, Cond::Le]
                    [(op - 0x9f) as usize];
                (Insn::IfIcmp(c, rel2(p + 1)?), 3)
            }
            0xa5 | 0xa6 => (
                Insn::IfAcmp {
                    eq: op == 0xa5,
                    target: rel2(p + 1)?,
                },
                3,
            ),
            0xa7 => (Insn::Goto(rel2(p + 1)?), 3),
            0xa8 => (Insn::Jsr(rel2(p + 1)?), 3),
            0xa9 => (Insn::Ret(u16::from(rd(p + 1)?)), 2),
            0xaa => {
                let q = p + 1 + (4 - (p + 1) % 4) % 4;
                let default = (pc as i64 + i64::from(i4(q)?)) as u32;
                let (low, high) = (i4(q + 4)?, i4(q + 8)?);
                if high < low {
                    return Err(Error(format!("tableswitch at {pc}: high below low")));
                }
                let n = (high as i64 - low as i64 + 1) as usize;
                let targets = (0..n)
                    .map(|k| Ok((pc as i64 + i64::from(i4(q + 12 + 4 * k)?)) as u32))
                    .collect::<Result<Vec<_>>>()?;
                (
                    Insn::TableSwitch {
                        default,
                        low,
                        targets,
                    },
                    q + 12 + 4 * n - p,
                )
            }
            0xab => {
                let q = p + 1 + (4 - (p + 1) % 4) % 4;
                let default = (pc as i64 + i64::from(i4(q)?)) as u32;
                let n = i4(q + 4)?.max(0) as usize;
                let pairs = (0..n)
                    .map(|k| {
                        Ok((
                            i4(q + 8 + 8 * k)?,
                            (pc as i64 + i64::from(i4(q + 12 + 8 * k)?)) as u32,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                (Insn::LookupSwitch { default, pairs }, q + 8 + 8 * n - p)
            }
            0xac..=0xb0 => (Insn::Return(Some(kind_of(op - 0xac))), 1),
            0xb1 => (Insn::Return(None), 1),
            0xb2 => (Insn::GetStatic(u2(p + 1)?), 3),
            0xb3 => (Insn::PutStatic(u2(p + 1)?), 3),
            0xb4 => (Insn::GetField(u2(p + 1)?), 3),
            0xb5 => (Insn::PutField(u2(p + 1)?), 3),
            0xb6 => (Insn::Invoke(Invoke::Virtual, u2(p + 1)?), 3),
            0xb7 => (Insn::Invoke(Invoke::Special, u2(p + 1)?), 3),
            0xb8 => (Insn::Invoke(Invoke::Static, u2(p + 1)?), 3),
            0xb9 => (Insn::Invoke(Invoke::Interface, u2(p + 1)?), 5),
            0xba => (Insn::InvokeDynamic(u2(p + 1)?), 5),
            0xbb => (Insn::New(u2(p + 1)?), 3),
            0xbc => {
                let t = match rd(p + 1)? {
                    4 => b'Z',
                    5 => b'C',
                    6 => b'F',
                    7 => b'D',
                    8 => b'B',
                    9 => b'S',
                    10 => b'I',
                    11 => b'J',
                    t => return Err(Error(format!("newarray type {t} at {pc}"))),
                };
                (Insn::NewArray(t), 2)
            }
            0xbd => (Insn::ANewArray(u2(p + 1)?), 3),
            0xbe => (Insn::ArrayLength, 1),
            0xbf => (Insn::Athrow, 1),
            0xc0 => (Insn::CheckCast(u2(p + 1)?), 3),
            0xc1 => (Insn::InstanceOf(u2(p + 1)?), 3),
            0xc2 => (Insn::MonitorEnter, 1),
            0xc3 => (Insn::MonitorExit, 1),
            0xc4 => {
                let op2 = rd(p + 1)?;
                let idx = u2(p + 2)?;
                match op2 {
                    0x15..=0x19 => (Insn::Load(kind_of(op2 - 0x15), idx), 4),
                    0x36..=0x3a => (Insn::Store(kind_of(op2 - 0x36), idx), 4),
                    0xa9 => (Insn::Ret(idx), 4),
                    0x84 => (Insn::Iinc(idx, i32::from(u2(p + 4)? as i16)), 6),
                    _ => return Err(Error(format!("wide {op2:#x} at {pc}"))),
                }
            }
            0xc5 => (Insn::MultiANewArray(u2(p + 1)?, rd(p + 3)?), 4),
            0xc6 | 0xc7 => (
                Insn::IfNull {
                    null: op == 0xc6,
                    target: rel2(p + 1)?,
                },
                3,
            ),
            0xc8 => (Insn::Goto((pc as i64 + i64::from(i4(p + 1)?)) as u32), 5),
            0xc9 => (Insn::Jsr((pc as i64 + i64::from(i4(p + 1)?)) as u32), 5),
            _ => return Err(Error(format!("unknown opcode {op:#x} at {pc}"))),
        };
        out.push((pc, insn));
        p += len;
    }
    Ok(out)
}
