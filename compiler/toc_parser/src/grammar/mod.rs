//! Grammar parsing

// Todo list for parity with `toc_frontend/parser` (x) and completion (|)

// stmts:
// - const_decl
// - var_decl
// - type_decl
// - bind_decl
// - procedure_decl
// - function_decl
// - process_decl
// - external_decl
// - forward_decl
// - deferred_decl
// - body_decl
// - module_decl
// - class_decl
// - monitor_decl

// - include_glob
// - assign_stmt
// | open_stmt
// | close_stmt
// | put_stmt
// | get_stmt
// | read_stmt
// | write_stmt
// | seek_stmt
// | tell_stmt
// - for_stmt
// - loop_stmt
// - exit_stmt
// - if_stmt
// - case_stmt
// - block_stmt
// - invariant_stmt
// - assert_stmt
// - call_stmt
// - return_stmt
// - result_stmt
// - new_stmt
// - free_stmt
// - tag_stmt
// - fork_stmt
// - signal_stmt
// - pause_stmt
// - quit_stmt
// - checkedness_stmt
// - pre_stmt
// - init_stmt
// - post_stmt
// - handler_stmt
// - inherit_stmt
// - implement_stmt
// - implement_by_stmt
// - import_stmt
// - export_stmt

// exprs:
// - include_glob
// - init_expr
// - literal_expr
// - call_expr
// - dot_expr
// - name_expr
// - deref_expr
// - binary_expr
// - unary_expr
// - paren_expr
// - indirect_expr
// - bits_expr
// - range_expr (used in string subscripting)
// - cheat_expr
// - objclass_expr
// - self_expr
// - nil_expr

// types
// - include_glob
// - primitive_type (including sized variants)
// - name_type
// - array_type
// - enum_type
// - pointer_type
// - range_type
// - set_type
// - function_type / subprog_type
// - record_type
// - union_type
// - collection_type
// - condition_type

mod expr;
mod stmt;
mod ty;

/// Helper for matching tokens
macro_rules! match_token {
    (|$parser:ident| match {
        $($($tok:expr),+ => $action:block $(,)?)+
        _ => $otherwise:expr $(,)?
    }) => {
        match () {
            $( _ if $($parser.at($tok) || )+ false => $action )+
            _ => $otherwise
        }
    };
}

pub(self) use match_token;

use crate::parser::marker::CompletedMarker;
use crate::parser::Expected;
use crate::parser::Parser;
use toc_scanner::token::TokenKind;
use toc_syntax::SyntaxKind;

pub(crate) fn source(p: &mut Parser) -> CompletedMarker {
    let source = p.start();
    p.hidden_eat(TokenKind::Unit);
    while !p.at_end() {
        stmt::stmt(p);
    }
    source.complete(p, SyntaxKind::Source)
}

pub(self) fn name(p: &mut Parser) -> Option<CompletedMarker> {
    if p.at(TokenKind::Identifier) {
        let m = p.start();
        p.bump();
        Some(m.complete(p, SyntaxKind::Name))
    } else {
        // not found
        p.error_unexpected().report();
        None
    }
}

pub(self) fn name_list(p: &mut Parser) -> Option<CompletedMarker> {
    let mut parsed_any = false;
    let m = p.start();

    p.with_extra_recovery(&[TokenKind::Comma], |p| {
        parsed_any |= self::name(p).is_some();

        while p.at(TokenKind::Comma) {
            // did parse something
            parsed_any = true;

            p.bump();

            self::name(p);
        }
    });

    Some(m.complete(p, SyntaxKind::NameList)).filter(|_| parsed_any)
}

/// ParamList ( `'(' Param ( ',' Param )* ')'` )
pub(self) fn param_list(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::LeftParen));

    let m = p.start();
    p.bump();

    if !p.at(TokenKind::RightParen) {
        if let Some((_, true)) = param(p) {
            loop {
                match param(p) {
                    Some((_, true)) => {}      // parsed param, expecting more
                    Some((_, false)) => break, // parsed param, end of list
                    None => {
                        // missing next param
                        break;
                    }
                }
            }
        }
    }

    p.expect(TokenKind::RightParen);

    Some(m.complete(p, SyntaxKind::ParamList))
}

pub(self) fn param(p: &mut Parser) -> Option<(CompletedMarker, bool)> {
    let m = p.start();

    p.with_extra_recovery(&[TokenKind::Comma, TokenKind::RightParen], |p| {
        expr::expect_expr_or_range_item(p);
    });

    // bump ',' onto param
    let found_comma = p.eat(TokenKind::Comma);

    Some((m.complete(p, SyntaxKind::Param), found_comma))
}

pub(self) fn param_spec(p: &mut Parser) -> Option<CompletedMarker> {
    // ParamSpec: '(' ParamDecl ( ',' ParamDecl )* ')'
    let m = p.start();

    p.expect(TokenKind::LeftParen);
    p.with_extra_recovery(&[TokenKind::RightParen, TokenKind::Comma], |p| {
        if !p.at(TokenKind::RightParen) {
            if let Some(..) = self::param_decl(p) {
                while p.eat(TokenKind::Comma) {
                    self::param_decl(p);
                }
            }
        }
    });
    p.expect(TokenKind::RightParen);

    Some(m.complete(p, SyntaxKind::ParamSpec))
}

pub(self) fn param_decl(p: &mut Parser) -> Option<CompletedMarker> {
    match_token!(|p| match {
        TokenKind::Function,
        TokenKind::Procedure => {
            let m = p.start();
            ty::subprog_type(p);
            Some(m.complete(p,SyntaxKind::ParamDecl))
        }
        TokenKind::Var,
        TokenKind::Register,
        TokenKind::Identifier => { ty::constvar_param(p) }
        _ => {
            // not a thing
            None
        }
    })
}

pub(self) fn include_glob(p: &mut Parser) -> Option<CompletedMarker> {
    // 'include' 'string_literal'
    debug_assert!(p.at(TokenKind::Include));

    let m = p.start();
    p.bump();
    p.expect(TokenKind::StringLiteral);
    Some(m.complete(p, SyntaxKind::IncludeGlob))
}
