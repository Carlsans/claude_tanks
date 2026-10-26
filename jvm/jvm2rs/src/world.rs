//! The classes a translation sees: translated ones (from class files) and
//! the runtime's native ones (`jrt::spec`). Lays them out the way the
//! runtime does (fields, method tables, interface tables), resolves fields
//! and methods by the JVM's rules, and finds what is reachable (rapid type
//! analysis: a virtual method counts only for classes that are instantiated).

use crate::classfile::{ClassFile, access};
use crate::classpath::ClassPath;
use crate::desc;
use crate::insn::{Insn, Invoke, K};
use crate::mangle;
use jrt::spec::{self, Kind, NativeClass};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub enum Source {
    Native(&'static NativeClass),
    Translated(ClassFile),
}

pub struct Class {
    pub name: String,
    pub source: Source,
    /// From the class path entry this translation emits (`Paths::own_source`).
    pub own: bool,
    pub sup: Option<String>,
    pub interfaces: Vec<String>,
    pub interface: bool,
}

/// A method with a body: in a translated class, or in the runtime.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Impl {
    pub class: String,
    /// Name and descriptor, `name(desc)ret`.
    pub sig: String,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub prims: u32,
    pub refs: u32,
    /// Instance fields declared here: (name, desc) to kind and index.
    pub fields: HashMap<(String, String), (K, u32)>,
    /// Signature and implementation per slot.
    pub vtable: Vec<(String, Option<Impl>)>,
    /// Interfaces it implements itself, with their superinterfaces.
    pub own_ifaces: Vec<String>,
    /// Every interface its instances implement, with its methods'
    /// implementations in the interface's order.
    pub itable: Vec<(String, Vec<Option<Impl>>)>,
    /// Inherited from the nearest native ancestor that has extra storage.
    pub ext: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct MethodInfo {
    pub access: u16,
    /// Has code (translated) or a Rust function (native).
    pub concrete: bool,
}

pub fn sig(name: &str, desc: &str) -> String {
    format!("{name}{desc}")
}

pub fn split_sig(sig: &str) -> (&str, &str) {
    let i = sig.find('(').unwrap_or(sig.len());
    (&sig[..i], &sig[i..])
}

/// Which Java library classes to translate. The library's internals are
/// entangled (locales, formatters, class loading, `Unsafe`); robots need its
/// collections and geometry. Classes outside are left out: code that
/// reaches them throws at run time.
pub struct Policy {
    /// Packages whose own classes (not subpackages') are translated.
    pub packages: Vec<String>,
    /// Single classes (with their nested classes) translated too.
    pub classes: Vec<String>,
    /// Classes left out although their package is in.
    pub deny: Vec<String>,
}

impl Policy {
    pub fn robots() -> Policy {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        Policy {
            packages: v(&[
                "java/util/",
                "java/util/function/",
                "java/awt/geom/",
                "jdk/internal/util/",
                "java/io/",
                "java/util/stream/",
                "java/util/regex/",
            ]),
            classes: v(&[
                "java/lang/Iterable",
                "java/lang/AutoCloseable",
                "java/awt/Shape",
                "java/security/PrivilegedAction",
                "sun/security/action/GetBooleanAction",
                "sun/security/action/GetPropertyAction",
            ]),
            deny: v(&[
                "java/util/Formatter",
                "java/util/Locale",
                "java/util/ResourceBundle",
                "java/util/ListResourceBundle",
                "java/util/PropertyResourceBundle",
                "java/util/Scanner",
                "java/util/Calendar",
                "java/util/GregorianCalendar",
                "java/util/JapaneseImperialCalendar",
                "java/util/Date",
                "java/util/TimeZone",
                "java/util/SimpleTimeZone",
                "java/util/ServiceLoader",
                "java/util/Currency",
                "java/util/Properties",
                "java/util/Timer",
                "java/util/TimerTask",
                "java/util/UUID",
                "java/util/Random",
                "java/util/SplittableRandom",
                "java/util/Base64",
                "java/util/HexFormat",
                "java/util/StringJoiner",
                "java/util/ImmutableCollections",
                "jdk/internal/util/SystemProps",
                "jdk/internal/util/StaticProperty",
                "java/io/File",
                "java/io/FileInputStream",
                "java/io/FileOutputStream",
                "java/io/FileReader",
                "java/io/FileWriter",
                "java/io/FileDescriptor",
                "java/io/FilePermission",
                "java/io/RandomAccessFile",
                "java/io/Console",
                "java/io/ObjectInputStream",
                "java/io/ObjectOutputStream",
                "java/io/ObjectStreamClass",
                "java/io/ObjectStreamField",
                "java/io/ObjectInputFilter",
                "java/io/PrintStream",
                "java/io/UnixFileSystem",
                "java/io/WinNTFileSystem",
                "java/io/FileSystem",
                "java/io/DefaultFileSystem",
                "java/io/ExpiringCache",
                "java/io/DeleteOnExitHook",
                "java/io/SerializablePermission",
            ]),
        }
    }

    fn matches(list: &[String], name: &str) -> bool {
        list.iter().any(|c| {
            name == c
                || name
                    .strip_prefix(c.as_str())
                    .is_some_and(|r| r.starts_with('$'))
        })
    }

    pub fn allows(&self, name: &str) -> bool {
        if Self::matches(&self.deny, name) {
            return false;
        }
        Self::matches(&self.classes, name)
            || self.packages.iter().any(|p| {
                name.strip_prefix(p.as_str())
                    .is_some_and(|r| !r.contains('/'))
            })
    }
}

/// Where translated classes live in Rust. A robot translated on its own
/// emits only its own classes (from one class-path entry); the rest is in a
/// crate translated once for all robots.
#[derive(Clone)]
pub struct Paths {
    /// Path prefix of the classes this translation emits.
    pub own: String,
    /// Path prefix of the others.
    pub other: String,
    /// The class-path entry whose classes are this translation's own; `None`
    /// for all of them.
    pub own_source: Option<usize>,
}

impl Default for Paths {
    fn default() -> Self {
        Paths {
            own: "crate".into(),
            other: "crate".into(),
            own_source: None,
        }
    }
}

pub struct World {
    pub cp: ClassPath,
    pub policy: Policy,
    pub paths: Paths,
    classes: HashMap<String, Option<Class>>,
    layouts: HashMap<String, Layout>,
    /// Classes that were referenced but found nowhere.
    pub missing: BTreeSet<String>,
    /// Library classes left out by the policy.
    pub excluded: BTreeSet<String>,
}

impl World {
    pub fn new(cp: ClassPath, policy: Policy) -> World {
        World {
            cp,
            policy,
            paths: Paths::default(),
            classes: HashMap::new(),
            layouts: HashMap::new(),
            missing: BTreeSet::new(),
            excluded: BTreeSet::new(),
        }
    }

    /// Loads a class (natives first, then the class path). `None` if missing.
    pub fn class(&mut self, name: &str) -> Option<&Class> {
        if !self.classes.contains_key(name) {
            let c = self.load(name);
            if c.is_none() && !self.excluded.contains(name) && !name.starts_with('[') {
                self.missing.insert(name.to_string());
            }
            self.classes.insert(name.to_string(), c);
        }
        self.classes.get(name).and_then(|c| c.as_ref())
    }

    fn load(&mut self, name: &str) -> Option<Class> {
        if let Some(n) = spec::NATIVE_CLASSES.iter().find(|c| c.name == name) {
            return Some(Class {
                name: name.to_string(),
                own: false,
                source: Source::Native(n),
                sup: n.sup.filter(|_| !n.interface).map(String::from),
                interfaces: n.interfaces.iter().map(|s| s.to_string()).collect(),
                interface: n.interface,
            });
        }
        if name.starts_with('[') {
            return None;
        }
        if self.cp.is_library(name) && !self.policy.allows(name) {
            self.excluded.insert(name.to_string());
            return None;
        }
        let own = match self.paths.own_source {
            None => true,
            Some(k) => self.cp.source_of(name) == Some(k),
        };
        match self.cp.load(name) {
            Ok(Some(cf)) => Some(Class {
                own,
                name: name.to_string(),
                sup: cf.super_name.clone().filter(|_| !cf.is_interface()),
                interfaces: cf.interfaces.clone(),
                interface: cf.is_interface(),
                source: Source::Translated(cf),
            }),
            Ok(None) => None,
            Err(e) => {
                eprintln!("warning: {e}");
                None
            }
        }
    }

    pub fn exists(&mut self, name: &str) -> bool {
        self.class(name).is_some()
    }

    pub fn is_native(&mut self, name: &str) -> bool {
        matches!(self.class(name).map(|c| &c.source), Some(Source::Native(_)))
    }

    pub fn super_of(&mut self, name: &str) -> Option<String> {
        self.class(name).and_then(|c| c.sup.clone())
    }

    pub fn is_interface(&mut self, name: &str) -> bool {
        self.class(name).is_some_and(|c| c.interface)
    }

    /// Whether `a` is `b` or extends or implements it.
    pub fn is_subtype(&mut self, a: &str, b: &str) -> bool {
        if a == b || b == spec::OBJECT {
            return true;
        }
        let Some(c) = self.class(a) else {
            return false;
        };
        let (sup, ifaces) = (c.sup.clone(), c.interfaces.clone());
        sup.is_some_and(|s| self.is_subtype(&s, b)) || ifaces.iter().any(|i| self.is_subtype(i, b))
    }

    /// The methods a class declares, by signature.
    pub fn declared(&mut self, class: &str, sig: &str) -> Option<MethodInfo> {
        let (name, d) = split_sig(sig);
        match &self.class(class)?.source {
            Source::Native(n) => n.methods.iter().find(|m| m.sig == sig).map(|m| MethodInfo {
                access: match m.kind {
                    Kind::Static => access::STATIC | access::PUBLIC,
                    Kind::Virtual => access::PUBLIC,
                    Kind::Direct => access::PUBLIC | access::FINAL,
                },
                concrete: !m.rust.is_empty(),
            }),
            Source::Translated(cf) => cf.method(name, d).map(|m| MethodInfo {
                access: m.access,
                concrete: m.code.is_some(),
            }),
        }
    }

    /// Method resolution (JVMS 5.4.3.3): the class, its superclasses, then
    /// its superinterfaces. Returns the declaring class.
    pub fn resolve_method(&mut self, class: &str, sig: &str) -> Option<(String, MethodInfo)> {
        let mut c = Some(class.to_string());
        while let Some(k) = c {
            if let Some(m) = self.declared(&k, sig) {
                return Some((k, m));
            }
            c = self.super_of(&k);
        }
        self.resolve_in_interfaces(class, sig)
    }

    /// The first declaration of `sig` among a class's superinterfaces,
    /// preferring ones with a body (default methods).
    fn resolve_in_interfaces(&mut self, class: &str, sig: &str) -> Option<(String, MethodInfo)> {
        let mut all = Vec::new();
        self.collect_interfaces(class, true, &mut all);
        let mut abstract_one = None;
        for i in all {
            if let Some(m) = self.declared(&i, sig)
                && m.access & (access::STATIC | access::PRIVATE) == 0
            {
                if m.concrete {
                    return Some((i, m));
                }
                abstract_one.get_or_insert((i, m));
            }
        }
        abstract_one
    }

    /// Interface method resolution: the interface, its superinterfaces, then
    /// Object's public methods.
    pub fn resolve_interface_method(
        &mut self,
        iface: &str,
        sig: &str,
    ) -> Option<(String, MethodInfo)> {
        if let Some(m) = self.declared(iface, sig) {
            return Some((iface.to_string(), m));
        }
        self.resolve_in_interfaces(iface, sig).or_else(|| {
            self.declared(spec::OBJECT, sig)
                .map(|m| (spec::OBJECT.to_string(), m))
        })
    }

    /// Every interface of a class: its own and (if `supers`) its
    /// superclasses', with their superinterfaces, nearest first.
    pub fn collect_interfaces(&mut self, class: &str, supers: bool, out: &mut Vec<String>) {
        let Some(c) = self.class(class) else {
            return;
        };
        let (ifaces, sup) = (c.interfaces.clone(), c.sup.clone());
        for i in &ifaces {
            if !out.contains(i) && self.exists(i) {
                out.push(i.clone());
                self.collect_interfaces(i, false, out);
            }
        }
        if supers && let Some(s) = sup {
            self.collect_interfaces(&s, true, out);
        }
    }

    /// Field resolution (JVMS 5.4.3.2): the class, its superinterfaces,
    /// then its superclass. Returns the declaring class and whether static.
    pub fn resolve_field(&mut self, class: &str, name: &str, desc: &str) -> Option<(String, bool)> {
        let c = self.class(class)?;
        let found = match &c.source {
            Source::Native(n) => {
                let s = format!("{name}:{desc}");
                n.statics.iter().any(|f| f.sig == s).then_some(true)
            }
            Source::Translated(cf) => cf
                .fields
                .iter()
                .find(|f| f.name == name && f.desc == desc)
                .map(|f| f.access & access::STATIC != 0),
        };
        if let Some(st) = found {
            return Some((class.to_string(), st));
        }
        let (ifaces, sup) = (c.interfaces.clone(), c.sup.clone());
        for i in ifaces {
            if let Some(r) = self.resolve_field(&i, name, desc) {
                return Some(r);
            }
        }
        self.resolve_field(&sup?, name, desc)
    }

    // ------------------------------------------------------------ layout

    pub fn layout(&mut self, name: &str) -> Option<Layout> {
        if let Some(l) = self.layouts.get(name) {
            return Some(l.clone());
        }
        let l = self.compute_layout(name)?;
        self.layouts.insert(name.to_string(), l.clone());
        Some(l)
    }

    fn compute_layout(&mut self, name: &str) -> Option<Layout> {
        let c = self.class(name)?;
        let (sup, interface) = (c.sup.clone(), c.interface);
        let mut l = match &sup {
            Some(s) if !interface => {
                let mut p = self.layout(s)?;
                p.fields.clear();
                p.own_ifaces.clear();
                p
            }
            _ => Layout {
                prims: 0,
                refs: 0,
                fields: HashMap::new(),
                vtable: Vec::new(),
                own_ifaces: Vec::new(),
                itable: Vec::new(),
                ext: None,
            },
        };
        let c = self.classes.get(name).unwrap().as_ref().unwrap();
        let native = match &c.source {
            Source::Native(n) => Some(*n),
            Source::Translated(_) => None,
        };
        // Fields.
        match &c.source {
            Source::Native(n) => {
                l.prims += n.prims;
                l.refs += n.refs;
                if !n.ext.is_empty() {
                    l.ext = Some(format!("jrt::{}::{}", n.module, n.ext));
                }
            }
            Source::Translated(cf) => {
                for f in cf.fields.iter().filter(|f| f.access & access::STATIC == 0) {
                    let k = desc::kind(&f.desc);
                    let idx = if k == K::A {
                        l.refs += 1;
                        l.refs - 1
                    } else {
                        l.prims += 1;
                        l.prims - 1
                    };
                    l.fields.insert((f.name.clone(), f.desc.clone()), (k, idx));
                }
            }
        }
        if interface {
            l.vtable.clear();
            l.itable.clear();
            return Some(l);
        }
        // Method table: inherited slots, overridden or extended.
        let own: Vec<(String, Option<Impl>)> = match &c.source {
            Source::Native(n) => n
                .methods
                .iter()
                .filter(|m| m.kind == Kind::Virtual)
                .map(|m| {
                    (
                        m.sig.to_string(),
                        (!m.rust.is_empty()).then(|| Impl {
                            class: name.to_string(),
                            sig: m.sig.to_string(),
                        }),
                    )
                })
                .collect(),
            Source::Translated(cf) => cf
                .methods
                .iter()
                .filter(|m| {
                    m.access & (access::STATIC | access::PRIVATE) == 0 && !m.name.starts_with('<')
                })
                .map(|m| {
                    let s = sig(&m.name, &m.desc);
                    let body = m.code.is_some() || m.access & access::NATIVE != 0;
                    (
                        s.clone(),
                        body.then(|| Impl {
                            class: name.to_string(),
                            sig: s,
                        }),
                    )
                })
                .collect(),
        };
        for (s, imp) in own {
            match l.vtable.iter_mut().find(|(x, _)| *x == s) {
                Some(slot) => slot.1 = imp,
                None => l.vtable.push((s, imp)),
            }
        }
        // Interfaces.
        let mut own_ifaces = Vec::new();
        self.collect_interfaces(name, false, &mut own_ifaces);
        let mut all = Vec::new();
        self.collect_interfaces(name, true, &mut all);
        if native.is_none() {
            // Interface methods no class here implements get slots too, with
            // the default method if there is one (as the JVM's "miranda"
            // methods), so `invokevirtual` can reach them.
            for i in &all {
                for s in self.interface_methods(i) {
                    let present = l.vtable.iter().any(|(x, imp)| *x == s && imp.is_some());
                    if present {
                        continue;
                    }
                    let default = self
                        .resolve_in_interfaces(name, &s)
                        .filter(|(_, m)| m.concrete)
                        .map(|(k, _)| Impl {
                            class: k,
                            sig: s.clone(),
                        });
                    match l.vtable.iter_mut().find(|(x, _)| *x == s) {
                        Some(slot) => slot.1 = default,
                        None => l.vtable.push((s, default)),
                    }
                }
            }
        }
        l.itable = all
            .iter()
            .map(|i| {
                let methods = self.interface_methods(i);
                let imps = methods
                    .iter()
                    .map(|s| match l.vtable.iter().find(|(x, _)| x == s) {
                        Some((_, imp)) => imp.clone(),
                        // Natives may implement an interface method directly.
                        None => native.and_then(|n| {
                            n.methods
                                .iter()
                                .find(|m| {
                                    m.sig == s.as_str()
                                        && !m.rust.is_empty()
                                        && m.kind != Kind::Static
                                })
                                .map(|m| Impl {
                                    class: name.to_string(),
                                    sig: m.sig.to_string(),
                                })
                        }),
                    })
                    .collect();
                (i.clone(), imps)
            })
            .collect();
        l.own_ifaces = own_ifaces;
        Some(l)
    }

    /// An interface's own overridable methods, in declaration order (its
    /// interface-table entries).
    pub fn interface_methods(&mut self, iface: &str) -> Vec<String> {
        match self.class(iface).map(|c| &c.source) {
            Some(Source::Native(n)) => n
                .methods
                .iter()
                .filter(|m| m.kind == Kind::Virtual)
                .map(|m| m.sig.to_string())
                .collect(),
            Some(Source::Translated(cf)) => cf
                .methods
                .iter()
                .filter(|m| {
                    m.access & (access::STATIC | access::PRIVATE) == 0 && !m.name.starts_with('<')
                })
                .map(|m| sig(&m.name, &m.desc))
                .collect(),
            None => Vec::new(),
        }
    }

    // ------------------------------------------------------------ Rust paths

    /// The Rust path of a class's `Class` static.
    pub fn class_static(&mut self, name: &str) -> String {
        if self.is_native(name) {
            format!("jrt::{}", mangle::native_class(name))
        } else {
            format!("{}::CLASS", self.module(name))
        }
    }

    /// Whether a class is one this translation emits.
    pub fn is_own(&self, name: &str) -> bool {
        self.classes
            .get(name)
            .and_then(|c| c.as_ref())
            .is_none_or(|c| c.own)
    }

    /// The Rust module of a translated class.
    pub fn module(&self, name: &str) -> String {
        let prefix = if self.is_own(name) {
            &self.paths.own
        } else {
            &self.paths.other
        };
        format!("{prefix}::c::{}", mangle::class(name))
    }

    /// The Rust function of a method implementation.
    pub fn fn_path(&mut self, imp: &Impl) -> String {
        let (name, d) = split_sig(&imp.sig);
        match self.class(&imp.class).map(|c| &c.source) {
            Some(Source::Native(n)) => {
                let m = n
                    .methods
                    .iter()
                    .find(|m| m.sig == imp.sig)
                    .expect("native method");
                format!("jrt::{}::{}", n.module, m.rust)
            }
            _ => format!("{}::{}", self.module(&imp.class), mangle::method(name, d)),
        }
    }
}

// ---------------------------------------------------------------- reachability

/// What a translation must contain.
#[derive(Default)]
pub struct Reach {
    /// Translated methods to emit.
    pub methods: BTreeSet<Impl>,
    queue: Vec<Impl>,
    pub instantiated: BTreeSet<String>,
    /// Virtual and interface call targets seen: (class, signature).
    vcalls: BTreeSet<(String, String)>,
    /// Classes whose `Class` static (and module) the output needs.
    pub used: BTreeSet<String>,
    /// Members that were referenced but could not be resolved.
    pub unresolved: BTreeMap<String, usize>,
}

impl Reach {
    /// Starts from `roots` and the `Object` methods the runtime calls
    /// virtually (toString, hashCode, equals).
    pub fn run(world: &mut World, roots: &[Impl], instantiated: &[String]) -> Reach {
        Reach::run_with(world, roots, instantiated, &[])
    }

    /// Also keeps the `Class` statics of `used` classes.
    pub fn run_with(
        world: &mut World,
        roots: &[Impl],
        instantiated: &[String],
        used: &[String],
    ) -> Reach {
        let mut r = Reach::default();
        for c in used {
            r.use_class(world, c);
        }
        for s in &spec::OBJECT_SLOTS[..3] {
            r.vcall(world, spec::OBJECT, s);
        }
        for (c, s) in spec::CALLBACKS {
            if world.exists(c) {
                r.vcall(world, c, s);
            }
        }
        let json: Vec<String> = world
            .cp
            .archive_classes()
            .filter(|n| spec::JSON_PACKAGES.iter().any(|p| n.starts_with(p)))
            .cloned()
            .collect();
        for c in json {
            if world.exists(&c) && !world.is_interface(&c) {
                r.instantiate(world, &c);
                if world.declared(&c, "<init>()V").is_some_and(|m| m.concrete) {
                    r.add(
                        world,
                        Impl {
                            class: c.clone(),
                            sig: "<init>()V".into(),
                        },
                    );
                }
            }
        }
        for (c, s) in spec::REFLECTED {
            if !world.exists(c) {
                continue;
            }
            if s.starts_with("<init>") {
                r.instantiate(world, c);
            }
            match world.resolve_method(c, s) {
                Some((decl, m))
                    if m.access & (access::STATIC | access::PRIVATE) != 0
                        || s.starts_with("<init>") =>
                {
                    r.init(world, &decl);
                    r.add(
                        world,
                        Impl {
                            class: decl,
                            sig: s.to_string(),
                        },
                    );
                }
                Some((decl, _)) => r.vcall(world, &decl, s),
                None => {}
            }
        }
        for c in instantiated {
            r.instantiate(world, c);
        }
        for m in roots {
            r.add(world, m.clone());
        }
        while let Some(m) = r.queue.pop() {
            r.scan(world, &m);
        }
        r
    }

    fn add(&mut self, world: &mut World, m: Impl) {
        self.use_class(world, &m.class);
        if world.is_native(&m.class) || self.methods.contains(&m) {
            return;
        }
        self.methods.insert(m.clone());
        self.queue.push(m);
    }

    pub fn use_class(&mut self, world: &mut World, name: &str) {
        let name = name.trim_start_matches('[');
        let name = desc::class_of(name).unwrap_or(name);
        if name.len() == 1 && "ZBCSIJFD".contains(name) {
            return;
        }
        if self.used.contains(name) || !world.exists(name) {
            return;
        }
        self.used.insert(name.to_string());
        if let Some(s) = world.super_of(name) {
            // Enum.valueOf finds the constants through values().
            if s == "java/lang/Enum" {
                let values = sig("values", &format!("()[L{name};"));
                if world.declared(name, &values).is_some_and(|m| m.concrete) {
                    self.add(
                        world,
                        Impl {
                            class: name.to_string(),
                            sig: values,
                        },
                    );
                }
            }
            self.use_class(world, &s);
        }
        let mut ifaces = Vec::new();
        world.collect_interfaces(name, true, &mut ifaces);
        for i in ifaces {
            self.use_class(world, &i);
        }
    }

    /// A class whose static initializer may run.
    fn init(&mut self, world: &mut World, name: &str) {
        self.use_class(world, name);
        let mut c = Some(name.to_string());
        while let Some(k) = c {
            if world
                .declared(&k, "<clinit>()V")
                .is_some_and(|m| m.concrete)
            {
                self.add(
                    world,
                    Impl {
                        class: k.clone(),
                        sig: "<clinit>()V".into(),
                    },
                );
            }
            c = world.super_of(&k);
        }
    }

    fn instantiate(&mut self, world: &mut World, name: &str) {
        self.init(world, name);
        if !self.instantiated.insert(name.to_string()) {
            return;
        }
        let calls: Vec<(String, String)> = self.vcalls.iter().cloned().collect();
        for (c, s) in calls {
            if world.is_subtype(name, &c) {
                self.dispatch(world, name, &s);
            }
        }
    }

    fn vcall(&mut self, world: &mut World, class: &str, sig: &str) {
        if !self.vcalls.insert((class.to_string(), sig.to_string())) {
            return;
        }
        let inst: Vec<String> = self.instantiated.iter().cloned().collect();
        for k in inst {
            if world.is_subtype(&k, class) {
                self.dispatch(world, &k, sig);
            }
        }
    }

    /// The implementation an instance of `class` runs for `sig`.
    fn dispatch(&mut self, world: &mut World, class: &str, sig: &str) {
        if let Some(l) = world.layout(class)
            && let Some((_, Some(imp))) = l.vtable.iter().find(|(s, _)| s == sig)
        {
            self.add(world, imp.clone());
        }
    }

    fn unresolved(&mut self, what: String) {
        *self.unresolved.entry(what).or_default() += 1;
    }

    fn scan(&mut self, world: &mut World, m: &Impl) {
        if overridden(&m.class, &m.sig).is_some() {
            return;
        }
        let (name, d) = split_sig(&m.sig);
        let Some(Source::Translated(cf)) = world.class(&m.class).map(|c| &c.source) else {
            return;
        };
        let Some(code) = cf.method(name, d).and_then(|mm| mm.code.clone()) else {
            return;
        };
        let cf_name = cf.name.clone();
        let pool = cf.pool.clone();
        let insns = match crate::insn::decode(&code.bytes, &pool) {
            Ok(i) => i,
            Err(e) => {
                self.unresolved(format!("{cf_name}.{}: {e}", m.sig));
                return;
            }
        };
        // Borrowing the class file across world calls is not possible, so
        // look members up through a copy of what is needed.
        let cf = match world.class(&m.class).map(|c| &c.source) {
            Some(Source::Translated(cf)) => cf.clone(),
            _ => return,
        };
        for h in &code.handlers {
            if let Some(c) = &h.catch {
                self.use_class(world, c);
            }
        }
        for (_, insn) in insns {
            match insn {
                Insn::New(i) => {
                    if let Ok(c) = cf.class_name(i) {
                        if world.exists(&c) {
                            self.instantiate(world, &c);
                        } else {
                            self.unresolved(format!("class {c}"));
                        }
                    }
                }
                Insn::GetStatic(i) | Insn::PutStatic(i) | Insn::GetField(i) | Insn::PutField(i) => {
                    let Ok(f) = cf.member(i) else { continue };
                    match world.resolve_field(&f.class, &f.name, &f.desc) {
                        Some((decl, true)) => self.init(world, &decl),
                        Some((decl, false)) => self.use_class(world, &decl),
                        None => self.unresolved(format!("field {}.{}:{}", f.class, f.name, f.desc)),
                    }
                }
                Insn::Invoke(kind, i) => {
                    let Ok(mut r) = cf.member(i) else { continue };
                    if r.class.starts_with('[') {
                        r.class = spec::OBJECT.to_string();
                    }
                    let s = sig(&r.name, &r.desc);
                    let resolved = if kind == Invoke::Interface {
                        world.resolve_interface_method(&r.class, &s)
                    } else {
                        world.resolve_method(&r.class, &s)
                    };
                    let Some((decl, info)) = resolved else {
                        self.unresolved(format!("method {}.{s}", r.class));
                        continue;
                    };
                    self.use_class(world, &r.class);
                    match kind {
                        Invoke::Static => {
                            self.init(world, &decl);
                            self.add(
                                world,
                                Impl {
                                    class: decl,
                                    sig: s,
                                },
                            );
                        }
                        Invoke::Special => {
                            // Constructors, private methods and super calls:
                            // the nearest concrete one.
                            match direct_target(world, &r.class, &s) {
                                Some(imp) => self.add(world, imp),
                                None => self.unresolved(format!("method {}.{s}", r.class)),
                            }
                        }
                        Invoke::Virtual | Invoke::Interface => {
                            // Private and final methods are called directly,
                            // as the generated code does.
                            let final_class = matches!(
                                world.class(&decl).map(|c| &c.source),
                                Some(Source::Translated(cf)) if cf.access & access::FINAL != 0
                            );
                            let direct =
                                info.access & (access::PRIVATE | access::FINAL) != 0 || final_class;
                            if direct {
                                if info.concrete {
                                    self.add(
                                        world,
                                        Impl {
                                            class: decl,
                                            sig: s,
                                        },
                                    );
                                }
                            } else {
                                self.use_class(world, &decl);
                                self.vcall(world, &decl, &s);
                            }
                        }
                    }
                }
                Insn::Ldc(i) => {
                    if let Ok(c) = cf.class_name(i) {
                        self.use_class(world, &c);
                    }
                }
                Insn::CheckCast(i)
                | Insn::InstanceOf(i)
                | Insn::ANewArray(i)
                | Insn::MultiANewArray(i, _) => {
                    if let Ok(c) = cf.class_name(i) {
                        self.use_class(world, &c);
                    }
                }
                Insn::InvokeDynamic(i) => match crate::indy::parse(&cf, i) {
                    crate::indy::Site::Concat(_) => {}
                    crate::indy::Site::Lambda(l) => self.lambda(world, &l),
                    crate::indy::Site::Unsupported(w) => {
                        self.unresolved(format!("{w} in {cf_name}"))
                    }
                },
                _ => {}
            }
        }
        let _ = cf_name;
    }
}

impl Reach {
    /// What a lambda class keeps alive: its interfaces' default methods and
    /// its target.
    fn lambda(&mut self, world: &mut World, l: &crate::indy::Lambda) {
        let mut ifaces = vec![l.iface.clone()];
        world.collect_interfaces(&l.iface, false, &mut ifaces);
        ifaces.extend(l.markers.iter().cloned());
        for i in &ifaces {
            self.use_class(world, i);
            for s in world.interface_methods(i) {
                if world.declared(i, &s).is_some_and(|m| m.concrete) {
                    self.add(
                        world,
                        Impl {
                            class: i.clone(),
                            sig: s,
                        },
                    );
                }
            }
        }
        let t = &l.target;
        let s = sig(&t.name, &t.desc);
        match l.kind {
            6 => match world.resolve_method(&t.class, &s) {
                Some((decl, _)) => {
                    self.init(world, &decl);
                    self.add(
                        world,
                        Impl {
                            class: decl,
                            sig: s,
                        },
                    );
                }
                None => self.unresolved(format!("method {}.{s}", t.class)),
            },
            7 | 8 => {
                if l.kind == 8 {
                    self.instantiate(world, &t.class);
                }
                match direct_target(world, &t.class, &s) {
                    Some(imp) => self.add(world, imp),
                    None => self.unresolved(format!("method {}.{s}", t.class)),
                }
            }
            _ => {
                let r = if l.kind == 9 {
                    world.resolve_interface_method(&t.class, &s)
                } else {
                    world.resolve_method(&t.class, &s)
                };
                match r {
                    Some((decl, m)) if m.access & access::PRIVATE != 0 => self.add(
                        world,
                        Impl {
                            class: decl,
                            sig: s,
                        },
                    ),
                    Some((decl, _)) => {
                        self.use_class(world, &decl);
                        self.vcall(world, &decl, &s);
                    }
                    None => self.unresolved(format!("method {}.{s}", t.class)),
                }
            }
        }
    }
}

/// The method an `invokespecial` of `sig` on `class` runs: the nearest
/// concrete declaration up the superclasses, then a default method.
pub fn direct_target(world: &mut World, class: &str, sig: &str) -> Option<Impl> {
    let mut c = Some(class.to_string());
    while let Some(k) = c {
        if let Some(m) = world.declared(&k, sig)
            && m.concrete
        {
            return Some(Impl {
                class: k,
                sig: sig.to_string(),
            });
        }
        if sig.starts_with("<init>") {
            // Constructors are not inherited.
            return None;
        }
        c = world.super_of(&k);
    }
    world
        .resolve_in_interfaces(class, sig)
        .filter(|(_, m)| m.concrete)
        .map(|(k, _)| Impl {
            class: k,
            sig: sig.to_string(),
        })
}

/// The Rust function replacing a translated method, if any (`jrt::spec::OVERRIDES`).
pub fn overridden(class: &str, sig: &str) -> Option<&'static str> {
    spec::OVERRIDES
        .iter()
        .find(|(c, s, _)| *c == class && *s == sig)
        .map(|(_, _, r)| *r)
}
