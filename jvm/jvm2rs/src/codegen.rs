//! Rust for one translated class: its `Class` static, static fields, class
//! initializer and methods.
//!
//! A method becomes a loop over its basic blocks (`match bb { .. }`), so any
//! control flow translates, including the irreducible kind obfuscators make.
//! Operand-stack slots and locals become typed variables: `s3i` is the int
//! at stack depth 3, `l2a` the reference in local 2. Their kinds come from
//! data flow over the bytecode. Whatever can throw goes through `ex` to a
//! dispatch block that picks the method's handler, as the JVM does.

use crate::classfile::{ClassFile, Const, Handler, Method, access};
use crate::desc;
use crate::insn::{Elem, Insn, Invoke, K, Op, decode};
use crate::mangle;
use crate::world::{Impl, Reach, World, direct_target, sig};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

/// String literals shared by the whole crate (`crate::lits`).
#[derive(Default)]
pub struct Lits {
    pub index: HashMap<Vec<u16>, usize>,
}

impl Lits {
    pub fn get(&mut self, s: &[u16]) -> String {
        let n = self.index.len();
        let i = *self.index.entry(s.to_vec()).or_insert(n);
        format!("&crate::lits::L{i}")
    }

    pub fn module(&self) -> String {
        let mut v: Vec<(&Vec<u16>, &usize)> = self.index.iter().collect();
        v.sort_by_key(|(_, i)| **i);
        let mut out = String::from("//! String literals.\n");
        for (s, i) in v {
            let chars: Vec<String> = s.iter().map(|c| c.to_string()).collect();
            writeln!(
                out,
                "pub static L{i}: [u16; {}] = [{}];",
                s.len(),
                chars.join(", ")
            )
            .unwrap();
        }
        out
    }
}

fn rust_ty(k: K) -> &'static str {
    desc::rust_type(k)
}

fn default_of(k: K) -> &'static str {
    match k {
        K::I => "0i32",
        K::J => "0i64",
        K::F => "0f32",
        K::D => "0f64",
        K::A => "None",
        K::R => "0u32",
    }
}

/// The runtime class expression of a class name or array descriptor.
pub fn class_expr(world: &mut World, name: &str) -> Option<String> {
    if let Some(e) = name.strip_prefix('[') {
        let inner = match e.as_bytes().first()? {
            b'L' => class_expr(world, desc::class_of(e)?)?,
            b'[' => class_expr(world, e)?,
            &c => return Some(format!("jrt::prim_array_class(b'{}')", c as char)),
        };
        return Some(format!("jrt::array_class({inner})"));
    }
    world
        .exists(name)
        .then(|| format!("&{}", world.class_static(name)))
}

fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

// ---------------------------------------------------------------- analysis

struct Blocks {
    /// Start pc of each block, in order.
    starts: Vec<u32>,
    /// Block index by start pc.
    by_pc: HashMap<u32, usize>,
    /// Stack kinds on entry, if reachable.
    entry: Vec<Option<Vec<K>>>,
}

/// Stack effect of an instruction: updates `st`. Errors on malformed code.
fn step(cf: &ClassFile, insn: &Insn, st: &mut Vec<K>) -> Result<(), String> {
    let pop = |st: &mut Vec<K>| st.pop().ok_or_else(|| "stack underflow".to_string());
    match insn {
        Insn::Nop | Insn::Goto(_) | Insn::Ret(_) | Insn::Iinc(..) => {}
        Insn::AconstNull => st.push(K::A),
        Insn::Iconst(_) => st.push(K::I),
        Insn::Lconst(_) => st.push(K::J),
        Insn::Fconst(_) => st.push(K::F),
        Insn::Dconst(_) => st.push(K::D),
        Insn::Ldc(_) => st.push(K::A),
        Insn::Load(k, _) => st.push(*k),
        Insn::Store(..) => {
            pop(st)?;
        }
        Insn::ArrLoad(e) => {
            pop(st)?;
            pop(st)?;
            st.push(e.kind());
        }
        Insn::ArrStore(_) => {
            pop(st)?;
            pop(st)?;
            pop(st)?;
        }
        Insn::Pop => {
            pop(st)?;
        }
        Insn::Pop2 => {
            if !pop(st)?.wide() {
                pop(st)?;
            }
        }
        Insn::Dup
        | Insn::DupX1
        | Insn::DupX2
        | Insn::Dup2
        | Insn::Dup2X1
        | Insn::Dup2X2
        | Insn::Swap => {
            let (g1, g2) = dup_groups(insn, st)?;
            let n = g1 + g2;
            let top: Vec<K> = st.split_off(st.len() - n);
            if matches!(insn, Insn::Swap) {
                st.push(top[1]);
                st.push(top[0]);
            } else {
                st.extend_from_slice(&top[g2..]);
                st.extend_from_slice(&top);
            }
        }
        Insn::Arith(op, k) => {
            if *op != Op::Neg {
                pop(st)?;
            }
            pop(st)?;
            st.push(*k);
        }
        Insn::Conv(_, to) => {
            pop(st)?;
            st.push(*to);
        }
        Insn::I2b | Insn::I2c | Insn::I2s => {}
        Insn::Lcmp | Insn::Fcmp { .. } | Insn::Dcmp { .. } => {
            pop(st)?;
            pop(st)?;
            st.push(K::I);
        }
        Insn::If(..) | Insn::IfNull { .. } => {
            pop(st)?;
        }
        Insn::IfIcmp(..) | Insn::IfAcmp { .. } => {
            pop(st)?;
            pop(st)?;
        }
        Insn::Jsr(_) => st.push(K::R),
        Insn::TableSwitch { .. } | Insn::LookupSwitch { .. } => {
            pop(st)?;
        }
        Insn::Return(k) => {
            if k.is_some() {
                pop(st)?;
            }
        }
        Insn::GetStatic(i) | Insn::GetField(i) => {
            let f = cf.member(*i).map_err(|e| e.0)?;
            if matches!(insn, Insn::GetField(_)) {
                pop(st)?;
            }
            st.push(desc::kind(&f.desc));
        }
        Insn::PutStatic(_) => {
            pop(st)?;
        }
        Insn::PutField(_) => {
            pop(st)?;
            pop(st)?;
        }
        Insn::Invoke(kind, i) => {
            let m = cf.member(*i).map_err(|e| e.0)?;
            let (params, ret) = desc::method(&m.desc);
            for _ in 0..params.len() + usize::from(*kind != Invoke::Static) {
                pop(st)?;
            }
            if let Some(r) = ret {
                st.push(desc::kind(r));
            }
        }
        Insn::InvokeDynamic(i) => {
            let (_, d) = match cf.pool.get(*i as usize) {
                Some(Const::InvokeDynamic { nat, .. }) => {
                    cf.name_and_type(*nat).map_err(|e| e.0)?
                }
                _ => return Err("bad invokedynamic".into()),
            };
            let (params, ret) = desc::method(&d);
            for _ in 0..params.len() {
                pop(st)?;
            }
            if let Some(r) = ret {
                st.push(desc::kind(r));
            }
        }
        Insn::New(_) => st.push(K::A),
        Insn::NewArray(_) | Insn::ANewArray(_) | Insn::ArrayLength | Insn::InstanceOf(_) => {
            pop(st)?;
            st.push(if matches!(insn, Insn::ArrayLength | Insn::InstanceOf(_)) {
                K::I
            } else {
                K::A
            });
        }
        Insn::MultiANewArray(_, n) => {
            for _ in 0..*n {
                pop(st)?;
            }
            st.push(K::A);
        }
        Insn::Athrow | Insn::MonitorEnter | Insn::MonitorExit => {
            pop(st)?;
        }
        Insn::CheckCast(_) => {}
    }
    Ok(())
}

/// How many values the dup family copies (`g1`) and skips over (`g2`),
/// counted in values (a long is one value of two words).
fn dup_groups(insn: &Insn, st: &[K]) -> Result<(usize, usize), String> {
    let (w1, w2) = match insn {
        Insn::Dup => (1, 0),
        Insn::DupX1 => (1, 1),
        Insn::DupX2 => (1, 2),
        Insn::Dup2 => (2, 0),
        Insn::Dup2X1 => (2, 1),
        Insn::Dup2X2 => (2, 2),
        Insn::Swap => (1, 1),
        _ => unreachable!(),
    };
    let mut i = st.len();
    let mut take = |words: usize| -> Result<usize, String> {
        let (mut n, mut w) = (0, 0);
        while w < words {
            i = i.checked_sub(1).ok_or("stack underflow")?;
            w += if st[i].wide() { 2 } else { 1 };
            n += 1;
        }
        if w != words {
            Err("dup splits a long or double".into())
        } else {
            Ok(n)
        }
    };
    let g1 = take(w1)?;
    let g2 = take(w2)?;
    Ok((g1, g2))
}

fn analyze(cf: &ClassFile, insns: &[(u32, Insn)], handlers: &[Handler]) -> Result<Blocks, String> {
    let mut leaders: BTreeSet<u32> = BTreeSet::new();
    leaders.insert(0);
    for (k, (_, insn)) in insns.iter().enumerate() {
        for t in insn.targets() {
            leaders.insert(t);
        }
        let ends = !insn.falls_through() || !insn.targets().is_empty();
        if ends && let Some((next, _)) = insns.get(k + 1) {
            leaders.insert(*next);
        }
    }
    for h in handlers {
        leaders.insert(u32::from(h.handler));
    }
    let pcs: BTreeSet<u32> = insns.iter().map(|(p, _)| *p).collect();
    if let Some(bad) = leaders.iter().find(|p| !pcs.contains(p)) {
        return Err(format!("branch into the middle of an instruction ({bad})"));
    }
    let starts: Vec<u32> = leaders.into_iter().collect();
    let by_pc: HashMap<u32, usize> = starts.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let index: HashMap<u32, usize> = insns
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (*p, i))
        .collect();
    let mut entry: Vec<Option<Vec<K>>> = vec![None; starts.len()];
    let mut work = vec![0usize];
    entry[0] = Some(Vec::new());
    for h in handlers {
        let b = by_pc[&u32::from(h.handler)];
        if entry[b].is_none() {
            entry[b] = Some(vec![K::A]);
            work.push(b);
        }
    }
    let merge = |entry: &mut Vec<Option<Vec<K>>>,
                 work: &mut Vec<usize>,
                 b: usize,
                 st: &[K]|
     -> Result<(), String> {
        match &entry[b] {
            None => {
                entry[b] = Some(st.to_vec());
                work.push(b);
                Ok(())
            }
            Some(e) if e == st => Ok(()),
            Some(e) => Err(format!("stack {e:?} and {st:?} meet at pc {}", starts[b])),
        }
    };
    while let Some(b) = work.pop() {
        let mut st = entry[b].clone().unwrap();
        let mut k = index[&starts[b]];
        loop {
            let (_, insn) = &insns[k];
            let before = st.clone();
            step(cf, insn, &mut st)?;
            for t in insn.targets() {
                merge(&mut entry, &mut work, by_pc[&t], &st)?;
            }
            if let Insn::Jsr(_) = insn {
                // The subroutine returns to the next instruction.
                if let Some((next, _)) = insns.get(k + 1) {
                    merge(&mut entry, &mut work, by_pc[next], &before)?;
                }
                break;
            }
            if !insn.falls_through() {
                break;
            }
            let Some((next, _)) = insns.get(k + 1) else {
                break;
            };
            if let Some(&nb) = by_pc.get(next) {
                merge(&mut entry, &mut work, nb, &st)?;
                break;
            }
            k += 1;
        }
    }
    Ok(Blocks {
        starts,
        by_pc,
        entry,
    })
}

// ---------------------------------------------------------------- methods

struct MethodGen<'a> {
    world: &'a mut World,
    lits: &'a mut Lits,
    cf: &'a ClassFile,
    reach: &'a Reach,
    out: String,
    /// Variables used: (name, kind).
    vars: BTreeSet<(String, K)>,
    st: Vec<K>,
    blocks: &'a Blocks,
    /// Handler list (indices into the code's handlers) per dispatch id.
    dispatch: Vec<Vec<usize>>,
    handlers: &'a [Handler],
    /// Problems worth reporting (unresolved members...).
    pub notes: &'a mut BTreeMap<String, usize>,
    extras: &'a mut Extras,
}

/// Code a class's methods add to its module (lambda classes).
#[derive(Default)]
pub struct Extras {
    pub code: String,
    pub lambdas: usize,
}

const DISPATCH_BASE: u32 = 1 << 30;

impl MethodGen<'_> {
    fn sv(&mut self, i: usize, k: K) -> String {
        let n = format!("s{i}{}", k.letter());
        self.vars.insert((n.clone(), k));
        n
    }

    fn lv(&mut self, slot: u16, k: K) -> String {
        let n = format!("l{slot}{}", k.letter());
        self.vars.insert((n.clone(), k));
        n
    }

    fn push(&mut self, k: K) -> String {
        self.st.push(k);
        self.sv(self.st.len() - 1, k)
    }

    /// Pops the top value: its variable and kind.
    fn pop(&mut self) -> (String, K) {
        let k = self.st.pop().expect("analysis checked the stack");
        (self.sv(self.st.len(), k), k)
    }

    /// The top value moved out (references are taken, leaving null).
    fn pop_val(&mut self) -> String {
        let (v, k) = self.pop();
        if k == K::A { format!("{v}.take()") } else { v }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str("                ");
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn note(&mut self, s: String) {
        *self.notes.entry(s).or_default() += 1;
    }

    /// `expr` (a `JResult`) unwrapped, or its error sent to the handlers
    /// covering `pc`.
    fn tr(&mut self, expr: &str, pc: u32) -> String {
        let hs: Vec<usize> = self
            .handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| u32::from(h.start) <= pc && pc < u32::from(h.end))
            .map(|(i, _)| i)
            .collect();
        if hs.is_empty() {
            return format!("({expr})?");
        }
        let id = match self.dispatch.iter().position(|d| *d == hs) {
            Some(i) => i,
            None => {
                self.dispatch.push(hs);
                self.dispatch.len() - 1
            }
        };
        format!(
            "match ({expr}) {{ Ok(v) => v, Err(e) => {{ ex = Some(e.0); bb = {}; continue; }} }}",
            DISPATCH_BASE + id as u32
        )
    }

    fn jump(&self, pc: u32) -> String {
        format!("bb = {}; continue;", self.blocks.by_pc[&pc])
    }

    /// An error thrown at `pc` (ends the block).
    fn throw(&mut self, err: &str, pc: u32) {
        let e = self.tr(&format!("Err::<(), jrt::Throw>({err})"), pc);
        self.line(&format!("{e};"));
        self.line("unreachable!();");
    }

    fn emit(&mut self, pc: u32, insn: &Insn, next_block: Option<usize>) {
        match insn {
            Insn::Nop => {}
            Insn::AconstNull => {
                let v = self.push(K::A);
                self.line(&format!("{v} = None;"));
            }
            Insn::Iconst(c) => {
                let v = self.push(K::I);
                let c = if *c == i32::MIN {
                    "i32::MIN".to_string()
                } else {
                    format!("{c}i32")
                };
                self.line(&format!("{v} = {c};"));
            }
            Insn::Lconst(c) => {
                let v = self.push(K::J);
                let c = if *c == i64::MIN {
                    "i64::MIN".to_string()
                } else {
                    format!("{c}i64")
                };
                self.line(&format!("{v} = {c};"));
            }
            Insn::Fconst(c) => {
                let v = self.push(K::F);
                self.line(&format!("{v} = f32::from_bits({:#x});", c.to_bits()));
            }
            Insn::Dconst(c) => {
                let v = self.push(K::D);
                self.line(&format!("{v} = f64::from_bits({:#x});", c.to_bits()));
            }
            Insn::Ldc(i) => {
                let expr = match self.cf.pool.get(*i as usize) {
                    Some(Const::String(u)) => match self.cf.utf16(*u) {
                        Ok(s) => {
                            let s = s.to_vec();
                            format!("jrt::lit({})", self.lits.get(&s))
                        }
                        Err(_) => "None".into(),
                    },
                    Some(Const::Class(_)) => {
                        let name = self.cf.class_name(*i).unwrap_or_default();
                        match class_expr(self.world, &name) {
                            Some(c) => format!("jrt::class_object({c})"),
                            // A class left out still has a class object.
                            None => format!(
                                "jrt::class_object(jrt::missing_class({}))",
                                rust_str(&name)
                            ),
                        }
                    }
                    _ => {
                        self.note("ldc of a method handle, method type or dynamic constant".into());
                        self.push(K::A);
                        self.throw(
                            "jrt::unsupported(\"ldc of a method handle or dynamic constant\")",
                            pc,
                        );
                        return;
                    }
                };
                let v = self.push(K::A);
                self.line(&format!("{v} = {expr};"));
            }
            Insn::Load(k, n) => {
                let l = self.lv(*n, *k);
                let v = self.push(*k);
                let clone = if *k == K::A { ".clone()" } else { "" };
                self.line(&format!("{v} = {l}{clone};"));
            }
            Insn::Store(_, n) => {
                let (v, k) = self.pop();
                let l = self.lv(*n, k);
                let take = if k == K::A { ".take()" } else { "" };
                self.line(&format!("{l} = {v}{take};"));
            }
            Insn::ArrLoad(e) => {
                let idx = self.pop_val();
                let (arr, _) = self.pop();
                let (f, cast) = match e {
                    Elem::B => ("ald_b", " as i32"),
                    Elem::C => ("ald_c", " as i32"),
                    Elem::S => ("ald_s", " as i32"),
                    Elem::I => ("ald_i", ""),
                    Elem::J => ("ald_j", ""),
                    Elem::F => ("ald_f", ""),
                    Elem::D => ("ald_d", ""),
                    Elem::A => ("ald_a", ""),
                };
                let call = self.tr(&format!("jrt::{f}(&{arr}.take(), {idx})"), pc);
                let v = self.push(e.kind());
                self.line(&format!("{v} = {call}{cast};"));
            }
            Insn::ArrStore(e) => {
                let val = self.pop_val();
                let idx = self.pop_val();
                let (arr, _) = self.pop();
                let (f, val) = match e {
                    Elem::B => ("ast_b", format!("{val} as i8")),
                    Elem::C => ("ast_c", format!("{val} as u16")),
                    Elem::S => ("ast_s", format!("{val} as i16")),
                    Elem::I => ("ast_i", val),
                    Elem::J => ("ast_j", val),
                    Elem::F => ("ast_f", val),
                    Elem::D => ("ast_d", val),
                    Elem::A => ("ast_a", val),
                };
                let call = self.tr(&format!("jrt::{f}(&{arr}.take(), {idx}, {val})"), pc);
                self.line(&format!("{call};"));
            }
            Insn::Pop => {
                let v = self.pop_val();
                self.line(&format!("let _ = {v};"));
            }
            Insn::Pop2 => {
                let (v, k) = self.pop();
                if k == K::A {
                    self.line(&format!("{v} = None;"));
                }
                if !k.wide() {
                    let w = self.pop_val();
                    self.line(&format!("let _ = {w};"));
                }
            }
            Insn::Dup
            | Insn::DupX1
            | Insn::DupX2
            | Insn::Dup2
            | Insn::Dup2X1
            | Insn::Dup2X2
            | Insn::Swap => {
                let (g1, g2) = dup_groups(insn, &self.st).expect("analysis checked the stack");
                let n = g1 + g2;
                let base = self.st.len() - n;
                let old: Vec<K> = self.st[base..].to_vec();
                // New order, as indices into `old`.
                let order: Vec<usize> = if matches!(insn, Insn::Swap) {
                    vec![1, 0]
                } else {
                    (g2..n).chain(0..n).collect()
                };
                let mut code = String::from("{ ");
                for (t, &o) in order.iter().enumerate() {
                    let v = self.sv(base + o, old[o]);
                    let clone = if old[o] == K::A { ".clone()" } else { "" };
                    write!(code, "let t{t} = {v}{clone}; ").unwrap();
                }
                self.st.truncate(base);
                for (t, &o) in order.iter().enumerate() {
                    let v = self.push(old[o]);
                    write!(code, "{v} = t{t}; ").unwrap();
                }
                code.push('}');
                self.line(&code);
            }
            Insn::Arith(op, k) => self.arith(*op, *k, pc),
            Insn::Iinc(n, c) => {
                let l = self.lv(*n, K::I);
                self.line(&format!("{l} = {l}.wrapping_add({c});"));
            }
            Insn::Conv(_, to) => {
                let v = self.pop_val();
                let r = self.push(*to);
                self.line(&format!("{r} = {v} as {};", rust_ty(*to)));
            }
            Insn::I2b | Insn::I2c | Insn::I2s => {
                let v = self.pop_val();
                let t = match insn {
                    Insn::I2b => "i8",
                    Insn::I2c => "u16",
                    _ => "i16",
                };
                let r = self.push(K::I);
                self.line(&format!("{r} = {v} as {t} as i32;"));
            }
            Insn::Lcmp | Insn::Fcmp { .. } | Insn::Dcmp { .. } => {
                let b = self.pop_val();
                let a = self.pop_val();
                let f = match insn {
                    Insn::Lcmp => "lcmp",
                    Insn::Fcmp { nan_greater: true } => "fcmpg",
                    Insn::Fcmp { .. } => "fcmpl",
                    Insn::Dcmp { nan_greater: true } => "dcmpg",
                    _ => "dcmpl",
                };
                let r = self.push(K::I);
                self.line(&format!("{r} = jrt::{f}({a}, {b});"));
            }
            Insn::If(c, t) => {
                let v = self.pop_val();
                let j = self.jump(*t);
                self.line(&format!("if {v} {} 0 {{ {j} }}", c.rust()));
            }
            Insn::IfIcmp(c, t) => {
                let b = self.pop_val();
                let a = self.pop_val();
                let j = self.jump(*t);
                self.line(&format!("if {a} {} {b} {{ {j} }}", c.rust()));
            }
            Insn::IfAcmp { eq, target } => {
                let b = self.pop_val();
                let a = self.pop_val();
                let j = self.jump(*target);
                let not = if *eq { "" } else { "!" };
                self.line(&format!("if {not}jrt::same(&{a}, &{b}) {{ {j} }}"));
            }
            Insn::IfNull { null, target } => {
                let v = self.pop_val();
                let j = self.jump(*target);
                let test = if *null { "is_none" } else { "is_some" };
                self.line(&format!("if {v}.{test}() {{ {j} }}"));
            }
            Insn::Goto(t) => {
                let j = self.jump(*t);
                self.line(&j);
            }
            Insn::Jsr(t) => {
                let r = self.push(K::R);
                let ret = next_block.expect("jsr returns somewhere");
                let j = self.jump(*t);
                self.line(&format!("{r} = {ret}; {j}"));
            }
            Insn::Ret(n) => {
                let l = self.lv(*n, K::R);
                self.line(&format!("bb = {l}; continue;"));
            }
            Insn::TableSwitch {
                default,
                low,
                targets,
            } => {
                let v = self.pop_val();
                let mut arms = String::new();
                for (k, t) in targets.iter().enumerate() {
                    write!(
                        arms,
                        "{} => {}, ",
                        i64::from(*low) + k as i64,
                        self.blocks.by_pc[t]
                    )
                    .unwrap();
                }
                self.line(&format!(
                    "bb = match {v} {{ {arms}_ => {} }}; continue;",
                    self.blocks.by_pc[default]
                ));
            }
            Insn::LookupSwitch { default, pairs } => {
                let v = self.pop_val();
                let mut arms = String::new();
                for (key, t) in pairs {
                    write!(arms, "{key} => {}, ", self.blocks.by_pc[t]).unwrap();
                }
                self.line(&format!(
                    "bb = match {v} {{ {arms}_ => {} }}; continue;",
                    self.blocks.by_pc[default]
                ));
            }
            Insn::Return(k) => match k {
                None => self.line("return Ok(());"),
                Some(_) => {
                    let v = self.pop_val();
                    self.line(&format!("return Ok({v});"));
                }
            },
            Insn::GetStatic(i) | Insn::PutStatic(i) => {
                self.static_field(*i, matches!(insn, Insn::GetStatic(_)), pc)
            }
            Insn::GetField(i) | Insn::PutField(i) => {
                self.field(*i, matches!(insn, Insn::GetField(_)), pc)
            }
            Insn::Invoke(kind, i) => self.invoke(*kind, *i, pc),
            Insn::InvokeDynamic(i) => match crate::indy::parse(self.cf, *i) {
                crate::indy::Site::Concat(pieces) => self.concat(&pieces, pc),
                crate::indy::Site::Lambda(l) => self.lambda(&l, pc),
                crate::indy::Site::Unsupported(why) => {
                    let (params, ret) = match self.cf.pool.get(*i as usize) {
                        Some(Const::InvokeDynamic { nat, .. }) => {
                            let (_, d) = self.cf.name_and_type(*nat).unwrap_or_default();
                            let (p, r) = desc::method(&d);
                            (p.len(), r.map(desc::kind))
                        }
                        _ => (0, None),
                    };
                    for _ in 0..params {
                        let v = self.pop_val();
                        self.line(&format!("let _ = {v};"));
                    }
                    if let Some(k) = ret {
                        self.push(k);
                    }
                    self.note(why.clone());
                    self.throw(&format!("jrt::unsupported({})", rust_str(&why)), pc);
                }
            },
            Insn::New(i) => {
                let name = self.cf.class_name(*i).unwrap_or_default();
                let v = self.push(K::A);
                if !self.world.exists(&name) {
                    self.note(format!("class {name}"));
                    self.line(&format!("{v} = None;"));
                    self.throw(
                        &format!("jrt::unsupported({})", rust_str(&format!("class {name}"))),
                        pc,
                    );
                    return;
                }
                self.init_class(&name, pc);
                let c = self.world.class_static(&name);
                self.line(&format!("{v} = jrt::alloc(&{c});"));
            }
            Insn::NewArray(t) => {
                let n = self.pop_val();
                let call = self.tr(&format!("jrt::newarray(b'{}', {n})", *t as char), pc);
                let v = self.push(K::A);
                self.line(&format!("{v} = {call};"));
            }
            Insn::ANewArray(i) => {
                let n = self.pop_val();
                let name = self.cf.class_name(*i).unwrap_or_default();
                let v = self.push(K::A);
                match class_expr(self.world, &name) {
                    Some(c) => {
                        let call = self.tr(&format!("jrt::anewarray({c}, {n})"), pc);
                        self.line(&format!("{v} = {call};"));
                    }
                    None => {
                        self.note(format!("class {name}"));
                        self.throw(
                            &format!("jrt::unsupported({})", rust_str(&format!("class {name}"))),
                            pc,
                        );
                    }
                }
            }
            Insn::MultiANewArray(i, dims) => {
                let mut ds: Vec<String> = (0..*dims).map(|_| self.pop_val()).collect();
                ds.reverse();
                let name = self.cf.class_name(*i).unwrap_or_default();
                let v = self.push(K::A);
                match class_expr(self.world, &name) {
                    Some(c) => {
                        let call = self.tr(
                            &format!("jrt::multianewarray({c}, &[{}])", ds.join(", ")),
                            pc,
                        );
                        self.line(&format!("{v} = {call};"));
                    }
                    None => {
                        self.note(format!("class {name}"));
                        self.throw(
                            &format!("jrt::unsupported({})", rust_str(&format!("class {name}"))),
                            pc,
                        );
                    }
                }
            }
            Insn::ArrayLength => {
                let (a, _) = self.pop();
                let call = self.tr(&format!("jrt::alen(&{a}.take())"), pc);
                let v = self.push(K::I);
                self.line(&format!("{v} = {call};"));
            }
            Insn::Athrow => {
                let (a, _) = self.pop();
                self.throw(
                    &format!("match {a}.take() {{ Some(o) => jrt::Throw(o), None => jrt::npe() }}"),
                    pc,
                );
            }
            Insn::CheckCast(i) => {
                let name = self.cf.class_name(*i).unwrap_or_default();
                let top = self.sv(self.st.len() - 1, K::A);
                match class_expr(self.world, &name) {
                    Some(c) => {
                        let call = self.tr(&format!("jrt::checkcast(&{top}, {c})"), pc);
                        self.line(&format!("{call};"));
                    }
                    // A missing class has no instances: only null passes.
                    None => {
                        self.note(format!("class {name}"));
                        let call = self.tr(
                            &format!(
                                "if {top}.is_some() {{ Err(jrt::unsupported({})) }} else {{ Ok(()) }}",
                                rust_str(&format!("class {name}"))
                            ),
                            pc,
                        );
                        self.line(&format!("{call};"));
                    }
                }
            }
            Insn::InstanceOf(i) => {
                let name = self.cf.class_name(*i).unwrap_or_default();
                let a = self.pop_val();
                let v = self.push(K::I);
                match class_expr(self.world, &name) {
                    Some(c) => self.line(&format!("{v} = jrt::instance_of(&{a}, {c}) as i32;")),
                    None => self.line(&format!("let _ = {a}; {v} = 0;")),
                }
            }
            Insn::MonitorEnter | Insn::MonitorExit => {
                let (a, _) = self.pop();
                let call = self.tr(&format!("jrt::nn(&{a}.take()).map(|_| ())"), pc);
                self.line(&format!("{call};"));
            }
        }
    }

    fn arith(&mut self, op: Op, k: K, pc: u32) {
        if op == Op::Neg {
            let a = self.pop_val();
            let r = self.push(k);
            let e = if matches!(k, K::I | K::J) {
                format!("{a}.wrapping_neg()")
            } else {
                format!("-{a}")
            };
            self.line(&format!("{r} = {e};"));
            return;
        }
        let b = self.pop_val();
        let a = self.pop_val();
        let int = matches!(k, K::I | K::J);
        let (u, s) = if k == K::I {
            ("u32", "i32")
        } else {
            ("u64", "i64")
        };
        let e = match op {
            Op::Add if int => format!("{a}.wrapping_add({b})"),
            Op::Sub if int => format!("{a}.wrapping_sub({b})"),
            Op::Mul if int => format!("{a}.wrapping_mul({b})"),
            Op::Div if int => {
                let f = if k == K::I { "idiv" } else { "ldiv" };
                self.tr(&format!("jrt::{f}({a}, {b})"), pc)
            }
            Op::Rem if int => {
                let f = if k == K::I { "irem" } else { "lrem" };
                self.tr(&format!("jrt::{f}({a}, {b})"), pc)
            }
            Op::Add => format!("{a} + {b}"),
            Op::Sub => format!("{a} - {b}"),
            Op::Mul => format!("{a} * {b}"),
            Op::Div => format!("{a} / {b}"),
            Op::Rem => format!("{a} % {b}"),
            Op::Shl => format!("{a}.wrapping_shl({b} as u32)"),
            Op::Shr => format!("{a}.wrapping_shr({b} as u32)"),
            Op::Ushr => format!("({a} as {u}).wrapping_shr({b} as u32) as {s}"),
            Op::And => format!("{a} & {b}"),
            Op::Or => format!("{a} | {b}"),
            Op::Xor => format!("{a} ^ {b}"),
            Op::Neg => unreachable!(),
        };
        let r = self.push(k);
        self.line(&format!("{r} = {e};"));
    }

    /// Runs a translated class's initializer if this code may be the first
    /// to touch it (not needed for this class or its superclasses).
    fn init_class(&mut self, name: &str, pc: u32) {
        if self.world.is_native(name) {
            return;
        }
        let mut c = Some(self.cf.name.clone());
        while let Some(k) = c {
            if k == name {
                return;
            }
            c = self.world.super_of(&k);
        }
        let m = self.world.module(name);
        let call = self.tr(&format!("{m}::init()"), pc);
        self.line(&format!("{call};"));
    }

    fn static_field(&mut self, i: u16, get: bool, pc: u32) {
        let f = self.cf.member(i).expect("analysis read it");
        let k = desc::kind(&f.desc);
        let resolved = self.world.resolve_field(&f.class, &f.name, &f.desc);
        let Some((decl, _)) = resolved else {
            self.note(format!("field {}.{}:{}", f.class, f.name, f.desc));
            let what = rust_str(&format!("field {}.{}", f.class, f.name));
            if get {
                self.push(k);
            } else {
                let v = self.pop_val();
                self.line(&format!("let _ = {v};"));
            }
            self.throw(&format!("jrt::unsupported({what})"), pc);
            return;
        };
        if self.world.is_native(&decl) {
            let getter = jrt::spec::NATIVE_CLASSES
                .iter()
                .find(|c| c.name == decl)
                .and_then(|c| {
                    c.statics
                        .iter()
                        .find(|s| s.sig == format!("{}:{}", f.name, f.desc))
                        .map(|s| (c.module, s.getter))
                });
            match (get, getter) {
                (true, Some((module, g))) => {
                    let call = self.tr(&format!("jrt::{module}::{g}()"), pc);
                    let v = self.push(k);
                    self.line(&format!("{v} = {call};"));
                }
                _ => {
                    if !get {
                        let v = self.pop_val();
                        self.line(&format!("let _ = {v};"));
                    }
                    self.throw("jrt::unsupported(\"writing a runtime static field\")", pc);
                }
            }
            return;
        }
        let m = self.world.module(&decl);
        let fname = mangle::field(&f.name, &f.desc);
        if get {
            let call = self.tr(&format!("{m}::gs_{fname}()"), pc);
            let v = self.push(k);
            self.line(&format!("{v} = {call};"));
        } else {
            let v = self.pop_val();
            let call = self.tr(&format!("{m}::ps_{fname}({v})"), pc);
            self.line(&format!("{call};"));
        }
    }

    fn field(&mut self, i: u16, get: bool, pc: u32) {
        let f = self.cf.member(i).expect("analysis read it");
        let k = desc::kind(&f.desc);
        let slot = self
            .world
            .resolve_field(&f.class, &f.name, &f.desc)
            .filter(|(_, st)| !st)
            .and_then(|(decl, _)| self.world.layout(&decl))
            .and_then(|l| l.fields.get(&(f.name.clone(), f.desc.clone())).copied());
        let Some((_, idx)) = slot else {
            self.note(format!("field {}.{}:{}", f.class, f.name, f.desc));
            if !get {
                let v = self.pop_val();
                self.line(&format!("let _ = {v};"));
            }
            let o = self.pop_val();
            self.line(&format!("let _ = {o};"));
            if get {
                self.push(k);
            }
            self.throw(
                &format!(
                    "jrt::unsupported({})",
                    rust_str(&format!("field {}.{}", f.class, f.name))
                ),
                pc,
            );
            return;
        };
        let letter = k.letter();
        if get {
            let (o, _) = self.pop();
            let call = self.tr(&format!("jrt::gf_{letter}(&{o}.take(), {idx})"), pc);
            let v = self.push(k);
            self.line(&format!("{v} = {call};"));
        } else {
            let v = self.pop_val();
            let v = match f.desc.as_str() {
                "Z" => format!("{v} & 1"),
                "B" => format!("{v} as i8 as i32"),
                "C" => format!("{v} as u16 as i32"),
                "S" => format!("{v} as i16 as i32"),
                _ => v,
            };
            let (o, _) = self.pop();
            let call = self.tr(&format!("jrt::pf_{letter}(&{o}.take(), {idx}, {v})"), pc);
            self.line(&format!("{call};"));
        }
    }

    fn invoke(&mut self, kind: Invoke, i: u16, pc: u32) {
        let r = self.cf.member(i).expect("analysis read it");
        let s = sig(&r.name, &r.desc);
        let (params, ret) = desc::method(&r.desc);
        let mut args: Vec<String> = (0..params.len()).map(|_| self.pop_val()).collect();
        args.reverse();
        let recv = (kind != Invoke::Static).then(|| self.pop_val());
        let ret_k = ret.map(desc::kind);
        let what = format!("{}.{s}", r.class);
        let fail = |me: &mut Self, why: &str| {
            me.note(format!("{why} {what}"));
            if let Some(k) = ret_k {
                me.push(k);
            }
            me.throw(
                &format!("jrt::unsupported({})", rust_str(&format!("{why} {what}"))),
                pc,
            );
        };
        // Arrays answer Object's methods.
        let class = if r.class.starts_with('[') {
            jrt::spec::OBJECT.to_string()
        } else {
            r.class.clone()
        };
        let resolved = if kind == Invoke::Interface {
            self.world.resolve_interface_method(&class, &s)
        } else {
            self.world.resolve_method(&class, &s)
        };
        let Some((decl, info)) = resolved else {
            return fail(self, "method");
        };
        let fn_ty = desc::fn_type(&r.desc, kind == Invoke::Static);
        let call = match kind {
            Invoke::Static => {
                self.init_class(&decl, pc);
                let imp = Impl {
                    class: decl.clone(),
                    sig: s.clone(),
                };
                if !self.callable(&imp) {
                    return fail(self, "untranslated method");
                }
                format!("{}({})", self.world.fn_path(&imp), args.join(", "))
            }
            Invoke::Special => {
                let Some(imp) = direct_target(self.world, &class, &s) else {
                    return fail(self, "method");
                };
                if !self.callable(&imp) {
                    return fail(self, "untranslated method");
                }
                let recv = recv.unwrap();
                let check = self.tr("jrt::nn(&rc).map(|_| ())", pc);
                self.line(&format!("let rc = {recv}; {check};"));
                let mut all = vec!["rc".to_string()];
                all.extend(args);
                format!("{}({})", self.world.fn_path(&imp), all.join(", "))
            }
            Invoke::Virtual | Invoke::Interface => {
                let recv = recv.unwrap();
                // Private and final methods, and methods of final or native
                // direct kind, need no table.
                let direct = info.access & (access::PRIVATE | access::FINAL) != 0
                    || self.is_final_class(&decl);
                let mut all = vec!["Some(o.clone())".to_string()];
                all.extend(args);
                if direct {
                    let imp = Impl {
                        class: decl.clone(),
                        sig: s.clone(),
                    };
                    if !info.concrete || !self.callable(&imp) {
                        return fail(self, "untranslated method");
                    }
                    let fp = self.world.fn_path(&imp);
                    let rc = format!(
                        "let rc = {recv}; let o = {}; ",
                        self.tr("jrt::nn(&rc).cloned()", pc)
                    );
                    self.line(&rc);
                    format!("{fp}({})", all.join(", "))
                } else if self.world.is_interface(&decl) {
                    let idx = self
                        .world
                        .interface_methods(&decl)
                        .iter()
                        .position(|x| *x == s);
                    let Some(idx) = idx else {
                        return fail(self, "method");
                    };
                    let c = self.world.class_static(&decl);
                    let pre = format!(
                        "let rc = {recv}; let o = {}; ",
                        self.tr("jrt::nn(&rc).cloned()", pc)
                    );
                    self.line(&pre);
                    let f = self.tr(&format!("jrt::islot(&o, &{c}, {idx})"), pc);
                    format!(
                        "{{ let f: {fn_ty} = unsafe {{ std::mem::transmute({f}) }}; f({}) }}",
                        all.join(", ")
                    )
                } else {
                    let slot = self
                        .world
                        .layout(&class)
                        .and_then(|l| l.vtable.iter().position(|(x, _)| *x == s));
                    let Some(slot) = slot else {
                        return fail(self, "method");
                    };
                    let pre = format!(
                        "let rc = {recv}; let o = {}; ",
                        self.tr("jrt::nn(&rc).cloned()", pc)
                    );
                    self.line(&pre);
                    let f = self.tr(&format!("jrt::vslot(&o, {slot})"), pc);
                    format!(
                        "{{ let f: {fn_ty} = unsafe {{ std::mem::transmute({f}) }}; f({}) }}",
                        all.join(", ")
                    )
                }
            }
        };
        let call = self.tr(&call, pc);
        match ret_k {
            Some(k) => {
                let v = self.push(k);
                self.line(&format!("{v} = {call};"));
            }
            None => self.line(&format!("{call};")),
        }
    }

    /// Java 9+ string concatenation (`StringConcatFactory`).
    fn concat(&mut self, pieces: &[crate::indy::Piece], pc: u32) {
        use crate::indy::Piece;
        let n = pieces
            .iter()
            .filter(|p| matches!(p, Piece::Arg(..)))
            .count();
        let mut args: Vec<String> = (0..n).map(|_| self.pop_val()).collect();
        args.reverse();
        let mut code = String::from("{ let mut b: Vec<u16> = Vec::new(); ");
        for p in pieces {
            match p {
                Piece::Text(t) => {
                    let l = self.lits.get(t);
                    write!(code, "b.extend_from_slice({l}); ").unwrap();
                }
                Piece::Arg(i, d) => {
                    let a = &args[*i];
                    let piece = match d.as_bytes()[0] {
                        b'Z' => format!(
                            "jrt::concat_str(&mut b, if {a} != 0 {{ \"true\" }} else {{ \"false\" }}); "
                        ),
                        b'C' => format!("b.push({a} as u16); "),
                        b'J' | b'I' | b'B' | b'S' => {
                            format!("jrt::concat_str(&mut b, &{a}.to_string()); ")
                        }
                        b'F' => {
                            format!("jrt::concat_str(&mut b, &jrt::fmt::float_to_string({a})); ")
                        }
                        b'D' => {
                            format!("jrt::concat_str(&mut b, &jrt::fmt::double_to_string({a})); ")
                        }
                        _ => {
                            let t = self.tr(&format!("jrt::concat_any(&mut b, &{a})"), pc);
                            format!("{t}; ")
                        }
                    };
                    code.push_str(&piece);
                }
            }
        }
        let v = self.push(K::A);
        write!(code, "{v} = jrt::new_string(b); }}").unwrap();
        self.line(&code);
    }

    /// A lambda or method reference: a new instance of a class made for this
    /// call site, holding the captured values.
    fn lambda(&mut self, l: &crate::indy::Lambda, pc: u32) {
        let mut caps: Vec<String> = (0..l.captured.len()).map(|_| self.pop_val()).collect();
        caps.reverse();
        let k = self.extras.lambdas;
        self.extras.lambdas += 1;
        match lambda_class(self.world, self.reach, &self.cf.name, k, l) {
            Ok(code) => self.extras.code.push_str(&code),
            Err(why) => {
                self.note(why.clone());
                self.push(K::A);
                self.throw(&format!("jrt::unsupported({})", rust_str(&why)), pc);
                return;
            }
        }
        let mut code = format!("{{ let o = jrt::alloc(&LAMBDA_{k}); let r = o.as_ref().unwrap(); ");
        let (mut pi, mut ri) = (0, 0);
        for (c, d) in caps.iter().zip(&l.captured) {
            match desc::kind(d) {
                K::A => {
                    write!(code, "*r.refs[{ri}].borrow_mut() = {c}; ").unwrap();
                    ri += 1;
                }
                kd => {
                    let bits = match kd {
                        K::I => format!("{c} as i64 as u64"),
                        K::J => format!("{c} as u64"),
                        K::F => format!("u64::from({c}.to_bits())"),
                        _ => format!("{c}.to_bits()"),
                    };
                    write!(code, "r.prims[{pi}].set({bits}); ").unwrap();
                    pi += 1;
                }
            }
        }
        let v = self.push(K::A);
        write!(code, "{v} = o; }}").unwrap();
        self.line(&code);
    }

    fn is_final_class(&mut self, name: &str) -> bool {
        match self.world.class(name).map(|c| &c.source) {
            Some(crate::world::Source::Translated(cf)) => cf.access & access::FINAL != 0,
            _ => false,
        }
    }

    /// Whether an implementation exists in the output (native, or a
    /// reachable translated method).
    fn callable(&mut self, imp: &Impl) -> bool {
        self.world.is_native(&imp.class) || self.reach.methods.contains(imp)
    }
}

/// The Rust function of one method.
fn method_fn(
    world: &mut World,
    lits: &mut Lits,
    reach: &Reach,
    cf: &ClassFile,
    m: &Method,
    notes: &mut BTreeMap<String, usize>,
    extras: &mut Extras,
) -> String {
    let is_static = m.access & access::STATIC != 0;
    let (params, ret) = desc::method(&m.desc);
    let name = mangle::method(&m.name, &m.desc);
    let ret_ty = ret.map_or("()".to_string(), |r| rust_ty(desc::kind(r)).to_string());
    let mut sig_params = Vec::new();
    let mut slot = 0u16;
    let mut param_vars = BTreeSet::new();
    if !is_static {
        sig_params.push("mut l0a: jrt::Ref".to_string());
        param_vars.insert("l0a".to_string());
        slot = 1;
    }
    for p in &params {
        let k = desc::kind(p);
        let v = format!("l{slot}{}", k.letter());
        sig_params.push(format!("mut {v}: {}", rust_ty(k)));
        param_vars.insert(v);
        slot += if k.wide() { 2 } else { 1 };
    }
    let header = format!(
        "/// `{}{}`\npub fn {name}({}) -> jrt::JResult<{ret_ty}> {{\n",
        m.name,
        m.desc,
        sig_params.join(", ")
    );
    let args: Vec<String> = sig_params
        .iter()
        .map(|p| {
            p.trim_start_matches("mut ")
                .split(':')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .collect();
    let body = match (
        crate::world::overridden(&cf.name, &sig(&m.name, &m.desc)),
        &m.code,
    ) {
        (Some(rust), _) => format!("    jrt::{rust}({})\n", args.join(", ")),
        (None, None) => format!(
            "    Err(jrt::unsupported({}))\n",
            rust_str(&format!("native method {}.{}{}", cf.name, m.name, m.desc))
        ),
        (None, Some(code)) => {
            match method_body(world, lits, reach, cf, code, notes, &param_vars, extras) {
                Ok(b) => b,
                Err(e) => {
                    notes
                        .entry(format!(
                            "untranslatable {}.{}{}: {e}",
                            cf.name, m.name, m.desc
                        ))
                        .or_default();
                    format!(
                        "    Err(jrt::unsupported({}))\n",
                        rust_str(&format!("{}.{}{}: {e}", cf.name, m.name, m.desc))
                    )
                }
            }
        }
    };
    format!("{header}{body}}}\n\n")
}

#[allow(clippy::too_many_arguments)]
fn method_body(
    world: &mut World,
    lits: &mut Lits,
    reach: &Reach,
    cf: &ClassFile,
    code: &crate::classfile::Code,
    notes: &mut BTreeMap<String, usize>,
    params: &BTreeSet<String>,
    extras: &mut Extras,
) -> Result<String, String> {
    let insns = decode(&code.bytes, &cf.pool).map_err(|e| e.0)?;
    let blocks = analyze(cf, &insns, &code.handlers)?;
    let index: HashMap<u32, usize> = insns
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (*p, i))
        .collect();
    let mut g = MethodGen {
        world,
        lits,
        cf,
        reach,
        out: String::new(),
        vars: BTreeSet::new(),
        st: Vec::new(),
        blocks: &blocks,
        dispatch: Vec::new(),
        handlers: &code.handlers,
        notes,
        extras,
    };
    let mut arms = String::new();
    for (b, start) in blocks.starts.iter().enumerate() {
        let Some(entry) = &blocks.entry[b] else {
            continue;
        };
        g.st = entry.clone();
        g.out.clear();
        let mut k = index[start];
        let next_start = blocks.starts.get(b + 1).copied();
        loop {
            let (pc, insn) = &insns[k];
            let next_block = insns
                .get(k + 1)
                .and_then(|(p, _)| blocks.by_pc.get(p).copied());
            g.emit(*pc, insn, next_block);
            let ends = !insn.falls_through() || matches!(insn, Insn::Jsr(_));
            if ends {
                break;
            }
            k += 1;
            match insns.get(k) {
                Some((p, _)) if Some(*p) == next_start => {
                    let j = g.jump(*p);
                    g.line(&j);
                    break;
                }
                Some(_) => {}
                None => {
                    g.line("unreachable!(\"fell off the end of the code\");");
                    break;
                }
            }
        }
        writeln!(arms, "            {b} => {{\n{}            }}", g.out).unwrap();
    }
    // Exception dispatch blocks.
    let dispatch = std::mem::take(&mut g.dispatch);
    for (id, hs) in dispatch.iter().enumerate() {
        let mut d = String::from("                let e = ex.take().unwrap();\n");
        for &h in hs {
            let h = &code.handlers[h];
            let target = blocks.by_pc[&u32::from(h.handler)];
            g.vars.insert(("s0a".into(), K::A));
            match &h.catch {
                None => {
                    writeln!(d, "                s0a = Some(e); bb = {target}; continue;").unwrap();
                    break;
                }
                Some(c) => {
                    if let Some(ce) = class_expr(g.world, c) {
                        writeln!(
                            d,
                            "                if jrt::is_assignable(e.class, {ce}) {{ s0a = Some(e); bb = {target}; continue; }}"
                        )
                        .unwrap();
                    }
                }
            }
        }
        d.push_str("                return Err(jrt::Throw(e));\n");
        writeln!(
            arms,
            "            {} => {{\n{d}            }}",
            DISPATCH_BASE + id as u32
        )
        .unwrap();
    }
    let mut out = String::new();
    for (v, k) in &g.vars {
        if !params.contains(v) {
            writeln!(
                out,
                "    let mut {v}: {} = {};",
                rust_ty(*k),
                default_of(*k)
            )
            .unwrap();
        }
    }
    out.push_str("    let mut bb: u32 = 0;\n    let mut ex: Option<std::rc::Rc<jrt::Obj>> = None;\n    loop {\n        match bb {\n");
    out.push_str(&arms);
    out.push_str("            _ => unreachable!(),\n        }\n    }\n");
    Ok(out)
}

// ---------------------------------------------------------------- classes

/// The whole module of a translated class.
pub fn class_module(
    world: &mut World,
    lits: &mut Lits,
    reach: &Reach,
    name: &str,
    notes: &mut BTreeMap<String, usize>,
) -> String {
    let cf = match world.class(name).map(|c| &c.source) {
        Some(crate::world::Source::Translated(cf)) => cf.clone(),
        _ => return String::new(),
    };
    let layout = world.layout(name);
    let mut out = format!("//! `{}`\n\nuse std::cell::{{Cell, RefCell}};\n\n", cf.name);
    // The Class static.
    let sup = match (&cf.super_name, cf.is_interface()) {
        (Some(s), false) if world.exists(s) => format!("Some(&{})", world.class_static(s)),
        _ => "None".to_string(),
    };
    let fp = |world: &mut World, imp: &Option<Impl>| -> String {
        match imp {
            Some(i) if world.is_native(&i.class) || reach.methods.contains(i) => {
                format!("jrt::Fp({} as *const ())", world.fn_path(i))
            }
            _ => "jrt::Fp(std::ptr::null())".to_string(),
        }
    };
    let (prims, refs, vtable, itable, ifaces, ext) = match &layout {
        Some(l) => {
            let vt: Vec<String> = l.vtable.iter().map(|(_, i)| fp(world, i)).collect();
            let mut it = Vec::new();
            for (iface, imps) in &l.itable {
                if !world.exists(iface) {
                    continue;
                }
                let fps: Vec<String> = imps.iter().map(|i| fp(world, i)).collect();
                it.push(format!(
                    "(&{}, &[{}])",
                    world.class_static(iface),
                    fps.join(", ")
                ));
            }
            let mut ifs = Vec::new();
            for i in &l.own_ifaces {
                if world.exists(i) {
                    ifs.push(format!("&{}", world.class_static(i)));
                }
            }
            (l.prims, l.refs, vt, it, ifs, l.ext.clone())
        }
        None => (0, 0, Vec::new(), Vec::new(), Vec::new(), None),
    };
    writeln!(
        out,
        "pub static CLASS: jrt::Class = jrt::Class {{\n    name: {},\n    sup: {sup},\n    ifaces: &[{}],\n    interface: {},\n    prims: {prims},\n    refs: {refs},\n    vtable: &[{}],\n    itable: &[{}],\n    ext: {},\n    component: jrt::Component::None,\n    values: {},\n    fields: &FIELDS,\n    methods: &METHODS,\n}};\n",
        rust_str(&cf.name),
        ifaces.join(", "),
        cf.is_interface(),
        vtable.join(", "),
        itable.join(", "),
        ext.map_or("None".into(), |e| format!("Some({e})")),
        enum_values(&cf, reach).unwrap_or("None".into()),
    )
    .unwrap();
    // Static fields and the initializer.
    let statics: Vec<_> = cf
        .fields
        .iter()
        .filter(|f| f.access & access::STATIC != 0)
        .collect();
    out.push_str("pub struct Statics {\n    pub state: Cell<u8>,\n");
    for f in &statics {
        let k = desc::kind(&f.desc);
        let ty = if k == K::A {
            "RefCell<jrt::Ref>".to_string()
        } else {
            format!("Cell<{}>", rust_ty(k))
        };
        writeln!(out, "    pub {}: {ty},", mangle::field(&f.name, &f.desc)).unwrap();
    }
    out.push_str("}\n\nthread_local! {\n    pub static S: Statics = Statics {\n        state: Cell::new(0),\n");
    let mut const_strings = Vec::new();
    for f in &statics {
        let k = desc::kind(&f.desc);
        let fname = mangle::field(&f.name, &f.desc);
        let init = match (f.constant.and_then(|c| cf.pool.get(c as usize)), k) {
            (Some(Const::Int(v)), K::I) => format!("Cell::new({v}i32)"),
            (Some(Const::Long(v)), K::J) => format!("Cell::new({v}i64)"),
            (Some(Const::Float(v)), K::F) => {
                format!("Cell::new(f32::from_bits({:#x}))", v.to_bits())
            }
            (Some(Const::Double(v)), K::D) => {
                format!("Cell::new(f64::from_bits({:#x}))", v.to_bits())
            }
            (Some(Const::String(u)), K::A) => {
                if let Ok(s) = cf.utf16(*u) {
                    let s = s.to_vec();
                    const_strings.push((fname.clone(), lits.get(&s)));
                }
                "RefCell::new(None)".to_string()
            }
            (_, K::A) => "RefCell::new(None)".to_string(),
            (_, k) => format!("Cell::new({})", default_of(k)),
        };
        writeln!(out, "        {fname}: {init},").unwrap();
    }
    out.push_str("    };\n}\n\n");
    // init(): superclass first, then <clinit>. Re-entry (and failure)
    // returns at once, as the JVM's initialization lock does for its own
    // thread.
    out.push_str("/// Initializes the class (once per thread).\npub fn init() -> jrt::JResult<()> {\n    if S.with(|s| s.state.get()) != 0 {\n        return Ok(());\n    }\n    S.with(|s| s.state.set(1));\n");
    for (f, l) in &const_strings {
        writeln!(out, "    S.with(|s| *s.{f}.borrow_mut() = jrt::lit({l}));").unwrap();
    }
    if !cf.is_interface()
        && let Some(s) = &cf.super_name
        && world.exists(s)
        && !world.is_native(s)
    {
        writeln!(out, "    {}::init()?;", world.module(s)).unwrap();
    }
    let clinit = Impl {
        class: name.to_string(),
        sig: "<clinit>()V".into(),
    };
    if reach.methods.contains(&clinit) {
        writeln!(out, "    {}()?;", mangle::method("<clinit>", "()V")).unwrap();
    }
    out.push_str("    S.with(|s| s.state.set(2));\n    Ok(())\n}\n\n");
    for f in &statics {
        let k = desc::kind(&f.desc);
        let fname = mangle::field(&f.name, &f.desc);
        let ty = rust_ty(k);
        if k == K::A {
            writeln!(
                out,
                "pub fn gs_{fname}() -> jrt::JResult<jrt::Ref> {{\n    init()?;\n    Ok(S.with(|s| s.{fname}.borrow().clone()))\n}}\n\npub fn ps_{fname}(v: jrt::Ref) -> jrt::JResult<()> {{\n    init()?;\n    S.with(|s| *s.{fname}.borrow_mut() = v);\n    Ok(())\n}}\n"
            )
            .unwrap();
        } else {
            let v = match f.desc.as_str() {
                "Z" => "v & 1",
                "B" => "v as i8 as i32",
                "C" => "v as u16 as i32",
                "S" => "v as i16 as i32",
                _ => "v",
            };
            writeln!(
                out,
                "pub fn gs_{fname}() -> jrt::JResult<{ty}> {{\n    init()?;\n    Ok(S.with(|s| s.{fname}.get()))\n}}\n\npub fn ps_{fname}(v: {ty}) -> jrt::JResult<()> {{\n    init()?;\n    S.with(|s| s.{fname}.set({v}));\n    Ok(())\n}}\n"
            )
            .unwrap();
        }
    }
    // Methods.
    let mut extras = Extras::default();
    for m in &cf.methods {
        let imp = Impl {
            class: name.to_string(),
            sig: sig(&m.name, &m.desc),
        };
        if reach.methods.contains(&imp) {
            out.push_str(&method_fn(world, lits, reach, &cf, m, notes, &mut extras));
        }
    }
    out.push_str(&extras.code);
    out.push_str(&reflection(world, reach, &cf, layout.as_ref()));
    out
}

/// A value of kind `k` as a `Val`, and back.
fn to_val(k: K, e: &str) -> String {
    match k {
        K::I => format!("jrt::reflect::Val::I({e})"),
        K::J => format!("jrt::reflect::Val::J({e})"),
        K::F => format!("jrt::reflect::Val::F({e})"),
        K::D => format!("jrt::reflect::Val::D({e})"),
        _ => format!("jrt::reflect::Val::A({e})"),
    }
}

fn from_val(k: K, e: &str) -> String {
    let m = match k {
        K::I => "i",
        K::J => "j",
        K::F => "f",
        K::D => "d",
        _ => "a",
    };
    format!("{e}.{m}()")
}

/// The class's field and method tables (`jrt::reflect`), with an invoker
/// per translated method and accessors for static fields.
fn reflection(
    world: &mut World,
    reach: &Reach,
    cf: &ClassFile,
    layout: Option<&crate::world::Layout>,
) -> String {
    let mut out = String::new();
    let mut fields = Vec::new();
    for f in &cf.fields {
        let k = desc::kind(&f.desc);
        let generic = rust_str(f.signature.as_deref().unwrap_or(""));
        if f.access & access::STATIC != 0 {
            let fname = mangle::field(&f.name, &f.desc);
            let conv = match f.desc.as_str() {
                "Z" | "B" | "C" | "S" | "I" => "v.i()",
                "J" => "v.j()",
                "F" => "v.f()",
                "D" => "v.d()",
                _ => "v.a()",
            };
            writeln!(
                out,
                "fn sget_{fname}() -> jrt::JResult<jrt::reflect::Val> {{\n    Ok({})\n}}\n\nfn sset_{fname}(v: jrt::reflect::Val) -> jrt::JResult<()> {{\n    ps_{fname}({conv})\n}}\n",
                to_val(k, &format!("gs_{fname}()?"))
            )
            .unwrap();
            fields.push(format!(
                "jrt::reflect::FieldMeta {{ name: {}, desc: {}, generic: {generic}, is_static: true, index: 0, get: Some(sget_{fname}), set: Some(sset_{fname}) }}",
                rust_str(&f.name),
                rust_str(&f.desc)
            ));
        } else {
            let index = layout
                .and_then(|l| l.fields.get(&(f.name.clone(), f.desc.clone())))
                .map_or(0, |(_, i)| *i);
            fields.push(format!(
                "jrt::reflect::FieldMeta {{ name: {}, desc: {}, generic: {generic}, is_static: false, index: {index}, get: None, set: None }}",
                rust_str(&f.name),
                rust_str(&f.desc)
            ));
        }
    }
    let mut methods = Vec::new();
    for m in &cf.methods {
        let imp = Impl {
            class: cf.name.clone(),
            sig: sig(&m.name, &m.desc),
        };
        if !reach.methods.contains(&imp) {
            continue;
        }
        let is_static = m.access & access::STATIC != 0;
        let f = mangle::method(&m.name, &m.desc);
        let (params, ret) = desc::method(&m.desc);
        let mut args = Vec::new();
        let mut i = 0;
        if !is_static {
            args.push("a[0].a()".to_string());
            i = 1;
        }
        for p in &params {
            args.push(from_val(desc::kind(p), &format!("a[{i}]")));
            i += 1;
        }
        let call = format!("{f}({})?", args.join(", "));
        let body = match ret {
            Some(r) => format!("Ok({})", to_val(desc::kind(r), &call)),
            None => format!("{call};\n    Ok(jrt::reflect::Val::V)"),
        };
        writeln!(out, "fn inv_{f}(a: &[jrt::reflect::Val]) -> jrt::JResult<jrt::reflect::Val> {{\n    {body}\n}}\n").unwrap();
        methods.push(format!(
            "jrt::reflect::MethodMeta {{ sig: {}, is_static: {is_static}, call: inv_{f} }}",
            rust_str(&imp.sig)
        ));
    }
    let _ = world;
    writeln!(
        out,
        "static FIELDS: [jrt::reflect::FieldMeta; {}] = [{}];",
        fields.len(),
        fields.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "static METHODS: [jrt::reflect::MethodMeta; {}] = [{}];",
        methods.len(),
        methods.join(", ")
    )
    .unwrap();
    out
}

/// An enum class's `values()`, for `Enum.valueOf`.
fn enum_values(cf: &ClassFile, reach: &Reach) -> Option<String> {
    if cf.super_name.as_deref() != Some("java/lang/Enum") {
        return None;
    }
    let desc = format!("()[L{};", cf.name);
    let imp = Impl {
        class: cf.name.clone(),
        sig: sig("values", &desc),
    };
    reach
        .methods
        .contains(&imp)
        .then(|| format!("Some({})", mangle::method("values", &desc)))
}

/// Converts a value of descriptor `from` to descriptor `to` as the lambda
/// metafactory does (boxing, unboxing, widening).
fn adapt(e: &str, from: &str, to: &str) -> String {
    let (fk, tk) = (desc::kind(from), desc::kind(to));
    match (fk, tk) {
        (K::A, K::A) => e.to_string(),
        (K::A, _) => {
            let f = match to.as_bytes()[0] {
                b'J' => "unbox_j",
                b'F' => "unbox_f",
                b'D' => "unbox_d",
                _ => "unbox_i",
            };
            format!("jrt::{f}(&{e})?")
        }
        (_, K::A) => match from.as_bytes()[0] {
            b'J' => format!("jrt::box_j({e})?"),
            b'F' => format!("jrt::box_f({e})?"),
            b'D' => format!("jrt::box_d({e})?"),
            c => format!("jrt::box_i(b'{}', {e})?", c as char),
        },
        _ if fk == tk => e.to_string(),
        _ => format!("({e} as {})", rust_ty(tk)),
    }
}

/// The class a lambda call site makes: its `Class` static and the function
/// answering the interface method (one per descriptor, bridges included).
fn lambda_class(
    world: &mut World,
    reach: &Reach,
    owner: &str,
    k: usize,
    l: &crate::indy::Lambda,
) -> Result<String, String> {
    if !world.exists(&l.iface) {
        return Err(format!("lambda of missing interface {}", l.iface));
    }
    let t = &l.target;
    let tsig = sig(&t.name, &t.desc);
    // How to call the target, given its arguments.
    let (tparams, tret) = desc::method(&t.desc);
    let mut targets: Vec<String> = Vec::new();
    if matches!(l.kind, 5 | 7 | 9) {
        targets.push(format!("L{};", t.class));
    }
    targets.extend(tparams.iter().map(|p| p.to_string()));
    let tret: Option<String> = if l.kind == 8 {
        Some(format!("L{};", t.class))
    } else {
        tret.map(String::from)
    };
    let call = |world: &mut World, args: &[String]| -> Result<String, String> {
        match l.kind {
            6 => {
                let (decl, _) = world
                    .resolve_method(&t.class, &tsig)
                    .ok_or(format!("method {}.{tsig}", t.class))?;
                let imp = Impl {
                    class: decl.clone(),
                    sig: tsig.clone(),
                };
                if !world.is_native(&decl) && !reach.methods.contains(&imp) {
                    return Err(format!("untranslated method {}.{tsig}", t.class));
                }
                let init = if world.is_native(&decl) {
                    String::new()
                } else {
                    format!("{}::init()?; ", world.module(&decl))
                };
                Ok(format!(
                    "{{ {init}{}({})? }}",
                    world.fn_path(&imp),
                    args.join(", ")
                ))
            }
            8 => {
                let imp = direct_target(world, &t.class, &tsig)
                    .ok_or(format!("constructor {}.{tsig}", t.class))?;
                let init = if world.is_native(&t.class) {
                    String::new()
                } else {
                    format!("{}::init()?; ", world.module(&t.class))
                };
                let c = world.class_static(&t.class);
                let mut all = vec!["n.clone()".to_string()];
                all.extend(args.iter().cloned());
                Ok(format!(
                    "{{ {init}let n = jrt::alloc(&{c}); {}({})?; n }}",
                    world.fn_path(&imp),
                    all.join(", ")
                ))
            }
            7 => {
                let imp = direct_target(world, &t.class, &tsig)
                    .ok_or(format!("method {}.{tsig}", t.class))?;
                Ok(format!("{}({})?", world.fn_path(&imp), args.join(", ")))
            }
            _ => {
                let fn_ty = desc::fn_type(&t.desc, false);
                let recv = &args[0];
                let rest = args[1..].join(", ");
                let rest = if rest.is_empty() {
                    String::new()
                } else {
                    format!(", {rest}")
                };
                let resolved = if l.kind == 9 {
                    world.resolve_interface_method(&t.class, &tsig)
                } else {
                    world.resolve_method(&t.class, &tsig)
                };
                let (decl, info) = resolved.ok_or(format!("method {}.{tsig}", t.class))?;
                // Private and final methods (natives' direct ones too) have
                // no table slot: call them directly.
                let final_class = matches!(
                    world.class(&decl).map(|c| &c.source),
                    Some(crate::world::Source::Translated(cf)) if cf.access & access::FINAL != 0
                );
                if info.access & (access::PRIVATE | access::FINAL) != 0 || final_class {
                    let imp = Impl {
                        class: decl,
                        sig: tsig.clone(),
                    };
                    return Ok(format!("{}({recv}{rest})?", world.fn_path(&imp)));
                }
                let f = if world.is_interface(&decl) {
                    let idx = world
                        .interface_methods(&decl)
                        .iter()
                        .position(|x| *x == tsig)
                        .ok_or(format!("method {}.{tsig}", t.class))?;
                    format!("jrt::islot(&o, &{}, {idx})?", world.class_static(&decl))
                } else {
                    let class = if t.class.starts_with('[') {
                        jrt::spec::OBJECT.to_string()
                    } else {
                        t.class.clone()
                    };
                    let slot = world
                        .layout(&class)
                        .and_then(|lay| lay.vtable.iter().position(|(x, _)| *x == tsig))
                        .ok_or(format!("method {}.{tsig}", t.class))?;
                    format!("jrt::vslot(&o, {slot})?")
                };
                Ok(format!(
                    "{{ let rc = {recv}; let o = jrt::nn(&rc)?.clone(); let f: {fn_ty} = unsafe {{ std::mem::transmute({f}) }}; f(Some(o){rest})? }}"
                ))
            }
        }
    };
    // One forwarding function per descriptor the interface method answers to.
    let mut descs = vec![l.sam_desc.clone()];
    descs.extend(l.bridges.iter().cloned());
    let mut out = String::new();
    let mut fns: Vec<(String, String)> = Vec::new();
    for (j, d) in descs.iter().enumerate() {
        let (sparams, sret) = desc::method(d);
        let fname = format!("lambda_{k}_{j}");
        let mut params = vec!["this: jrt::Ref".to_string()];
        let mut sources: Vec<(String, String)> = Vec::new();
        let mut body = String::from("    let me = jrt::nn(&this)?.clone();\n");
        let (mut pi, mut ri) = (0, 0);
        for (c, cd) in l.captured.iter().enumerate() {
            let e = match desc::kind(cd) {
                K::A => {
                    ri += 1;
                    format!("me.refs[{}].borrow().clone()", ri - 1)
                }
                K::I => {
                    pi += 1;
                    format!("(me.prims[{}].get() as i32)", pi - 1)
                }
                K::J => {
                    pi += 1;
                    format!("(me.prims[{}].get() as i64)", pi - 1)
                }
                K::F => {
                    pi += 1;
                    format!("f32::from_bits(me.prims[{}].get() as u32)", pi - 1)
                }
                _ => {
                    pi += 1;
                    format!("f64::from_bits(me.prims[{}].get())", pi - 1)
                }
            };
            writeln!(body, "    let c{c} = {e};").unwrap();
            sources.push((format!("c{c}"), cd.clone()));
        }
        for (i, p) in sparams.iter().enumerate() {
            params.push(format!("p{i}: {}", rust_ty(desc::kind(p))));
            sources.push((format!("p{i}"), p.to_string()));
        }
        if sources.len() != targets.len() {
            return Err(format!("lambda arguments do not match {}.{tsig}", t.class));
        }
        let args: Vec<String> = sources
            .iter()
            .zip(&targets)
            .map(|((e, f), to)| adapt(e, f, to))
            .collect();
        let c = call(world, &args)?;
        let ret_ty = sret.map_or("()".to_string(), |r| rust_ty(desc::kind(r)).to_string());
        match (tret.as_deref(), sret) {
            (_, None) => writeln!(body, "    let _ = {c};\n    Ok(())").unwrap(),
            (None, Some(_)) => writeln!(body, "    {c};\n    Ok(None)").unwrap(),
            (Some(tr), Some(sr)) => {
                writeln!(body, "    let r = {c};\n    Ok({})", adapt("r", tr, sr)).unwrap()
            }
        }
        writeln!(
            out,
            "fn {fname}({}) -> jrt::JResult<{ret_ty}> {{\n{body}}}\n",
            params.join(", ")
        )
        .unwrap();
        fns.push((format!("{}{d}", l.name), fname));
    }
    // The class: Object's methods, the interfaces' methods.
    let mut ifaces = vec![l.iface.clone()];
    world.collect_interfaces(&l.iface, false, &mut ifaces);
    for m in &l.markers {
        if world.exists(m) && !ifaces.contains(m) {
            ifaces.push(m.clone());
        }
    }
    let mut itable = Vec::new();
    for i in &ifaces {
        let mut entries = Vec::new();
        for ms in world.interface_methods(i) {
            let e = match fns.iter().find(|(s, _)| *s == ms) {
                Some((_, f)) => format!("jrt::Fp({f} as *const ())"),
                None => {
                    let imp = Impl {
                        class: i.clone(),
                        sig: ms.clone(),
                    };
                    if world.declared(i, &ms).is_some_and(|m| m.concrete)
                        && reach.methods.contains(&imp)
                    {
                        format!("jrt::Fp({} as *const ())", world.fn_path(&imp))
                    } else {
                        "jrt::Fp(std::ptr::null())".to_string()
                    }
                }
            };
            entries.push(e);
        }
        itable.push(format!(
            "(&{}, &[{}])",
            world.class_static(i),
            entries.join(", ")
        ));
    }
    let (mut prims, mut refs) = (0, 0);
    for d in &l.captured {
        if desc::kind(d) == K::A {
            refs += 1;
        } else {
            prims += 1;
        }
    }
    let if_list: Vec<String> = ifaces
        .iter()
        .map(|i| format!("&{}", world.class_static(i)))
        .collect();
    writeln!(
        out,
        "pub static LAMBDA_{k}: jrt::Class = jrt::Class {{\n    name: {},\n    sup: Some(&jrt::C_java_lang_Object),\n    ifaces: &[{}],\n    interface: false,\n    prims: {prims},\n    refs: {refs},\n    vtable: &jrt::OBJECT_VTABLE,\n    itable: &[{}],\n    ext: None,\n    component: jrt::Component::None,\n    values: None,\n    fields: &[],\n    methods: &[],\n}};\n",
        rust_str(&format!("{owner}$$Lambda${k}")),
        if_list.join(", "),
        itable.join(", ")
    )
    .unwrap();
    Ok(out)
}
