//! AST structure definitions
use crate::scope::ScopeBlock;
use crate::types::TypeRef;
use toc_core::Location;

use std::fmt;

/// Identifier id, associated with a unique declaration of an identifier
#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
pub struct IdentId(pub u32);

/// A reference to an identifier in the AST.
///
/// Just a named reference to both
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct IdentRef {
    /// Id of the referenced identifier
    pub id: IdentId,
    /// Location of the reference
    pub location: Location,
}

impl IdentRef {
    pub fn new(id: IdentId, location: Location) -> Self {
        Self { id, location }
    }
}

/// Definition of an identifier
#[derive(Debug, Clone)]
pub struct Identifier {
    /// The declaration location of this identifier in the source code.
    pub location: Location,
    /// The name of the identifier.
    pub name: String,
    /// The type for this identifier.
    pub type_spec: TypeRef,
    /// If the identifier backs a storage unit not mutable at runtime.
    pub is_const: bool,
    /// If the identifier is the name for the type definition pointed to by
    /// `type_spec`.
    pub is_typedef: bool,
    /// If the identifier has been declared in a declaration statement, or
    /// has been defined by reference to the name (used to keep track of undefined
    /// identifiers).
    pub is_declared: bool,
    /// If the identifier references a value that can be evaluated at compile time.
    pub is_compile_eval: bool,
    /// If the identifier is pervasive and is able to be implicitly imported into
    /// child scopes
    pub is_pervasive: bool,
    /// The number of times this identifier has been used
    pub usages: usize,
}

impl Identifier {
    /// Creates a new identifier.
    /// Specifying an import index of '0' indicates that the identifier is not imported
    /// `token` Location of the reference token
    pub fn new(
        location: Location,
        type_spec: TypeRef,
        name: String,
        is_const: bool,
        is_typedef: bool,
        is_declared: bool,
        is_pervasive: bool,
    ) -> Self {
        Self {
            location,
            name,
            type_spec,
            is_const,
            is_typedef,
            is_declared,
            is_pervasive,
            is_compile_eval: false,
            usages: 0,
        }
    }
}

/// Binary operators
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BinaryOp {
    /// Addition / Set Union / String Concatenation (`+`)
    Add,
    /// Subtraction / Set Subtraction (`*`)
    Sub,
    /// Multiplication / Set Intersection (`*`)
    Mul,
    /// Integer Division (`div`)
    Div,
    /// Real Division (`/`)
    RealDiv,
    /// Modulo (`mod`)
    Mod,
    /// Remainder (`rem`)
    Rem,
    /// Exponentiation (`**`)
    Exp,
    /// Bitwise/boolean And (`and`)
    And,
    /// Bitwise/boolean Or (`or`)
    Or,
    /// Bitwise/boolean Exclusive-Or (`xor`)
    Xor,
    /// Logical Shift Left (`shl`)
    Shl,
    /// Logical Shift Right (`shr`)
    Shr,
    /// Less than (`<`)
    Less,
    /// Less than or Equal (`<=`)
    LessEq,
    /// Greater than (`>`)
    Greater,
    /// Greater than or Equal (`>=`)
    GreaterEq,
    /// Equality (`=` or `=`)
    Equal,
    /// Inequality (`not=` or `~=`)
    NotEqual,
    /// Set inclusion (`in`)
    In,
    /// Set exclusion (`not in`)
    NotIn,
    /// Material Implication (`=>`)
    Imply,
    /// Arrow (`->`)
    Arrow,
    /// Dot (`.`)
    Dot,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOp::Add => f.write_str("+"),
            BinaryOp::Sub => f.write_str("-"),
            BinaryOp::Mul => f.write_str("*"),
            BinaryOp::Div => f.write_str("div"),
            BinaryOp::RealDiv => f.write_str("/"),
            BinaryOp::Mod => f.write_str("mod"),
            BinaryOp::Rem => f.write_str("rem"),
            BinaryOp::Exp => f.write_str("**"),
            BinaryOp::And => f.write_str("and"),
            BinaryOp::Or => f.write_str("or"),
            BinaryOp::Xor => f.write_str("xor"),
            BinaryOp::Shl => f.write_str("shl"),
            BinaryOp::Shr => f.write_str("shr"),
            BinaryOp::Less => f.write_str("<"),
            BinaryOp::LessEq => f.write_str("<="),
            BinaryOp::Greater => f.write_str(">"),
            BinaryOp::GreaterEq => f.write_str(">="),
            BinaryOp::Equal => f.write_str("="),
            BinaryOp::NotEqual => f.write_str("not="),
            BinaryOp::In => f.write_str("in"),
            BinaryOp::NotIn => f.write_str("not in"),
            BinaryOp::Imply => f.write_str("=>"),
            BinaryOp::Arrow => f.write_str("->"),
            BinaryOp::Dot => f.write_str("."),
        }
    }
}

/// Unary operators
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum UnaryOp {
    /// Binary/boolean negation operator (`not`)
    Not,
    /// Nat cheat (`#`)
    NatCheat,
    /// Integer identity (`+`)
    Identity,
    /// Integer negation (`-`)
    Negate,
    /// Pointer dereferencing operator (`^`)
    Deref,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Not => f.write_str("not"),
            UnaryOp::NatCheat => f.write_str("#"),
            UnaryOp::Identity => f.write_str("+"),
            UnaryOp::Negate => f.write_str("-"),
            UnaryOp::Deref => f.write_str("^"),
        }
    }
}

/// Literal values
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// String sequence (`"abcd"`)
    StrSequence(String),
    /// Character sequence (`'abcd'`)
    CharSequence(String),
    /// Natural literal (`123456`)
    Nat(u64),
    /// Integer literal (`-123456`)
    Int(i64),
    /// Real literal (`0.1234`)
    Real(f64),
    /// Boolean Literal (`true` or `false`)
    Bool(bool),
    /// Nil literal
    Nil,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::StrSequence(s) => f.write_fmt(format_args!("\"{}\"", s)),
            Literal::CharSequence(s) => f.write_fmt(format_args!("'{}'", s)),
            Literal::Nat(n) => f.write_fmt(format_args!("nat({})", n)),
            Literal::Int(n) => f.write_fmt(format_args!("int({})", n)),
            Literal::Real(n) => f.write_fmt(format_args!("real({})", n)),
            Literal::Bool(b) => f.write_fmt(format_args!("bool({})", b)),
            Literal::Nil => f.write_fmt(format_args!("nil")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub type_spec: TypeRef,
    pub is_const: bool,
    pub is_typedef: bool,
}

/// Expression Node Kind
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Error expression, always evaluates to a type error.
    Error,
    /// Parentheses, only used to preserve operand order in AST dumping
    Parens {
        /// Inner parentheses expression
        inner: Box<Expr>,
    },
    /// Binary expression
    BinaryOp {
        /// Left operand for the binary operation
        left: Box<Expr>,
        /// Operator of the binary expression
        op: (BinaryOp, Location),
        /// Right operand for the binary operation
        right: Box<Expr>,
    },
    /// Unary expression
    UnaryOp {
        /// Operator of the unary expression
        op: (UnaryOp, Location),
        /// Operand for the unary operation
        right: Box<Expr>,
    },
    /// Common literal value
    Literal {
        /// The literal value
        value: Literal,
    },
    // Note: Some functions & procedures may be in the AST as pure references (in the middle of expressions)
    // This is checked in the validator stage
    Reference {
        /// The identifier associated with this referenece
        ident: IdentRef,
    },
    /// Funcion call expression
    Call {
        /// Expression evaluating to a reference
        left: Box<Expr>,
        /// Token location
        paren_at: Location,
        /// The argument list for the call
        arg_list: Vec<Expr>, // Parens may be omitted, indicated by left's eval type
    },
    /// Dot/field expression
    Dot {
        /// Expression evaluating to a reference
        left: Box<Expr>,
        /// Field to be referenced.
        ///
        /// A tuple of the field def, and the location of the field
        field: (FieldDef, Location),
    },
    /// Arrow expression
    Arrow {
        /// Expression evaluating to a reference
        left: Box<Expr>,
        /// Field to be referenced.
        ///
        /// A tuple of the field def, and the location of the field
        field: (FieldDef, Location),
    },
    /// "init" expression
    Init {
        /// Location of "init"
        init: Location,
        /// Expressions part of the "init"
        exprs: Vec<Expr>,
    },
    /// "Indirect" expression
    Indirect {
        // The reference type in the indirect expression.
        // If None, the type is a primitive and is contained in `eval_type`.
        // Must be a type reference.
        reference: Option<Box<Expr>>,
        /// The address section of the indirect expression
        addr: Box<Expr>,
        /// The type to read at the address.
        /// If TypeRef::Unknown, type is derived from the reference expression
        indirect_type: TypeRef,
    },
}

/// Common expression node
///
/// `eval_type` is the type produced after evaluating the expression.
/// For reference and dot expressions, the evaluation type may be different from the identifier type spec
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    /// The expression evaluation type
    pub eval_type: TypeRef,
    /// If the expression is compile-time evaluable
    pub is_compile_eval: bool,
    /// The span of the expression
    pub span: Location,
}

impl Expr {
    /// Gets the evaluation type produced by the expression
    pub fn get_eval_type(&self) -> TypeRef {
        self.eval_type
    }

    /// Gets the span of the location
    pub fn get_span(&self) -> &Location {
        &self.span
    }

    pub fn set_span(&mut self, at: Location) {
        self.span = at;
    }

    /// Gets the compile evaluability status of the expression
    pub fn is_compile_eval(&self) -> bool {
        self.is_compile_eval
    }
}

/// A grouping of statements.
/// Used in multiple statements.
#[derive(Debug, Clone)]
pub struct Block {
    /// Associated scope block
    pub block: ScopeBlock,
    /// Statements as part of the block
    pub stmts: Vec<Stmt>,
}

/// Statement Node Variant
#[derive(Debug, Clone)]
pub enum StmtKind {
    /// No-op statement, does nothing (produced by semi-colons)
    Nop,
    /// Error statement, produced if a valid statement could not be parsed (e.g. in nesting errors or unknown statments)
    Error,
    // Decls
    /// Variable & Constant declaration
    VarDecl {
        /// The identifier(s) declared
        /// If `None`, this statement is a no-op, and only present to allow access to the `type_spec` & `expr`
        idents: Option<Vec<IdentRef>>,
        /// The type spec for all of the identifiers
        type_spec: TypeRef,
        /// The (semi-optional) initialization value
        value: Option<Box<Expr>>,
        /// If the declare is for a const declaration
        is_const: bool,
    },
    /// `type` statement declaration.
    /// The type_spec of `ident` is the declared type
    TypeDecl {
        /// The identifier associated with this type declare.
        /// If `None`, this statement is a no-op, and only allows access to the `resolved_type`
        ident: Option<IdentRef>,
        /// Resolved type for a forward type declare
        resolved_type: Option<TypeRef>,
        /// If the identifier actually declares a new identifier
        is_new_def: bool,
    },
    // Stmts
    /// Simple & Compound assignment expression
    Assign {
        /// The variable reference expression
        var_ref: Box<Expr>,
        /// The (optional) assignment operation
        op: Option<BinaryOp>,
        /// The value to assign
        value: Box<Expr>,
    },
    /// Procedure or function call
    ProcedureCall {
        /// The reference to the procedure or function variable
        proc_ref: Box<Expr>,
    },
    /// Block of statements
    Block { block: Block },
    /// If statement and associated else(if)
    If {
        /// The condition of the if statement
        condition: Box<Expr>,
        /// Link to true branch statement
        true_branch: Box<Stmt>,
        /// Link to false branch statement
        false_branch: Option<Box<Stmt>>,
    },
}

/// Common statement node
#[derive(Debug, Clone)]
pub struct Stmt {
    /// The kind of statement node
    pub kind: StmtKind,
    /// The span of the statement
    pub span: Location,
}

/// Mutable Visitor for a generated AST.
/// `St` is the type returned from visiting statements, and `Ex` is the type
/// returned from visiting expressions.
pub trait VisitorMut<St, Ex> {
    /// Starts a visit to the tree. Allows the visitor to set itself up
    fn start_visit(&mut self) {}

    /// Visit a single statement in the tree
    fn visit_stmt(&mut self, stmt: &mut Stmt) -> St;

    /// Visit an expression in the tree
    fn visit_expr(&mut self, expr: &mut Expr) -> Ex;

    /// Ends a visit to the tree. Allows the visitor to perform any cleanup
    fn end_visit(&mut self) {}
}

/// Immutable Visitor for a generated AST.
/// `St` is the type returned from visiting statements, and `Ex` is the type
/// returned from visiting expressions.
pub trait Visitor<St, Ex> {
    /// Starts a visit to the tree. Allows the visitor to set itself up
    fn start_visit(&mut self) {}

    /// Visit a single statement in the tree
    fn visit_stmt(&mut self, stmt: &Stmt) -> St;

    /// Visit an expression in the tree
    fn visit_expr(&mut self, expr: &Expr) -> Ex;

    /// Ends a visit to the tree. Allows the visitor to perform any cleanup
    fn end_visit(&mut self) {}
}

mod pretty_print {
    use super::{Expr, ExprKind, IdentId, IdentRef, Identifier, Stmt, StmtKind};
    use crate::pretty_print;
    use std::fmt::{self, Write};

    impl fmt::Display for IdentId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_fmt(format_args!("id:{}", self.0))
        }
    }

    impl fmt::Display for IdentRef {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.id.fmt(f)
        }
    }

    impl fmt::Display for Identifier {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_fmt(format_args!(
                "{{ {} ty: {}, used: {}, ",
                self.name, self.type_spec, self.usages
            ))?;

            match (self.is_typedef, self.is_const) {
                (false, false) => f.write_str("var")?,
                (false, true) => f.write_str("const")?,
                (true, _) => f.write_str("tydef")?,
            }

            let props = [
                ("decl", &self.is_declared),
                ("pervasive", &self.is_pervasive),
                ("comp_eval", &self.is_compile_eval),
            ];

            for (name, is_present) in props.iter() {
                if **is_present {
                    f.write_str(" ")?;
                    f.write_str(name)?;
                }
            }

            f.write_str(" }\n")
        }
    }

    impl fmt::Display for ExprKind {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ExprKind::Error => f.write_str("<error>"),
                ExprKind::Init { exprs, .. } => {
                    f.write_str("init(")?;
                    pretty_print::print_list(f, exprs.iter())?;
                    f.write_str(")")
                }
                ExprKind::Indirect {
                    indirect_type,
                    reference,
                    addr,
                    ..
                } => {
                    if let Some(reference) = &reference {
                        f.write_fmt(format_args!("[{}] @ ({})", reference, addr))
                    } else {
                        f.write_fmt(format_args!("[{}] @ ({})", indirect_type, addr))
                    }
                }
                ExprKind::Parens { inner } => f.write_fmt(format_args!("({})", inner)),
                ExprKind::BinaryOp {
                    left, op, right, ..
                } => f.write_fmt(format_args!("{0} {1} {2}", left, &op.0, right)),
                ExprKind::UnaryOp { op, right, .. } => {
                    if op.0 == super::UnaryOp::Not {
                        f.write_fmt(format_args!("{0} {1}", &op.0, right))
                    } else {
                        f.write_fmt(format_args!("{0}{1}", &op.0, right))
                    }
                }
                ExprKind::Literal { value, .. } => f.write_fmt(format_args!("{}", value)),
                ExprKind::Reference { ident, .. } => f.write_fmt(format_args!("ref({})", ident)),
                ExprKind::Call { left, arg_list, .. } => {
                    f.write_fmt(format_args!("{}(", left))?;
                    pretty_print::print_list(f, arg_list.iter())?;
                    f.write_str(")")
                }
                ExprKind::Dot { left, field, .. } => {
                    f.write_fmt(format_args!("{} . {}", left, field.0.name))
                }
                ExprKind::Arrow { left, field, .. } => {
                    f.write_fmt(format_args!("{} -> {}", left, field.0.name))
                }
            }
        }
    }

    impl fmt::Display for Expr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.kind.fmt(f)
        }
    }

    impl fmt::Display for StmtKind {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                StmtKind::Nop => f.write_str("<nop>")?,
                StmtKind::Error => f.write_str("<error>")?,
                StmtKind::VarDecl {
                    idents,
                    type_spec,
                    is_const,
                    value,
                } => {
                    f.write_str(if *is_const { "const [" } else { "var [" })?;
                    if let Some(idents) = idents {
                        pretty_print::print_list(f, idents.iter())?;
                    }
                    f.write_fmt(format_args!("] : {}", type_spec))?;

                    if let Some(value) = value {
                        f.write_fmt(format_args!(" := {}", value))?;
                    }
                }
                StmtKind::TypeDecl {
                    ident,
                    resolved_type,
                    ..
                } => {
                    f.write_str("type [")?;
                    if let Some(ident) = ident {
                        ident.fmt(f)?;
                    }
                    f.write_str("] : ")?;

                    if let Some(resolved_type) = resolved_type {
                        resolved_type.fmt(f)?;
                    } else {
                        f.write_str("forward")?;
                    }
                }
                StmtKind::Assign { var_ref, op, value } => {
                    if let Some(op) = op {
                        f.write_fmt(format_args!("{0} {1}= {2}", var_ref, op, value))?;
                    } else {
                        f.write_fmt(format_args!("{0} := {1}", var_ref, value))?;
                    }
                }
                StmtKind::ProcedureCall { proc_ref } => proc_ref.fmt(f)?,
                StmtKind::Block { block } => {
                    // use '{' and '}' for block delimiter as it's nicer
                    f.write_char('{')?;

                    if !block.stmts.is_empty() {
                        f.write_char('\n')?;
                    }

                    {
                        let mut indenter = pretty_print::IndentWriter::new(f);

                        for stmt in &block.stmts {
                            indenter.write_fmt(format_args!("{}\n", stmt))?;
                        }
                    }

                    f.write_char('}')?;
                }
                StmtKind::If {
                    condition,
                    true_branch,
                    false_branch,
                } => {
                    f.write_fmt(format_args!("if ({}) then {}", condition, true_branch))?;

                    if let Some(false_branch) = false_branch {
                        f.write_fmt(format_args!("\nelse {}", false_branch))?;
                    }
                }
            }

            Ok(())
        }
    }

    impl fmt::Display for Stmt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.kind.fmt(f)
        }
    }
}
