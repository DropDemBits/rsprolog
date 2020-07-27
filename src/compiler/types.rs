//! Type facilities
//! A type is considered a 'Named' type if it is:
//! - A pointer to another type
//! - A complex or compound type, which includes
//!   - Any type including a range (e.g. 'set', 'array')
//!   - Any type including a grouping of other types
//! Otherwise, the type is considered to be a 'Primative' type
use crate::compiler::ast::Expr;
use crate::compiler::frontend::token::TokenType;
use crate::compiler::value::{self, Value};
use std::collections::HashMap;
use std::convert::TryFrom;

/// Default string size, in bytes
/// This is the default size for a string if it is not specified
/// Includes the null terminator
pub const DEFAULT_STRING_SIZE: usize = 256;

/// Maximum string size, in bytes
/// Includes the null terminator
pub const MAX_STRING_SIZE: usize = 65536;

/// Unique type reference for a type
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TypeRef {
    /// Unknown type reference, to be resolved in the validator stage
    Unknown,
    /// Error during type validation or parsing
    TypeError,
    /// Reference to a primitive type
    Primitive(PrimitiveType),
    /// Reference to a named type, with the unit type
    Named(usize),
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SequenceSize {
    /// Compile-time expression, resolved to Size at validator time.
    /// Points into the type table.
    CompileExpr(usize),
    /// Constant parsed. 0 indicates any length
    Size(usize),
}

/// Enum of basic primitive types
/// Not included in the unit type table
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum PrimitiveType {
    // Base primative types
    /// Boolean type
    Boolean,
    /// Basic integer type, equivalent to 'i32'
    Int,
    /// Sized integer type, equivalent to 'i8'
    Int1,
    /// Sized integer type, equivalent to 'i16'
    Int2,
    /// Sized integer type, equivalent to 'i32'
    /// Allows for the assignment of 0x80000000, or UNINIT_INT
    Int4,
    /// Sized integer type, equivalent to 'i64'
    /// Allows for the assignment of 0x8000000080000000, or UNINIT_LONG_INT
    Int8,
    /// Basic unsigned integer type, equivalent to 'u32'
    Nat,
    /// Sized unsigned integer type, equivalent to 'u8'
    Nat1,
    /// Sized unsigned integer type, equivalent to 'u16'
    Nat2,
    /// Sized unsigned integer type, equivalent to 'u32'
    /// Allows for the assignment of 0xFFFFFFFF, or UNINIT_NAT
    Nat4,
    /// Sized unsigned integer type, equivalent to 'u64'
    /// Allows for the assignment of 0xFFFFFFFFFFFFFFFF, or UNINIT_LONG_NAT
    Nat8,
    /// Basic, checked long integer type, equivalent to 'i64'
    LongInt,
    /// Basic, checked long natural type, equivalent to 'u64'
    LongNat,
    /// Ambiguious int/nat type, produced by literals.
    /// Converts into the int or nat of the appropriate size, by default into
    /// an int.
    IntNat,
    /// Basic real type, equivalent to 'f64'
    Real,
    /// Sized real type, equivalent to 'f32'
    Real4,
    /// Sized real type, equivalent to 'f64'
    /// Allows for the assignment of the denormalized 0x800000000_800000000, or
    /// UNINIT_REAL
    Real8,
    /// Variable-sized string of ASCII characters (u8's)
    /// The default size of a string is `DEFAULT_STRING_SIZE`, but can grow to
    // accommodate larger strings
    String_,
    /// Fixed-size string of ASCII characters (u8's)
    /// `SequenceSize` is the maximum length storable in the string
    /// A size of zero indicates a dynamic length type, or a '*' size specifier
    /// Assignable to other StrN's of the same or larger size
    StringN(SequenceSize),
    /// A single ASCII character
    Char,
    /// Multiple ASCII characters (u8's)
    /// `SequenceSize` is the maximum length storable in the string
    /// A size of zero indicates a dynamic length type, or a '*' size specifier
    /// Assignable to other CharN's of the same or larger size
    CharN(SequenceSize),
    /// A type able to store a pointer address
    /// The size of an AddressInt varies between compiling for 32-bit or 64-bit machines
    /// If compiling for 32-bit, the pointer size is 4 bytes
    /// If compiling for 64-bit, the pointer size is 8 bytes
    AddressInt,
    /// General nil pointer type
    Nil,
}

/// Parameter definition
/// Two parameter definitions (ParamDef's) are equivalent if, and only if, all
/// fields except for name are equivalent.
#[derive(Debug, Clone)]
pub struct ParamDef {
    /// The name of the parameter
    pub name: String,
    /// The type_spec for the parameter
    pub type_spec: TypeRef,
    // Whether to pass the parameter by reference, allowing the function to modify the value (specified by "var")
    pub pass_by_ref: bool,
    // Whether to bind the parameter into a register (specified by "register")
    pub bind_to_register: bool,
    /// Whether to coerece the type of the input argument into binding the declared type
    pub force_type: bool,
}

impl PartialEq for ParamDef {
    fn eq(&self, other: &Self) -> bool {
        self.type_spec == other.type_spec
            && self.pass_by_ref == other.pass_by_ref
            && self.bind_to_register == other.bind_to_register
            && self.force_type == other.force_type
    }
}

/// Base Type Root
#[derive(Debug, Clone)]
pub enum Type {
    /// Alias to another Type
    Alias {
        /// other Type aliased by the current Type
        to: TypeRef,
    },
    /// Array Type
    Array {
        /// Ranges for the array
        ranges: Vec<TypeRef>,
        /// Element type of the array
        element_type: TypeRef,
        /// If the array can be resized at runtime
        is_flexible: bool,
        /// If the array has an upper bound based on the initializing expression
        is_init_sized: bool,
    },
    /// Enum Type
    Enum {
        /// Valid enumeration fields, and the associated enum field
        fields: HashMap<String, TypeRef>,
    },
    /// Enum field type
    EnumField {
        /// Reference to the base enum type.
        /// Always points to the dealiased type
        enum_type: TypeRef,
        /// The ordinal value of the field
        ordinal: usize,
    },
    /// Forward reference to a type
    Forward {
        /// If the reference has been resolved in the current unit
        is_resolved: bool,
    },
    /// Function / Procedure definition \
    /// Having both as Options allows differentiation between parameter and
    /// parameterless declarations, and between functions and procedures
    Function {
        /// Parameter specification for the function
        params: Option<Vec<ParamDef>>,
        /// Result type for the function
        result: Option<TypeRef>,
    },
    /// Pointer to a given TypeRef
    Pointer { to: TypeRef },
    /// Inclusive range type, encoding `start` .. `end` and `start` .. * \
    /// `start` must evaluate to be less than or equal to `end` \
    /// Expressions are used, as dynamic arrays have an upper bound
    /// that can be a runtime dependent value
    Range {
        /// Start of the range
        start: Expr,
        /// End of the range
        /// None is equivalent to specifiying *
        end: Option<Expr>,
        /// Base type for the range.
        /// Can be an int, enum type, char, or boolean, depending on the range evaluation.
        /// This is always a de-aliased type.
        base_type: TypeRef,
    },
    /// A reference to a named type.
    /// This type is resolved into the corresponding type at the validation stage,
    /// as imports are resovled before validation
    Reference { expr: Box<Expr> },
    /// Set of values in a given range.
    /// The start and end expressions of the range must be compile-time evaluable.
    Set { range: TypeRef },
    /// Expression holding the size of a Char(n) or a String(n)
    SizeExpr { expr: Box<Expr> },
}

/// Table of all named references defined in the scope
#[derive(Debug)]
pub struct TypeTable {
    /// Next type id
    next_id: usize,
    /// Type Table
    types: Vec<Type>,
}

impl TypeTable {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            types: vec![],
        }
    }

    /// Declares a new type in the current unit, returning the type id
    pub fn declare_type(&mut self, type_info: Type) -> usize {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("Too many types defined in the unit");
        self.types.push(type_info);
        id
    }

    /// Replaces an existing type in the unit with the given type info
    pub fn replace_type(&mut self, replace_id: usize, type_info: Type) {
        self.types[replace_id] = type_info;
    }

    /// Converts the `type_ref` into the corresponding type info
    pub fn type_from_ref(&self, type_ref: &TypeRef) -> Option<&Type> {
        if let TypeRef::Named(type_id) = type_ref {
            Some(self.get_type(*type_id))
        } else {
            None
        }
    }

    /// Gets a reference to a defined type
    pub fn get_type(&self, type_id: usize) -> &Type {
        &self.types[type_id]
    }

    /// Gets a mutable reference to a defined type
    pub fn get_type_mut(&mut self, type_id: usize) -> &mut Type {
        &mut self.types[type_id]
    }

    /// Checks if the given type is an indirect alias for another type.
    /// This includes both Alias and Reference types.
    pub fn is_indirect_alias(&self, type_id: usize) -> bool {
        matches!(self.get_type(type_id), Type::Alias{ .. } | Type::Reference { .. })
    }
}

// --- Helpers for deriving the appropriate type ---
/// Makes the appropriate `StringN` type for the given `String`
pub fn get_string_kind(s: &String) -> PrimitiveType {
    let size = s.bytes().count();

    PrimitiveType::StringN(SequenceSize::Size(size))
}

/// Makes the appropriate `CharN` type for the given `String`
pub fn get_char_kind(s: &String) -> PrimitiveType {
    let size = s.bytes().count();

    PrimitiveType::CharN(SequenceSize::Size(size))
}

/// Gets the appropriate int type for the given integer value
pub fn get_int_kind(v: i64) -> PrimitiveType {
    if i32::MIN as i64 >= v && v <= i32::MAX as i64 {
        PrimitiveType::Int
    } else {
        PrimitiveType::LongInt
    }
}

/// Gets the appropriate nat type for the given integer value
pub fn get_nat_kind(v: u64) -> PrimitiveType {
    if v <= u32::MAX as u64 {
        PrimitiveType::Nat
    } else {
        PrimitiveType::LongNat
    }
}

/// Gets the appropriate int/nat type for the given integer value
pub fn get_intnat_kind(v: u64) -> PrimitiveType {
    if v <= i32::MAX as u64 {
        PrimitiveType::IntNat
    } else if v <= u32::MAX as u64 {
        PrimitiveType::Nat
    } else if v <= i64::MAX as u64 {
        PrimitiveType::LongInt
    } else {
        PrimitiveType::LongNat
    }
}

/// Gets the appropriate character sequence base type for the given
/// primitive TypeRef.
pub fn get_char_seq_base_type(seq_ref: TypeRef) -> TypeRef {
    if let TypeRef::Primitive(primitive) = seq_ref {
        let base_primivite = match primitive {
            PrimitiveType::CharN(_) => PrimitiveType::Char,
            PrimitiveType::StringN(_) => PrimitiveType::String_,
            PrimitiveType::String_ => PrimitiveType::String_,
            PrimitiveType::Char => PrimitiveType::Char,
            _ => panic!("Tried to convert a non char sequence type into a char sequence base type"),
        };

        TypeRef::Primitive(base_primivite)
    } else {
        panic!("Tried to convert non primitive type ref into a primitive char sequence type");
    }
}

// Helpers for comparing types

/// Checks if the given `type_ref` is a type error
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_error(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::TypeError)
}

/// Checks if the given `type_ref` is a primitive reference
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_primitive(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Primitive(_))
}

/// Checks if the given `type_ref` is a named reference
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_named(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Named(_))
}

/// Checks if the given `type_ref` references an unsized string type (String_)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_string(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(kind, PrimitiveType::String_),
        _ => false,
    }
}

/// Checks if the given `type_ref` references a sized char type (CharN(x))
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_charn(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(kind, PrimitiveType::CharN(_)),
        _ => false,
    }
}

/// Checks if the given `type_ref` references a single char type (Char)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_char(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(kind, PrimitiveType::Char),
        _ => false,
    }
}

/// Checks if the given `type_ref` references a string-class type (String_ & StringN(x)).
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_string_type(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => {
            matches!(kind, PrimitiveType::String_ | PrimitiveType::StringN(_))
        }
        _ => false,
    }
}

/// Checks if the given `type_ref` references a char sequence class type (String_, StringN(x), CharN(x))
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_char_seq_type(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => {
            matches!(kind, PrimitiveType::String_ | PrimitiveType::StringN(_) | PrimitiveType::CharN(_))
        }
        _ => false,
    }
}

/// Checks if the given `type_ref` references a sized char sequence class type (StringN(x), CharN(x))
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_sized_char_seq_type(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => {
            matches!(kind, PrimitiveType::StringN(_) | PrimitiveType::CharN(_))
        }
        _ => false,
    }
}

pub fn get_sized_len(type_ref: &TypeRef) -> Option<usize> {
    match type_ref {
        TypeRef::Primitive(kind) => match kind {
            PrimitiveType::StringN(SequenceSize::Size(s))
            | PrimitiveType::CharN(SequenceSize::Size(s)) => Some(*s),
            _ => None, // Can't resolve a compile-time expression or not a string sequence
        },
        _ => None,
    }
}

/// Gets a type id from a type reference
/// Returns `None` if the type is not named
pub fn get_type_id(type_ref: &TypeRef) -> Option<usize> {
    if let TypeRef::Named(type_id) = type_ref {
        Some(*type_id)
    } else {
        None
    }
}

/// Checks if the given `type_ref` references a real type (real, real4, real8)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_real(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(
            kind,
            PrimitiveType::Real | PrimitiveType::Real4 | PrimitiveType::Real8
        ),
        _ => false,
    }
}

/// Checks if the given `type_ref` references an integer class type (int, nat, long int, long nat, addressint)
pub fn is_integer_type(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(
            kind,
            PrimitiveType::Int
                | PrimitiveType::Int1
                | PrimitiveType::Int2
                | PrimitiveType::Int4
                | PrimitiveType::Nat
                | PrimitiveType::Nat1
                | PrimitiveType::Nat2
                | PrimitiveType::Nat4
                | PrimitiveType::LongInt
                | PrimitiveType::Int8
                | PrimitiveType::LongNat
                | PrimitiveType::Nat8
                | PrimitiveType::AddressInt
                | PrimitiveType::IntNat
        ),
        _ => false,
    }
}

/// Checks if the given `type_ref` references an int (long int, int, int1, int2, int4, int8)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_int(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(
            kind,
            PrimitiveType::Int
                | PrimitiveType::Int1
                | PrimitiveType::Int2
                | PrimitiveType::Int4
                | PrimitiveType::LongInt
                | PrimitiveType::Int8
        ),
        _ => false,
    }
}

/// Checks if the given `type_ref` references an int/nat convertable type (int & nat class types)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_intnat(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(kind, PrimitiveType::IntNat),
        _ => false,
    }
}

/// Checks if the given `type_ref` references a nat (addressint, long nat, nat, nat1, nat2, nat4, nat8)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_nat(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(
            kind,
            PrimitiveType::Nat
                | PrimitiveType::Nat1
                | PrimitiveType::Nat2
                | PrimitiveType::Nat4
                | PrimitiveType::LongNat
                | PrimitiveType::Nat8
                | PrimitiveType::AddressInt
        ),
        _ => false,
    }
}

/// Checks if the given `type_ref` references number type (real, int, nat, long int, long nat)
pub fn is_number_type(type_ref: &TypeRef) -> bool {
    is_real(type_ref) || is_integer_type(type_ref)
}

/// Checks if the given `type_ref` references a boolean
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_boolean(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Primitive(kind) => matches!(kind, PrimitiveType::Boolean),
        _ => false,
    }
}

/// Checks if the given `type_ref` references a base type (i.e. the
/// reference does not point to a Type::Alias, Type::Named, Type::Forward,
/// TypeRef::Unknown, or TypeRef::TypeError)
pub fn is_base_type(type_ref: &TypeRef, type_table: &TypeTable) -> bool {
    match type_ref {
        TypeRef::Unknown | TypeRef::TypeError => false,
        TypeRef::Primitive(_) => true,
        TypeRef::Named(type_id) => !matches!(
            type_table.get_type(*type_id),
            Type::Alias { .. } | Type::Reference { .. } | Type::Forward { .. }
        ),
    }
}

/// Checks if the given `type_ref` references a set type (Type::Set)
/// Requires that `type_ref` is de-aliased (i.e. all aliased references are
/// forwarded to the base type)
pub fn is_set(type_ref: &TypeRef, type_table: &TypeTable) -> bool {
    matches!(type_table.type_from_ref(type_ref), Some(Type::Set { .. }))
}

pub fn is_index_type(type_ref: &TypeRef, type_table: &TypeTable) -> bool {
    matches!(
        type_ref,
        TypeRef::Primitive(PrimitiveType::Char) | TypeRef::Primitive(PrimitiveType::Boolean)
    ) || matches!(type_table.type_from_ref(type_ref), Some(Type::Range { .. })) // Don't forget about enum types!
}

/// Gets the common type between the two given type refs
pub fn common_type<'a>(
    lhs: &'a TypeRef,
    rhs: &'a TypeRef,
    _type_table: &'_ TypeTable,
) -> Option<&'a TypeRef> {
    // TODO: Between strings, stringNs, charNs, chars, sets, classes, pointers, etc.
    if lhs == rhs && !(is_intnat(lhs) && is_intnat(rhs)) {
        // Both are the same type, so they're both in common with eachother
        Some(lhs)
    } else if (is_real(lhs) && is_number_type(rhs)) || (is_number_type(lhs) && is_real(rhs)) {
        // Number types get promoted to real types if any 'real' exists
        Some(&TypeRef::Primitive(PrimitiveType::Real))
    } else if (is_int(lhs) && is_integer_type(rhs)) || (is_integer_type(lhs) && is_int(rhs)) {
        // Integer types get promoted to int types if any 'int' exists
        if is_int(lhs) {
            Some(lhs)
        } else {
            Some(rhs)
        }
    } else if (is_intnat(lhs) && is_integer_type(rhs)) || (is_integer_type(lhs) && is_intnat(rhs)) {
        // Int/Nats get converted into Ints by default
        if !is_intnat(lhs) {
            Some(lhs)
        } else {
            Some(rhs)
        }
    } else if is_nat(lhs) && is_nat(rhs) {
        // Common nat types produce 'nat's
        Some(lhs)
    } else {
        // No common type
        None
    }
}

/// Checks if `rvalue` is assignable into `lvalue`
///
/// # Root types
/// Most types have their root types defined as themselves. However, range types and string-class types have different root types.
/// - All string-class types (i.e. `string` and `string(n)`) have `string` as the root type.
/// - All integer range types (e.g. `3 .. 20`) have `int` as the root type (i.e. all `int`s are assignable to the given range).
/// - All enum range types have the enum type as the root type
/// - All boolean range types (e.g. `false .. true`) have `boolean` as the root type (i.e. all `boolean`s are assignable to the given range).
///
/// # Assignability rules
/// - If two types (after de-aliasing) have the same `type_id` (i.e, have the same root definition) \
/// - If two types (after de-aliasing) have equvalent root types \
/// - If `lvalue` is 'real' and `rvalue` is either `real` or an integer-class type (`rvalue` is converted into an int) \
/// - If `lvalue` is a range and `rvalue` both is the same type kind, as well as existing in the given range \
/// - If `lvalue` is a `char` and `rvalue` is a `char(1)` (or vice versa)
/// - If `lvalue` is a `char` and `rvalue` is a char sequence of length 1 (i.e. `string(1)` or `char(1)`)
/// - If `lvalue` is a string-class type and `rvalue` is a `char` (producing a string of length 1)
/// - If `lvalue` is a `char(n)` and `rvalue` is a `string(n)` (`rvalue` gets converted into a `string(n)`)
/// - If `lvalue` and `rvalue` are pointers to classes and they are the same class or share a common ancestor
/// As well as:
/// - If `lvalue` is a string(x) and `rvalue` is a char
/// - If `lvalue` is a char(x) and `rvalue` is a char
/// - If `lvalue` is a string(x) and `rvalue` is a char(y), and if x >= y
/// - If `lvalue` is a string(n) or a char(n) and `rvalue` is a `string` (the assignment is checked at runtime)
pub fn is_assignable_to(lvalue: &TypeRef, rvalue: &TypeRef, type_table: &TypeTable) -> bool {
    // TODO: Not all of the assignability rules listed above are checked yet, still need to do
    // - Other equivalencies
    // - pointer class inheritance
    // - range containment
    // - set, function/procedure, and array equivalency
    if rvalue == lvalue {
        // Same types are assignable / equivalent

        // Also Covers:
        // - Record, Enum, Union assignment
        // - Opaque assignment

        // Somewhat covers
        // - Pointer to non-class type assignment (need to check pointed type equivalency)
        // - Set assignment (need to check for range equivalency)
        // - Array assignment (need to check for both ranges and element equivalency)
        // - Function/Procedure assignment (need to check param & result type equivalency)
        true
    } else if is_error(lvalue) || is_error(rvalue) {
        // Quick escape for type errors
        false
    } else if is_integer_type(lvalue) && is_integer_type(rvalue) {
        // Integer-class types are mutually assignable to eachother
        true
    } else if is_real(lvalue) && is_number_type(rvalue) {
        // Number-class types are assignable into 'real' types
        true
    } else if is_string(lvalue) && is_char_seq_type(rvalue) {
        // Char Sequence types are assignable into unsized 'string's
        true
    } else if is_sized_char_seq_type(lvalue) && is_sized_char_seq_type(rvalue) {
        // Must check length
        let lvalue_len = get_sized_len(lvalue).unwrap();
        let rvalue_len = get_sized_len(rvalue).unwrap();

        // Assignable if lvalue is a char(*), string(*) or if the rvalue can be contained inside of the lvalue
        lvalue_len == 0 || lvalue_len >= rvalue_len
    } else if is_sized_char_seq_type(lvalue) && is_string(rvalue) {
        // String assignment into char(x) or string(y) is checked at runtime
        true
    } else if (is_char_seq_type(lvalue) && is_char(rvalue))
        || (is_char(lvalue) && is_char_seq_type(rvalue))
    {
        // Must check length
        let lvalue_len = get_sized_len(lvalue).unwrap_or(1);
        let rvalue_len = get_sized_len(rvalue).unwrap_or(1);

        // `char` is only assignable into `char(n)` iff `char(n)` is of length 1 or greater
        // `char` is only assignable into `string(n)` iff `string(n)` is of length 1 or greater
        // `charn(n)` is only assignable into `char` iff `char(n)` is of length 1
        // `string(n)` is only assignable into `char` iff `string(n)` is of length 1
        // `char` is not assignable into `char(*)`
        // `char` is not assignable into `string(*)`

        // For the `char` <- `string` case, we have to default to true,
        // as the value of an unsized `string` can only be checked at runtime
        lvalue_len >= rvalue_len
    } else if let Some(Type::Range { base_type, .. }) = type_table.type_from_ref(lvalue) {
        // A value is assignable inside of a range type if the value is equivalent to the
        // range's base type
        is_equivalent_to(base_type, rvalue, type_table)
    } else {
        // This check is last as it performs very heavy type checking
        is_equivalent_to(lvalue, rvalue, type_table)
    }
}

/// Checks if the types are equivalent
pub fn is_equivalent_to(lhs: &TypeRef, rhs: &TypeRef, type_table: &TypeTable) -> bool {
    if is_error(lhs) || is_error(rhs) {
        // Quick escape for type errors
        return false;
    }

    if lhs == rhs {
        // Quick escape for simple equivalent types (e.g. primitives)
        return true;
    }

    // Other primitives
    if is_integer_type(lhs) && is_integer_type(rhs) {
        // Integer class types are equivalent
        return true;
    } else if is_real(lhs) && is_real(rhs) {
        // Real types are equivalent
        return true;
    } else if is_char(lhs) && is_char(rhs) {
        // Char types are equivalent
        return true;
    } else if (is_char(lhs) && is_charn(rhs)) || (is_charn(lhs) && is_char(rhs)) {
        // Must check length
        let lvalue_len = get_sized_len(lhs).unwrap_or(1);
        let rvalue_len = get_sized_len(rhs).unwrap_or(1);

        // char is equivalent to char(1), but not general char(n)
        return lvalue_len == rvalue_len;
    }

    // Perform equivalence testing based on the type info
    // TODO: Finish the equivalency cases
    // Unions, Records, Enums, and Collections have equivalency based on the type_id, so they will always fail this check
    if let TypeRef::Named(left_id) = lhs {
        if let TypeRef::Named(right_id) = rhs {
            let left_info = type_table.get_type(*left_id);
            let right_info = type_table.get_type(*right_id);

            match left_info {
                Type::Function { params, result } => {
                    if let Type::Function {
                        params: other_params,
                        result: other_result,
                    } = right_info
                    {
                        return params == other_params && result == other_result;
                    }
                }
                Type::Set { range } => {
                    if let Type::Set { range: other_range } = right_info {
                        // Sets are equivalent if the range types are equivalent
                        return is_equivalent_to(range, other_range, type_table);
                    }
                }
                Type::Range {
                    start,
                    end,
                    base_type,
                } => {
                    if let Type::Range {
                        start: other_start,
                        end: other_end,
                        base_type: other_type,
                    } = right_info
                    {
                        // Range type equivalency follows base type equivalency
                        if !is_equivalent_to(base_type, other_type, type_table) {
                            return false;
                        }

                        // Check the end range presence
                        // If either is absent, the ranges are not equivalent
                        if !end.is_none() && other_end.is_none() {
                            return false;
                        }

                        // Compare the start ranges
                        let is_start_eq = {
                            let start_value = Value::try_from(start.clone()).ok();
                            let other_start_value = Value::try_from(other_start.clone()).ok();

                            start_value
                                .and_then(|v| Some((v, other_start_value?)))
                                .and_then(|(a, b)| value::apply_binary(a, &TokenType::Equ, b).ok())
                                .map(|v| {
                                    let is_eq: bool = v.into();
                                    is_eq
                                })
                                .unwrap_or(false)
                        };

                        // Compare the end ranges
                        let is_end_eq = if end.as_ref().and(other_end.as_ref()).is_some() {
                            let end_value = Value::try_from(end.clone().unwrap()).ok();
                            let other_end_value = Value::try_from(other_end.clone().unwrap()).ok();

                            end_value
                                .and_then(|v| Some((v, other_end_value?)))
                                .and_then(|(a, b)| value::apply_binary(a, &TokenType::Equ, b).ok())
                                .map(|v| {
                                    let is_eq: bool = v.into();
                                    is_eq
                                })
                                .unwrap_or(false)
                        } else {
                            // Mismatched sizes
                            false
                        };

                        return is_start_eq && is_end_eq;
                    }
                }
                _ => todo!("??? {:?} & {:?}", left_info, right_info),
            }
        }
    }

    false
}

/// Dealiases the given ref, using the given type table.
/// Does not perform resolving of any types, and requires the previous
/// resolution of any Type::Reference found along the chain.
pub fn dealias_ref(type_ref: &TypeRef, type_table: &TypeTable) -> TypeRef {
    let mut current_ref = type_ref;

    loop {
        let type_id = if let Some(id) = get_type_id(current_ref) {
            id
        } else {
            // Reached a primitive type
            break *current_ref;
        };

        if let Type::Alias { to } = type_table.get_type(type_id) {
            // Advance the chain
            current_ref = to;
        } else {
            // Reached a non alias type
            debug_assert!(
                !matches!(type_table.get_type(type_id), Type::Reference { .. }),
                "A reference was not resolved at this point"
            );
            break *current_ref;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_typeref_eq() {
        // PartialEq is a type-wise comparison
        assert_eq!(TypeRef::TypeError, TypeRef::TypeError);
        assert_eq!(TypeRef::Unknown, TypeRef::Unknown);
        assert_eq!(
            TypeRef::Primitive(PrimitiveType::LongInt),
            TypeRef::Primitive(PrimitiveType::LongInt)
        );
        assert_eq!(TypeRef::Named(0), TypeRef::Named(0));

        assert_ne!(
            TypeRef::Primitive(PrimitiveType::LongInt),
            TypeRef::Primitive(PrimitiveType::String_)
        );
        assert_ne!(TypeRef::Named(1), TypeRef::Named(5));
    }
}
