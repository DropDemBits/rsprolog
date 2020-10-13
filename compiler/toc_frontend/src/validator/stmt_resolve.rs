//! Validator fragment, resolves all statements and declarations
use super::expr_resolve;
use super::{ResolveContext, ScopeInfo, Validator};

use std::cell::RefCell;
use std::rc::Rc;
use toc_ast::ast::{BinaryOp, Expr, Identifier, Stmt, UnaryOp, VisitorMut};
use toc_ast::block::CodeBlock;
use toc_ast::types::{self, PrimitiveType, Type, TypeRef, TypeTable};
use toc_ast::value::Value;

impl Validator {
    // --- Decl Resolvers --- //

    pub(super) fn resolve_decl_var(
        &mut self,
        idents: &mut Vec<Identifier>,
        type_spec: &mut TypeRef,
        value: &mut Option<Box<Expr>>,
        is_const: bool,
    ) {
        let mut is_compile_eval = false;

        if types::is_error(type_spec) {
            // The identifiers should all have matching values
            debug_assert_eq!(
                idents
                    .iter()
                    .filter(|ident| !types::is_error(&ident.type_spec))
                    .count(),
                0
            );
        }

        // Visit the expression to update the eval type
        if value.is_some() {
            let expr = value.as_mut().unwrap();
            let init_eval = self.visit_expr(expr);

            // Try to replace the initializer value with the folded value
            super::replace_with_folded(expr, init_eval);

            is_compile_eval = expr.is_compile_eval();

            if super::is_type_reference(expr) {
                self.reporter.report_error(
                    expr.get_span(),
                    format_args!("A type reference cannot be used as an initializer value"),
                );
                *value = None;
            }
        }

        // Resolve the identifier type spec or sized char sequence type spec, if possible
        if types::is_named(type_spec) || types::is_sized_char_seq_type(type_spec) {
            // Only required to be compile-time if the decl is a const decl, or if the type spec
            // is not directly an array
            let resolving_context = if is_const
                || !matches!(
                    self.type_table.type_from_ref(type_spec),
                    Some(Type::Array { .. })
                ) {
                ResolveContext::CompileTime(false)
            } else {
                ResolveContext::Any
            };

            *type_spec = self.resolve_type(*type_spec, resolving_context);

            // If the `type_spec` is a range, verify it is not a zero sized range
            if let Some(Type::Range {
                start, end, size, ..
            }) = self.type_table.type_from_ref(&type_spec)
            {
                // No guarrantess that end is a Some
                if end.is_none() {
                    // Error should be reported by parser
                    *type_spec = TypeRef::TypeError;
                } else if *size == Some(0) {
                    // Zero sized ranges aren't allowed in variable/constant range types
                    let range_span = start.get_span().span_to(end.as_ref().unwrap().get_span());
                    self.reporter.report_error(
                        &range_span,
                        format_args!("Range bounds creates a zero-sized range"),
                    );
                    *type_spec = TypeRef::TypeError;
                }
            }
        }

        // Handle the type spec propogation
        if *type_spec == TypeRef::Unknown {
            // Unknown type, use the type of the expr
            // Safe to unwrap as if no expr was provided, the type_spec would be TypeError

            let expr = value.as_ref().unwrap();
            *type_spec = expr.get_eval_type();

            if types::is_intnat(type_spec) {
                // Always convert IntNats into Ints
                // (larger sizes are automatically converted into the appropriate type)
                *type_spec = TypeRef::Primitive(PrimitiveType::Int);
            }
        } else if value.is_some() {
            // Type of the identifier is known, validate that the types are assignable
            let expr = value.as_ref().unwrap();

            let left_type = &self.dealias_resolve_type(*type_spec);
            let right_type = &self.dealias_resolve_type(expr.get_eval_type());

            // If both of the types are not an error, check for assignability
            if !types::is_error(left_type) && !types::is_error(right_type) {
                debug_assert!(
                    types::is_base_type(left_type, &self.type_table),
                    "Of type {:?}",
                    left_type
                );
                debug_assert!(
                    types::is_base_type(right_type, &self.type_table),
                    "Of type {:?}",
                    right_type
                );

                // Validate that the types are assignable
                if !types::is_assignable_to(&left_type, &right_type, &self.type_table) {
                    // Value to assign is the wrong type, just report the error
                    self.reporter.report_error(
                        &idents.last().as_ref().unwrap().location,
                        format_args!("Initialization value is the wrong type"),
                    );
                } else {
                    // Update compile-time evaluability status
                    is_compile_eval = value.as_ref().unwrap().is_compile_eval();
                }
            }
        }
        // Variable declarations with no assignment value will have the type already given

        // If value is an init expression, verify compatibility
        if !types::is_error(type_spec) {
            if let Some(expr) = value {
                if let Expr::Init { init, exprs, .. } = &**expr {
                    // Check if the type can accept the "init"
                    // Only valid for arrays, records, and unions
                    let mut field_types = if let Some(type_info) =
                        self.type_table.type_from_ref(type_spec)
                    {
                        match type_info {
                            Type::Array {
                                ranges,
                                element_type,
                                is_init_sized,
                                is_flexible,
                            } => {
                                // `did_overflow` indicates it was capped at usize::MAX
                                let (elem_count, did_overflow) =
                                    types::get_array_element_count(ranges, &self.type_table);

                                if ranges.is_empty() {
                                    // No ranges on the array, error reported by the parser
                                    None
                                } else if *is_flexible {
                                    self.reporter.report_error(init, format_args!("'init' initializers are not allowed for flexible arrays"));
                                    None
                                } else if elem_count == 0 && !is_init_sized {
                                    // We know it to be dynamic, as one of the ranges isn't a compile-time expression and it isn't a flexible array
                                    self.reporter.report_error(init, format_args!("'init' initializers are not allowed for dynamic arrays"));
                                    None
                                } else if did_overflow {
                                    // Array has more elements than can be handled
                                    // Definitely an error (stop yourself, for your own sake)
                                    self.reporter.report_error(init, format_args!("'init' has more initializer values than can be represented by a machine-size integer"));
                                    None
                                } else if *is_init_sized {
                                    // Match type count with init size
                                    Some(std::iter::once(element_type).cycle().take(exprs.len()))
                                } else {
                                    // Build type iter on array count sizes
                                    Some(std::iter::once(element_type).cycle().take(elem_count))
                                }
                            }
                            _ => {
                                // Not the requested type
                                None
                            }
                        }
                    } else {
                        // Nope!
                        None
                    };

                    // If a none, errors are already produced
                    if let Some(field_types) = field_types.as_mut() {
                        // Iterate over the init types
                        let mut init_types = exprs.iter().map(|e| (e, e.get_eval_type()));
                        let mut has_fields_remaining = false;

                        for field_type in field_types {
                            let init_field = init_types.next();

                            if init_field.is_none() {
                                has_fields_remaining = true;
                                // None left
                                break;
                            }

                            let (init_expr, init_type) = init_field.unwrap();

                            if !matches!(*init_expr, Expr::Empty)
                                && !types::is_assignable_to(
                                    field_type,
                                    &init_type,
                                    &self.type_table,
                                )
                            {
                                // Wrong types (skipping over empty expressions as those are produced by the parser)
                                // ???: Report field name for records?
                                self.reporter.report_error(
                                    init_expr.get_span(),
                                    format_args!("Initializer value evaluates to the wrong type"),
                                );
                            }
                        }

                        // Check if there are any remaining
                        let next_init = init_types.next();

                        match next_init {
                            Some((next_expr, _)) if !has_fields_remaining => {
                                // Too many init fields
                                let report_at = if !matches!(next_expr, Expr::Empty) {
                                    next_expr.get_span()
                                } else {
                                    // If empty, at init
                                    // No other close location to report at
                                    init
                                };

                                self.reporter.report_error(
                                    report_at,
                                    format_args!("Too many initializer values"),
                                );
                            }
                            None if has_fields_remaining => {
                                // Too few init
                                let report_at = if !matches!(exprs.last(), Some(Expr::Empty)) {
                                    exprs.last().unwrap().get_span()
                                } else {
                                    // If empty, at init
                                    // Empty case captured by Parser
                                    init
                                };

                                // ???: Report field name for records?
                                // ???: Report missing count for arrays?
                                self.reporter.report_error(
                                    report_at,
                                    format_args!("Too few initializer values"),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Grab the compile-time value
        let const_val = if is_compile_eval && is_const {
            // Create a value to clone from
            let value = Value::from_expr(*value.as_ref().unwrap().clone(), &self.type_table)
                .unwrap_or_else(|msg| {
                    panic!(
                        "Initializer value '{:?}' is not a compile-time expression ({})",
                        value, msg
                    )
                });
            Some(value)
        } else {
            // No compile-time value is produced
            None
        };

        // Update the identifiers to the new identifier type
        for ident in idents.iter_mut() {
            ident.type_spec = *type_spec;
            // Only compile-time evaluable if the identifier referencences a constant
            ident.is_compile_eval = is_compile_eval && ident.is_const;
            let resolved_ident = self
                .active_block
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap()
                .borrow_mut()
                .scope
                .resolve_ident(&ident.name, &ident);

            // Must always be a defined identifier
            let resolved_ident = resolved_ident
                .expect("Internal Compiler Error: Consistency of identifiers has been broken");

            // Add identifier to the scope info (including the compile-time value)
            if self
                .scope_infos
                .last_mut()
                .unwrap()
                .decl_ident_with(resolved_ident, const_val.clone())
            {
                // Report the error
                self.reporter.report_error(
                    &ident.location,
                    format_args!("'{}' has already been declared", ident.name),
                );
            }
        }
    }

    pub(super) fn resolve_decl_type(
        &mut self,
        ident: &mut Identifier,
        resolved_type: &mut Option<TypeRef>,
        is_new_def: bool,
    ) {
        if !ident.is_declared {
            // This is a dummy type declare, and only provides resolving access
            // Resolve the type, and return
            let dummy_ref = resolved_type.take().unwrap();
            let dummy_ref = self.resolve_type(dummy_ref, ResolveContext::CompileTime(false));
            resolved_type.replace(dummy_ref);

            return;
        }

        if is_new_def {
            if resolved_type.is_some() {
                // Resolve the associated type (do not allow forward references)
                ident.type_spec =
                    self.resolve_type(ident.type_spec, ResolveContext::CompileTime(false));
            } else if let Some(Type::Forward { is_resolved: false }) =
                self.type_table.type_from_ref(&ident.type_spec)
            {
                // Not resolved in the current unit
                self.reporter.report_error(
                    &ident.location,
                    format_args!("'{}' is not resolved in the current unit", ident.name),
                );
            }

            // Declare the identifier and check for redeclaration errors
            if self
                .scope_infos
                .last_mut()
                .unwrap()
                .decl_ident(ident.clone())
            {
                self.reporter.report_error(
                    &ident.location,
                    format_args!("'{}' has already been declared", ident.name),
                );
            }
        } else {
            // Use the identifier
            // Must be defined
            let is_defined = self.scope_infos.last_mut().unwrap().use_ident(&ident).1;
            assert!(is_defined);

            if let Some(resolve_ref) = resolved_type {
                // This is a type resolution statement, update the associated type reference
                if let TypeRef::Named(replace_id) = ident.type_spec {
                    // Make an alias to the resolved type
                    self.type_table
                        .replace_type(replace_id, Type::Alias { to: *resolve_ref });
                }

                // Resolve the rest of the type
                ident.type_spec =
                    self.resolve_type(ident.type_spec, ResolveContext::CompileTime(false));
            } else {
                // This is a redeclared forward, and is safe to ignore
            }
        }
    }

    // --- Stmt Resolvers --- //

    pub(super) fn resolve_stmt_assign(
        &mut self,
        var_ref: &mut Box<Expr>,
        op: Option<&mut BinaryOp>,
        value: &mut Box<Expr>,
    ) {
        let ref_eval = self.visit_expr(var_ref);
        let value_eval = self.visit_expr(value);

        // Try to replace the operands with the folded values
        super::replace_with_folded(var_ref, ref_eval);
        super::replace_with_folded(value, value_eval);

        // Resolve types first
        let left_type = &self.dealias_resolve_type(var_ref.get_eval_type());
        let right_type = &self.dealias_resolve_type(value.get_eval_type());

        // Validate that the types are assignable for the given operation
        if types::is_error(left_type) || types::is_error(right_type) {
            // Silently drop propogated TypeErrors
            return;
        }

        // Check the reference expression
        if !can_assign_to_ref_expr(&var_ref, &self.type_table) {
            // Not a var ref
            self.reporter.report_error(var_ref.get_span(), format_args!("Left side of assignment does not reference a variable and cannot be assigned to"));
            return;
        }

        debug_assert!(types::is_base_type(left_type, &self.type_table));
        debug_assert!(types::is_base_type(right_type, &self.type_table));

        if let Some(op) = op {
            let produce_type =
                expr_resolve::check_binary_operands(left_type, *op, right_type, &self.type_table);
            if produce_type.is_err()
                || !types::is_assignable_to(left_type, &produce_type.unwrap(), &self.type_table)
            {
                // Value to assign is the wrong type
                self.reporter.report_error(
                    &value.get_span(),
                    format_args!("Assignment value is the wrong type"),
                );
            }
        } else if !types::is_assignable_to(left_type, right_type, &self.type_table) {
            // Value to assign is the wrong type
            self.reporter.report_error(
                &value.get_span(),
                format_args!("Assignment value is the wrong type"),
            );
        }
    }

    pub(super) fn resolve_stmt_block(
        &mut self,
        block: &Rc<RefCell<CodeBlock>>,
        stmts: &mut Vec<Stmt>,
    ) {
        let mut scope_info = ScopeInfo::new();

        // Import all of the identifiers from above scopes
        // Don't need to worry about the "pervasive" import attribute,
        // as that is handled by the parser
        // An identifier is only in the import table if and only if it
        // has been used
        {
            // Drop the scope ref after importing everything
            let scope = &block.borrow().scope;

            for import in scope.import_table() {
                let imported_info = &mut self.scope_infos[import.downscopes];

                // Fetch ident from the new scope
                let base_ident = imported_info.get_ident(&import.name, import.instance.into());

                if let Some(mut base_ident) = base_ident {
                    // Use identifier from the imported scope
                    let imported_ident = base_ident.clone();
                    // Should be the same
                    assert_eq!(imported_ident.instance, import.instance);

                    // Get compile value from the imported scope info
                    let (compile_value, is_declared) = imported_info.use_ident(&imported_ident);
                    assert!(is_declared, "Imported identifier was never declared");

                    // Import into the new scope info
                    base_ident.instance = 0;
                    assert!(
                        !scope_info.decl_ident_with(base_ident, compile_value),
                        "Duplicate import identifier?"
                    );
                }

                // If None, identfifier is not declared in the imported scope
                // Error will be reported later on, so don't need ro do anything
            }
        }

        // Change the active block and push the new scope info
        let previous_scope = self.active_block.replace(Rc::downgrade(block));
        self.scope_infos.push(scope_info);

        for stmt in stmts.iter_mut() {
            self.visit_stmt(stmt);
        }

        // Revert to previous scope and pop the last scope info
        self.active_block.replace(previous_scope.unwrap());
        let last_info = self.scope_infos.pop().unwrap();

        // Report unused identifiers
        self.report_unused_identifiers(&last_info);
    }
}

/// Checks if the given `ref_expr` references a variable or a mutable reference.
/// Assumes the `ref_expr` has already had the types propogated.
fn can_assign_to_ref_expr(ref_expr: &Expr, type_table: &TypeTable) -> bool {
    match ref_expr {
        Expr::Reference { ident, .. } | Expr::Dot { field: ident, .. } => {
            // Can only assign to a variable reference
            !ident.is_const && !ident.is_typedef
        }
        Expr::Call { left, .. } => {
            match &**left {
                Expr::Reference { ident, .. } | Expr::Dot { field: ident, .. } => {
                    let dealiased_ref = types::dealias_ref(&ident.type_spec, type_table);
                    let type_info = type_table.type_from_ref(&dealiased_ref);

                    // Can only assign if the ref is to an array variable
                    // Can't assign to a const ref or a type def
                    !ident.is_const
                        && !ident.is_typedef
                        && matches!(type_info, Some(Type::Array { .. }))
                }
                _ => can_assign_to_ref_expr(&left, type_table), // Go further down the chain
            }
        }
        Expr::UnaryOp { op, eval_type, .. } => {
            // Only assignable if the expression is a deref, and the eval type isn't an error
            matches!(op.0, UnaryOp::Deref) && !types::is_error(eval_type)
        }
        _ => false, // Not one of the above, likely unable to assign to
    }
}