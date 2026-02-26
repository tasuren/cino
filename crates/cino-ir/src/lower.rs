use std::collections::HashMap;

use cino_sema::analyze;
use cino_syntax::{
    BlockExpr, Expr, ExprKind, FnDecl, MatchExpr, Pattern, PatternField, PatternKind, Program,
    Span, Stmt, TopDecl, TypeDeclKind, TypeExpr, TypeExprKind,
};

use crate::{
    validate_program, Diagnostic, IrExpr, IrExprKind, IrFunction, IrMatchArm, IrParam, IrPattern,
    IrPatternField, IrPatternKind, IrProgram, IrType, LoweringResult, SourceLoc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FnSig {
    params: Vec<IrType>,
    return_type: IrType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeDeclKindTag {
    State,
    Record,
    Event,
    Query,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeInfo {
    kind: TypeDeclKindTag,
    generics: Vec<String>,
    fields: Vec<(String, IrType)>,
    variants: HashMap<String, Vec<(String, IrType)>>,
}

struct Lowerer {
    diagnostics: Vec<Diagnostic>,
    fn_table: HashMap<String, FnSig>,
    type_table: HashMap<String, TypeInfo>,
    variant_to_type: HashMap<String, String>,
}

impl Lowerer {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            fn_table: HashMap::new(),
            type_table: HashMap::new(),
            variant_to_type: HashMap::new(),
        }
    }

    fn lower(mut self, program: &Program) -> LoweringResult {
        let sema = analyze(program);
        if !sema.is_ok() {
            return LoweringResult {
                program: None,
                diagnostics: sema
                    .diagnostics
                    .into_iter()
                    .map(|d| Diagnostic {
                        code: d.code,
                        message: d.message,
                        line: d.line,
                        column: d.column,
                    })
                    .collect(),
            };
        }

        self.collect_decls(program);
        let mut functions = Vec::new();
        for decl in &program.decls {
            if let TopDecl::Function(fd) = decl {
                if let Some(function) = self.lower_function(fd) {
                    functions.push(function);
                }
            }
        }

        if !self.diagnostics.is_empty() {
            return LoweringResult {
                program: None,
                diagnostics: self.diagnostics,
            };
        }

        let ir_program = IrProgram { functions };
        let validation = validate_program(&ir_program);
        if validation.is_empty() {
            LoweringResult {
                program: Some(ir_program),
                diagnostics: Vec::new(),
            }
        } else {
            LoweringResult {
                program: None,
                diagnostics: validation,
            }
        }
    }

    fn collect_decls(&mut self, program: &Program) {
        for decl in &program.decls {
            match decl {
                TopDecl::Function(fd) => {
                    let params = fd
                        .params
                        .iter()
                        .map(|param| self.ty_from_ast(&param.type_expr))
                        .collect::<Vec<_>>();
                    let return_type = self.ty_from_ast(&fd.return_type);
                    self.fn_table.insert(
                        fd.name.clone(),
                        FnSig {
                            params,
                            return_type,
                        },
                    );
                }
                TopDecl::Type(td) => {
                    let (kind, fields, variants) = match &td.kind {
                        TypeDeclKind::State(r) => (
                            TypeDeclKindTag::State,
                            self.collect_fields(r.fields.iter()),
                            HashMap::new(),
                        ),
                        TypeDeclKind::Record(r) => (
                            TypeDeclKindTag::Record,
                            self.collect_fields(r.fields.iter()),
                            HashMap::new(),
                        ),
                        TypeDeclKind::Event(v) => (
                            TypeDeclKindTag::Event,
                            Vec::new(),
                            self.collect_variants(v.variants.iter(), &td.name),
                        ),
                        TypeDeclKind::Query(v) => (
                            TypeDeclKindTag::Query,
                            Vec::new(),
                            self.collect_variants(v.variants.iter(), &td.name),
                        ),
                        TypeDeclKind::Enum(v) => (
                            TypeDeclKindTag::Enum,
                            Vec::new(),
                            self.collect_variants(v.variants.iter(), &td.name),
                        ),
                    };
                    self.type_table.insert(
                        td.name.clone(),
                        TypeInfo {
                            kind,
                            generics: td.generics.clone(),
                            fields,
                            variants,
                        },
                    );
                }
            }
        }
    }

    fn collect_fields<'a, I>(&self, fields: I) -> Vec<(String, IrType)>
    where
        I: Iterator<Item = &'a cino_syntax::FieldDecl>,
    {
        fields
            .map(|f| (f.name.clone(), self.ty_from_ast(&f.type_expr)))
            .collect()
    }

    fn collect_variants<'a, I>(
        &mut self,
        variants: I,
        type_name: &str,
    ) -> HashMap<String, Vec<(String, IrType)>>
    where
        I: Iterator<Item = &'a cino_syntax::VariantDecl>,
    {
        let mut out = HashMap::new();
        for variant in variants {
            let payload = variant
                .payload
                .iter()
                .map(|field| (field.name.clone(), self.ty_from_ast(&field.type_expr)))
                .collect::<Vec<_>>();
            self.variant_to_type
                .insert(variant.name.clone(), type_name.to_string());
            out.insert(variant.name.clone(), payload);
        }
        out
    }

    fn lower_function(&mut self, fd: &FnDecl) -> Option<IrFunction> {
        let mut env = HashMap::new();
        let params = fd
            .params
            .iter()
            .map(|param| {
                let ty = self.ty_from_ast(&param.type_expr);
                env.insert(param.name.clone(), ty.clone());
                IrParam {
                    name: param.name.clone(),
                    ty,
                    span: loc(param.span),
                }
            })
            .collect::<Vec<_>>();
        let return_type = self.ty_from_ast(&fd.return_type);
        let body = self.lower_block_expr(&fd.body, &mut env)?;

        if !self.ty_compatible(&body.ty, &return_type) {
            self.emit(
                "E-IR-004",
                format!(
                    "function `{}` body has {:?}, expected {:?}",
                    fd.name, body.ty, return_type
                ),
                fd.return_type.span,
            );
            return None;
        }

        Some(IrFunction {
            kind: fd.kind,
            name: fd.name.clone(),
            params,
            return_type,
            body,
            span: loc(fd.span),
        })
    }

    fn lower_block_expr(
        &mut self,
        block: &BlockExpr,
        env: &mut HashMap<String, IrType>,
    ) -> Option<IrExpr> {
        let mut scoped = env.clone();
        let mut lowered_lets = Vec::new();

        for stmt in &block.statements {
            match stmt {
                Stmt::Let(let_stmt) => {
                    let value = self.lower_expr(&let_stmt.value, &mut scoped)?;
                    scoped.insert(let_stmt.name.clone(), value.ty.clone());
                    lowered_lets.push((let_stmt.name.clone(), value, loc(let_stmt.span)));
                }
                Stmt::Return(ret) => {
                    self.emit(
                        "E-IR-001",
                        "`return` statement is not lowerable in MVP".to_string(),
                        ret.span,
                    );
                    return None;
                }
            }
        }

        let mut body = match &block.tail {
            Some(tail) => self.lower_expr(tail, &mut scoped)?,
            None => {
                self.emit(
                    "E-IR-001",
                    "block expression requires a tail expression".to_string(),
                    block.span,
                );
                return None;
            }
        };

        for (name, value, span) in lowered_lets.into_iter().rev() {
            body = IrExpr {
                ty: body.ty.clone(),
                span,
                kind: IrExprKind::Let {
                    name,
                    value: Box::new(value),
                    body: Box::new(body),
                },
            };
        }

        Some(body)
    }

    fn lower_expr(&mut self, expr: &Expr, env: &mut HashMap<String, IrType>) -> Option<IrExpr> {
        match &expr.kind {
            ExprKind::Ident(name) => {
                let ty = env
                    .get(name)
                    .cloned()
                    .or_else(|| self.resolve_builtin(name))
                    .unwrap_or_else(|| {
                        self.emit(
                            "E-IR-002",
                            format!("unresolved symbol `{name}` during lowering"),
                            expr.span,
                        );
                        IrType::Unknown
                    });
                Some(IrExpr {
                    kind: IrExprKind::LocalRef { name: name.clone() },
                    ty,
                    span: loc(expr.span),
                })
            }
            ExprKind::Int(v) => Some(IrExpr {
                kind: IrExprKind::Int(*v),
                ty: IrType::Int,
                span: loc(expr.span),
            }),
            ExprKind::Bool(v) => Some(IrExpr {
                kind: IrExprKind::Bool(*v),
                ty: IrType::Bool,
                span: loc(expr.span),
            }),
            ExprKind::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|item| self.lower_expr(item, env))
                    .collect::<Option<Vec<_>>>()?;
                let ty =
                    IrType::Tuple(items.iter().map(|item| item.ty.clone()).collect::<Vec<_>>());
                Some(IrExpr {
                    kind: IrExprKind::Tuple(items),
                    ty,
                    span: loc(expr.span),
                })
            }
            ExprKind::List(items) => {
                let items = items
                    .iter()
                    .map(|item| self.lower_expr(item, env))
                    .collect::<Option<Vec<_>>>()?;
                let mut item_ty = IrType::Unknown;
                if let Some(first) = items.first() {
                    item_ty = first.ty.clone();
                }
                Some(IrExpr {
                    kind: IrExprKind::List(items),
                    ty: IrType::Named {
                        name: "List".to_string(),
                        args: vec![item_ty],
                    },
                    span: loc(expr.span),
                })
            }
            ExprKind::Record { name, fields } => {
                let (ty_name, expected_fields) = if let Some(type_info) = self.type_table.get(name)
                {
                    (name.clone(), type_info.fields.clone())
                } else if let Some(type_name) = self.variant_to_type.get(name) {
                    let type_info = self.type_table.get(type_name).unwrap();
                    (
                        type_name.clone(),
                        type_info.variants.get(name).cloned().unwrap_or_default(),
                    )
                } else {
                    self.emit(
                        "E-IR-002",
                        format!("unresolved type or variant `{name}` during lowering"),
                        expr.span,
                    );
                    return None;
                };

                let mut lowered_fields = Vec::new();
                for f in fields {
                    let value = self.lower_expr(&f.value, env)?;
                    if let Some((_, expected_ty)) =
                        expected_fields.iter().find(|(n, _)| n == &f.name)
                    {
                        let type_info = self.type_table.get(&ty_name).unwrap();
                        let is_generic_param = matches!(expected_ty, IrType::Named { name: n, args } if args.is_empty() && type_info.generics.contains(n));

                        if !is_generic_param && !self.ty_compatible(&value.ty, expected_ty) {
                            self.emit(
                                "E-IR-003",
                                format!(
                                    "field `{}` type mismatch: expected {:?}, got {:?}",
                                    f.name, expected_ty, value.ty
                                ),
                                f.span,
                            );
                        }
                    }

                    lowered_fields.push(crate::IrRecordField {
                        name: f.name.clone(),
                        value,
                    });
                }

                let type_info = self.type_table.get(&ty_name).unwrap();
                Some(IrExpr {
                    kind: IrExprKind::Record {
                        name: name.clone(),
                        fields: lowered_fields,
                    },
                    ty: IrType::Named {
                        name: ty_name,
                        args: vec![IrType::Unknown; type_info.generics.len()],
                    },
                    span: loc(expr.span),
                })
            }
            ExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.lower_expr(lhs, env)?;
                let rhs = self.lower_expr(rhs, env)?;
                if !self.ty_compatible(&lhs.ty, &IrType::Int)
                    || !self.ty_compatible(&rhs.ty, &IrType::Int)
                {
                    self.emit(
                        "E-IR-003",
                        format!("binary operator `{:?}` requires Int operands", op),
                        expr.span,
                    );
                }
                Some(IrExpr {
                    kind: IrExprKind::Binary {
                        lhs: Box::new(lhs),
                        op: *op,
                        rhs: Box::new(rhs),
                    },
                    ty: IrType::Int,
                    span: loc(expr.span),
                })
            }
            ExprKind::Call { callee, args } => {
                let ExprKind::Ident(name) = &callee.kind else {
                    self.emit(
                        "E-IR-001",
                        "only named function calls are lowerable in MVP".to_string(),
                        expr.span,
                    );
                    return None;
                };

                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg, env))
                    .collect::<Option<Vec<_>>>()?;

                let Some(sig) = self.fn_table.get(name).cloned() else {
                    self.emit(
                        "E-IR-002",
                        format!("unknown function `{name}` during lowering"),
                        expr.span,
                    );
                    return Some(IrExpr {
                        kind: IrExprKind::Call {
                            callee: name.clone(),
                            args,
                        },
                        ty: IrType::Unknown,
                        span: loc(expr.span),
                    });
                };

                if args.len() != sig.params.len() {
                    self.emit(
                        "E-IR-003",
                        format!(
                            "call `{name}` expects {} arguments, got {}",
                            sig.params.len(),
                            args.len()
                        ),
                        expr.span,
                    );
                }

                for (index, arg) in args.iter().enumerate() {
                    if let Some(expected) = sig.params.get(index) {
                        if !self.ty_compatible(&arg.ty, expected) {
                            self.emit(
                                "E-IR-003",
                                format!(
                                    "call `{name}` argument {} expected {:?}, got {:?}",
                                    index + 1,
                                    expected,
                                    arg.ty
                                ),
                                expr.span,
                            );
                        }
                    }
                }

                Some(IrExpr {
                    kind: IrExprKind::Call {
                        callee: name.clone(),
                        args,
                    },
                    ty: sig.return_type,
                    span: loc(expr.span),
                })
            }
            ExprKind::Block(block) => self.lower_block_expr(block, env),
            ExprKind::Match(m) => self.lower_match_expr(m, env),
        }
    }

    fn lower_match_expr(
        &mut self,
        match_expr: &MatchExpr,
        env: &mut HashMap<String, IrType>,
    ) -> Option<IrExpr> {
        let subject = self.lower_expr(&match_expr.subject, env)?;
        let mut arm_type: Option<IrType> = None;
        let mut arms = Vec::new();

        for arm in &match_expr.arms {
            if arm.guard.is_some() {
                self.emit(
                    "E-IR-001",
                    "match guards are not lowerable in MVP".to_string(),
                    arm.span,
                );
                return None;
            }

            let mut arm_env = env.clone();
            let lowered_pattern = self.lower_pattern(&arm.pattern, &subject.ty, &mut arm_env)?;
            let body = self.lower_expr(&arm.body, &mut arm_env)?;
            if let Some(prev) = &arm_type {
                if !self.ty_compatible(prev, &body.ty) {
                    self.emit(
                        "E-IR-003",
                        format!(
                            "match arm type mismatch: expected {:?}, got {:?}",
                            prev, body.ty
                        ),
                        arm.body.span,
                    );
                }
            } else {
                arm_type = Some(body.ty.clone());
            }

            arms.push(IrMatchArm {
                pattern: lowered_pattern,
                body,
                span: loc(arm.span),
            });
        }

        Some(IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(subject),
                arms,
            },
            ty: arm_type.unwrap_or(IrType::Unknown),
            span: loc(match_expr.span),
        })
    }

    fn lower_pattern(
        &mut self,
        pattern: &Pattern,
        expected_ty: &IrType,
        env: &mut HashMap<String, IrType>,
    ) -> Option<IrPattern> {
        match &pattern.kind {
            PatternKind::Wildcard => Some(IrPattern {
                kind: IrPatternKind::Wildcard,
                ty: expected_ty.clone(),
                span: loc(pattern.span),
            }),
            PatternKind::Variant { name, fields } => {
                let IrType::Named { name: ty_name, .. } = expected_ty else {
                    self.emit(
                        "E-IR-003",
                        "variant pattern requires enum/event/query subject".to_string(),
                        pattern.span,
                    );
                    return None;
                };

                let Some(type_info) = self.type_table.get(ty_name).cloned() else {
                    self.emit(
                        "E-IR-002",
                        format!("unknown type `{ty_name}` during lowering"),
                        pattern.span,
                    );
                    return None;
                };

                if !matches!(
                    type_info.kind,
                    TypeDeclKindTag::Enum | TypeDeclKindTag::Event | TypeDeclKindTag::Query
                ) {
                    self.emit(
                        "E-IR-003",
                        format!("type `{ty_name}` does not support variant patterns"),
                        pattern.span,
                    );
                    return None;
                }

                let Some(payload) = type_info.variants.get(name).cloned() else {
                    self.emit(
                        "E-IR-002",
                        format!("unknown variant `{name}` for type `{ty_name}`"),
                        pattern.span,
                    );
                    return None;
                };

                let fields = self.lower_pattern_fields(fields, &payload, env)?;
                Some(IrPattern {
                    kind: IrPatternKind::Variant {
                        name: name.clone(),
                        fields,
                    },
                    ty: expected_ty.clone(),
                    span: loc(pattern.span),
                })
            }
        }
    }

    fn lower_pattern_fields(
        &mut self,
        fields: &[PatternField],
        payload: &[(String, IrType)],
        env: &mut HashMap<String, IrType>,
    ) -> Option<Vec<IrPatternField>> {
        let mut lowered = Vec::new();

        for field in fields {
            let Some((_, field_ty)) = payload.iter().find(|(name, _)| name == &field.name) else {
                self.emit(
                    "E-IR-002",
                    format!("unknown payload field `{}`", field.name),
                    field.span,
                );
                return None;
            };

            let pattern = if let Some(nested) = &field.pattern {
                self.lower_pattern(nested, field_ty, env)?
            } else {
                env.insert(field.name.clone(), field_ty.clone());
                IrPattern {
                    kind: IrPatternKind::Binding {
                        name: field.name.clone(),
                    },
                    ty: field_ty.clone(),
                    span: loc(field.span),
                }
            };

            lowered.push(IrPatternField {
                name: field.name.clone(),
                pattern,
                span: loc(field.span),
            });
        }

        Some(lowered)
    }

    fn ty_from_ast(&self, ty: &TypeExpr) -> IrType {
        match &ty.kind {
            TypeExprKind::Tuple { items } => {
                IrType::Tuple(items.iter().map(|item| self.ty_from_ast(item)).collect())
            }
            TypeExprKind::Named { name, generics } => {
                if generics.is_empty() {
                    match name.as_str() {
                        "Int" => IrType::Int,
                        "Bool" => IrType::Bool,
                        "Decimal" => IrType::Decimal,
                        "String" => IrType::String,
                        _ => IrType::Named {
                            name: name.clone(),
                            args: Vec::new(),
                        },
                    }
                } else {
                    IrType::Named {
                        name: name.clone(),
                        args: generics.iter().map(|g| self.ty_from_ast(g)).collect(),
                    }
                }
            }
        }
    }

    fn ty_compatible(&self, a: &IrType, b: &IrType) -> bool {
        if a == b || matches!(a, IrType::Unknown) || matches!(b, IrType::Unknown) {
            return true;
        }

        match (a, b) {
            (IrType::Named { name: na, args: aa }, IrType::Named { name: nb, args: ab }) => {
                if na != nb || aa.len() != ab.len() {
                    return false;
                }
                aa.iter()
                    .zip(ab.iter())
                    .all(|(l, r)| self.ty_compatible(l, r))
            }
            (IrType::Tuple(la), IrType::Tuple(lb)) => {
                if la.len() != lb.len() {
                    return false;
                }
                la.iter()
                    .zip(lb.iter())
                    .all(|(l, r)| self.ty_compatible(l, r))
            }
            _ => false,
        }
    }

    fn resolve_builtin(&self, name: &str) -> Option<IrType> {
        match name {
            "true" | "false" => Some(IrType::Bool),
            _ => None,
        }
    }

    fn emit(&mut self, code: &'static str, message: String, span: Span) {
        self.diagnostics.push(Diagnostic {
            code,
            message,
            line: span.start.line,
            column: span.start.column,
        });
    }
}

fn loc(span: Span) -> SourceLoc {
    SourceLoc {
        line: span.start.line,
        column: span.start.column,
    }
}

pub fn lower_program(program: &Program) -> LoweringResult {
    Lowerer::new().lower(program)
}
