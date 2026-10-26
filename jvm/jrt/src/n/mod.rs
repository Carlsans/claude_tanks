//! The native classes' methods (see `spec.rs`). Instance methods take the
//! receiver first; booleans, bytes, chars and shorts travel as `i32`, as on
//! the JVM's operand stack.

pub mod boxes;
pub mod builder;
pub mod class;
pub mod enums;
pub mod format;
pub mod iface;
pub mod math;
pub mod object;
pub mod print;
pub mod random;
pub mod string;
pub mod system;
pub mod throwable;
pub mod unsafe_;
