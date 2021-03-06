//! Errors during constant evaluation
use toc_span::Span;

use crate::const_eval::{ConstValue, RestrictType};

#[derive(Debug, Clone)]
pub struct ConstError {
    kind: ErrorKind,
    span: Span,
}

impl ConstError {
    pub(super) fn new(kind: ErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub(super) fn without_span(kind: ErrorKind) -> Self {
        Self {
            kind,
            span: Span::default(),
        }
    }

    pub(super) fn reported(span: Span) -> Self {
        Self {
            kind: ErrorKind::Reported,
            span,
        }
    }

    pub(super) fn change_span(self, new_span: Span) -> Self {
        Self {
            kind: self.kind,
            span: new_span,
        }
    }

    pub(super) fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Reports the detailed version of the `ConstError` to the given reporter
    pub fn report_to(&self, reporter: &mut toc_reporting::MessageSink) {
        // Ignore already reported messages, or for missing expressions
        if matches!(self.kind, ErrorKind::Reported | ErrorKind::MissingExpr) {
            return;
        }

        // Report common message header
        let msg = reporter.error_detailed(&format!("{}", self.kind), self.span);

        // Report extra details
        match &self.kind {
            ErrorKind::NoConstExpr(def_span) => {
                // Report at the reference's definition spot
                msg.with_note("reference declared here", *def_span)
            }
            ErrorKind::WrongResultType(found, expected) => {
                let expected_name = match expected {
                    RestrictType::None => panic!("Wrong result type on no restriction"),
                    RestrictType::Integer => "integer value",
                    RestrictType::Real => "real value",
                    RestrictType::Boolean => "boolean value",
                };

                msg.with_note(
                    &format!("expected {}, found {}", expected_name, found.type_name()),
                    self.span,
                )
            }
            _ => msg,
        }
        .finish();
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub(super) enum ErrorKind {
    // Traversal errors
    /// Encountered an evaluation cycle
    #[error("detected a compile-time evaluation cycle")]
    EvalCycle,
    /// Missing expression operand
    #[error("operand is an invalid expression")]
    MissingExpr,
    /// No const expr is associated with this identifer.
    /// Provided span is the span of the symbol's definition
    #[error("reference cannot be computed at compile-time")]
    NoConstExpr(toc_span::Span),
    /// Error is already reported
    #[error("compile-time evaluation error already reported")]
    Reported,

    // Computation errors
    /// Wrong operand type in eval expression
    #[error("wrong type for compile-time expression")]
    WrongOperandType,
    /// Wrong resultant type in eval expression
    #[error("wrong type for compile-time expression")]
    WrongResultType(ConstValue, RestrictType),
    /// Integer overflow
    #[error("integer overflow in compile-time expression")]
    IntOverflow,
    /// Floating point overflow
    #[error("real overflow in compile-time expression")]
    RealOverflow,
    /// Division by zero
    #[error("division by zero in compile-time expression")]
    DivByZero,
    /// Negative int exponent provided during power raising
    #[error("raising integer to a negative exponent")]
    NegativeIntExp,
    /// Negative int shift provided during bit shifting
    #[error("bit shifting integer by a negative amount")]
    NegativeIntShift,

    // Unsupported messages
    /// Currently unsupported const eval value
    #[error("literal is currently not implemented for compile-time evaluation")]
    UnsupportedValue,
    /// Currently unsupported const eval operation
    #[error("operation is currently not implemented for compile-time evaluation")]
    UnsupportedOp,
}
