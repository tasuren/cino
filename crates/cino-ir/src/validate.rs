use std::collections::HashMap;

use crate::{Diagnostic, IrExpr, IrExprKind, IrPattern, IrPatternKind, IrProgram, IrType};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FnSig {
    params: Vec<IrType>,
    return_type: IrType,
}

pub fn validate_program(program: &IrProgram) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let signatures = program
        .functions
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                FnSig {
                    params: function.params.iter().map(|p| p.ty.clone()).collect(),
                    return_type: function.return_type.clone(),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for function in &program.functions {
        if !function.body.ty.is_compatible(&function.return_type) {
            diagnostics.push(Diagnostic {
                code: "E-IR-VAL-001",
                message: format!(
                    "function `{}` return type mismatch: body {:?}, declared {:?}",
                    function.name, function.body.ty, function.return_type
                ),
                line: function.span.line,
                column: function.span.column,
            });
        }

        let mut env = HashMap::new();
        for param in &function.params {
            env.insert(param.name.clone(), param.ty.clone());
        }
        validate_expr(&function.body, &mut env, &signatures, &mut diagnostics);
    }

    diagnostics
}

fn validate_expr(
    expr: &IrExpr,
    env: &mut HashMap<String, IrType>,
    signatures: &HashMap<String, FnSig>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &expr.kind {
        IrExprKind::LocalRef { name } => {
            let Some(expected) = env.get(name) else {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-002",
                    message: format!("unknown local reference `{name}`"),
                    line: expr.span.line,
                    column: expr.span.column,
                });
                return;
            };
            if !expr.ty.is_compatible(expected) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-003",
                    message: format!(
                        "local `{name}` type mismatch: expr {:?}, env {:?}",
                        expr.ty, expected
                    ),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::Int(_) => {
            if !expr.ty.is_compatible(&IrType::Int) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-004",
                    message: "int literal must have Int type".to_string(),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::Bool(_) => {
            if !expr.ty.is_compatible(&IrType::Bool) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-004",
                    message: "bool literal must have Bool type".to_string(),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::Tuple(items) => {
            for item in items {
                validate_expr(item, env, signatures, diagnostics);
            }
            let tuple_ty = IrType::Tuple(items.iter().map(|item| item.ty.clone()).collect());
            if !expr.ty.is_compatible(&tuple_ty) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-004",
                    message: format!(
                        "tuple expression type mismatch: expr {:?}, inferred {:?}",
                        expr.ty, tuple_ty
                    ),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::List(items) => {
            for item in items {
                validate_expr(item, env, signatures, diagnostics);
            }
            let mut item_ty = IrType::Unknown;
            if let Some(first) = items.first() {
                item_ty = first.ty.clone();
            }
            let list_ty = IrType::Named {
                name: "List".to_string(),
                args: vec![item_ty],
            };
            if !expr.ty.is_compatible(&list_ty) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-004",
                    message: format!(
                        "list expression type mismatch: expr {:?}, inferred {:?}",
                        expr.ty, list_ty
                    ),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::Record { name: _, fields } => {
            for field in fields {
                validate_expr(&field.value, env, signatures, diagnostics);
            }
            // For MVP, we don't strictly validate that all required fields are present here,
            // as it should have been caught by sema.
        }
        IrExprKind::Binary { lhs, rhs, .. } => {
            validate_expr(lhs, env, signatures, diagnostics);
            validate_expr(rhs, env, signatures, diagnostics);
            if !lhs.ty.is_compatible(&IrType::Int)
                || !rhs.ty.is_compatible(&IrType::Int)
                || !expr.ty.is_compatible(&IrType::Int)
            {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-005",
                    message: "binary expression must use Int operands and return Int".to_string(),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
        }
        IrExprKind::Call { callee, args } => {
            for arg in args {
                validate_expr(arg, env, signatures, diagnostics);
            }
            let Some(sig) = signatures.get(callee) else {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-006",
                    message: format!("unknown callee `{callee}`"),
                    line: expr.span.line,
                    column: expr.span.column,
                });
                return;
            };
            if !expr.ty.is_compatible(&sig.return_type) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-007",
                    message: format!(
                        "call `{callee}` return type mismatch: expr {:?}, signature {:?}",
                        expr.ty, sig.return_type
                    ),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
            if args.len() != sig.params.len() {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-008",
                    message: format!(
                        "call `{callee}` arity mismatch: expected {}, got {}",
                        sig.params.len(),
                        args.len()
                    ),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
            for (idx, arg) in args.iter().enumerate() {
                if let Some(expected) = sig.params.get(idx) {
                    if !arg.ty.is_compatible(expected) {
                        diagnostics.push(Diagnostic {
                            code: "E-IR-VAL-009",
                            message: format!(
                                "call `{callee}` arg {} type mismatch: expected {:?}, got {:?}",
                                idx + 1,
                                expected,
                                arg.ty
                            ),
                            line: expr.span.line,
                            column: expr.span.column,
                        });
                    }
                }
            }
        }
        IrExprKind::Let { name, value, body } => {
            validate_expr(value, env, signatures, diagnostics);
            let previous = env.insert(name.clone(), value.ty.clone());
            validate_expr(body, env, signatures, diagnostics);
            if !body.ty.is_compatible(&expr.ty) {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-010",
                    message: "let expression type must equal body type".to_string(),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
            if let Some(prev) = previous {
                env.insert(name.clone(), prev);
            } else {
                env.remove(name);
            }
        }
        IrExprKind::Match { subject, arms } => {
            validate_expr(subject, env, signatures, diagnostics);
            if arms.is_empty() {
                diagnostics.push(Diagnostic {
                    code: "E-IR-VAL-011",
                    message: "match expression requires at least one arm".to_string(),
                    line: expr.span.line,
                    column: expr.span.column,
                });
            }
            for arm in arms {
                let mut arm_env = env.clone();
                validate_pattern(&arm.pattern, &subject.ty, &mut arm_env, diagnostics);
                validate_expr(&arm.body, &mut arm_env, signatures, diagnostics);
                if !arm.body.ty.is_compatible(&expr.ty) {
                    diagnostics.push(Diagnostic {
                        code: "E-IR-VAL-012",
                        message: format!(
                            "match arm body type mismatch: expected {:?}, got {:?}",
                            expr.ty, arm.body.ty
                        ),
                        line: arm.span.line,
                        column: arm.span.column,
                    });
                }
            }
        }
    }
}

fn validate_pattern(
    pattern: &IrPattern,
    expected_ty: &IrType,
    env: &mut HashMap<String, IrType>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !pattern.ty.is_compatible(expected_ty) {
        diagnostics.push(Diagnostic {
            code: "E-IR-VAL-013",
            message: format!(
                "pattern type mismatch: expected {:?}, got {:?}",
                expected_ty, pattern.ty
            ),
            line: pattern.span.line,
            column: pattern.span.column,
        });
    }

    match &pattern.kind {
        IrPatternKind::Wildcard => {}
        IrPatternKind::Binding { name } => {
            env.insert(name.clone(), pattern.ty.clone());
        }
        IrPatternKind::Variant { fields, .. } => {
            for field in fields {
                validate_pattern(&field.pattern, &field.pattern.ty, env, diagnostics);
            }
        }
    }
}
