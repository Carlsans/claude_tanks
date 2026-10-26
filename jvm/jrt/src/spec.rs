// The classes the runtime implements in Rust ("native classes"), as plain
// data. The runtime's build script reads this file to lay out their method
// tables, and the translator reads it (as `jrt::spec`) to lay out the classes
// that extend them and to call their methods. Keep it free of dependencies.

/// How a method is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `invokestatic`.
    Static,
    /// Overridable: has a method-table slot.
    Virtual,
    /// Constructors, private and final methods: called directly.
    Direct,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeMethod {
    /// Name and descriptor, e.g. `length()I`.
    pub sig: &'static str,
    pub kind: Kind,
    /// The Rust function, relative to the class's module. Empty for abstract
    /// methods (interfaces, abstract classes).
    pub rust: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeField {
    /// Name and descriptor, e.g. `out:Ljava/io/PrintStream;`.
    pub sig: &'static str,
    /// Static fields: the Rust getter, relative to the class's module.
    pub getter: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeClass {
    /// Internal name, e.g. `java/lang/String`.
    pub name: &'static str,
    /// Module of its Rust functions, relative to the runtime crate root.
    pub module: &'static str,
    pub sup: Option<&'static str>,
    pub interfaces: &'static [&'static str],
    pub interface: bool,
    /// Instance storage that subclasses build on.
    pub prims: u32,
    pub refs: u32,
    /// Rust function making the extra storage of a new instance (`Ext`).
    pub ext: &'static str,
    pub methods: &'static [NativeMethod],
    pub statics: &'static [NativeField],
}

const fn m(sig: &'static str, kind: Kind, rust: &'static str) -> NativeMethod {
    NativeMethod { sig, kind, rust }
}

const fn v(sig: &'static str, rust: &'static str) -> NativeMethod {
    m(sig, Kind::Virtual, rust)
}

const fn s(sig: &'static str, rust: &'static str) -> NativeMethod {
    m(sig, Kind::Static, rust)
}

const fn d(sig: &'static str, rust: &'static str) -> NativeMethod {
    m(sig, Kind::Direct, rust)
}

const fn iface(
    name: &'static str,
    sup: &'static [&'static str],
    methods: &'static [NativeMethod],
) -> NativeClass {
    NativeClass {
        name,
        module: "n::iface",
        sup: Some("java/lang/Object"),
        interfaces: sup,
        interface: true,
        prims: 0,
        refs: 0,
        ext: "",
        methods,
        statics: &[],
    }
}

/// An exception class with the usual constructors, extending `sup`.
const fn exc(name: &'static str, sup: &'static str) -> NativeClass {
    NativeClass {
        name,
        module: "n::throwable",
        sup: Some(sup),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: EXC_METHODS,
        statics: &[],
    }
}

const fn index_exc(
    name: &'static str,
    sup: &'static str,
    methods: &'static [NativeMethod],
) -> NativeClass {
    NativeClass {
        methods,
        ..exc(name, sup)
    }
}

const INT_TYPE: &[NativeField] = &[NativeField {
    sig: "TYPE:Ljava/lang/Class;",
    getter: "type_class",
}];

const ASSERTION_METHODS: &[NativeMethod] = &[
    d("<init>()V", "init"),
    d("<init>(Ljava/lang/Object;)V", "init_object"),
    d(
        "<init>(Ljava/lang/String;Ljava/lang/Throwable;)V",
        "init_msg_cause",
    ),
];

const IOOBE_METHODS: &[NativeMethod] = &[
    d("<init>()V", "init"),
    d("<init>(Ljava/lang/String;)V", "init_msg"),
    d("<init>(I)V", "init_ioobe_index"),
    d("<init>(J)V", "init_ioobe_index_j"),
];
const AIOOBE_METHODS: &[NativeMethod] = &[
    d("<init>()V", "init"),
    d("<init>(Ljava/lang/String;)V", "init_msg"),
    d("<init>(I)V", "init_aioobe_index"),
];
const SIOOBE_METHODS: &[NativeMethod] = &[
    d("<init>()V", "init"),
    d("<init>(Ljava/lang/String;)V", "init_msg"),
    d("<init>(I)V", "init_sioobe_index"),
];

const EXC_METHODS: &[NativeMethod] = &[
    d(
        "<init>(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
        "init_msg_cause_flags",
    ),
    d("<init>()V", "init"),
    d("<init>(Ljava/lang/String;)V", "init_msg"),
    d(
        "<init>(Ljava/lang/String;Ljava/lang/Throwable;)V",
        "init_msg_cause",
    ),
    d("<init>(Ljava/lang/Throwable;)V", "init_cause"),
];

/// The primitive types' boxes share one shape: the value in `prims[0]`.
const fn boxed(
    name: &'static str,
    module: &'static str,
    methods: &'static [NativeMethod],
) -> NativeClass {
    NativeClass {
        name,
        module,
        sup: Some("java/lang/Number"),
        interfaces: &["java/lang/Comparable"],
        interface: false,
        prims: 1,
        refs: 0,
        ext: "",
        methods,
        statics: &[],
    }
}

pub const OBJECT: &str = "java/lang/Object";

/// Object's method-table slots, in order. The runtime calls them by index.
pub const OBJECT_SLOTS: [&str; 5] = [
    "equals(Ljava/lang/Object;)Z",
    "hashCode()I",
    "toString()Ljava/lang/String;",
    "clone()Ljava/lang/Object;",
    "finalize()V",
];

/// Translated methods replaced by Rust functions (relative to the runtime
/// crate root): library methods that read memory through `Unsafe`, or
/// whose translation would drag in too much.
pub const OVERRIDES: &[(&str, &str, &str)] = &[
    (
        "jdk/internal/util/ArraysSupport",
        "vectorizedMismatch(Ljava/lang/Object;JLjava/lang/Object;JII)I",
        "n::unsafe_::vectorized_mismatch",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/json/JsonConverter",
        "fromJson(Lcom/google/gson/JsonObject;Ljava/lang/Class;)Ljava/lang/Object;",
        "n::json::from_tree",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/json/JsonConverter",
        "fromJson(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;",
        "n::json::from_string",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/json/JsonConverter",
        "toJson(Ljava/lang/Object;)Ljava/lang/String;",
        "n::json::to_json",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "verifyServerVersionCompatibility(Ljava/lang/String;)V",
        "n::system::ignore_text",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/BaseBotInternals",
        "redirectStdOutAndStdErr()V",
        "n::system::nop_this",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/BaseBotInternals",
        "transferStdOutToBotIntent()V",
        "n::system::nop_this",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/BotHandshakeFactory",
        "isDebuggerAttached()Z",
        "n::system::no_static",
    ),
    (
        "dev/robocode/tankroyale/botapi/util/CountryCodeUtil",
        "getLocalCountryCode()Ljava/lang/String;",
        "n::system::null_static",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "onText(Ljava/net/http/WebSocket;Ljava/lang/CharSequence;Z)Ljava/util/concurrent/CompletionStage;",
        "n::json::ws_on_text",
    ),
];

/// Packages of classes the runtime creates from JSON (`n::json`): all their
/// classes are kept, with their no-argument constructors.
pub const JSON_PACKAGES: &[&str] = &["dev/robocode/tankroyale/schema/"];

/// Translated methods the runtime calls by reflection (`reflect`), so
/// translation keeps them: (class, signature). Constructors also mark their
/// class as instantiated.
pub const REFLECTED: &[(&str, &str)] = &[
    ("java/util/ArrayList", "<init>()V"),
    ("java/util/ArrayList", "add(Ljava/lang/Object;)Z"),
    ("java/util/ArrayList", "toArray()[Ljava/lang/Object;"),
    ("java/util/LinkedHashSet", "<init>()V"),
    ("java/util/HashSet", "add(Ljava/lang/Object;)Z"),
    ("java/util/HashSet", "toArray()[Ljava/lang/Object;"),
    ("java/util/LinkedHashMap", "<init>()V"),
    (
        "java/util/HashMap",
        "put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (
        "java/util/concurrent/CompletableFuture",
        "completedFuture(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;",
    ),
    ("com/google/gson/JsonObject", "<init>()V"),
    (
        "com/google/gson/JsonObject",
        "add(Ljava/lang/String;Lcom/google/gson/JsonElement;)V",
    ),
    ("com/google/gson/JsonArray", "<init>()V"),
    (
        "com/google/gson/JsonArray",
        "add(Lcom/google/gson/JsonElement;)V",
    ),
    (
        "com/google/gson/JsonPrimitive",
        "<init>(Ljava/lang/String;)V",
    ),
    (
        "com/google/gson/JsonPrimitive",
        "<init>(Ljava/lang/Number;)V",
    ),
    (
        "com/google/gson/JsonPrimitive",
        "<init>(Ljava/lang/Boolean;)V",
    ),
    ("com/google/gson/JsonNull", "<init>()V"),
    (
        "java/util/regex/Pattern",
        "matches(Ljava/lang/String;Ljava/lang/CharSequence;)Z",
    ),
    (
        "java/util/regex/Pattern",
        "compile(Ljava/lang/String;)Ljava/util/regex/Pattern;",
    ),
    (
        "java/util/regex/Pattern",
        "split(Ljava/lang/CharSequence;I)[Ljava/lang/String;",
    ),
    (
        "java/util/regex/Pattern",
        "matcher(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
    ),
    (
        "java/util/regex/Matcher",
        "replaceAll(Ljava/lang/String;)Ljava/lang/String;",
    ),
    (
        "java/util/regex/Matcher",
        "replaceFirst(Ljava/lang/String;)Ljava/lang/String;",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleTick(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleRoundStarted(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleRoundEnded(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleGameStarted(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleGameEnded(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleSkippedTurn(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleServerHandshake(Lcom/google/gson/JsonObject;)V",
    ),
    (
        "dev/robocode/tankroyale/botapi/internal/WebSocketHandler",
        "handleGameAborted()V",
    ),
];

/// Methods the runtime calls on translated objects (besides Object's), so
/// translation keeps their implementations: (class or interface, signature).
pub const CALLBACKS: &[(&str, &str)] = &[
    ("java/security/PrivilegedAction", "run()Ljava/lang/Object;"),
    ("java/lang/Thread", "run()V"),
    ("java/lang/Runnable", "run()V"),
    (
        "java/net/http/WebSocket$Listener",
        "onOpen(Ljava/net/http/WebSocket;)V",
    ),
    (
        "java/net/http/WebSocket$Listener",
        "onText(Ljava/net/http/WebSocket;Ljava/lang/CharSequence;Z)Ljava/util/concurrent/CompletionStage;",
    ),
];

pub const NATIVE_CLASSES: &[NativeClass] = &[
    NativeClass {
        name: OBJECT,
        module: "n::object",
        sup: None,
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>()V", "init"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("clone()Ljava/lang/Object;", "clone"),
            v("finalize()V", "finalize"),
            d("getClass()Ljava/lang/Class;", "get_class"),
            d("notify()V", "notify"),
            d("notifyAll()V", "notify_all"),
            d("wait()V", "wait"),
            d("wait(J)V", "wait_j"),
            d("wait(JI)V", "wait_j_nanos"),
        ],
        statics: &[],
    },
    iface("java/io/Serializable", &[], &[]),
    iface("java/lang/Cloneable", &[], &[]),
    iface(
        "java/lang/Comparable",
        &[],
        &[v("compareTo(Ljava/lang/Object;)I", "")],
    ),
    iface(
        "java/lang/CharSequence",
        &[],
        &[
            v("length()I", ""),
            v("charAt(I)C", ""),
            v("subSequence(II)Ljava/lang/CharSequence;", ""),
            v("toString()Ljava/lang/String;", ""),
        ],
    ),
    iface("java/lang/Runnable", &[], &[v("run()V", "")]),
    iface(
        "java/lang/Appendable",
        &[],
        &[
            v("append(Ljava/lang/CharSequence;)Ljava/lang/Appendable;", ""),
            v(
                "append(Ljava/lang/CharSequence;II)Ljava/lang/Appendable;",
                "",
            ),
            v("append(C)Ljava/lang/Appendable;", ""),
        ],
    ),
    NativeClass {
        name: "java/lang/String",
        module: "n::string",
        sup: Some(OBJECT),
        interfaces: &[
            "java/io/Serializable",
            "java/lang/Comparable",
            "java/lang/CharSequence",
        ],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "ext",
        methods: &[
            d("<init>()V", "init"),
            d("<init>(Ljava/lang/String;)V", "init_str"),
            d("<init>([C)V", "init_chars"),
            d("<init>([CII)V", "init_chars_range"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("length()I", "length"),
            v("charAt(I)C", "char_at"),
            v("subSequence(II)Ljava/lang/CharSequence;", "substring_range"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
            d("compareTo(Ljava/lang/String;)I", "compare_to"),
            d(
                "compareToIgnoreCase(Ljava/lang/String;)I",
                "compare_to_ignore_case",
            ),
            d(
                "equalsIgnoreCase(Ljava/lang/String;)Z",
                "equals_ignore_case",
            ),
            d("isEmpty()Z", "is_empty"),
            d("concat(Ljava/lang/String;)Ljava/lang/String;", "concat"),
            d("substring(I)Ljava/lang/String;", "substring"),
            d("substring(II)Ljava/lang/String;", "substring_range"),
            d("indexOf(I)I", "index_of_char"),
            d("indexOf(II)I", "index_of_char_from"),
            d("indexOf(Ljava/lang/String;)I", "index_of"),
            d("indexOf(Ljava/lang/String;I)I", "index_of_from"),
            d("lastIndexOf(I)I", "last_index_of_char"),
            d("lastIndexOf(Ljava/lang/String;)I", "last_index_of"),
            d("startsWith(Ljava/lang/String;)Z", "starts_with"),
            d("endsWith(Ljava/lang/String;)Z", "ends_with"),
            d("contains(Ljava/lang/CharSequence;)Z", "contains"),
            d("replace(CC)Ljava/lang/String;", "replace_char"),
            d(
                "replace(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;",
                "replace_seq",
            ),
            d("trim()Ljava/lang/String;", "trim"),
            d("toLowerCase()Ljava/lang/String;", "to_lower"),
            d("toUpperCase()Ljava/lang/String;", "to_upper"),
            d("toCharArray()[C", "to_char_array"),
            d("getChars(II[CI)V", "get_chars"),
            d("split(Ljava/lang/String;)[Ljava/lang/String;", "split"),
            d("isBlank()Z", "is_blank"),
            d("strip()Ljava/lang/String;", "trim"),
            d("repeat(I)Ljava/lang/String;", "repeat"),
            d(
                "replaceAll(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                "replace_all_regex",
            ),
            d(
                "replaceFirst(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                "replace_first_regex",
            ),
            d("getBytes()[B", "get_bytes"),
            d(
                "getBytes(Ljava/nio/charset/Charset;)[B",
                "get_bytes_charset",
            ),
            d("getBytes(Ljava/lang/String;)[B", "get_bytes_charset"),
            d(
                "toLowerCase(Ljava/util/Locale;)Ljava/lang/String;",
                "to_lower_locale",
            ),
            d(
                "toUpperCase(Ljava/util/Locale;)Ljava/lang/String;",
                "to_upper_locale",
            ),
            d("<init>([B)V", "init_bytes"),
            d(
                "<init>([BLjava/nio/charset/Charset;)V",
                "init_bytes_charset",
            ),
            d("<init>([BLjava/lang/String;)V", "init_bytes_charset"),
            d("<init>([BII)V", "init_bytes_range"),
            d("<init>([III)V", "init_code_points"),
            d("codePointAt(I)I", "code_point_at"),
            d("codePointCount(II)I", "code_point_count"),
            d(
                "<init>([BIILjava/nio/charset/Charset;)V",
                "init_bytes_range_charset",
            ),
            d("<init>(Ljava/lang/StringBuilder;)V", "init_str"),
            d(
                "split(Ljava/lang/String;I)[Ljava/lang/String;",
                "split_limit",
            ),
            d("matches(Ljava/lang/String;)Z", "matches"),
            d("intern()Ljava/lang/String;", "intern"),
            s(
                "valueOf(Ljava/lang/Object;)Ljava/lang/String;",
                "value_of_obj",
            ),
            s(
                "format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
                "format",
            ),
            s("valueOf(I)Ljava/lang/String;", "value_of_i"),
            s("valueOf(J)Ljava/lang/String;", "value_of_j"),
            s("valueOf(F)Ljava/lang/String;", "value_of_f"),
            s("valueOf(D)Ljava/lang/String;", "value_of_d"),
            s("valueOf(Z)Ljava/lang/String;", "value_of_z"),
            s("valueOf(C)Ljava/lang/String;", "value_of_c"),
            s("valueOf([C)Ljava/lang/String;", "value_of_chars"),
            s("valueOf([CII)Ljava/lang/String;", "value_of_chars_range"),
            s("copyValueOf([C)Ljava/lang/String;", "value_of_chars"),
            s(
                "copyValueOf([CII)Ljava/lang/String;",
                "value_of_chars_range",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/AbstractStringBuilder",
        module: "n::builder",
        sup: Some(OBJECT),
        interfaces: &["java/lang/Appendable", "java/lang/CharSequence"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "ext",
        methods: &[
            v("toString()Ljava/lang/String;", "to_string"),
            v("length()I", "length"),
            v("charAt(I)C", "char_at"),
            v("subSequence(II)Ljava/lang/CharSequence;", "substring_range"),
            d("setLength(I)V", "set_length"),
            d("setCharAt(IC)V", "set_char_at"),
        ],
        statics: &[],
    },
    builder("java/lang/StringBuilder"),
    builder("java/lang/StringBuffer"),
    NativeClass {
        name: "java/lang/Class",
        module: "n::class",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            d("getName()Ljava/lang/String;", "get_name"),
            d("getSimpleName()Ljava/lang/String;", "get_simple_name"),
            d("isInstance(Ljava/lang/Object;)Z", "is_instance"),
            d("desiredAssertionStatus()Z", "no"),
            d("getComponentType()Ljava/lang/Class;", "get_component_type"),
            d(
                "getGenericInterfaces()[Ljava/lang/reflect/Type;",
                "no_types",
            ),
            d("getInterfaces()[Ljava/lang/Class;", "no_types"),
            d("isArray()Z", "is_array"),
            d("isInterface()Z", "is_interface"),
            d("isPrimitive()Z", "no"),
            d("getSuperclass()Ljava/lang/Class;", "get_superclass"),
            d("isAssignableFrom(Ljava/lang/Class;)Z", "is_assignable_from"),
            d("getClassLoader()Ljava/lang/ClassLoader;", "null_this"),
            d(
                "getDeclaredConstructor([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
                "get_constructor",
            ),
            d(
                "getConstructor([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
                "get_constructor",
            ),
            d(
                "getMethod(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
                "no_method",
            ),
            d(
                "getDeclaredMethod(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
                "no_method",
            ),
            d("newInstance()Ljava/lang/Object;", "new_instance"),
            s("forName(Ljava/lang/String;)Ljava/lang/Class;", "for_name"),
            v("toString()Ljava/lang/String;", "to_string"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/Math",
        module: "n::math",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: MATH,
        statics: &[],
    },
    NativeClass {
        name: "java/lang/StrictMath",
        module: "n::math",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: MATH,
        statics: &[],
    },
    NativeClass {
        name: "java/lang/System",
        module: "n::system",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            s(
                "arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V",
                "arraycopy",
            ),
            s("currentTimeMillis()J", "current_time_millis"),
            s("nanoTime()J", "nano_time"),
            s(
                "identityHashCode(Ljava/lang/Object;)I",
                "identity_hash_code",
            ),
            s(
                "getProperty(Ljava/lang/String;)Ljava/lang/String;",
                "get_property",
            ),
            s("gc()V", "nop"),
            s("getenv(Ljava/lang/String;)Ljava/lang/String;", "getenv"),
            s("lineSeparator()Ljava/lang/String;", "line_separator"),
            s("exit(I)V", "exit"),
        ],
        statics: &[
            NativeField {
                sig: "out:Ljava/io/PrintStream;",
                getter: "out",
            },
            NativeField {
                sig: "err:Ljava/io/PrintStream;",
                getter: "err",
            },
        ],
    },
    NativeClass {
        name: "java/io/PrintStream",
        module: "n::print",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 1,
        refs: 0,
        ext: "",
        methods: &[
            v("println()V", "println"),
            v("println(Ljava/lang/String;)V", "println_obj"),
            v("println(Ljava/lang/Object;)V", "println_obj"),
            v("println(I)V", "println_i"),
            v("println(J)V", "println_j"),
            v("println(F)V", "println_f"),
            v("println(D)V", "println_d"),
            v("println(Z)V", "println_z"),
            v("println(C)V", "println_c"),
            v("println([C)V", "println_chars"),
            v("print(Ljava/lang/String;)V", "print_obj"),
            v("print(Ljava/lang/Object;)V", "print_obj"),
            v("print(I)V", "print_i"),
            v("print(J)V", "print_j"),
            v("print(F)V", "print_f"),
            v("print(D)V", "print_d"),
            v("print(Z)V", "print_z"),
            v("print(C)V", "print_c"),
            v("flush()V", "flush"),
            v(
                "printf(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
                "printf",
            ),
            v(
                "format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
                "printf",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/Number",
        module: "n::boxes",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>()V", "number_init"),
            v("intValue()I", ""),
            v("longValue()J", ""),
            v("floatValue()F", ""),
            v("doubleValue()D", ""),
            v("byteValue()B", "number_byte_value"),
            v("shortValue()S", "number_short_value"),
        ],
        statics: &[],
    },
    NativeClass {
        statics: INT_TYPE,
        ..boxed(
            "java/lang/Integer",
            "n::boxes::integer",
            &[
                d("<init>(I)V", "init"),
                s("valueOf(I)Ljava/lang/Integer;", "value_of"),
                s(
                    "valueOf(Ljava/lang/String;)Ljava/lang/Integer;",
                    "value_of_str",
                ),
                s(
                    "valueOf(Ljava/lang/String;I)Ljava/lang/Integer;",
                    "value_of_str_radix",
                ),
                s("parseInt(Ljava/lang/String;I)I", "parse_radix"),
                s("toString(II)Ljava/lang/String;", "to_string_radix"),
                s("parseInt(Ljava/lang/String;)I", "parse"),
                s("toString(I)Ljava/lang/String;", "to_string_static"),
                s("toHexString(I)Ljava/lang/String;", "to_hex_string"),
                s("toBinaryString(I)Ljava/lang/String;", "to_binary_string"),
                s("bitCount(I)I", "bit_count"),
                s("signum(I)I", "signum"),
                s("compare(II)I", "compare"),
                s("sum(II)I", "sum"),
                s("max(II)I", "max"),
                s("min(II)I", "min"),
                s("abs(I)I", "abs"),
                s("numberOfTrailingZeros(I)I", "trailing_zeros"),
                s("numberOfLeadingZeros(I)I", "leading_zeros"),
                s("highestOneBit(I)I", "highest_one_bit"),
                s("reverse(I)I", "reverse"),
                s("rotateLeft(II)I", "rotate_left"),
                s("rotateRight(II)I", "rotate_right"),
                v("intValue()I", "int_value"),
                v("longValue()J", "long_value"),
                v("floatValue()F", "float_value"),
                v("doubleValue()D", "double_value"),
                v("equals(Ljava/lang/Object;)Z", "equals"),
                v("hashCode()I", "hash_code"),
                v("toString()Ljava/lang/String;", "to_string"),
                v("compareTo(Ljava/lang/Object;)I", "compare_to"),
                d("compareTo(Ljava/lang/Integer;)I", "compare_to"),
            ],
        )
    },
    boxed(
        "java/lang/Long",
        "n::boxes::long",
        &[
            d("<init>(J)V", "init"),
            s("valueOf(J)Ljava/lang/Long;", "value_of"),
            s("parseLong(Ljava/lang/String;)J", "parse"),
            s("toString(J)Ljava/lang/String;", "to_string_static"),
            s("compare(JJ)I", "compare"),
            s("bitCount(J)I", "bit_count"),
            s("signum(J)I", "signum"),
            s("numberOfTrailingZeros(J)I", "trailing_zeros"),
            s("numberOfLeadingZeros(J)I", "leading_zeros"),
            s("toHexString(J)Ljava/lang/String;", "to_hex_string"),
            s("toBinaryString(J)Ljava/lang/String;", "to_binary_string"),
            s("max(JJ)J", "max"),
            s("min(JJ)J", "min"),
            v("intValue()I", "int_value"),
            v("longValue()J", "long_value"),
            v("floatValue()F", "float_value"),
            v("doubleValue()D", "double_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
            d("compareTo(Ljava/lang/Long;)I", "compare_to"),
        ],
    ),
    boxed(
        "java/lang/Double",
        "n::boxes::double",
        &[
            d("<init>(D)V", "init"),
            s("valueOf(D)Ljava/lang/Double;", "value_of"),
            s(
                "valueOf(Ljava/lang/String;)Ljava/lang/Double;",
                "value_of_str",
            ),
            s("parseDouble(Ljava/lang/String;)D", "parse"),
            s("toString(D)Ljava/lang/String;", "to_string_static"),
            s("isNaN(D)Z", "is_nan"),
            s("isInfinite(D)Z", "is_infinite"),
            s("isFinite(D)Z", "is_finite"),
            s("compare(DD)I", "compare"),
            s("doubleToLongBits(D)J", "to_long_bits"),
            s("doubleToRawLongBits(D)J", "to_raw_long_bits"),
            s("longBitsToDouble(J)D", "from_long_bits"),
            s("max(DD)D", "max"),
            s("min(DD)D", "min"),
            s("sum(DD)D", "sum"),
            s("hashCode(D)I", "hash_code_static"),
            v("isNaN()Z", "is_nan_inst"),
            v("intValue()I", "int_value"),
            v("longValue()J", "long_value"),
            v("floatValue()F", "float_value"),
            v("doubleValue()D", "double_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
            d("compareTo(Ljava/lang/Double;)I", "compare_to"),
        ],
    ),
    boxed(
        "java/lang/Float",
        "n::boxes::float",
        &[
            d("<init>(F)V", "init"),
            s("valueOf(F)Ljava/lang/Float;", "value_of"),
            s("parseFloat(Ljava/lang/String;)F", "parse"),
            s("toString(F)Ljava/lang/String;", "to_string_static"),
            s("isNaN(F)Z", "is_nan"),
            s("isInfinite(F)Z", "is_infinite"),
            s("compare(FF)I", "compare"),
            s("floatToIntBits(F)I", "to_int_bits"),
            s("floatToRawIntBits(F)I", "to_raw_int_bits"),
            s("intBitsToFloat(I)F", "from_int_bits"),
            s("max(FF)F", "max"),
            s("min(FF)F", "min"),
            v("intValue()I", "int_value"),
            v("longValue()J", "long_value"),
            v("floatValue()F", "float_value"),
            v("doubleValue()D", "double_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
            d("compareTo(Ljava/lang/Float;)I", "compare_to"),
        ],
    ),
    boxed(
        "java/lang/Byte",
        "n::boxes::small",
        &[
            d("<init>(B)V", "init"),
            s("valueOf(B)Ljava/lang/Byte;", "byte_value_of"),
            s("parseByte(Ljava/lang/String;)B", "parse_byte"),
            s("toString(B)Ljava/lang/String;", "to_string_static"),
            v("intValue()I", "int_value"),
            v("longValue()J", "long_value"),
            v("floatValue()F", "float_value"),
            v("doubleValue()D", "double_value"),
            v("byteValue()B", "int_value"),
            v("shortValue()S", "int_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "int_value"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
        ],
    ),
    boxed(
        "java/lang/Short",
        "n::boxes::small",
        &[
            d("<init>(S)V", "init"),
            s("valueOf(S)Ljava/lang/Short;", "short_value_of"),
            s("parseShort(Ljava/lang/String;)S", "parse_short"),
            s("toString(S)Ljava/lang/String;", "to_string_static"),
            v("intValue()I", "int_value"),
            v("longValue()J", "long_value"),
            v("floatValue()F", "float_value"),
            v("doubleValue()D", "double_value"),
            v("byteValue()B", "int_value"),
            v("shortValue()S", "int_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "int_value"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
        ],
    ),
    NativeClass {
        name: "java/lang/reflect/Constructor",
        module: "n::class",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        // The class.
        prims: 0,
        refs: 1,
        ext: "",
        methods: &[
            d(
                "newInstance([Ljava/lang/Object;)Ljava/lang/Object;",
                "constructor_new_instance",
            ),
            d("setAccessible(Z)V", "nop_z"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/nio/charset/Charset",
        module: "n::string",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        // The name.
        prims: 0,
        refs: 1,
        ext: "",
        methods: &[
            d("name()Ljava/lang/String;", "charset_name"),
            s(
                "forName(Ljava/lang/String;)Ljava/nio/charset/Charset;",
                "charset_for_name",
            ),
            s("defaultCharset()Ljava/nio/charset/Charset;", "charset_utf8"),
            v("toString()Ljava/lang/String;", "charset_name"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/nio/charset/StandardCharsets",
        module: "n::string",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[],
        statics: &[
            NativeField {
                sig: "UTF_8:Ljava/nio/charset/Charset;",
                getter: "charset_utf8",
            },
            NativeField {
                sig: "US_ASCII:Ljava/nio/charset/Charset;",
                getter: "charset_ascii",
            },
            NativeField {
                sig: "ISO_8859_1:Ljava/nio/charset/Charset;",
                getter: "charset_latin1",
            },
        ],
    },
    NativeClass {
        name: "java/lang/Boolean",
        module: "n::boxes::boolean",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable", "java/lang/Comparable"],
        interface: false,
        prims: 1,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>(Z)V", "init"),
            s("valueOf(Z)Ljava/lang/Boolean;", "value_of"),
            s("parseBoolean(Ljava/lang/String;)Z", "parse"),
            s("getBoolean(Ljava/lang/String;)Z", "get_boolean"),
            s("toString(Z)Ljava/lang/String;", "to_string_static"),
            d("booleanValue()Z", "boolean_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
        ],
        statics: &[
            NativeField {
                sig: "TRUE:Ljava/lang/Boolean;",
                getter: "get_true",
            },
            NativeField {
                sig: "FALSE:Ljava/lang/Boolean;",
                getter: "get_false",
            },
        ],
    },
    NativeClass {
        name: "java/lang/Character",
        module: "n::boxes::character",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable", "java/lang/Comparable"],
        interface: false,
        prims: 1,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>(C)V", "init"),
            s("valueOf(C)Ljava/lang/Character;", "value_of"),
            s("isDigit(C)Z", "is_digit"),
            s("isLetter(C)Z", "is_letter"),
            s("isLetterOrDigit(C)Z", "is_letter_or_digit"),
            s("isWhitespace(C)Z", "is_whitespace"),
            s("isUpperCase(C)Z", "is_upper_case"),
            s("isLowerCase(C)Z", "is_lower_case"),
            s("toUpperCase(C)C", "to_upper_case"),
            s("toLowerCase(C)C", "to_lower_case"),
            s("isDigit(I)Z", "is_digit"),
            s("isLetter(I)Z", "is_letter"),
            s("isLetterOrDigit(I)Z", "is_letter_or_digit"),
            s("isWhitespace(I)Z", "is_whitespace"),
            s("isUpperCase(I)Z", "is_upper_case"),
            s("isLowerCase(I)Z", "is_lower_case"),
            s("toUpperCase(I)I", "to_upper_case"),
            s("toLowerCase(I)I", "to_lower_case"),
            s("isAlphabetic(I)Z", "is_letter"),
            s("isSpaceChar(I)Z", "is_space_char"),
            s("isSpaceChar(C)Z", "is_space_char"),
            s("isISOControl(I)Z", "is_iso_control"),
            s("isISOControl(C)Z", "is_iso_control"),
            s("isDefined(I)Z", "is_defined"),
            s("isIdeographic(I)Z", "is_ideographic"),
            s("isMirrored(I)Z", "no"),
            s("isIdentifierIgnorable(I)Z", "is_identifier_ignorable"),
            s("isJavaIdentifierStart(I)Z", "is_java_identifier_start"),
            s("isJavaIdentifierPart(I)Z", "is_java_identifier_part"),
            s("isTitleCase(I)Z", "no"),
            s("isHighSurrogate(C)Z", "is_high_surrogate"),
            s("isLowSurrogate(C)Z", "is_low_surrogate"),
            s("isSurrogate(C)Z", "is_surrogate"),
            s("isSupplementaryCodePoint(I)Z", "is_supplementary"),
            s("isValidCodePoint(I)Z", "is_valid_code_point"),
            s("charCount(I)I", "char_count"),
            s("toCodePoint(CC)I", "to_code_point"),
            s("codePointAt(Ljava/lang/CharSequence;I)I", "code_point_at"),
            s(
                "codePointBefore(Ljava/lang/CharSequence;I)I",
                "code_point_before",
            ),
            s("getType(I)I", "get_type"),
            s("getType(C)I", "get_type"),
            s("digit(CI)I", "digit"),
            s("digit(II)I", "digit"),
            s("forDigit(II)C", "for_digit"),
            s("getNumericValue(C)I", "numeric_value"),
            s("toString(C)Ljava/lang/String;", "to_string_static"),
            d("charValue()C", "char_value"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/Enum",
        module: "n::enums",
        sup: Some(OBJECT),
        interfaces: &["java/lang/Comparable", "java/io/Serializable"],
        interface: false,
        // The ordinal and the name.
        prims: 1,
        refs: 1,
        ext: "",
        methods: &[
            d("<init>(Ljava/lang/String;I)V", "init"),
            d("name()Ljava/lang/String;", "name"),
            d("ordinal()I", "ordinal"),
            v("toString()Ljava/lang/String;", "to_string"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("clone()Ljava/lang/Object;", "clone"),
            v("compareTo(Ljava/lang/Object;)I", "compare_to"),
            d("compareTo(Ljava/lang/Enum;)I", "compare_to"),
            d(
                "getDeclaringClass()Ljava/lang/Class;",
                "get_declaring_class",
            ),
            s(
                "valueOf(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;",
                "value_of",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/Throwable",
        module: "n::throwable",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        prims: 0,
        // The message and the cause.
        refs: 2,
        ext: "",
        methods: &[
            d("<init>()V", "init"),
            d("<init>(Ljava/lang/String;)V", "init_msg"),
            d(
                "<init>(Ljava/lang/String;Ljava/lang/Throwable;)V",
                "init_msg_cause",
            ),
            d("<init>(Ljava/lang/Throwable;)V", "init_cause"),
            v("getMessage()Ljava/lang/String;", "get_message"),
            v("getLocalizedMessage()Ljava/lang/String;", "get_message"),
            v("getCause()Ljava/lang/Throwable;", "get_cause"),
            v(
                "initCause(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
                "set_cause",
            ),
            v("toString()Ljava/lang/String;", "to_string"),
            v("printStackTrace()V", "print_stack_trace"),
            v(
                "fillInStackTrace()Ljava/lang/Throwable;",
                "fill_in_stack_trace",
            ),
            d("addSuppressed(Ljava/lang/Throwable;)V", "add_suppressed"),
            d("getSuppressed()[Ljava/lang/Throwable;", "no_throwables"),
            d("getStackTrace()[Ljava/lang/StackTraceElement;", "no_stack"),
            d(
                "setStackTrace([Ljava/lang/StackTraceElement;)V",
                "add_suppressed",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/util/Random",
        module: "n::random",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        // The seed, whether a second Gaussian is ready, and that Gaussian.
        prims: 3,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>()V", "init"),
            d("<init>(J)V", "init_seed"),
            v("setSeed(J)V", "set_seed"),
            v("next(I)I", "next"),
            v("nextInt()I", "next_int"),
            v("nextInt(I)I", "next_int_bound"),
            v("nextInt(II)I", "next_int_range"),
            v("nextLong()J", "next_long"),
            v("nextBoolean()Z", "next_boolean"),
            v("nextFloat()F", "next_float"),
            v("nextDouble()D", "next_double"),
            v("nextGaussian()D", "next_gaussian"),
            v("nextBytes([B)V", "next_bytes"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/Thread",
        module: "n::thread",
        sup: Some(OBJECT),
        interfaces: &["java/lang/Runnable"],
        interface: false,
        // The scheduler id + 1, daemon, priority; the name, the target.
        prims: 3,
        refs: 2,
        ext: "",
        methods: &[
            v("run()V", "run"),
            v("start()V", "start"),
            v("toString()Ljava/lang/String;", "to_string"),
            d("<init>()V", "init"),
            d("<init>(Ljava/lang/Runnable;)V", "init_target"),
            d("<init>(Ljava/lang/String;)V", "init_name"),
            d(
                "<init>(Ljava/lang/Runnable;Ljava/lang/String;)V",
                "init_target_name",
            ),
            d(
                "<init>(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;)V",
                "init_group_target",
            ),
            d(
                "<init>(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;Ljava/lang/String;)V",
                "init_group_target_name",
            ),
            s("currentThread()Ljava/lang/Thread;", "current_thread"),
            s("sleep(J)V", "sleep"),
            s("sleep(JI)V", "sleep_nanos"),
            s("yield()V", "yield_now"),
            s("onSpinWait()V", "spin_wait"),
            s("interrupted()Z", "interrupted"),
            d("interrupt()V", "interrupt"),
            d("isInterrupted()Z", "is_interrupted"),
            d("isAlive()Z", "is_alive"),
            d("join()V", "join"),
            d("join(J)V", "join_ms"),
            d("getName()Ljava/lang/String;", "get_name"),
            d("setName(Ljava/lang/String;)V", "set_name"),
            d("setDaemon(Z)V", "set_daemon"),
            d("isDaemon()Z", "is_daemon"),
            d("setPriority(I)V", "set_priority"),
            d("getPriority()I", "get_priority"),
            d("getId()J", "get_id"),
            d("getStackTrace()[Ljava/lang/StackTraceElement;", "no_stack"),
            d(
                "setUncaughtExceptionHandler(Ljava/lang/Thread$UncaughtExceptionHandler;)V",
                "ignore_ref",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/net/URI",
        module: "n::net",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        prims: 0,
        refs: 1,
        ext: "",
        methods: &[
            d("<init>(Ljava/lang/String;)V", "uri_init"),
            s("create(Ljava/lang/String;)Ljava/net/URI;", "uri_create"),
            v("toString()Ljava/lang/String;", "uri_to_string"),
            v("equals(Ljava/lang/Object;)Z", "uri_equals"),
            v("hashCode()I", "uri_hash_code"),
            d("getScheme()Ljava/lang/String;", "uri_get_scheme"),
            d("getHost()Ljava/lang/String;", "uri_get_host"),
            d("getPort()I", "uri_get_port"),
            d("getPath()Ljava/lang/String;", "uri_get_path"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/net/http/HttpClient",
        module: "n::net",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            s(
                "newBuilder()Ljava/net/http/HttpClient$Builder;",
                "http_new_builder",
            ),
            v("newWebSocketBuilder()Ljava/net/http/WebSocket$Builder;", ""),
        ],
        statics: &[],
    },
    iface(
        "java/net/http/HttpClient$Builder",
        &[],
        &[v("build()Ljava/net/http/HttpClient;", "")],
    ),
    iface(
        "java/net/http/WebSocket$Builder",
        &[],
        &[v(
            "buildAsync(Ljava/net/URI;Ljava/net/http/WebSocket$Listener;)Ljava/util/concurrent/CompletableFuture;",
            "",
        )],
    ),
    iface(
        "java/net/http/WebSocket",
        &[],
        &[
            v(
                "sendText(Ljava/lang/CharSequence;Z)Ljava/util/concurrent/CompletableFuture;",
                "",
            ),
            v(
                "sendClose(ILjava/lang/String;)Ljava/util/concurrent/CompletableFuture;",
                "",
            ),
            v("request(J)V", ""),
            v("abort()V", ""),
            v("isOutputClosed()Z", ""),
            v("isInputClosed()Z", ""),
            v("getSubprotocol()Ljava/lang/String;", ""),
        ],
    ),
    iface(
        "java/net/http/WebSocket$Listener",
        &[],
        &[
            // Default methods, as in the JDK.
            v("onOpen(Ljava/net/http/WebSocket;)V", "listener_on_open"),
            v(
                "onText(Ljava/net/http/WebSocket;Ljava/lang/CharSequence;Z)Ljava/util/concurrent/CompletionStage;",
                "listener_on_text",
            ),
            v(
                "onClose(Ljava/net/http/WebSocket;ILjava/lang/String;)Ljava/util/concurrent/CompletionStage;",
                "listener_on_close",
            ),
            v(
                "onError(Ljava/net/http/WebSocket;Ljava/lang/Throwable;)V",
                "listener_on_error",
            ),
        ],
    ),
    NativeClass {
        name: "jrt/net/HttpClientBuilder",
        module: "n::net",
        sup: Some(OBJECT),
        interfaces: &["java/net/http/HttpClient$Builder"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[v("build()Ljava/net/http/HttpClient;", "http_build")],
        statics: &[],
    },
    NativeClass {
        name: "jrt/net/HttpClientImpl",
        module: "n::net",
        sup: Some("java/net/http/HttpClient"),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[v(
            "newWebSocketBuilder()Ljava/net/http/WebSocket$Builder;",
            "http_new_websocket_builder",
        )],
        statics: &[],
    },
    NativeClass {
        name: "jrt/net/WebSocketBuilder",
        module: "n::net",
        sup: Some(OBJECT),
        interfaces: &["java/net/http/WebSocket$Builder"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[v(
            "buildAsync(Ljava/net/URI;Ljava/net/http/WebSocket$Listener;)Ljava/util/concurrent/CompletableFuture;",
            "ws_build_async",
        )],
        statics: &[],
    },
    NativeClass {
        name: "jrt/net/InProcessSocket",
        module: "n::net",
        sup: Some(OBJECT),
        interfaces: &["java/net/http/WebSocket"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            v(
                "sendText(Ljava/lang/CharSequence;Z)Ljava/util/concurrent/CompletableFuture;",
                "ws_send_text",
            ),
            v(
                "sendClose(ILjava/lang/String;)Ljava/util/concurrent/CompletableFuture;",
                "ws_send_close",
            ),
            v("request(J)V", "ws_request"),
            v("abort()V", "ws_abort"),
            v("isOutputClosed()Z", "ws_closed"),
            v("isInputClosed()Z", "ws_closed"),
            v("getSubprotocol()Ljava/lang/String;", "ws_subprotocol"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/util/Locale",
        module: "n::system",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        // The language tag.
        prims: 0,
        refs: 1,
        ext: "",
        methods: &[
            s("getDefault()Ljava/util/Locale;", "locale_us"),
            d("getCountry()Ljava/lang/String;", "locale_country"),
            d("getLanguage()Ljava/lang/String;", "locale_language"),
            v("toString()Ljava/lang/String;", "locale_tag"),
        ],
        statics: &[
            NativeField {
                sig: "ENGLISH:Ljava/util/Locale;",
                getter: "locale_en",
            },
            NativeField {
                sig: "US:Ljava/util/Locale;",
                getter: "locale_us",
            },
            NativeField {
                sig: "UK:Ljava/util/Locale;",
                getter: "locale_uk",
            },
            NativeField {
                sig: "ROOT:Ljava/util/Locale;",
                getter: "locale_root",
            },
        ],
    },
    iface(
        "jdk/internal/access/JavaLangAccess",
        &[],
        &[v(
            "getEnumConstantsShared(Ljava/lang/Class;)[Ljava/lang/Enum;",
            "",
        )],
    ),
    NativeClass {
        name: "jdk/internal/access/SharedSecrets",
        module: "n::enums",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[s(
            "getJavaLangAccess()Ljdk/internal/access/JavaLangAccess;",
            "java_lang_access",
        )],
        statics: &[],
    },
    NativeClass {
        name: "jrt/JavaLangAccess",
        module: "n::enums",
        sup: Some(OBJECT),
        interfaces: &["jdk/internal/access/JavaLangAccess"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[v(
            "getEnumConstantsShared(Ljava/lang/Class;)[Ljava/lang/Enum;",
            "enum_constants",
        )],
        statics: &[],
    },
    NativeClass {
        name: "java/awt/Color",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &["java/awt/Paint", "java/io/Serializable"],
        interface: false,
        // The ARGB value.
        prims: 1,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>(III)V", "init_rgb"),
            d("<init>(IIII)V", "init_rgba"),
            d("<init>(I)V", "init_int"),
            d("<init>(IZ)V", "init_int_alpha"),
            d("<init>(FFF)V", "init_float"),
            d("getRGB()I", "get_rgb"),
            d("getRed()I", "get_red"),
            d("getGreen()I", "get_green"),
            d("getBlue()I", "get_blue"),
            d("getAlpha()I", "get_alpha"),
            d("brighter()Ljava/awt/Color;", "brighter"),
            d("darker()Ljava/awt/Color;", "darker"),
            s("decode(Ljava/lang/String;)Ljava/awt/Color;", "decode"),
            v("equals(Ljava/lang/Object;)Z", "equals"),
            v("hashCode()I", "hash_code"),
            v("toString()Ljava/lang/String;", "to_string"),
        ],
        statics: &[
            NativeField {
                sig: "white:Ljava/awt/Color;",
                getter: "white",
            },
            NativeField {
                sig: "WHITE:Ljava/awt/Color;",
                getter: "white",
            },
            NativeField {
                sig: "lightGray:Ljava/awt/Color;",
                getter: "light_gray",
            },
            NativeField {
                sig: "LIGHT_GRAY:Ljava/awt/Color;",
                getter: "light_gray",
            },
            NativeField {
                sig: "gray:Ljava/awt/Color;",
                getter: "gray",
            },
            NativeField {
                sig: "GRAY:Ljava/awt/Color;",
                getter: "gray",
            },
            NativeField {
                sig: "darkGray:Ljava/awt/Color;",
                getter: "dark_gray",
            },
            NativeField {
                sig: "DARK_GRAY:Ljava/awt/Color;",
                getter: "dark_gray",
            },
            NativeField {
                sig: "black:Ljava/awt/Color;",
                getter: "black",
            },
            NativeField {
                sig: "BLACK:Ljava/awt/Color;",
                getter: "black",
            },
            NativeField {
                sig: "red:Ljava/awt/Color;",
                getter: "red",
            },
            NativeField {
                sig: "RED:Ljava/awt/Color;",
                getter: "red",
            },
            NativeField {
                sig: "pink:Ljava/awt/Color;",
                getter: "pink",
            },
            NativeField {
                sig: "PINK:Ljava/awt/Color;",
                getter: "pink",
            },
            NativeField {
                sig: "orange:Ljava/awt/Color;",
                getter: "orange",
            },
            NativeField {
                sig: "ORANGE:Ljava/awt/Color;",
                getter: "orange",
            },
            NativeField {
                sig: "yellow:Ljava/awt/Color;",
                getter: "yellow",
            },
            NativeField {
                sig: "YELLOW:Ljava/awt/Color;",
                getter: "yellow",
            },
            NativeField {
                sig: "green:Ljava/awt/Color;",
                getter: "green",
            },
            NativeField {
                sig: "GREEN:Ljava/awt/Color;",
                getter: "green",
            },
            NativeField {
                sig: "magenta:Ljava/awt/Color;",
                getter: "magenta",
            },
            NativeField {
                sig: "MAGENTA:Ljava/awt/Color;",
                getter: "magenta",
            },
            NativeField {
                sig: "cyan:Ljava/awt/Color;",
                getter: "cyan",
            },
            NativeField {
                sig: "CYAN:Ljava/awt/Color;",
                getter: "cyan",
            },
            NativeField {
                sig: "blue:Ljava/awt/Color;",
                getter: "blue",
            },
            NativeField {
                sig: "BLUE:Ljava/awt/Color;",
                getter: "blue",
            },
        ],
    },
    NativeClass {
        name: "javax/swing/UIManager",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[s(
            "getColor(Ljava/lang/Object;)Ljava/awt/Color;",
            "ui_color",
        )],
        statics: &[],
    },
    iface("java/awt/Paint", &[], &[]),
    iface("java/awt/Stroke", &[], &[]),
    iface("java/awt/Composite", &[], &[]),
    NativeClass {
        name: "java/awt/Graphics",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[d("<init>()V", "nothing")],
        statics: &[],
    },
    NativeClass {
        name: "java/awt/Graphics2D",
        module: "n::awt",
        sup: Some("java/awt/Graphics"),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[d("<init>()V", "nothing")],
        statics: &[],
    },
    NativeClass {
        name: "java/awt/Font",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &["java/io/Serializable"],
        interface: false,
        // Style and size; the name.
        prims: 2,
        refs: 1,
        ext: "",
        methods: &[
            d("<init>(Ljava/lang/String;II)V", "font_init"),
            d("getName()Ljava/lang/String;", "font_name"),
            d("getStyle()I", "font_style"),
            d("getSize()I", "font_size"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/awt/BasicStroke",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &["java/awt/Stroke"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[d("<init>()V", "nothing"), d("<init>(F)V", "nothing_f")],
        statics: &[],
    },
    NativeClass {
        name: "java/awt/RenderingHints",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            d("<init>(Ljava/util/Map;)V", "nothing_ref"),
            v(
                "put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                "hints_put",
            ),
            v("get(Ljava/lang/Object;)Ljava/lang/Object;", "hints_get"),
        ],
        statics: &[
            NativeField {
                sig: "KEY_ANTIALIASING:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_RENDERING:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_DITHERING:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_TEXT_ANTIALIASING:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_FRACTIONALMETRICS:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_INTERPOLATION:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_ALPHA_INTERPOLATION:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_COLOR_RENDERING:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_STROKE_CONTROL:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "KEY_TEXT_LCD_CONTRAST:Ljava/awt/RenderingHints$Key;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_ANTIALIAS_ON:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_ANTIALIAS_OFF:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_ANTIALIAS_DEFAULT:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_RENDER_SPEED:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_RENDER_QUALITY:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_RENDER_DEFAULT:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_TEXT_ANTIALIAS_ON:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_TEXT_ANTIALIAS_OFF:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_TEXT_ANTIALIAS_DEFAULT:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_FRACTIONALMETRICS_ON:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_FRACTIONALMETRICS_OFF:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_FRACTIONALMETRICS_DEFAULT:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_STROKE_DEFAULT:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_STROKE_NORMALIZE:Ljava/lang/Object;",
                getter: "hint",
            },
            NativeField {
                sig: "VALUE_STROKE_PURE:Ljava/lang/Object;",
                getter: "hint",
            },
        ],
    },
    NativeClass {
        name: "java/awt/AlphaComposite",
        module: "n::awt",
        sup: Some(OBJECT),
        interfaces: &["java/awt/Composite"],
        interface: false,
        prims: 1,
        refs: 0,
        ext: "",
        methods: &[
            s("getInstance(I)Ljava/awt/AlphaComposite;", "alpha_composite"),
            s(
                "getInstance(IF)Ljava/awt/AlphaComposite;",
                "alpha_composite_alpha",
            ),
        ],
        statics: &[],
    },
    NativeClass {
        name: "jrt/JsonValue",
        module: "n::json",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "value_ext",
        methods: &[],
        statics: &[],
    },
    NativeClass {
        name: "java/lang/reflect/Array",
        module: "n::class",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            s(
                "newInstance(Ljava/lang/Class;I)Ljava/lang/Object;",
                "array_new_instance",
            ),
            s("getLength(Ljava/lang/Object;)I", "array_get_length"),
        ],
        statics: &[],
    },
    NativeClass {
        name: "java/security/AccessController",
        module: "n::system",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[s(
            "doPrivileged(Ljava/security/PrivilegedAction;)Ljava/lang/Object;",
            "do_privileged",
        )],
        statics: &[],
    },
    NativeClass {
        name: "jdk/internal/misc/Unsafe",
        module: "n::unsafe_",
        sup: Some(OBJECT),
        interfaces: &[],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "",
        methods: &[
            s("getUnsafe()Ljdk/internal/misc/Unsafe;", "get_unsafe"),
            v("isBigEndian()Z", "is_big_endian"),
        ],
        statics: &[
            NativeField {
                sig: "ARRAY_BOOLEAN_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_BYTE_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_CHAR_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_SHORT_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_INT_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_LONG_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_FLOAT_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_DOUBLE_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_OBJECT_BASE_OFFSET:I",
                getter: "base_offset",
            },
            NativeField {
                sig: "ARRAY_BOOLEAN_INDEX_SCALE:I",
                getter: "scale_1",
            },
            NativeField {
                sig: "ARRAY_BYTE_INDEX_SCALE:I",
                getter: "scale_1",
            },
            NativeField {
                sig: "ARRAY_CHAR_INDEX_SCALE:I",
                getter: "scale_2",
            },
            NativeField {
                sig: "ARRAY_SHORT_INDEX_SCALE:I",
                getter: "scale_2",
            },
            NativeField {
                sig: "ARRAY_INT_INDEX_SCALE:I",
                getter: "scale_4",
            },
            NativeField {
                sig: "ARRAY_LONG_INDEX_SCALE:I",
                getter: "scale_8",
            },
            NativeField {
                sig: "ARRAY_FLOAT_INDEX_SCALE:I",
                getter: "scale_4",
            },
            NativeField {
                sig: "ARRAY_DOUBLE_INDEX_SCALE:I",
                getter: "scale_8",
            },
            NativeField {
                sig: "ARRAY_OBJECT_INDEX_SCALE:I",
                getter: "scale_4",
            },
        ],
    },
    exc("java/lang/Exception", "java/lang/Throwable"),
    exc("java/lang/Error", "java/lang/Throwable"),
    exc("java/lang/RuntimeException", "java/lang/Exception"),
    exc(
        "java/lang/NullPointerException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/ArithmeticException",
        "java/lang/RuntimeException",
    ),
    index_exc(
        "java/lang/IndexOutOfBoundsException",
        "java/lang/RuntimeException",
        IOOBE_METHODS,
    ),
    index_exc(
        "java/lang/ArrayIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
        AIOOBE_METHODS,
    ),
    index_exc(
        "java/lang/StringIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
        SIOOBE_METHODS,
    ),
    exc("java/lang/ClassCastException", "java/lang/RuntimeException"),
    exc(
        "java/lang/NegativeArraySizeException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/ArrayStoreException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/IllegalArgumentException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/NumberFormatException",
        "java/lang/IllegalArgumentException",
    ),
    exc(
        "java/lang/IllegalStateException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/UnsupportedOperationException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/CloneNotSupportedException",
        "java/lang/Exception",
    ),
    exc("java/lang/InterruptedException", "java/lang/Exception"),
    exc(
        "java/lang/ReflectiveOperationException",
        "java/lang/Exception",
    ),
    exc(
        "java/lang/ClassNotFoundException",
        "java/lang/ReflectiveOperationException",
    ),
    exc("java/lang/LinkageError", "java/lang/Error"),
    exc("java/lang/NoClassDefFoundError", "java/lang/LinkageError"),
    exc(
        "java/lang/IncompatibleClassChangeError",
        "java/lang/LinkageError",
    ),
    exc(
        "java/lang/AbstractMethodError",
        "java/lang/IncompatibleClassChangeError",
    ),
    exc(
        "java/lang/NoSuchMethodError",
        "java/lang/IncompatibleClassChangeError",
    ),
    exc(
        "java/lang/NoSuchFieldError",
        "java/lang/IncompatibleClassChangeError",
    ),
    exc(
        "java/lang/ExceptionInInitializerError",
        "java/lang/LinkageError",
    ),
    exc("java/lang/UnsatisfiedLinkError", "java/lang/LinkageError"),
    exc("java/lang/VirtualMachineError", "java/lang/Error"),
    exc(
        "java/lang/StackOverflowError",
        "java/lang/VirtualMachineError",
    ),
    exc(
        "java/lang/OutOfMemoryError",
        "java/lang/VirtualMachineError",
    ),
    NativeClass {
        methods: ASSERTION_METHODS,
        ..exc("java/lang/AssertionError", "java/lang/Error")
    },
    exc(
        "java/lang/NoSuchMethodException",
        "java/lang/ReflectiveOperationException",
    ),
    exc(
        "java/lang/NoSuchFieldException",
        "java/lang/ReflectiveOperationException",
    ),
    exc(
        "java/lang/IllegalAccessException",
        "java/lang/ReflectiveOperationException",
    ),
    exc(
        "java/lang/InstantiationException",
        "java/lang/ReflectiveOperationException",
    ),
    exc(
        "java/lang/reflect/InvocationTargetException",
        "java/lang/ReflectiveOperationException",
    ),
    exc("java/io/UncheckedIOException", "java/lang/RuntimeException"),
    exc("java/lang/InternalError", "java/lang/VirtualMachineError"),
    exc("java/lang/SecurityException", "java/lang/RuntimeException"),
    exc("java/io/IOException", "java/lang/Exception"),
    exc("java/io/FileNotFoundException", "java/io/IOException"),
    exc(
        "java/util/NoSuchElementException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/util/ConcurrentModificationException",
        "java/lang/RuntimeException",
    ),
];

const fn builder(name: &'static str) -> NativeClass {
    NativeClass {
        name,
        module: "n::builder",
        sup: Some("java/lang/AbstractStringBuilder"),
        interfaces: &["java/io/Serializable"],
        interface: false,
        prims: 0,
        refs: 0,
        ext: "ext",
        methods: BUILDER_METHODS,
        statics: &[],
    }
}

const BUILDER_METHODS: &[NativeMethod] = &[
    d(
        "appendCodePoint(I)Ljava/lang/StringBuilder;",
        "append_code_point",
    ),
    d(
        "appendCodePoint(I)Ljava/lang/StringBuffer;",
        "append_code_point",
    ),
    d(
        "append(Ljava/lang/CharSequence;)Ljava/lang/Appendable;",
        "append_obj",
    ),
    d(
        "append(Ljava/lang/CharSequence;II)Ljava/lang/Appendable;",
        "append_seq_range",
    ),
    d("append(C)Ljava/lang/Appendable;", "append_c"),
    d(
        "append(Ljava/lang/CharSequence;II)Ljava/lang/StringBuilder;",
        "append_seq_range",
    ),
    d(
        "append(Ljava/lang/CharSequence;II)Ljava/lang/StringBuffer;",
        "append_seq_range",
    ),
    d(
        "append([CII)Ljava/lang/StringBuilder;",
        "append_chars_range",
    ),
    d("append([CII)Ljava/lang/StringBuffer;", "append_chars_range"),
    d("indexOf(Ljava/lang/String;)I", "index_of"),
    d("<init>()V", "init"),
    d("<init>(I)V", "init_capacity"),
    d("<init>(Ljava/lang/String;)V", "init_str"),
    d("<init>(Ljava/lang/CharSequence;)V", "init_str"),
    d(
        "append(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        "append_obj",
    ),
    d(
        "append(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        "append_obj",
    ),
    d(
        "append(Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;",
        "append_obj",
    ),
    d("append(I)Ljava/lang/StringBuilder;", "append_i"),
    d("append(J)Ljava/lang/StringBuilder;", "append_j"),
    d("append(F)Ljava/lang/StringBuilder;", "append_f"),
    d("append(D)Ljava/lang/StringBuilder;", "append_d"),
    d("append(Z)Ljava/lang/StringBuilder;", "append_z"),
    d("append(C)Ljava/lang/StringBuilder;", "append_c"),
    d("append([C)Ljava/lang/StringBuilder;", "append_chars"),
    d(
        "append(Ljava/lang/String;)Ljava/lang/StringBuffer;",
        "append_obj",
    ),
    d(
        "append(Ljava/lang/Object;)Ljava/lang/StringBuffer;",
        "append_obj",
    ),
    d(
        "append(Ljava/lang/CharSequence;)Ljava/lang/StringBuffer;",
        "append_obj",
    ),
    d("append(I)Ljava/lang/StringBuffer;", "append_i"),
    d("append(J)Ljava/lang/StringBuffer;", "append_j"),
    d("append(F)Ljava/lang/StringBuffer;", "append_f"),
    d("append(D)Ljava/lang/StringBuffer;", "append_d"),
    d("append(Z)Ljava/lang/StringBuffer;", "append_z"),
    d("append(C)Ljava/lang/StringBuffer;", "append_c"),
    d("append([C)Ljava/lang/StringBuffer;", "append_chars"),
    d(
        "insert(ILjava/lang/String;)Ljava/lang/StringBuilder;",
        "insert_str",
    ),
    d(
        "insert(ILjava/lang/String;)Ljava/lang/StringBuffer;",
        "insert_str",
    ),
    d("insert(IC)Ljava/lang/StringBuilder;", "insert_c"),
    d("insert(IC)Ljava/lang/StringBuffer;", "insert_c"),
    d("reverse()Ljava/lang/StringBuilder;", "reverse"),
    d("reverse()Ljava/lang/StringBuffer;", "reverse"),
    d("deleteCharAt(I)Ljava/lang/StringBuilder;", "delete_char_at"),
    d("deleteCharAt(I)Ljava/lang/StringBuffer;", "delete_char_at"),
    d("delete(II)Ljava/lang/StringBuilder;", "delete"),
    d("delete(II)Ljava/lang/StringBuffer;", "delete"),
    v("toString()Ljava/lang/String;", "to_string"),
];

const MATH: &[NativeMethod] = &[
    s("sin(D)D", "sin"),
    s("cos(D)D", "cos"),
    s("tan(D)D", "tan"),
    s("asin(D)D", "asin"),
    s("acos(D)D", "acos"),
    s("atan(D)D", "atan"),
    s("atan2(DD)D", "atan2"),
    s("sinh(D)D", "sinh"),
    s("cosh(D)D", "cosh"),
    s("tanh(D)D", "tanh"),
    s("sqrt(D)D", "sqrt"),
    s("cbrt(D)D", "cbrt"),
    s("pow(DD)D", "pow"),
    s("exp(D)D", "exp"),
    s("expm1(D)D", "expm1"),
    s("log(D)D", "log"),
    s("log10(D)D", "log10"),
    s("log1p(D)D", "log1p"),
    s("hypot(DD)D", "hypot"),
    s("floor(D)D", "floor"),
    s("ceil(D)D", "ceil"),
    s("rint(D)D", "rint"),
    s("round(D)J", "round_d"),
    s("round(F)I", "round_f"),
    s("abs(I)I", "abs_i"),
    s("abs(J)J", "abs_j"),
    s("abs(F)F", "abs_f"),
    s("abs(D)D", "abs_d"),
    s("max(II)I", "max_i"),
    s("max(JJ)J", "max_j"),
    s("max(FF)F", "max_f"),
    s("max(DD)D", "max_d"),
    s("min(II)I", "min_i"),
    s("min(JJ)J", "min_j"),
    s("min(FF)F", "min_f"),
    s("min(DD)D", "min_d"),
    s("signum(D)D", "signum_d"),
    s("signum(F)F", "signum_f"),
    s("toRadians(D)D", "to_radians"),
    s("toDegrees(D)D", "to_degrees"),
    s("random()D", "random"),
    s("ulp(D)D", "ulp"),
    s("IEEEremainder(DD)D", "ieee_remainder"),
    s("copySign(DD)D", "copy_sign"),
    s("floorMod(II)I", "floor_mod_i"),
    s("floorDiv(II)I", "floor_div_i"),
    s("addExact(II)I", "add_exact_i"),
    s("addExact(JJ)J", "add_exact_j"),
    s("subtractExact(II)I", "sub_exact_i"),
    s("subtractExact(JJ)J", "sub_exact_j"),
    s("multiplyExact(II)I", "mul_exact_i"),
    s("multiplyExact(JJ)J", "mul_exact_j"),
    s("toIntExact(J)I", "to_int_exact"),
    s("floorDiv(JJ)J", "floor_div_j"),
    s("floorMod(JJ)J", "floor_mod_j"),
];
