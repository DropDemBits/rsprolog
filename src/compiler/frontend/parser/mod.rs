//! Main parser for tokens to build the AST
// Parser fragments
mod expr;
mod stmt;
mod types;

use crate::compiler::ast::Identifier;
use crate::compiler::block::{BlockKind, CodeBlock, CodeUnit};
use crate::compiler::frontend::token::{Token, TokenType};
use crate::compiler::types::{Type, TypeRef};
use crate::compiler::Location;
use crate::status_reporter::StatusReporter;
use std::cell::RefCell;
use std::fmt::Arguments;
use std::rc::Rc;

/// Maximum nesting depth during parsing
const MAX_NESTING_DEPTH: usize = 256;

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
    /// Expression nesting depth
    expr_nesting: usize,
    /// Statement nesting depth
    stmt_nesting: usize,
    /// Type nesting depth
    type_nesting: usize,
}

#[derive(Debug)]
enum ParsingStatus {
    /// Error during parsing
    Error,
    /// Skipping tokens during parsing
    Skip,
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
            expr_nesting: 0,
            stmt_nesting: 0,
            type_nesting: 0,
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

    // -- Wrappers around the scope list -- //
    // See `Scope` for the documentation of these functions

    /// Declares an identifer in the current scope, providing the error message
    /// Allows ignoring the error message, which is all cases
    fn declare_ident(
        &self,
        ident: Token,
        type_spec: TypeRef,
        is_const: bool,
        is_typedef: bool,
    ) -> (Identifier, Option<String>) {
        let name = ident.location.get_lexeme(self.source).to_string();

        self.blocks
            .last()
            .unwrap()
            .borrow_mut()
            .scope
            .declare_ident(ident, name, type_spec, is_const, is_typedef)
    }

    /// Uses an identifer, providing the error message
    /// Allows ignoring the error message, which is all cases
    fn use_ident(&self, ident: Token) -> (Identifier, Option<String>) {
        let name = ident.location.get_lexeme(self.source);

        self.blocks
            .last()
            .unwrap()
            .borrow_mut()
            .scope
            .use_ident(ident, name)
    }

    /// Gets the identifier from the current scope
    fn get_ident(&self, name: &str) -> Option<Identifier> {
        self.blocks
            .last()
            .unwrap()
            .borrow_mut()
            .scope
            .get_ident(name)
            .map(|i| i.clone())
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

    fn replace_type(&mut self, type_ref: &TypeRef, new_info: Type) {
        if let TypeRef::Named(replace_id) = type_ref {
            self.unit
                .as_mut()
                .unwrap()
                .types_mut()
                .replace_type(*replace_id, new_info);
        } else {
            panic!("Not a named type ref");
        }
    }

    // -- Block Helpers -- //

    /// Pushes a new block onto the block list
    fn push_block(&mut self, block_kind: BlockKind) {
        let block = CodeBlock::new(block_kind, &self.blocks);

        // Add the block to the list
        self.blocks.push(Rc::new(RefCell::new(block)));
    }

    /// Pops a block off of the block list, returning the block
    fn pop_block(&mut self) -> Rc<RefCell<CodeBlock>> {
        let block = self.blocks.pop().unwrap();

        block
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::compiler::ast::{self, Expr, Stmt};
    use crate::compiler::frontend::scanner::Scanner;
    use crate::compiler::types::{self, *};

    fn make_test_parser(source: &str) -> Parser {
        let mut scanner = Scanner::new(source);
        scanner.scan_tokens();

        Parser::new(scanner.tokens, source, CodeUnit::new(true))
    }

    // Get the latest version of the identifier
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

    // Gets the identifier with the specified instance
    fn get_ident_instance(
        parser: &Parser,
        name: &str,
        instance: ast::IdentInstance,
    ) -> Option<Identifier> {
        parser
            .unit
            .as_ref()
            .unwrap()
            .root_block()
            .borrow()
            .scope
            .get_ident_instance(name, instance)
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
    fn test_opt_semicolon() {
        let mut parser = make_test_parser(";;;;;\nvar a : int := 1;\n;;;;;var b : int := 1;");
        assert!(parser.parse());
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
            TypeRef::Primitive(PrimitiveType::StringN(SequenceSize::Size(300))),
            TypeRef::Primitive(PrimitiveType::Char),
            TypeRef::Primitive(PrimitiveType::CharN(SequenceSize::Size(768))),
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

        // Expressions are allowed for string(n) and char(n), resolved at validator time
        // They don't parse into to the base type
        let mut parser = make_test_parser("var c : string(1 + 1 + 1 - 2 + 4 * 8 div 2)");
        assert!(parser.parse());
        assert_ne!(
            get_ident_type(&parser, "c"),
            TypeRef::Primitive(PrimitiveType::String_)
        );

        let mut parser =
            make_test_parser("const c := 1 + 1 + 1 - 2 + 4 * 8 div 2\nvar d : string(c)");
        assert!(parser.parse());
        assert_ne!(
            get_ident_type(&parser, "d"),
            TypeRef::Primitive(PrimitiveType::String_)
        );

        let mut parser =
            make_test_parser("const c := 1 + 1 + 1 - 2 + 4 * 8 div 2\nvar d : char(c + 4)");
        assert!(parser.parse());
        assert_ne!(
            get_ident_type(&parser, "d"),
            TypeRef::Primitive(PrimitiveType::Char)
        );

        // Wrong types will be captured by the validator

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

        // Invalid: '*' specifiec is only valid in subprogram parameter declarations
        let mut parser = make_test_parser("var c : string(*)");
        assert!(!parser.parse());
        // Failed to parse, as string
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::String_));

        let mut parser = make_test_parser("var c : char(*)");
        assert!(!parser.parse());
        // Failed to parse, as char
        check_ident_expected_type(&parser, "c", TypeRef::Primitive(PrimitiveType::Char));

        // Invalid: Not a type specification (shouldn't parse the := "hee" nor the 'to' as it may cause
        // phantom errors)
        let mut parser = make_test_parser("var c : to := 'hee'");
        assert!(!parser.parse());
        // Failed to parse, as type error
        check_ident_expected_type(&parser, "c", TypeRef::TypeError);
    }

    #[test]
    fn test_compound_type_parser() {
        // Undeclared type identifiers don't produce an error until the validator stage
        let mut parser = make_test_parser(
            "
var a : pointer to int
var a_alt : unchecked pointer to int
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
type some_set_c : set of char
type some_set_b : set of boolean

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
var implicit_size : array 1 .. * of real := init (1, 2, 3, 4, 5)
var flexi : flexible array 1 .. 0 of real

var up_size := 5
var runtime_size : array 1 .. up_size of real

% Identifier reference (resolved at validation time)
var some_external_use : some.thing.with.these.given.fields := 3
var ranged_external : some.thing.with.start .. some.thing.with.end_thing := 5
var implicit_external : array 1 .. some.thing.with.end_thing of int

% Enum types
type enumeration : enum (a, b, c, d, e, f)
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

        // Pointer type expects "to"
        let mut parser = make_test_parser("var a : unchecked pointer int");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : ^");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : pointer");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : unchecked ^");
        assert!(!parser.parse());

        // Pointer type expects type
        let mut parser = make_test_parser("var a : unchecked pointer");
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

        // No range end in function parameter
        let mut parser = make_test_parser("var a : function _ (a : array 1 .. )");
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
    fn test_ident_ref_invalids() {
        // Missing identifier after '.'
        let mut parser = make_test_parser("var inv : an.ident.list.");
        assert!(!parser.parse());

        // Expression does not contain only field refs
        let mut parser = make_test_parser("var inv : an.ident.list.of(1, 2, 3)");
        assert!(!parser.parse());
    }

    #[test]
    fn test_enum_invalids() {
        // Enums can have 1 or more fields
        let mut parser = make_test_parser("type a : enum (a)");
        assert!(parser.parse());

        let mut parser = make_test_parser("type a : enum (a, b, c)");
        assert!(parser.parse());

        // At least one field must be specified
        let mut parser = make_test_parser("type a : enum ()\nvar b : int");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);
        // Ensure this gets parsed
        assert_eq!(
            get_ident_type(&parser, "b"),
            TypeRef::Primitive(PrimitiveType::Int)
        );

        let mut parser = make_test_parser("type a : enum )\nvar b : int");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);
        // Ensure this gets parsed
        assert_eq!(
            get_ident_type(&parser, "b"),
            TypeRef::Primitive(PrimitiveType::Int)
        );

        let mut parser = make_test_parser("type a : enum \nvar b : int");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);
        // Ensure this gets parsed
        assert_eq!(
            get_ident_type(&parser, "b"),
            TypeRef::Primitive(PrimitiveType::Int)
        );

        // Right paren is required, but should not create an error type
        let mut parser = make_test_parser("type a : enum (a");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);

        let mut parser = make_test_parser("type a : enum (a, b, c");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);

        // Field identifiers must be separated by comma delimiters (ends the list otherwise)
        let mut parser = make_test_parser("var c := 3\ntype a : enum (a, b c += 1");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);

        // Non-identifiers terminate the list
        let mut parser = make_test_parser("type a : enum (a, to\nvar b : int");
        assert!(!parser.parse());
        assert_ne!(get_ident_type(&parser, "a"), TypeRef::TypeError);
        assert_eq!(
            get_ident_type(&parser, "b"),
            TypeRef::Primitive(PrimitiveType::Int)
        );

        // Enums not in top-level type contexts are rejected (producing type errors)
        // i.e. anonymous enums are not allowed
        let mut parser = make_test_parser("var a : enum (a, b, c)");
        assert!(!parser.parse());
        assert_eq!(get_ident_type(&parser, "a"), TypeRef::TypeError);

        let mut parser = make_test_parser("const a : enum (a, b, c)");
        assert!(!parser.parse());
        assert_eq!(get_ident_type(&parser, "a"), TypeRef::TypeError);

        let mut parser = make_test_parser("type a : set of enum (a, b, c)");
        assert!(!parser.parse());
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
        assert!(parser.parse()); // Checked at validator time

        // Validate the types
        if let Stmt::Block { block, .. } = &parser.unit.as_ref().unwrap().stmts()[1] {
            // Inner scope is still int
            assert_eq!(
                block
                    .as_ref()
                    .borrow()
                    .scope
                    .get_ident("yay")
                    .unwrap()
                    .type_spec,
                TypeRef::Primitive(PrimitiveType::Int)
            );
        } else {
            unreachable!();
        }

        // Outer scope is still string
        assert_eq!(
            get_ident(&parser, "yay").unwrap().type_spec,
            TypeRef::Primitive(PrimitiveType::String_)
        );

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
        assert!(parser.parse()); // Checked at validator time

        // Validate the types
        if let Stmt::Block { block, .. } = &parser.unit.as_ref().unwrap().stmts()[0] {
            // Innermost scope is still int
            assert_eq!(
                block
                    .as_ref()
                    .borrow()
                    .scope
                    .get_ident("yay")
                    .unwrap()
                    .type_spec,
                TypeRef::Primitive(PrimitiveType::Int)
            );
        } else {
            unreachable!();
        }

        // Outermost scope is still string
        assert_eq!(
            get_ident(&parser, "yay").unwrap().type_spec,
            TypeRef::Primitive(PrimitiveType::String_)
        );
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
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident(&parser, "a").unwrap().type_spec),
                Some(Type::Alias {
                    to: TypeRef::TypeError,
                })
            )
        );
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);

        let mut parser = make_test_parser("var a : string\ntype a int");
        assert!(!parser.parse());
        assert!(get_ident(&parser, "a").is_some());
        assert!(get_ident(&parser, "a").unwrap().is_typedef);
        assert_eq!(
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident(&parser, "a").unwrap().type_spec),
                Some(Type::Alias {
                    to: TypeRef::Primitive(PrimitiveType::Int),
                })
            )
        );
        assert_eq!(get_ident(&parser, "a").unwrap().is_typedef, true);

        // Check that the forward reference is updated
        let mut parser = make_test_parser("type a : forward");
        assert!(parser.parse()); // Checked by the validator
        assert_eq!(
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident(&parser, "a").unwrap().type_spec),
                Some(Type::Forward { is_resolved: false })
            )
        );

        // Check that the forward reference is updated
        let mut parser = make_test_parser("type a : forward\ntype a : int");
        assert!(parser.parse()); // Checked by the validator
        assert_eq!(
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident(&parser, "a").unwrap().type_spec),
                Some(Type::Forward { is_resolved: true })
            )
        );

        // Forward refs after resolves create a new type
        let mut parser = make_test_parser("type a : forward\ntype a : int\ntype a : forward");
        assert!(parser.parse()); // Checked at validator time
        assert_eq!(
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident_instance(&parser, "a", 2).unwrap().type_spec),
                Some(Type::Forward { is_resolved: false })
            )
        );

        // Duplicate forward refs should not affect resolved state
        let mut parser = make_test_parser("type a : forward\ntype a : forward\ntype a : int");
        assert!(!parser.parse());
        assert_eq!(
            true,
            matches!(
                parser
                    .unit
                    .as_ref()
                    .unwrap()
                    .types()
                    .type_from_ref(&get_ident_instance(&parser, "a", 1).unwrap().type_spec),
                Some(Type::Forward { is_resolved: true })
            )
        );
    }

    #[test]
    fn test_init_expr() {
        fn nab_init_len(stmt: &Stmt) -> Option<usize> {
            if let Stmt::VarDecl {
                value: Some(init_expr),
                ..
            } = stmt
            {
                if let Expr::Init { exprs, .. } = &**init_expr {
                    Some(exprs.len())
                } else {
                    None
                }
            } else {
                None
            }
        }

        // Size checking & compile-time checking is performed by the validator
        let mut parser = make_test_parser("var a : array 1 .. 3 of int := init(1, 2, 3)");
        assert!(parser.parse());
        assert_eq!(
            Some(3),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        let mut parser = make_test_parser("var a : array 1 .. * of int := init(1)");
        assert!(parser.parse());
        assert_eq!(
            Some(1),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect at least one expression
        let mut parser = make_test_parser("var a : array 1 .. * of int := init() begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(1),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect closing paren
        let mut parser = make_test_parser("var a : array 1 .. * of int := init( begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(1),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect starting paren
        let mut parser = make_test_parser("var a : array 1 .. * of int := init) begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(1),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect parens
        let mut parser = make_test_parser("var a : array 1 .. * of int := init begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(1),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect expr after comma (length 2)
        let mut parser = make_test_parser("var a : array 1 .. * of int := init(1,) begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(2),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Expect expr after comma (length 3)
        let mut parser = make_test_parser("var a : array 1 .. * of int := init(1,,) begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(3),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Bad exprs should still contribute to length
        let mut parser = make_test_parser("var a : array 1 .. * of int := init(1,+,+,4) begin end");
        assert!(!parser.parse());
        assert_eq!(
            Some(4),
            nab_init_len(&parser.unit.as_ref().unwrap().stmts()[0])
        );

        // Can only be used in initalization of const's & var's
        let mut parser = make_test_parser("var a : array 1 .. 3 of int\n a := init(1,2,3)");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : array 1 .. 3 of int\n a := +init(1,2,3)");
        assert!(!parser.parse());

        let mut parser = make_test_parser("var a : array 1 .. 3 of int\n a := -init(1,2,3)");
        assert!(!parser.parse());

        let mut parser =
            make_test_parser("var a : array 1 .. 3 of int\n a := init(1,2,3)+init(1,2,3)");
        assert!(!parser.parse());

        // Arrays require init initializers
        let mut parser = make_test_parser("var a : array 1 .. * of int");
        assert!(!parser.parse());

        // Can't infer type from init
        let mut parser = make_test_parser("var a := init(1, 2, 3)");
        assert!(!parser.parse());
        assert_eq!(TypeRef::TypeError, get_ident_type(&parser, "a"));

        // Can't infer type from init
        let mut parser = make_test_parser("const a := init(1, 2, 3)");
        assert!(!parser.parse());
        assert_eq!(TypeRef::TypeError, get_ident_type(&parser, "a"));
    }

    #[test]
    fn test_infix_operators() {
        // Test all operators in infix positions
        // Should not crash

        // Types don't matter here, as that's checked in validator
        let mut parser = make_test_parser("const a := 1 + 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 - 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 * 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 div 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 shl 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 shr 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 and 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 or 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 xor 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 in 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 not in 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 ~ in 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 ~in 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 < 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 <= 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 > 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 >= 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 = 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 ~= 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 ~ = 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 not = 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 not= 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const a := 1 => 1");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const ba := 2\nconst a := ba.a");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const ba := 2\nconst a := ba->a");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const ba := 2\nconst a := ba()");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const ba := 2\nconst a := ba(1, 2, 3)");
        assert_eq!(parser.parse(), true);

        // Only prefix
        let mut parser = make_test_parser("const a := 1 ~ ");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 not ");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 # ");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 1.0 ");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 1 ");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 \"keke\"");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 'keke'");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 true");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 false");
        assert_eq!(parser.parse(), false);
        let mut parser = make_test_parser("const a := 1 nil");
        assert_eq!(parser.parse(), false);

        // Identifiers and ^ are okay as they are interpreted as a new statement
        let mut parser = make_test_parser("const ba := 2\nconst a := 1 ba");
        assert_eq!(parser.parse(), true);
        let mut parser = make_test_parser("const ba := 2\nconst a := 1 ^ba");
        assert_eq!(parser.parse(), true);
    }

    #[test]
    fn test_nesting_limit() {
        // Should not panic

        // Expr limit, unary
        let mut parser = make_test_parser("var k := ####################################################################################################################################################################################################################################################################################1");
        assert_eq!(parser.parse(), false);

        // Expr limit, binary
        let mut parser = make_test_parser("var k := 1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1+(1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))");
        assert_eq!(parser.parse(), false);

        // Stmt limit
        let mut parser = make_test_parser("begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin begin ");
        assert_eq!(parser.parse(), false);

        // Type limit
        let mut parser = make_test_parser("type k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : proc a (k : int))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))");
        assert_eq!(parser.parse(), false);
    }
}