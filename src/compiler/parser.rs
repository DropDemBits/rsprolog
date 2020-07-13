//! Main parser for tokens to build the AST
use crate::compiler::ast::{Expr, Identifier, Stmt};
use crate::compiler::block::{BlockKind, CodeBlock, CodeUnit};
use crate::compiler::token::{Token, TokenType};
use crate::compiler::types::{self, ParamDef, PrimitiveType, Type, TypeRef};
use crate::compiler::Location;
use crate::status_reporter::StatusReporter;
use std::cell::RefCell;
use std::fmt::Arguments;
use std::rc::Rc;

/*
expr:
    primary
    funCall | setCons
    unary
    binary

primary:
    literal
    reference | enumValue

param_list:
    expr (',' expr)*

literal:
    NatLiteral
    RealLiteral
    StringLiteral
    CharLiteral

selector:
    '(' arg_list ')'  // arrays (params: Expr::List)
    '.' reference		// fields (Expr::BinaryOp, op: Dot)

reference:
    identifier
    array_reference
    property_reference


array_reference:
    reference '(' arg_list ')' // Call on "op_subscript" (Expr::Call { Expr, List })

property_reference:
    reference '.' identifier // (Expr::Get {Expr, Identifier} )

binary:
    expr binOp expr

unary:
    unOp expr

funCall:
    reference '(' arg_list? ')' // (Expr::Call {Expr, List})

setCons:
    reference '(' arg_list? ')' // Call on "set_cons" (Expr::Call { Expr, List })

enumValue:
    reference '.' identifier // Documented encoding
    reference 				 // Real encoding

*/

/// Operator precedence
/// All operators have left associativity, with the exceptions of ^ and #
#[derive(Debug, PartialOrd, PartialEq, Copy, Clone)]
enum Precedence {
    /// No precedence for this level
    NoPrec,
    /// Imply Operator \ =>
    Imply,
    /// Or Operator \ or/'|'
    BitOr,
    /// And Operator \ and/'&'
    BitAnd,
    /// Not Operator \ not/'~'
    BitNot,
    /// Comparator Operators \ < > = <= >= not=/'~=' in not_in/'~in'
    Comparison,
    /// Additive Operators \ + - xor
    Sum,
    /// Product Operators \ * / div mod rem shl shr
    Product,
    /// Unary Operators \ + -
    Unary,
    /// Exponent Operator \ **
    Exponent,
    /// Conversion Operator \ #
    Conversion,
    /// Deref Operator \ ->
    Deref,
    /// Calling & Get Operators \ ( .
    Call,
    /// Pointer Follow Operator \ ^
    Follow,
    /// Primaries
    Primary,
}

impl Precedence {
    /// Gets the next precedence up
    pub fn up(&self) -> Self {
        use Precedence::*;

        match self {
            // Lowest & highest saturate
            NoPrec => NoPrec,
            Primary => Primary,

            Imply => BitOr,
            BitOr => BitAnd,
            BitAnd => BitNot,
            BitNot => Comparison,
            Comparison => Sum,
            Sum => Product,
            Product => Unary,
            Unary => Exponent,
            Exponent => Conversion,
            Conversion => Deref,
            Deref => Call,
            Call => Follow,
            Follow => Primary,
        }
    }
}

// The contents of the Parser are valid for the entire lifetime of the parser,
// but the mutable reference is only bound to the lifetime of the
// PrecedenceRule reference
struct PrecedenceRule<'s, 'local> {
    precedence: Precedence,
    prefix_rule: Option<fn(&'local mut Parser<'s>) -> Result<Expr, ParsingStatus>>,
    infix_rule: Option<fn(&'local mut Parser<'s>, Expr) -> Result<Expr, ParsingStatus>>,
}

/// Main parser
#[derive(Debug)]
pub struct Parser<'s> {
    /// Status reporter
    reporter: StatusReporter,
    /// File source used for getting lexemes for reporting
    source: &'s str,
    /// Source for tokens
    tokens: Vec<Token>,
    /// Current token being parsed
    current: usize,
    /// Parsed Code Unit
    unit: Option<CodeUnit>,
    /// Actively parsed blocks
    blocks: Vec<Rc<RefCell<CodeBlock>>>,
}

#[derive(Debug)]
enum ParsingStatus {
    Error,
}

impl<'s> Parser<'s> {
    pub fn new(tokens: Vec<Token>, source: &'s str, unit: CodeUnit) -> Self {
        Self {
            reporter: StatusReporter::new(),
            source,
            tokens,
            current: 0,
            // Clone a ref to the root block
            blocks: vec![unit.root_block().clone()],
            unit: Some(unit),
        }
    }

    /// Parses the token stream
    /// Returns if the parse has no errors
    pub fn parse(&mut self) -> bool {
        // TODO: Check if the root block is a unit block
        // Parse the statements
        let mut stmts = vec![];

        while !self.is_at_end() {
            match self.decl() {
                Ok(expr) => stmts.push(expr),
                Err(_) => {}
            }

            if self.current().token_type == TokenType::Semicolon {
                // Nom the semicolon
                self.next_token();
            }
        }

        // Transfer statements over to the CodeUnit
        self.unit.as_mut().unwrap().stmts_mut().append(&mut stmts);

        !self.reporter.has_error()
    }

    /// Takes the unit from the parser
    pub fn take_unit(&mut self) -> CodeUnit {
        let code_unit = self.unit.take().unwrap();
        code_unit
    }

    /// Gets the previous token in the stream
    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }

    /// Gets the current token in the stream
    fn current(&self) -> &Token {
        &self.tokens[self.current]
    }

    /// Peeks at the next token in the stream
    fn peek(&self) -> &Token {
        if self.is_at_end() {
            // At the end of file, return the Eof token
            self.tokens.last().as_ref().unwrap()
        } else {
            &self.tokens[self.current.saturating_add(1)]
        }
    }

    /// Advances to the next token, returning the previous token
    fn next_token(&mut self) -> Token {
        if !self.is_at_end() {
            // Advance cursor
            self.current = self.current.saturating_add(1);
        }

        self.previous().clone()
    }

    /// Checks if all of the tokens have been consumed yet
    fn is_at_end(&self) -> bool {
        self.current().token_type == TokenType::Eof
    }

    /// Expects a certain token to be next in the stream. \
    /// If the current token matches the expected token, the current token is consumed.
    /// Otherwise an error message is reported.
    fn expects(
        &mut self,
        expected_type: TokenType,
        message: Arguments,
    ) -> Result<Token, ParsingStatus> {
        if self.current().token_type == expected_type {
            Ok(self.next_token().clone())
        } else {
            self.reporter
                .report_error(&self.current().location, message);
            Err(ParsingStatus::Error)
        }
    }

    /// Optionally expects a certain token to be next in the stream. \
    /// If the current token matches the expected token, the current token is
    /// consumed and true is returned.
    /// Otherwise, false is returned.
    fn optional(&mut self, optional_type: TokenType) -> bool {
        if self.current().token_type == optional_type {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn warn_equ_as_assign(&self, at: Location) {
        self.reporter
            .report_warning(&at, format_args!("'=' found, assumed it to be ':='"));
    }

    fn get_token_lexeme(&self, token: &Token) -> &str {
        let token_content = token.location.get_lexeme(self.source);

        if !token_content.is_empty() {
            token_content
        } else {
            "<end of file>"
        }
    }

    // --- Decl Parsing --- //

    fn decl(&mut self) -> Result<Stmt, ParsingStatus> {
        let nom = self.current();
        match nom.token_type {
            TokenType::Var => self.decl_var(false),
            TokenType::Const => self.decl_var(true),
            TokenType::Type => self.decl_type(),
            _ => self.stmt(),
        }
    }

    fn decl_var(&mut self, is_const: bool) -> Result<Stmt, ParsingStatus> {
        let decl_name = if is_const { "const" } else { "var" };

        // Consume decl_tok
        let decl_tok = self.next_token();

        // Grab identifier token
        let ident_tokens = {
            let mut idents = vec![self.expects(
                TokenType::Identifier,
                format_args!("Expected an identifier for the declared {}", decl_name),
            )?];

            while self.current().token_type == TokenType::Comma {
                // Consume comma
                self.next_token();

                // Identifier expected, but can break out of loop
                let token = self.expects(
                    TokenType::Identifier,
                    format_args!("Expected an identifier after the comma"),
                );

                if token.is_err() {
                    break;
                }

                // Add to identifier list
                idents.push(token.unwrap());
            }

            idents
        };

        // Grab typespec
        let mut type_spec = if self.current().token_type == TokenType::Colon {
            // Consume colon
            self.next_token();

            // Parse the type spec
            // If type parsing fails (i.e. TypeRef::TypeError is produced), the
            // assignment value is automagically skipped
            self.parse_type(&decl_tok.token_type)
        } else {
            // Will be resolved in the validation stage
            TypeRef::Unknown
        };

        // Grab assign value
        let assign_expr = if self.is_simple_assignment() {
            if &self.current().token_type == &TokenType::Equ {
                // Warn of mistake
                self.warn_equ_as_assign(self.current().location);
            }

            // Consume assign
            self.next_token();

            // Get the assign expression
            let asn_expr = self.expr();

            asn_expr
                .map(|expr| Some(Box::new(expr)))
                .unwrap_or_else(|_| None)
        } else {
            None
        };

        // Validate if the declaration requirements have been met
        // Otherwise, produce an error and a TypeError
        if is_const && assign_expr.is_none() {
            // const declares require the assignment expression
            // Recoverable error, just use TypeError as the type_spec
            self.reporter.report_error(
                &decl_tok.location,
                format_args!("const declaration requires an initial value"),
            );

            type_spec = TypeRef::TypeError;
        } else if type_spec == TypeRef::Unknown && assign_expr.is_none() {
            // No type inferrable
            // Recoverable error, just use TypeError as the type_spec
            self.reporter.report_error(
                &decl_tok.location,
                format_args!("Cannot infer type for given {} declaration (no type specification or initial value given)", decl_name)
            );

            type_spec = TypeRef::TypeError;
        }

        // Declare the identifiers
        let idents: Vec<Identifier> = ident_tokens
            .into_iter()
            .map(|token| self.declare_ident(token, type_spec, is_const, false))
            .collect();

        Ok(Stmt::VarDecl {
            idents,
            type_spec,
            value: assign_expr,
            is_const,
        })
    }

    fn decl_type(&mut self) -> Result<Stmt, ParsingStatus> {
        // Nom "type"
        let type_tok = self.next_token();

        // Expect identifier (continue parsing after the identifier)
        let ident = self
            .expects(
                TokenType::Identifier,
                format_args!("Expected identifier after 'type'"),
            )
            .ok();

        if ident.is_some() {
            // Only require a colon if there was an identifier
            let _ = self.expects(
                TokenType::Colon,
                format_args!("Expected ':' after identifier"),
            );
        } else {
            // If there is a ':', consume it
            // Otherwise, continue onwards
            let _ = self.optional(TokenType::Colon);
        }

        // Get the type spec (in the type declaration context)
        let type_spec = self.parse_type(&type_tok.token_type);

        if ident.is_none() {
            // Cannot declare a named type without an identifier
            return Err(ParsingStatus::Error);
        }

        let ident_tok = ident.unwrap();

        Ok(Stmt::TypeDecl {
            ident: self.declare_ident(ident_tok, type_spec, true, true),
        })
    }

    // --- Stmt Parsing --- //

    fn stmt(&mut self) -> Result<Stmt, ParsingStatus> {
        match self.current().token_type {
            TokenType::Identifier | TokenType::Caret => self.stmt_reference(),
            TokenType::Begin => self.stmt_block(),
            _ => {
                // Nom as token isn't consumed by anything else
                self.next_token();

                self.reporter.report_error(
                    &self.previous().location,
                    format_args!(
                        "'{}' does not begin a statement or declaration",
                        self.get_token_lexeme(self.previous())
                    ),
                );

                // Cause an error
                Err(ParsingStatus::Error)
            }
        }
    }

    fn stmt_reference(&mut self) -> Result<Stmt, ParsingStatus> {
        // Identifiers & References can begin either an assignment or a procedure call
        // Both take references as the primary expression

        // If the reference expr can't be parsed, bail out
        // Referring to an identifier (aside from checking if it's been declared)
        // does not have any side effects in an invalid parse and can be
        // safely ignored
        let reference = self.expr_precedence(Precedence::Deref)?;
        let is_compound_assign = self.is_compound_assignment();

        if is_compound_assign || self.is_simple_assignment() {
            // Is a (compound) assignment or '='
            // '=' is checked for as it's a common mistake to have '=' instead of ':='
            let mut assign_op = self.next_token();

            if is_compound_assign {
                // Nom the other equ in the compound assignment
                self.next_token();
            } else if assign_op.token_type != TokenType::Assign {
                // Current assignment op is '=', not ':='
                // Warn of mistake, convert into ':='
                let locate = self.previous().location;
                self.warn_equ_as_assign(locate);

                assign_op.token_type = TokenType::Assign;
            };

            // If the assign value expr can't be parsed, bail out
            // Assignment after variable declaration isn't really important
            // in an invalid parse state
            let value = self.expr()?;

            Ok(Stmt::Assign {
                var_ref: Box::new(reference),
                op: assign_op,
                value: Box::new(value),
            })
        } else {
            // Is a procedure call
            Ok(Stmt::ProcedureCall {
                proc_ref: Box::new(reference),
            })
        }
    }

    fn stmt_block(&mut self) -> Result<Stmt, ParsingStatus> {
        // Nom begin
        let begin_loc = self.next_token().location;

        self.push_block(BlockKind::InnerBlock);

        let mut stmts = vec![];
        while !matches!(self.current().token_type, TokenType::End | TokenType::Eof) {
            let stmt = self.decl();

            // Only add the Stmt if it was parsed successfully
            if stmt.is_ok() {
                stmts.push(stmt.unwrap());
            }
        }

        if matches!(self.current().token_type, TokenType::Eof) {
            // If at the end of file, do nothing
            // All of the statements have been absolved into this block

            self.reporter.report_error(
                &begin_loc,
                format_args!("'begin' block does not have a matching 'end'"),
            );
        } else {
            let _ = self.expects(
                TokenType::End,
                format_args!("Expected 'end' to close off 'begin' block"),
            );
        }

        // Close the block
        let block = self.pop_block(&mut stmts);

        Ok(Stmt::Block { block })
    }

    // --- Expr Parsing --- //

    fn expr(&mut self) -> Result<Expr, ParsingStatus> {
        self.expr_precedence(Precedence::Imply)
    }

    fn expr_precedence(&mut self, min_precedence: Precedence) -> Result<Expr, ParsingStatus> {
        // Keep track of last token
        let before_op = self.previous();

        // Get prefix side
        let op = self.current();

        let prefix = Parser::get_rule(&op.token_type);
        let prefix_rule = prefix.prefix_rule.ok_or_else(|| {
            // Try to figure out if the typo was a reasonable one
            // ???: Rework this system to use a hashmap with key (token_type, token_type)?

            let hint = if before_op.location != op.location {
                if before_op.token_type == TokenType::Equ && op.token_type == TokenType::Equ {
                    "(Did you mean '=' instead of '=='?)"
                } else {
                    ""
                }
            } else {
                // No hints for looking back at the start or end of the file
                ""
            };

            // The token reference are relative to the next token, as the next
            // token is consumed unconditionally
            self.reporter.report_error(
                &self.current().location,
                format_args!(
                    "Expected expression before '{}' {}",
                    self.current().location.get_lexeme(self.source),
                    hint
                ),
            );

            ParsingStatus::Error
        });

        // Consume the token
        self.next_token();

        let mut expr = prefix_rule?(self)?;

        // Go over infix operators
        while !self.is_at_end()
            && min_precedence <= Parser::get_rule(&self.current().token_type).precedence
        {
            let op = self.current();
            let infix = Parser::get_rule(&op.token_type);

            if infix.precedence >= Precedence::Follow {
                // Is a deref, identifier, or literal
                // Most likely end of expression, so return
                return Ok(expr);
            }

            let infix_rule = infix
                .infix_rule
                .expect(&format!("No infix function for given rule '{:?}'", op).to_string());

            // Consume token for infix rule
            self.next_token();

            // Produce the next expression
            expr = infix_rule(self, expr)?;
        }

        // Give back parsed expression
        Ok(expr)
    }

    fn expr_grouping(&mut self) -> Result<Expr, ParsingStatus> {
        let expr = self.expr()?;
        self.expects(
            TokenType::RightParen,
            format_args!("Expected ')' to close off parenthetical grouping"),
        )?;

        Ok(Expr::Grouping {
            expr: Box::new(expr),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_binary(&mut self, lhs: Expr) -> Result<Expr, ParsingStatus> {
        let op = self.previous().clone();
        let precedence = Parser::get_rule(&op.token_type).precedence.up();
        // Get rhs
        let rhs = self.expr_precedence(precedence)?;

        Ok(Expr::BinaryOp {
            left: Box::new(lhs),
            op,
            right: Box::new(rhs),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_unary(&mut self) -> Result<Expr, ParsingStatus> {
        let op = self.previous().clone();
        let right = self.expr_precedence(Precedence::Unary)?;

        Ok(Expr::UnaryOp {
            op,
            right: Box::new(right),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_unary_rule(&mut self) -> Result<Expr, ParsingStatus> {
        let mut op = self.previous().clone();
        let precedence = Parser::get_rule(&op.token_type).precedence;
        let right = self.expr_precedence(precedence)?;

        if op.token_type == TokenType::Tilde {
            // Convert '~'s into 'not's
            op.token_type = TokenType::Not;
        }

        Ok(Expr::UnaryOp {
            op,
            right: Box::new(right),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_call(&mut self, func_ref: Expr) -> Result<Expr, ParsingStatus> {
        let op = self.previous().clone();
        let arg_list = self.make_arg_list()?.unwrap();

        Ok(Expr::Call {
            left: Box::new(func_ref),
            op,
            arg_list,
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_dot(&mut self, var_ref: Expr) -> Result<Expr, ParsingStatus> {
        // Get the ident
        let ident = self.expects(
            TokenType::Identifier,
            format_args!("Missing identifier after '.'"),
        )?;

        // The actual identifier information will be resolved at validator time,
        // so we can just store the field name and location info

        let name = ident.location.get_lexeme(self.source).to_string();

        Ok(Expr::Dot {
            left: Box::new(var_ref),
            field: (ident, name),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_deref(&mut self, var_ref: Expr) -> Result<Expr, ParsingStatus> {
        let op = self.previous().clone();

        // Wrap the var_ref in a deref
        self.expr_dot(Expr::UnaryOp {
            op: Token {
                token_type: TokenType::Caret,
                location: op.location,
            },
            right: Box::new(var_ref),
            eval_type: TypeRef::Unknown,
            is_compile_eval: false,
        })
    }

    fn expr_primary(&mut self) -> Result<Expr, ParsingStatus> {
        let token = self.previous().clone();
        let token_type = token.token_type.clone();

        match token_type {
            TokenType::StringLiteral(s) => Ok(Expr::Literal {
                value: token,
                eval_type: TypeRef::Primitive(types::get_string_kind(&s)),
            }),
            TokenType::CharLiteral(s) => Ok(Expr::Literal {
                value: token,
                eval_type: TypeRef::Primitive(types::get_char_kind(&s)),
            }),
            TokenType::NatLiteral(_) => Ok(Expr::Literal {
                value: token,
                eval_type: TypeRef::Primitive(PrimitiveType::Int),
            }),
            TokenType::RealLiteral(_) => Ok(Expr::Literal {
                value: token,
                eval_type: TypeRef::Primitive(PrimitiveType::Real),
            }),
            TokenType::True => Ok(Expr::Literal {
                value: Token {
                    token_type: TokenType::BoolLiteral(true),
                    location: token.location,
                },
                eval_type: TypeRef::Primitive(PrimitiveType::Boolean),
            }),
            TokenType::False => Ok(Expr::Literal {
                value: Token {
                    token_type: TokenType::BoolLiteral(false),
                    location: token.location,
                },
                eval_type: TypeRef::Primitive(PrimitiveType::Boolean),
            }),
            TokenType::Nil => {
                // Consume optional collection / class id

                // TODO: validate that theses are the same as the pointer type (ie produce nil for a given type id)
                // For classes, both must have a common ancestor
                if &self.current().token_type == &TokenType::LeftParen {
                    // Consume optional identifier & parens
                    self.next_token(); // (

                    if &self.current().token_type == &TokenType::Identifier {
                        self.next_token(); // identifier
                    }

                    self.expects(
                        TokenType::RightParen,
                        format_args!("Expected ')' to close off parentheses for 'nil'"),
                    )?;
                }

                Ok(Expr::Literal {
                    value: token,
                    eval_type: TypeRef::Primitive(PrimitiveType::Nil),
                })
            }
            _ => {
                self.reporter.report_error(
                    &token.location,
                    format_args!("Unexpected token '{}'", self.get_token_lexeme(&token)),
                );
                Err(ParsingStatus::Error)
            }
        }
    }

    fn expr_ident(&mut self) -> Result<Expr, ParsingStatus> {
        let ident = self.previous().clone();

        if let TokenType::Identifier = &ident.token_type {
            Ok(Expr::Reference {
                ident: self.use_ident(ident),
            })
        } else {
            panic!(
                "Identifier found but also not found (at {:?})",
                ident.location
            )
        }
    }

    // --- Type Parsing --- //
    /// Tries to parse the given type, returning TypeRef::TypeError if the parsing couldn't be salvaged.
    /// If a TypeRef::TypeError is produced, the token that caused the error is not consumed
    ///
    /// `parse_context`         The token type describing where the type is being parsed
    fn parse_type(&mut self, parse_context: &TokenType) -> TypeRef {
        match &self.current().token_type {
            // Basic primitive types
            TokenType::Addressint => self.type_primitive(PrimitiveType::AddressInt),
            TokenType::Boolean => self.type_primitive(PrimitiveType::Boolean),
            TokenType::Int => self.type_primitive(PrimitiveType::Int),
            TokenType::Int1 => self.type_primitive(PrimitiveType::Int1),
            TokenType::Int2 => self.type_primitive(PrimitiveType::Int2),
            TokenType::Int4 => self.type_primitive(PrimitiveType::Int4),
            TokenType::Nat => self.type_primitive(PrimitiveType::Nat),
            TokenType::Nat1 => self.type_primitive(PrimitiveType::Nat1),
            TokenType::Nat2 => self.type_primitive(PrimitiveType::Nat2),
            TokenType::Nat4 => self.type_primitive(PrimitiveType::Nat4),
            TokenType::Real => self.type_primitive(PrimitiveType::Real),
            TokenType::Real4 => self.type_primitive(PrimitiveType::Real4),
            TokenType::Real8 => self.type_primitive(PrimitiveType::Real8),
            TokenType::String_ | TokenType::Char => self.type_char_seq(parse_context),

            // Compound primitives
            TokenType::Flexible if self.peek().token_type == TokenType::Array => {
                self.type_array(parse_context)
            }
            TokenType::Array => self.type_array(parse_context),
            TokenType::Pointer | TokenType::Caret => self.type_pointer(parse_context),
            TokenType::Set => self.type_set(parse_context),
            TokenType::Function | TokenType::Procedure => {
                // subprogram_header

                // Consume function & the identifier (don't care about those)
                // "function" / "procedure"
                let is_function = matches!(self.next_token().token_type, TokenType::Function);
                // Identifier
                let _ = self.expects(
                    TokenType::Identifier,
                    format_args!("Expected identifier in function type declaration"),
                );

                self.type_function(parse_context, is_function)
            }
            TokenType::Enum => unimplemented!(),
            TokenType::Record => unimplemented!(),
            TokenType::Union => unimplemented!(),
            TokenType::Identifier if self.peek().token_type != TokenType::Range => {
                // TODO: Handle field references in conjunction with range parsing
                self.type_ident()
            }
            _ => {
                // Try to parse a range type
                let range = self.type_range(parse_context);

                if range.is_err() {
                    self.reporter.report_error(
                        &self.current().location,
                        format_args!(
                            "Unexpected '{}', expected a type specifier",
                            self.get_token_lexeme(self.current())
                        ),
                    );

                    // Return a type error
                    TypeRef::TypeError
                } else {
                    range.ok().unwrap()
                }
            }
        }
    }

    /// Parse a basic primitive type
    fn type_primitive(&mut self, primitive: PrimitiveType) -> TypeRef {
        // Consume name
        self.next_token();

        TypeRef::Primitive(primitive)
    }

    /// Gets the size specifier for a char(n) or string(n)
    fn get_size_specifier(&mut self, parse_context: &TokenType) -> Result<usize, ()> {
        match self.current().token_type {
            TokenType::NatLiteral(size) if size > 0 => {
                if size as usize >= types::MAX_STRING_SIZE {
                    self.reporter.report_error(
                        &self.current().location,
                        format_args!(
                            "'{}' is larger than or equal to the maximum string length of '{}' (after including the end byte)",
                            size,
                            types::MAX_STRING_SIZE
                        ),
                    );
                    Err(())
                } else {
                    // Consume parsed type
                    self.next_token();
                    Ok(size as usize)
                }
            }
            TokenType::Star => {
                // Consume parsed type
                self.next_token();

                if matches!(parse_context, TokenType::Function | TokenType::Procedure) {
                    Ok(0 as usize)
                } else {
                    self.reporter.report_error(
                        &self.previous().location,
                        format_args!(
                            "Length specifier of '*' is only valid in subprogram parameter types"
                        ),
                    );

                    Err(())
                }
            }
            _ => {
                self.reporter.report_error(
                    &self.current().location,
                    format_args!(
                        "Length specifier is not a '*' or a non-zero compile time expression"
                    ),
                );
                Err(())
            }
        }
    }

    /// Parse character sequence (string, char, char(n))
    fn type_char_seq(&mut self, parse_context: &TokenType) -> TypeRef {
        // Nom "string" / "char"
        let is_char_type = matches!(self.next_token().token_type, TokenType::Char);

        // If left paren, construct sized type
        if self.current().token_type == TokenType::LeftParen {
            self.next_token();

            // Try to get the size
            let parsed_size = self.get_size_specifier(parse_context);

            // Missing ) is recoverable
            let _ = self.expects(
                TokenType::RightParen,
                format_args!("Expected ')' after length specifier"),
            );

            if is_char_type {
                match parsed_size {
                    Ok(size) => TypeRef::Primitive(PrimitiveType::CharN(size)),
                    // Try to return as a single char, for preserving validator semantics and preserving compatibility with Turing proper
                    Err(_) => TypeRef::Primitive(PrimitiveType::Char),
                }
            } else {
                match parsed_size {
                    Ok(size) => TypeRef::Primitive(PrimitiveType::StringN(size)),
                    // Try to return as a normal string, for preserving validator semantics
                    Err(_) => TypeRef::Primitive(PrimitiveType::String_),
                }
            }
        } else {
            // Produce bracketless versions
            if is_char_type {
                // Make single char type
                TypeRef::Primitive(PrimitiveType::Char)
            } else {
                // Make varsized type
                TypeRef::Primitive(PrimitiveType::String_)
            }
        }
    }

    /// Parse pointer to another type
    fn type_pointer(&mut self, parse_context: &TokenType) -> TypeRef {
        // Consume "pointer" or '^'
        if let TokenType::Pointer = &self.next_token().token_type {
            // Consume the "to"
            let _ = self.expects(TokenType::To, format_args!("Expected 'to' after 'pointer'"));
        }

        // Get the pointer to type
        let pointer_to = self.parse_type(parse_context);
        let typedef = Type::Pointer { to: pointer_to };

        self.declare_type(typedef)
    }

    /// Parse procedure & function parameter specification & result type
    fn type_function(&mut self, parse_context: &TokenType, has_result: bool) -> TypeRef {
        let param_decl = if let TokenType::LeftParen = self.current().token_type {
            // Parameter Declaration

            // Consume left paren
            self.next_token();

            let mut params = vec![];

            if self.current().token_type != TokenType::RightParen {
                let param_context = if has_result {
                    &TokenType::Function
                } else {
                    &TokenType::Procedure
                };

                loop {
                    // Parse a parameter type
                    match self.current().token_type {
                        TokenType::Function | TokenType::Procedure => {
                            // Parse in the context of a function/procedure, as those allow omission of '()'
                            // for function subprograms
                            params.push(self.type_subprogram_param(param_context))
                        }
                        _ => params.append(&mut self.type_var_param(param_context)),
                    }

                    if self.current().token_type != TokenType::Comma {
                        // No more things to parse, or invalid token
                        break;
                    }

                    // Consume ','
                    self.next_token();
                }
            }

            let _ = self.expects(
                TokenType::RightParen,
                format_args!("Expected ')' after parameter declaration"),
            );

            Some(params)
        } else {
            // Parameterless declaration
            None
        };

        if has_result
            && param_decl.is_none()
            && matches!(
                parse_context,
                TokenType::Const | TokenType::Var | TokenType::Type
            )
        {
            // In the context of a function type declaration, which requires the '()'
            if matches!(self.previous().token_type, TokenType::Identifier) {
                self.reporter.report_error(
                    &self.previous().location,
                    format_args!(
                        "Function type declarations must specifiy '()' after the identifier"
                    ),
                );
            } else {
                // Identifier is not really needed in these situations, though
                // we still do so for compatibility
                self.reporter.report_error(
                    &self.previous().location,
                    format_args!(
                        "Function type declarations must specifiy '()' after '{}'",
                        &self.previous().token_type
                    ),
                );
            }
        }

        let result_type = if has_result {
            let _ = self.expects(
                TokenType::Colon,
                format_args!("Expected ':' before the result type"),
            );

            Some(self.parse_type(parse_context))
        } else {
            // No result type
            None
        };

        self.declare_type(Type::Function {
            params: param_decl,
            result: result_type,
        })
    }

    /// Parses a sequence of function variable-type parameters, producing one or more parameter definitions
    fn type_var_param(&mut self, parse_context: &TokenType) -> Vec<ParamDef> {
        // "var"? "register"? identifier ( ',' identifier )* ':' "cheat"? type_spec

        // Attributes apply to all idents
        let pass_by_ref = self.optional(TokenType::Var);
        let bind_to_register = self.optional(TokenType::Register);
        let mut idents = vec![];

        // Gather all identifiers
        loop {
            let ident = self
                .expects(
                    TokenType::Identifier,
                    format_args!("Expected identifier for parameter name"),
                )
                .map(|tok| tok.location.get_lexeme(self.source).to_string())
                .unwrap_or(String::from("<invalid>"));

            idents.push(ident);

            if !self.optional(TokenType::Comma) {
                break;
            }
        }

        let _ = self.expects(
            TokenType::Colon,
            format_args!("Expected ':' after parameter name"),
        );

        let force_type = self.optional(TokenType::Cheat);
        let type_spec = self.parse_type(parse_context);

        // Unfold the ident list into the individual parameter types
        idents
            .into_iter()
            .map(|name| ParamDef {
                name,
                type_spec,
                pass_by_ref,
                bind_to_register,
                force_type,
            })
            .collect()
    }

    /// Parses a single function subprogram-type parameter, producing one parameter definition
    fn type_subprogram_param(&mut self, parse_context: &TokenType) -> ParamDef {
        // "function" | "procedure" identifier param_list

        // Consume "function" or "procedure"
        let has_result = matches!(self.next_token().token_type, TokenType::Function);

        let name = self
            .expects(
                TokenType::Identifier,
                format_args!("Expected identifier for parameter name"),
            )
            .map(|tok| tok.location.get_lexeme(self.source).to_string())
            .unwrap_or(String::from("<invalid>"));

        let type_spec = self.type_function(parse_context, has_result);

        ParamDef {
            name,
            type_spec,
            pass_by_ref: false,
            bind_to_register: false,
            force_type: false,
        }
    }

    /// Parse an identifier
    fn type_ident(&mut self) -> TypeRef {
        // Get ident token
        let ident_tok = self.next_token();
        let (ident, _) = self.use_ident_msg(ident_tok);

        // Postpone type resolution until the validation stage
        // The identifier may refer to an imported unqualified identifier,
        // which are not resolved until after AST building. The error message
        // is therefore ignored, as the identifier may be resolved later.
        self.declare_type(Type::Named { ident })
    }

    /// Parse a range type
    fn type_range(&mut self, parse_context: &TokenType) -> Result<TypeRef, ()> {
        // Try to parse the start range
        let start_range = self.expr().map_err(|_| ())?;

        if self.current().token_type != TokenType::Range {
            // Not a range expression
            // Propogate the error up
            return Err(());
        }

        Ok(self.type_range_rest(start_range, parse_context))
    }

    // Parse the rest of a range type
    // Will always produce a range
    fn type_range_rest(&mut self, start_range: Expr, parse_context: &TokenType) -> TypeRef {
        // A None for the range is only acceptable in array decls
        // Everywhere else is invalid
        let is_inferred_valid = *parse_context == TokenType::Array;

        // Consume range dots
        let range_tok = self.next_token();

        // Parse the end range
        let end_range = match self.current().token_type {
            // Inferred range
            TokenType::Star => {
                let star_tok = self.next_token();

                if is_inferred_valid {
                    Ok(None)
                } else {
                    self.reporter.report_error(
                        &star_tok.location,
                        format_args!("'*' as a range end is only valid in an array range"),
                    );

                    // Bail out completely
                    return TypeRef::TypeError;
                }
            }
            // Explicit range
            _ => self.expr().map(|expr| Some(expr)),
        };

        match end_range {
            Err(_) => {
                self.reporter.report_error(
                    &range_tok.location,
                    if is_inferred_valid {
                        format_args!("Expected expression or '*' after '..'")
                    } else {
                        format_args!("Expected expression after '..'")
                    },
                );

                TypeRef::TypeError
            }
            Ok(end_range) => self.declare_type(Type::Range {
                start: start_range,
                end: end_range,
                base_type: TypeRef::Unknown,
            }),
        }
    }

    /// Parse a single index specificier
    fn type_index(&mut self, parse_context: &TokenType) -> Result<TypeRef, ()> {
        // "boolean" | "char" | type_ident | type_range
        // TODO: Identifiers aren't properly parsed yet as they may be part of a range expression
        match self.current().token_type {
            TokenType::Boolean | TokenType::Char => Ok(self.parse_type(parse_context)),
            _ => self.type_range(parse_context),
        }
    }

    /// Parse an array type
    fn type_array(&mut self, parse_context: &TokenType) -> TypeRef {
        // "flexible"? "array" range_spec (',' range_spec)* "of" type_spec

        let is_flexible = self.optional(TokenType::Flexible);
        let flexible_tok = self.previous().clone();

        // Guarranteed to always be called with array as the current or next token
        let array_tok = self
            .expects(TokenType::Array, format_args!("!!! oopsies !!!"))
            .expect("'type_array' called without checking if next or current token is 'array'");

        let mut is_init_sized = false;
        let mut ranges = vec![];

        loop {
            let range = self.type_index(&TokenType::Array);

            if range.is_err() {
                let _ = self.reporter.report_error(
                    &self.current().location,
                    format_args!("Expected a range specifier after ','"),
                );
                break;
            }

            let range = range.ok().unwrap();

            if ranges.is_empty() {
                // Pushing the first range, check for implicit range
                if let Some(Type::Range { ref end, .. }) =
                    self.unit.as_ref().unwrap().types().type_from_ref(&range)
                {
                    if is_flexible && end.is_none() {
                        // Flexible array cannot have an implicit range specifier
                        self.reporter.report_error(
                            &self.previous().location,
                            format_args!("Flexible array cannot have an implicit range specifier"),
                        );
                    }

                    // An array is init-sized if the first range is an implicit range
                    is_init_sized = end.is_none();
                }
                // Add the range
                ranges.push(range);
            } else if is_init_sized {
                // Part of an init sized array, drop the extra ranges
                self.reporter.report_error(
                    &self.previous().location,
                    format_args!("Extra range specifier found in implicit size array"),
                );
            } else {
                // Pushing in an additional range, where an additional range is accepted
                if let Some(Type::Range { ref end, .. }) =
                    self.unit.as_ref().unwrap().types().type_from_ref(&range)
                {
                    // Report if the range is an implicit range
                    if end.is_none() {
                        self.reporter.report_error(
                            &self.previous().location,
                            format_args!("'*' is only valid for the first range specifier"),
                        );
                    }
                }

                // Add the range
                ranges.push(range);
            }

            if !self.optional(TokenType::Comma) {
                break;
            }
        }

        if ranges.is_empty() {
            self.reporter.report_error(
                &self.current().location,
                format_args!("Expected a range specifier after 'array'"),
            );
        }

        let _ = self.expects(
            TokenType::Of,
            format_args!("Expected 'of' after the last range specifier"),
        );

        let element_type = self.parse_type(&TokenType::Array);

        // Check if the current array is allowed in the current parsing context
        if matches!(
            parse_context,
            TokenType::Union
                | TokenType::Record
                | TokenType::Array
                | TokenType::Function
                | TokenType::Procedure
        ) {
            let context_name = match parse_context {
                TokenType::Union => "unions",
                TokenType::Record => "records",
                TokenType::Array => "array element type specifiers",
                TokenType::Function => "function parameters",
                TokenType::Procedure => "procedure parameters",
                _ => unreachable!(),
            };

            if is_flexible {
                self.reporter.report_error(
                    &flexible_tok.location,
                    format_args!("Flexible arrays are not allowed inside of {}", context_name),
                );
            } else if is_init_sized {
                self.reporter.report_error(
                    &array_tok.location,
                    format_args!(
                        "Implicit size arrays are not allowed inside of {}",
                        context_name
                    ),
                );
            }
        }

        // Build the type
        self.declare_type(Type::Array {
            ranges,
            element_type,
            is_flexible,
            is_init_sized,
        })
    }

    /// Parse a set type
    fn type_set(&mut self, parse_context: &TokenType) -> TypeRef {
        // Parse the entire set declaration
        let set_tok = self.next_token();
        // nab 'of'
        let _ = self.expects(TokenType::Of, format_args!("Expected 'of' after 'set'"));

        // Parse the range!
        let range = match self.type_range(&TokenType::Set) {
            Err(_) => TypeRef::TypeError,
            Ok(range) => range,
        };

        if *parse_context != TokenType::Type {
            // Only allowed in "type" statements
            self.reporter.report_error(
                &set_tok.location,
                format_args!("Set types can only be declared inside of 'type' statements"),
            );
            TypeRef::TypeError
        } else {
            self.declare_type(Type::Set { range })
        }
    }

    // -- Wrappers around the scope list -- //
    // See `Scope` for the documentation of these functions

    /// Declares an identifer in the current scope, reporting the error message
    fn declare_ident(
        &self,
        ident: Token,
        type_spec: TypeRef,
        is_const: bool,
        is_typedef: bool,
    ) -> Identifier {
        let name = ident.location.get_lexeme(self.source).to_string();

        let (reference, err) = self
            .blocks
            .last()
            .unwrap()
            .borrow_mut()
            .scope
            .declare_ident(ident, name, type_spec, is_const, is_typedef);

        if let Some(msg) = err {
            self.reporter
                .report_error(&reference.token.location, format_args!("{}", msg));
        }

        reference
    }

    /// Uses an identifer, providing the error message
    fn use_ident_msg(&self, ident: Token) -> (Identifier, Option<String>) {
        let name = ident.location.get_lexeme(self.source);

        self.blocks
            .last()
            .unwrap()
            .borrow_mut()
            .scope
            .use_ident(ident, name)
    }

    /// Uses an identifer, reporting the error message
    fn use_ident(&mut self, ident: Token) -> Identifier {
        let (reference, err) = self.use_ident_msg(ident);

        if let Some(msg) = err {
            self.reporter
                .report_error(&reference.token.location, format_args!("{}", msg));
        }

        reference
    }

    /// Pushes a new block onto the block list
    fn push_block(&mut self, block_kind: BlockKind) {
        let block = CodeBlock::new(block_kind, &self.blocks);

        // Add the block to the list
        self.blocks.push(Rc::new(RefCell::new(block)));
    }

    /// Pops a block off of the block list, and moving the given statement list
    /// into the block
    fn pop_block(&mut self, stmts: &mut Vec<Stmt>) -> Rc<RefCell<CodeBlock>> {
        let block = self.blocks.pop().unwrap();
        block.borrow_mut().stmts.append(stmts);

        block
    }

    // -- Wrappers around the type table -- //
    fn declare_type(&mut self, type_info: Type) -> TypeRef {
        TypeRef::Named(
            self.unit
                .as_mut()
                .unwrap()
                .types_mut()
                .declare_type(type_info),
        )
    }

    // --- Helpers --- //

    /// Checks if the current tokens form a compound assignment (operator '=')
    fn is_compound_assignment(&self) -> bool {
        if &self.peek().token_type == &TokenType::Equ {
            // Look ahead token is a '=', check if current is one of the valid compound assign operators
            match &self.current().token_type {
                TokenType::Plus
                | TokenType::Minus
                | TokenType::Star
                | TokenType::Div
                | TokenType::Slash
                | TokenType::Rem
                | TokenType::Mod
                | TokenType::Exp
                | TokenType::And
                | TokenType::Or
                | TokenType::Xor
                | TokenType::Shl
                | TokenType::Shr
                | TokenType::Imply => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Checks if the current token is a simple assignment (':=' or '=')
    fn is_simple_assignment(&self) -> bool {
        matches!(
            &self.current().token_type,
            TokenType::Assign | TokenType::Equ
        )
    }

    /// Builds an argument list for an expression
    fn make_arg_list(&mut self) -> Result<Option<Vec<Expr>>, ParsingStatus> {
        if self.previous().token_type != TokenType::LeftParen {
            // No arg_list to be found
            return Ok(None);
        }

        let mut arg_list = vec![];

        if self.current().token_type != TokenType::RightParen {
            loop {
                arg_list.push(self.expr()?);

                if self.current().token_type != TokenType::Comma {
                    break;
                }

                // Consume comma
                self.next_token();
            }
        }

        self.expects(
            TokenType::RightParen,
            format_args!("Missing ')' after parameter list"),
        )?;

        // Give back the arg list
        return Ok(Some(arg_list));
    }

    /// Gets the precedence rule for the given token
    fn get_rule<'a>(token_type: &'a TokenType) -> &PrecedenceRule<'s, 'a> {
        match token_type {
            TokenType::LeftParen => &PrecedenceRule {
                precedence: Precedence::Call,
                prefix_rule: Some(Parser::expr_grouping),
                infix_rule: Some(Parser::expr_call),
            },
            TokenType::Imply => &PrecedenceRule {
                precedence: Precedence::Imply,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Or => &PrecedenceRule {
                precedence: Precedence::BitOr,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::And => &PrecedenceRule {
                precedence: Precedence::BitAnd,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Tilde | TokenType::Not => &PrecedenceRule {
                precedence: Precedence::BitNot,
                prefix_rule: Some(Parser::expr_unary_rule),
                infix_rule: None,
            },
            TokenType::Less
            | TokenType::Greater
            | TokenType::Equ
            | TokenType::LessEqu
            | TokenType::GreaterEqu
            | TokenType::NotEq
            | TokenType::In
            | TokenType::NotIn => &PrecedenceRule {
                precedence: Precedence::Comparison,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Plus | TokenType::Minus => &PrecedenceRule {
                precedence: Precedence::Sum,
                prefix_rule: Some(Parser::expr_unary),
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Xor => &PrecedenceRule {
                precedence: Precedence::Sum,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Star
            | TokenType::Slash
            | TokenType::Div
            | TokenType::Mod
            | TokenType::Rem
            | TokenType::Shl
            | TokenType::Shr => &PrecedenceRule {
                precedence: Precedence::Product,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Pound => &PrecedenceRule {
                precedence: Precedence::Conversion,
                prefix_rule: Some(Parser::expr_unary_rule),
                infix_rule: None,
            },
            TokenType::Caret => &PrecedenceRule {
                precedence: Precedence::Follow,
                prefix_rule: Some(Parser::expr_unary_rule),
                infix_rule: None,
            },
            TokenType::Deref => &PrecedenceRule {
                precedence: Precedence::Deref,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_deref),
            },
            TokenType::Exp => &PrecedenceRule {
                precedence: Precedence::Exponent,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_binary),
            },
            TokenType::Dot => &PrecedenceRule {
                precedence: Precedence::Call,
                prefix_rule: None,
                infix_rule: Some(Parser::expr_dot),
            },
            TokenType::NatLiteral(_)
            | TokenType::RealLiteral(_)
            | TokenType::CharLiteral(_)
            | TokenType::StringLiteral(_)
            | TokenType::True
            | TokenType::False
            | TokenType::Nil => &PrecedenceRule {
                precedence: Precedence::Primary,
                prefix_rule: Some(Parser::expr_primary),
                infix_rule: None,
            },
            TokenType::Identifier => &PrecedenceRule {
                precedence: Precedence::Primary,
                prefix_rule: Some(Parser::expr_ident),
                infix_rule: None,
            },
            _ => &PrecedenceRule {
                precedence: Precedence::NoPrec,
                prefix_rule: None,
                infix_rule: None,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::compiler::scanner::Scanner;
    use crate::compiler::types;

    fn make_test_parser(source: &str) -> Parser {
        let mut scanner = Scanner::new(source);
        scanner.scan_tokens();

        Parser::new(scanner.tokens, source, CodeUnit::new(true))
    }

    fn get_ident(parser: &Parser, name: &str) -> Option<Identifier> {
        parser
            .unit
            .as_ref()
            .unwrap()
            .root_block()
            .borrow()
            .scope
            .get_ident(name)
            .map(|i| i.clone())
    }

    fn get_ident_type(parser: &Parser, name: &str) -> TypeRef {
        parser
            .unit
            .as_ref()
            .unwrap()
            .root_block()
            .borrow()
            .scope
            .get_ident(name)
            .unwrap()
            .type_spec
    }

    fn check_ident_expected_type(parser: &Parser, name: &str, expected: TypeRef) {
        assert_eq!(get_ident_type(parser, name), expected);
    }

    fn is_ident_type_equivalent_to(parser: &Parser, lhs: &str, rhs: &str) -> bool {
        types::is_equivalent_to(
            &get_ident_type(&parser, lhs),
            &get_ident_type(&parser, rhs),
            parser.unit.as_ref().unwrap().types(),
        )
    }

    #[test]
    fn test_var_decl() {
        let mut parser = make_test_parser(
            "
        % Valid forms
        var a : int := 1
        var b : int
        var c := 3 + 6 ** 2
        var d, e, f : string := \"hai\"
        var x, y, z : real := 42e10
        
        % Accepted forms
        var g : int = -5
        var h : int = -10 + 3 * 2
        var i, j, k : nat = 20 + 40 shl 5
        ",
        );
        assert!(parser.parse());
        for name in ["x", "y", "z"].iter() {
            check_ident_expected_type(&parser, name, TypeRef::Primitive(PrimitiveType::Real));
        }

        for name in ["d", "e", "f"].iter() {
            check_ident_expected_type(&parser, name, TypeRef::Primitive(PrimitiveType::String_));
        }

        for name in ["i", "j", "k"].iter() {
            check_ident_expected_type(&parser, name, TypeRef::Primitive(PrimitiveType::Nat));
        }

        // Invalid forms - can't deduce type
        let mut parser = make_test_parser(
            "
        % Invalid forms
        var a
        var c
        var e, b, k",
        );
        assert!(!parser.parse());
        for name in ["a", "c", "e", "b", "k"].iter() {
            check_ident_expected_type(&parser, name, TypeRef::TypeError);
        }

        // Invalid forms - comma after last item
        let mut parser = make_test_parser(
            "
        % Invalid forms
        var a, b, c, : int := 5",
        );
        assert!(!parser.parse());
    }

    #[test]
    fn test_const_decl() {
        let mut parser = make_test_parser(
            "
        % Valid forms
        const a : int := 1
        const b := 5.0
        const c, d : int := 3
        const e, f := 3 + 6 ** 2
        
        % Accepted forms
        const g : int = -5
        const h : int = -10 + 3 * 2",
        );
        assert!(parser.parse());
        for name in ["c", "d"].iter() {
            check_ident_expected_type(&parser, name, TypeRef::Primitive(PrimitiveType::Int));
        }

        // Invalid forms
        let mut parser = make_test_parser(
            "
        % Invalid forms - No type or value
        const a
        const b",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::TypeError);
        check_ident_expected_type(&parser, "b", TypeRef::TypeError);

        let mut parser = make_test_parser(
            "
        % Invalid forms - No value
        const a : int
        const b : int",
        );
        assert!(!parser.parse());
    }

    #[test]
    fn test_simple_assignment() {
        let mut parser = make_test_parser(
            "
        % Setup
        var a : int
        
        % Valid forms
        a := 1
        a := 3 + 5 + 7
        a := #9 * 2 ** 3 and 5 xor 6

        % Accepted forms
        a = 2
        a = 194812
        a = -6
        ",
        );
        parser.parse();
        assert!(!parser.reporter.has_error());

        // Invaild: Dropped value
        let mut parser = make_test_parser(
            "
        % Setup
        var a : int
        a := 
        ",
        );
        parser.parse();
        assert!(parser.reporter.has_error());

        let mut parser = make_test_parser(
            "
        % Setup
        var a : int
        a = 
        ",
        );
        parser.parse();
        assert!(parser.reporter.has_error());
    }

    #[test]
    fn test_compound_assignment() {
        // Main operators
        let mut parser = make_test_parser(
            "
        % Setup
        var a : int
        var r : real
        
        % Valid forms
        a := 3
        a += 5
        a -= 7
        a *= 9
        a div= 11
        r /= 12.0
        a rem= 3
        a mod= 5
        a **= 2
        a and= 3
        a or= 5
        a xor= 6
        a shl= 9
        a shr= 12

        % Accepted forms
        a = 2
        ",
        );
        parser.parse();
        assert!(!parser.reporter.has_error());
        let expected_ops = [
            TokenType::Assign,
            TokenType::Plus,
            TokenType::Minus,
            TokenType::Star,
            TokenType::Div,
            TokenType::Slash,
            TokenType::Rem,
            TokenType::Mod,
            TokenType::Exp,
            TokenType::And,
            TokenType::Or,
            TokenType::Xor,
            TokenType::Shl,
            TokenType::Shr,
            TokenType::Assign,
        ];

        let root_stmts = parser.unit.as_ref().unwrap().stmts();

        for test_stmt in root_stmts[2..].iter().zip(expected_ops.iter()) {
            if let Stmt::Assign {
                op: Token { ref token_type, .. },
                ..
            } = test_stmt.0
            {
                if token_type != test_stmt.1 {
                    panic!(
                        "Mismatch between expected {:?} and parsed {:?}",
                        test_stmt.1, token_type
                    );
                }
            }
        }

        // Boolean operator versions
        let mut parser = make_test_parser(
            "
        % Setup
        var b : boolean
        
        % Valid forms
        b =>= true
        b and= false
        b or= true
        % xor= only valid for integers (int, nat, long, ulong) & sets
        ",
        );
        parser.parse();
        assert!(!parser.reporter.has_error());
        let expected_ops = [TokenType::Imply, TokenType::And, TokenType::Or];

        let root_stmts = parser.unit.as_ref().unwrap().stmts();

        for test_stmt in root_stmts[1..].iter().zip(expected_ops.iter()) {
            if let Stmt::Assign {
                op: Token { ref token_type, .. },
                ..
            } = test_stmt.0
            {
                if token_type != test_stmt.1 {
                    panic!(
                        "Mismatch between expected {:?} and parsed {:?}",
                        test_stmt.1, token_type
                    );
                }
            }
        }

        // The forbidden not expression is invalid in RsProlog
        let mut parser = make_test_parser("var a : boolean := true\na ~==~ a");
        assert!(!parser.parse());
        let mut parser = make_test_parser("var a : boolean := true\na not==not a");
        assert!(!parser.parse());
    }

    #[test]
    fn test_primitive_type_parser() {
        let mut parser = make_test_parser(
            "
        var a : boolean
        var b : int
        var c : int1
        var d : int2
        var e : int4
        var f : nat
        var g : nat1
        var h : nat2
        var i : nat4
        var j : real
        var k : real4
        var l : real8
        var m : string
        var n : string(300)
        var o : char
        var p : char(768)
        var q : addressint
        ",
        );
        assert!(parser.parse());

        let expected_types = [
            TypeRef::Primitive(PrimitiveType::Boolean),
            TypeRef::Primitive(PrimitiveType::Int),
            TypeRef::Primitive(PrimitiveType::Int1),
            TypeRef::Primitive(PrimitiveType::Int2),
            TypeRef::Primitive(PrimitiveType::Int4),
            TypeRef::Primitive(PrimitiveType::Nat),
            TypeRef::Primitive(PrimitiveType::Nat1),
            TypeRef::Primitive(PrimitiveType::Nat2),
            TypeRef::Primitive(PrimitiveType::Nat4),
            TypeRef::Primitive(PrimitiveType::Real),
            TypeRef::Primitive(PrimitiveType::Real4),
            TypeRef::Primitive(PrimitiveType::Real8),
            TypeRef::Primitive(PrimitiveType::String_),
            TypeRef::Primitive(PrimitiveType::StringN(300)),
            TypeRef::Primitive(PrimitiveType::Char),
            TypeRef::Primitive(PrimitiveType::CharN(768)),
            TypeRef::Primitive(PrimitiveType::AddressInt),
        ];

        let root_stmts = parser.unit.as_ref().unwrap().stmts();

        for test_stmt in root_stmts.iter().zip(expected_types.iter()) {
            if let Stmt::VarDecl { ref idents, .. } = test_stmt.0 {
                assert_eq!(&idents[0].type_spec, test_stmt.1);
            }
        }

        // Star lengths in subprogram parameters
        let mut parser = make_test_parser(
            "
        var a : proc _ (a : string(*))
        var b : proc _ (b : char(*))
        ",
        );
        assert!(parser.parse());

        // Compile time expr (we don't evaluate them yet)
        /*
        let mut parser = make_test_parser(
            "
        const c := 5 + 25 * 2 % 55
        var str : string(c)
        ",
        );
        assert!(parser.parse());
        assert_eq!(
            match parser.stmts[1] {
                Stmt::VarDecl {
                    ident: Identifier { type_spec, .. },
                    ..
                } => type_spec,
                _ => unreachable!(),
            },
            TypeRef::Primitive(PrimitiveType::StringN(55))
        );
        */

        // Invalid: Bigger than the maximum size
        let mut parser = make_test_parser("var c : string(16#10000)");
        assert!(!parser.parse());
        // Tried to parse as a "string"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        let mut parser = make_test_parser("var c : string(16#10001)");
        assert!(!parser.parse());
        // Tried to parse as a "string"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        // Invalid: Zero length size expression
        let mut parser = make_test_parser("var c : char(16#0)");
        assert!(!parser.parse());
        // Tried to parse as a "char"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::Char));

        let mut parser = make_test_parser("var c : string(16#0)");
        assert!(!parser.parse());
        // Tried to parse as a "string"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        // Invalid: Dropping the right paren
        let mut parser = make_test_parser("var c : char(16#0");
        assert!(!parser.parse());
        // Tried to parse as a "char"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::Char));

        // Invalid: No length specification
        let mut parser = make_test_parser("var c : string(");
        assert!(!parser.parse());
        // Tried to parse as a "string"
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        // Invalid: Not a type specification (shouldn't parse the := "hee" nor the 'to' as it may cause
        // phantom errors)
        let mut parser = make_test_parser("var c : to := 'hee'");
        assert!(!parser.parse());
        // Failed to parse, as type error
        check_ident_expected_type(&parser, "c", TypeRef::TypeError);

        // Invalid: '*' specifiec is only valid in subprogram parameter declarations
        let mut parser = make_test_parser("var c : string(*)");
        assert!(!parser.parse());
        // Failed to parse, as string
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        let mut parser = make_test_parser("var c : char(*)");
        assert!(!parser.parse());
        // Failed to parse, as char
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::Char));
    }

    #[test]
    fn test_compound_type_parser() {
        // Undeclared type identifiers don't produce an error until the validator stage
        let mut parser = make_test_parser(
            "
var a : pointer to int
var b : ^ string
var c : some_type
var d : procedure nps
var e : procedure np   ()
var f : procedure p1   (a : int)
var g : procedure p2   (a : int, b : string)
var h : procedure pisp (a : int, b : string, c : procedure _ ())
var j : function np   () : real
var k : function p1   (a : int) : string
var l : function p2   (a : int, b : string) : addressint
var m : function pisp (a : int, b : string, c : procedure _ ()) : boolean

% Pairs are to be equivalent
var n : function _ (a, b : int, c : real) : int
var o : function _ (a : int, b : int, c : real) : int
var p : function _ (var a, b : int, c : string) : int
var q : function _ (var a : int, var b : int, c : string) : int

% Other variations
var r : function _ (var a : cheat int, var register b : cheat int, proc c) : int
% Nesting fun!
% While not valid in TProlog, it should still be valid syntax as inner parameter names are ignored
var s : function _ (function a (function a : int ) : int, proc b (proc a (proc a( proc a))), proc c) : int

% Range parsing
var a_range : (1 - 3 shl 5) .. (2 * 50 - 8 * 4)

% Set parsing (only valid in type statements)
type some_set : set of 1 .. 5

% Array parsing setup
var start_range := 1
var end_range := 5

% Array parsing (enum ranges aren't parsed yet, but are equivalent to identifiers)
var t : array 1 .. 2 of int
% Multiple ranges
var u : array 1 .. 2, (-1 - 20) .. (2 + 3), (1 + 8) .. (2 + 16) of string
% Char ranges
var v : array 'a' .. 'f' of real
var w : array char of nat
% Boolean ranges
var x : array false .. true of char
var y : array boolean of boolean
% Other ranges
var z : array start_range .. end_range of real
var implicit_size : array 1 .. * of real
var flexi : flexible array 1 .. 0 of real

var up_size := 5
var runtime_size : array 1 .. up_size of real
        ",
        );
        assert!(parser.parse());
        assert!(is_ident_type_equivalent_to(&parser, "n", "o"));
        assert!(is_ident_type_equivalent_to(&parser, "p", "q"));

        // Arbitrary expressions are not valid types
        let mut parser = make_test_parser("var a : 1");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : 1 ** 2");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : (1 * 6 - 1 + 4 = 1)");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : false");
        assert!(!parser.parse());
    }

    #[test]
    fn test_pointer_type_invalids() {
        // Pointer type expects "to"
        let mut parser = make_test_parser("var a : pointer int");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : ^");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : pointer");
        assert!(!parser.parse());
    }

    #[test]
    fn test_subprogram_type_invalids() {
        // Function expects ':' before result type
        let mut parser = make_test_parser("var a : function a int");
        assert!(!parser.parse());

        // Function expects type after ':'
        let mut parser = make_test_parser("var a : function a :");
        assert!(!parser.parse());

        // Function type declaration expects '()' if there are no parameters
        let mut parser = make_test_parser("var a : function amphy : int");
        assert!(!parser.parse());

        // Function / procedure expects identifier after keyword (this can be made optional in the future)
        let mut parser = make_test_parser("var a : procedure");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : function : int");
        assert!(!parser.parse());
    }

    #[test]
    fn test_range_type_invalids() {
        // Inferred range end is only valid in array range contexts
        let mut parser = make_test_parser("var a : 1 .. *");
        assert!(!parser.parse());

        // No range end
        let mut parser = make_test_parser("var a : 1 .. ");
        assert!(!parser.parse());
    }

    #[test]
    fn test_set_type_invalids() {
        // Set type declarations are only valid in type statements
        let mut parser = make_test_parser("var a : set of 1 .. 3");
        assert!(!parser.parse());
        assert_eq!(get_ident(&parser, "a").unwrap().is_declared, true);
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, false);

        // Set type declarations expect 'of'
        let mut parser = make_test_parser("type a : set 1 .. 3");
        assert!(!parser.parse());
        assert_eq!(get_ident(&parser, "a").unwrap().is_declared, true);
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);

        let mut parser = make_test_parser("type a : set");
        assert!(!parser.parse());
        assert_eq!(get_ident(&parser, "a").unwrap().is_declared, true);
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);

        // Set type declarations expect a range
        let mut parser = make_test_parser("type a : set of ");
        assert!(!parser.parse());
        assert_eq!(get_ident(&parser, "a").unwrap().is_declared, true);
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);
    }

    #[test]
    fn test_array_type_invalids() {
        // Flexible array cannot have an implicit range
        let mut parser = make_test_parser("var inv : flexible array 1 .. * of real");
        assert!(!parser.parse());

        // Array cannot have a flexible array as an element type
        let mut parser =
            make_test_parser("var inv : flexible array 1 .. 2 of flexible array 1 .. 2 of real");
        assert!(!parser.parse());

        let mut parser =
            make_test_parser("var inv : array 1 .. 2 of flexible array 1 .. 2 of real");
        assert!(!parser.parse());

        let mut parser =
            make_test_parser("var inv : array 1 .. * of flexible array 1 .. 2 of real");
        assert!(!parser.parse());

        // Array cannot have an implicit size array as an element type
        let mut parser =
            make_test_parser("var inv : flexible array 1 .. 2 array of 1 .. * of real");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var inv : array 1 .. 2 of array 1 .. * of real");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var inv : array 1 .. * of array 1 .. * of real");
        assert!(!parser.parse());

        // Implicit size array cannot have more than one range specifier
        let mut parser = make_test_parser("var inv : array 1 .. *, char of real");
        assert!(!parser.parse());
        if let Some(Type::Array { ranges, .. }) = parser
            .unit
            .as_ref()
            .unwrap()
            .types()
            .type_from_ref(&get_ident_type(&parser, "inv"))
        {
            assert_eq!(ranges.len(), 1);
        } else {
            panic!("Not an array");
        }

        let mut parser = make_test_parser("var inv : array 1 .. *, 1 .. *, char of real");
        assert!(!parser.parse());
        if let Some(Type::Array { ranges, .. }) = parser
            .unit
            .as_ref()
            .unwrap()
            .types()
            .type_from_ref(&get_ident_type(&parser, "inv"))
        {
            assert_eq!(ranges.len(), 1);
        } else {
            panic!("Not an array");
        }

        // Implicit size range is only allowed for the first range specifier
        let mut parser = make_test_parser("var inv : array 1 .. 2, 1 .. *, char of real");
        assert!(!parser.parse());
        if let Some(Type::Array { ranges, .. }) = parser
            .unit
            .as_ref()
            .unwrap()
            .types()
            .type_from_ref(&get_ident_type(&parser, "inv"))
        {
            assert_eq!(ranges.len(), 3);
        } else {
            panic!("Not an array");
        }
    }

    #[test]
    fn test_identifier_resolution() {
        // v decl usage
        let mut parser = make_test_parser(
            "
        var a : int
        a := a + 1
        ",
        );
        assert!(parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::Int));

        // v decl usage usage
        let mut parser = make_test_parser(
            "
        var a : int
        a := a + 1
        var b := a + 1
        ",
        );
        assert!(parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::Int));

        // x usage
        let mut parser = make_test_parser(
            "
        a := a + 1 % final type
        ",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::TypeError);

        // x usage decl
        let mut parser = make_test_parser(
            "
        a := a + 1
        var a : int % final type
        ",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::Int));

        // x usage decl decl
        let mut parser = make_test_parser(
            "
        a := a + 1
        var a : int
        var a : string % final type
        ",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::String_));

        // x decl decl
        let mut parser = make_test_parser(
            "
        var a : string
        var a : real8 % final type
        ",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::Real8));

        // x decl usage-in-asn
        let mut parser = make_test_parser(
            "
        var a : string := a + \"oops\"
        ",
        );
        assert!(!parser.parse());
        check_ident_expected_type(&parser, "a", TypeRef::Primitive(PrimitiveType::String_));
    }

    #[test]
    fn test_block_stmt() {
        let mut parser = make_test_parser(
            "
        % Local declarations & importation
        begin
            var hey := 2
            begin
                var yay : real := 5 + hey
            end
            begin
                % Different scope!
                var yay : real := 5 + hey
            end
            var yay : int := 6 - hey
        end
        var yay : string := \"hello!\"
        ",
        );
        assert!(parser.parse());

        // Missing end
        let mut parser = make_test_parser(
            "
        begin
            var yay : int := 5
        var yay : string := \"hello!\"
        ",
        );
        assert!(!parser.parse());

        // Redeclaration of declared - global - inner
        let mut parser = make_test_parser(
            "
        var yay : string := \"hello!\"
        begin
            var yay : int := 5
        end
        ",
        );
        assert!(!parser.parse());

        // Redeclaration of declared - inner - inner
        let mut parser = make_test_parser(
            "
        begin
            var yay : int := 5
            begin
                var yay : int := 5
            end
        end
        var yay : string := \"hello!\"
        ",
        );
        assert!(!parser.parse());
    }

    #[test]
    fn test_type_decl() {
        let mut parser = make_test_parser("type a : int");
        assert!(parser.parse());

        // Requires identifer, will consume the type and colon
        let mut parser = make_test_parser("type : a := 1");
        assert!(!parser.parse());
        // The a := 1 should not produce a statement (the a should be consumed by "type")
        assert_eq!(parser.unit.unwrap().stmts().len(), 0);

        // Requires colon, will parse the rest and produce a declaration
        let mut parser = make_test_parser("var a : string\ntype a");
        assert!(!parser.parse());
        assert!(get_ident(&parser, "a").is_some());
        assert_eq!(
            get_ident(&parser, "a").unwrap().type_spec,
            TypeRef::TypeError
        );
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);

        let mut parser = make_test_parser("var a : string\ntype a int");
        assert!(!parser.parse());
        assert!(get_ident(&parser, "a").is_some());
        assert!(get_ident(&parser, "a").unwrap().is_typedef);
        assert_eq!(
            get_ident(&parser, "a").unwrap().type_spec,
            TypeRef::Primitive(PrimitiveType::Int)
        );
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);
    }
}
