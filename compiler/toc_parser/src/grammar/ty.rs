//! Type grammars
#[cfg(test)]
mod test;

use super::*;

pub(super) fn ty(p: &mut Parser) -> Option<CompletedMarker> {
    ty_primitive(p).or_else(|| {
        match_token!(|p| match {
            TokenKind::Flexible,
            TokenKind::Array => { array_type(p) } // array_type
            TokenKind::Enum => { enum_type(p) } // enum_type
            TokenKind::Procedure,
            TokenKind::Function => { todo!() } // subprog_type
            TokenKind::Unchecked,
            TokenKind::Pointer => { pointer_type(p) } // pointer_type
            TokenKind::Caret => { short_pointer_type(p) } // pointer_type (short form)
            TokenKind::Set => { set_type(p) } // set_type
            TokenKind::Record => { todo!() } // record_type
            TokenKind::Union => { todo!() } // union_type
            TokenKind::Collection => { collection_type(p) } // collection_type
            TokenKind::Priority,
            TokenKind::Deferred,
            TokenKind::Timeout,
            TokenKind::Condition => { condition_type(p) } // condition_type
            _ => {
                expr::expr(p).and_then(|cm| {
                    // either name type or range type
                    // further checks are pushed down to AST validation
                    // so e.g. int literals are allowed in type position
                    if p.at(TokenKind::Range) {
                        // range tail
                        range_type_tail(p, cm)
                    } else {
                        // Enclose expr (potential name ref) inside NameType
                        Some(cm.precede(p).complete(p, SyntaxKind::NameType))
                    }
                }).or_else(|| {
                    // not a ty
                    p.error(Expected::Type);
                    None
                })
            }
        })
    })
}

pub(super) fn ty_primitive(p: &mut Parser) -> Option<CompletedMarker> {
    // TODO: Add 64 bit types (int8, nat8, long int, long nat)
    match_token!(|p| match {
        TokenKind::Addressint,
        TokenKind::Boolean,
        TokenKind::Int, TokenKind::Int1, TokenKind::Int2, TokenKind::Int4,
        TokenKind::Nat, TokenKind::Nat1, TokenKind::Nat2, TokenKind::Nat4,
        TokenKind::Real, TokenKind::Real4, TokenKind::Real8 => { prim_type(p) }
        TokenKind::Char => { prim_charseq_type(p, TokenKind::Char) }
        TokenKind::String_ => { prim_charseq_type(p, TokenKind::String_) }
        _ => None // Not a primitive type
    })
}

fn prim_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(
        p.at(TokenKind::Addressint)
            || p.at(TokenKind::Boolean)
            || p.at(TokenKind::Int)
            || p.at(TokenKind::Int1)
            || p.at(TokenKind::Int2)
            || p.at(TokenKind::Int4)
            || p.at(TokenKind::Nat)
            || p.at(TokenKind::Nat1)
            || p.at(TokenKind::Nat2)
            || p.at(TokenKind::Nat4)
            || p.at(TokenKind::Real)
            || p.at(TokenKind::Real4)
            || p.at(TokenKind::Real8)
    );

    let m = p.start();
    p.bump();
    Some(m.complete(p, SyntaxKind::PrimType))
}

fn prim_charseq_type(p: &mut Parser, prim_kind: TokenKind) -> Option<CompletedMarker> {
    debug_assert!(p.at(prim_kind));

    let m = p.start();
    p.bump();

    if p.at(TokenKind::LeftParen) {
        // make it sized!
        let kind = match prim_kind {
            TokenKind::Char => SyntaxKind::SizedCharType,
            TokenKind::String_ => SyntaxKind::SizedStringType,
            _ => unreachable!(),
        };

        p.bump();

        p.with_extra_recovery(&[TokenKind::RightParen], |p| {
            let m = p.start();

            if !p.eat(TokenKind::Star) {
                // if not dyn sized, parse an expr
                expr::expect_expr(p);
            }

            m.complete(p, SyntaxKind::SeqLength);
        });

        p.expect(TokenKind::RightParen);

        Some(m.complete(p, kind))
    } else {
        // basic unsized type
        Some(m.complete(p, prim_kind.into()))
    }
}

fn array_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Flexible) || p.at(TokenKind::Array));

    let m = p.start();

    if p.eat(TokenKind::Flexible) {
        if !p.at(TokenKind::Array) {
            // stop, not an array type
            p.error_unexpected_at(m, None);
            return None;
        }
    }

    // on 'array'
    p.bump();

    p.with_extra_recovery(&[TokenKind::Of], |p| {
        self::range_list(p);
    });

    p.expect(TokenKind::Of);

    // on index type
    self::ty(p);

    Some(m.complete(p, SyntaxKind::ArrayType))
}

fn range_list(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();

    p.with_extra_recovery(&[TokenKind::Comma], |p| {
        self::ty(p);

        while p.at(TokenKind::Comma) {
            p.bump();

            self::ty(p);
        }
    });

    Some(m.complete(p, SyntaxKind::RangeList))
}

fn enum_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Enum));

    let m = p.start();
    p.bump();

    p.expect(TokenKind::LeftParen);
    p.with_extra_recovery(&[TokenKind::RightParen], |p| {
        super::name_list(p);
    });
    p.expect(TokenKind::RightParen);

    Some(m.complete(p, SyntaxKind::EnumType))
}

fn set_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Set));

    let m = p.start();
    p.bump();

    p.expect(TokenKind::Of);

    // parse index type
    self::ty(p);

    Some(m.complete(p, SyntaxKind::SetType))
}

fn pointer_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Unchecked) || p.at(TokenKind::Pointer));

    let m = p.start();
    p.eat(TokenKind::Unchecked);
    p.eat(TokenKind::Pointer);
    p.expect(TokenKind::To);

    // parse pointed to type
    self::ty(p);

    Some(m.complete(p, SyntaxKind::PointerType))
}

fn short_pointer_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Caret));

    let m = p.start();
    p.bump(); // nom on ^

    // parse pointed to type
    self::ty(p);

    Some(m.complete(p, SyntaxKind::PointerType))
}

fn collection_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Collection));

    let m = p.start();
    p.bump();
    p.expect(TokenKind::Of);

    if !p.eat(TokenKind::Forward) {
        self::ty(p);
    }

    Some(m.complete(p, SyntaxKind::CollectionType))
}

fn condition_type(p: &mut Parser) -> Option<CompletedMarker> {
    debug_assert!(
        p.at(TokenKind::Priority)
            || p.at(TokenKind::Deferred)
            || p.at(TokenKind::Timeout)
            || p.at(TokenKind::Condition)
    );

    let mut m = p.start();

    if p.eat(TokenKind::Priority) || p.eat(TokenKind::Deferred) || p.eat(TokenKind::Timeout) {
        // wrap the kind in a wrapper node
        m = m.complete(p, SyntaxKind::ConditionKind).precede(p);

        // expect condition after attribute
        p.expect(TokenKind::Condition);
    } else {
        // nom on 'condition'
        p.bump();
    }

    Some(m.complete(p, SyntaxKind::ConditionType))
}

fn range_type_tail(p: &mut Parser, lhs: CompletedMarker) -> Option<CompletedMarker> {
    debug_assert!(p.at(TokenKind::Range));

    let m = lhs.precede(p);
    p.bump();

    if !p.eat(TokenKind::Star) {
        // Just a regular range bound
        expr::expect_expr(p);
    }

    Some(m.complete(p, SyntaxKind::RangeType))
}
