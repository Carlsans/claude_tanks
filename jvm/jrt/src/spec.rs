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

const EXC_METHODS: &[NativeMethod] = &[
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
            d("notify()V", "nop"),
            d("notifyAll()V", "nop"),
            d("wait()V", "nop"),
            d("wait(J)V", "wait_j"),
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
    iface("java/lang/Appendable", &[], &[]),
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
            d("intern()Ljava/lang/String;", "intern"),
            s(
                "valueOf(Ljava/lang/Object;)Ljava/lang/String;",
                "value_of_obj",
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
    boxed(
        "java/lang/Integer",
        "n::boxes::integer",
        &[
            d("<init>(I)V", "init"),
            s("valueOf(I)Ljava/lang/Integer;", "value_of"),
            s(
                "valueOf(Ljava/lang/String;)Ljava/lang/Integer;",
                "value_of_str",
            ),
            s("parseInt(Ljava/lang/String;)I", "parse"),
            s("toString(I)Ljava/lang/String;", "to_string_static"),
            s("toHexString(I)Ljava/lang/String;", "to_hex_string"),
            s("toBinaryString(I)Ljava/lang/String;", "to_binary_string"),
            s("bitCount(I)I", "bit_count"),
            s("signum(I)I", "signum"),
            s("compare(II)I", "compare"),
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
    ),
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
        ],
        statics: &[],
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
    exc(
        "java/lang/IndexOutOfBoundsException",
        "java/lang/RuntimeException",
    ),
    exc(
        "java/lang/ArrayIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
    ),
    exc(
        "java/lang/StringIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
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
    exc("java/lang/AssertionError", "java/lang/Error"),
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
];
