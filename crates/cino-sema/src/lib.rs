#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

use cino_syntax::{
    BlockExpr, Expr, ExprKind, FnDecl, FnKind, MatchExpr, Pattern, PatternField, PatternKind,
    Program, Stmt, TopDecl, TypeDeclKind, TypeExpr, TypeExprKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

pub fn analyze(program: &Program) -> Analysis {
    let mut checker = Checker::new();
    checker.collect_decls(program);
    checker.check_program(program);
    Analysis {
        diagnostics: checker.diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    Int,
    Bool,
    Decimal,
    String,
    Named { name: String, args: Vec<Ty> },
    Tuple(Vec<Ty>),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    fields: Vec<(String, Ty)>,
    variants: HashMap<String, Vec<(String, Ty)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FnInfo {
    kind: FnKind,
    params: Vec<Ty>,
    return_ty: Ty,
}

struct Checker {
    diagnostics: Vec<Diagnostic>,
    fn_table: HashMap<String, FnInfo>,
    type_table: HashMap<String, TypeInfo>,
    variant_to_type: HashMap<String, String>,
}

impl Checker {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            fn_table: HashMap::new(),
            type_table: HashMap::new(),
            variant_to_type: HashMap::new(),
        }
    }

    fn collect_decls(&mut self, program: &Program) {
        for decl in &program.decls {
            match decl {
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
                TopDecl::Function(fd) => {
                    let params = fd
                        .params
                        .iter()
                        .map(|p| self.ty_from_ast(&p.type_expr))
                        .collect::<Vec<_>>();
                    let return_ty = self.ty_from_ast(&fd.return_type);
                    self.fn_table.insert(
                        fd.name.clone(),
                        FnInfo {
                            kind: fd.kind,
                            params,
                            return_ty,
                        },
                    );
                }
            }
        }
    }

    fn collect_fields<'a, I>(&self, fields: I) -> Vec<(String, Ty)>
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
    ) -> HashMap<String, Vec<(String, Ty)>>
    where
        I: Iterator<Item = &'a cino_syntax::VariantDecl>,
    {
        let mut out = HashMap::new();
        for v in variants {
            let payload = v
                .payload
                .iter()
                .map(|f| (f.name.clone(), self.ty_from_ast(&f.type_expr)))
                .collect::<Vec<_>>();
            self.variant_to_type
                .insert(v.name.clone(), type_name.to_string());
            out.insert(v.name.clone(), payload);
        }
        out
    }

    fn check_program(&mut self, program: &Program) {
        for decl in &program.decls {
            if let TopDecl::Function(fd) = decl {
                self.check_fn(fd);
            }
        }
    }

    fn check_fn(&mut self, fd: &FnDecl) {
        self.check_fn_signature(fd);

        let mut env = HashMap::new();
        for p in &fd.params {
            env.insert(p.name.clone(), self.ty_from_ast(&p.type_expr));
        }

        let body_ty = self.check_block(&fd.body, &mut env, fd.kind);
        let declared = self.ty_from_ast(&fd.return_type);
        if !self.ty_compatible(&body_ty, &declared) {
            self.emit(
                "E-TYPE-001",
                format!(
                    "function `{}` returns {:?} but declared {:?}",
                    fd.name, body_ty, declared
                ),
                fd.return_type.span.start.line,
                fd.return_type.span.start.column,
            );
        }
    }

    fn check_fn_signature(&mut self, fd: &FnDecl) {
        match fd.kind {
            FnKind::User => {}
            FnKind::Update => {
                if fd.params.len() != 2 {
                    self.emit(
                        "E-TYPE-004",
                        "`update` must take exactly 2 parameters".to_string(),
                        fd.span.start.line,
                        fd.span.start.column,
                    );
                    return;
                }

                let state_ty = self.ty_from_ast(&fd.params[0].type_expr);
                let event_ty = self.ty_from_ast(&fd.params[1].type_expr);
                if !self.is_event_type(&event_ty) {
                    self.emit(
                        "E-TYPE-004",
                        "`update` second parameter must be an `event` type".to_string(),
                        fd.params[1].span.start.line,
                        fd.params[1].span.start.column,
                    );
                }

                let ret = self.ty_from_ast(&fd.return_type);
                match ret {
                    Ty::Tuple(items) if items.len() == 2 => {
                        if !self.ty_compatible(&items[0], &state_ty) {
                            self.emit(
                                "E-TYPE-004",
                                "`update` first return tuple item must equal state type"
                                    .to_string(),
                                fd.return_type.span.start.line,
                                fd.return_type.span.start.column,
                            );
                        }
                        if !self.is_list_action(&items[1]) {
                            self.emit(
                                "E-TYPE-004",
                                "`update` second return tuple item must be List<Action>"
                                    .to_string(),
                                fd.return_type.span.start.line,
                                fd.return_type.span.start.column,
                            );
                        }
                    }
                    _ => self.emit(
                        "E-TYPE-004",
                        "`update` return type must be (State, List<Action>)".to_string(),
                        fd.return_type.span.start.line,
                        fd.return_type.span.start.column,
                    ),
                }
            }
            FnKind::Query => {
                if fd.params.len() != 2 {
                    self.emit(
                        "E-TYPE-004",
                        "`query` must take exactly 2 parameters".to_string(),
                        fd.span.start.line,
                        fd.span.start.column,
                    );
                    return;
                }

                let q_ty = self.ty_from_ast(&fd.params[1].type_expr);
                if !self.is_query_type(&q_ty) {
                    self.emit(
                        "E-TYPE-004",
                        "`query` second parameter must be a `query` type".to_string(),
                        fd.params[1].span.start.line,
                        fd.params[1].span.start.column,
                    );
                }

                let ret = self.ty_from_ast(&fd.return_type);
                if !self.is_result(&ret) {
                    self.emit(
                        "E-TYPE-004",
                        "`query` return type must be Result<R, E>".to_string(),
                        fd.return_type.span.start.line,
                        fd.return_type.span.start.column,
                    );
                }
            }
        }
    }

    fn check_block(
        &mut self,
        block: &BlockExpr,
        env: &mut HashMap<String, Ty>,
        fn_kind: FnKind,
    ) -> Ty {
        let mut scoped = env.clone();

        for stmt in &block.statements {
            match stmt {
                Stmt::Let(let_stmt) => {
                    let ty = self.infer_expr(&let_stmt.value, &mut scoped, fn_kind);
                    scoped.insert(let_stmt.name.clone(), ty);
                }
                Stmt::Return(ret) => {
                    self.emit(
                        "E-FN-003",
                        "`return` statement is not supported in MVP".to_string(),
                        ret.span.start.line,
                        ret.span.start.column,
                    );
                    let _ = self.infer_expr(&ret.value, &mut scoped, fn_kind);
                }
            }
        }

        match &block.tail {
            Some(tail) => self.infer_expr(tail, &mut scoped, fn_kind),
            None => Ty::Unknown,
        }
    }

    fn infer_expr(&mut self, expr: &Expr, env: &mut HashMap<String, Ty>, fn_kind: FnKind) -> Ty {
        match &expr.kind {
            ExprKind::Ident(name) => env
                .get(name)
                .cloned()
                .or_else(|| self.resolve_builtin(name))
                .unwrap_or_else(|| {
                    self.emit(
                        "E-TYPE-002",
                        format!("unresolved symbol `{name}`"),
                        expr.span.start.line,
                        expr.span.start.column,
                    );
                    Ty::Unknown
                }),
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|item| self.infer_expr(item, env, fn_kind))
                    .collect::<Vec<_>>(),
            ),
            ExprKind::List(items) => {
                let mut item_ty = Ty::Unknown;
                for item in items {
                    let ty = self.infer_expr(item, env, fn_kind);
                    if item_ty == Ty::Unknown {
                        item_ty = ty;
                    } else if !self.ty_compatible(&item_ty, &ty) {
                        self.emit(
                            "E-TYPE-001",
                            format!(
                                "list items must have same type, found {:?} and {:?}",
                                item_ty, ty
                            ),
                            item.span.start.line,
                            item.span.start.column,
                        );
                    }
                }
                Ty::Named {
                    name: "List".to_string(),
                    args: vec![item_ty],
                }
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
                        "E-TYPE-002",
                        format!("unresolved type or variant `{name}`"),
                        expr.span.start.line,
                        expr.span.start.column,
                    );
                    return Ty::Unknown;
                };

                for f in fields {
                    let val_ty = self.infer_expr(&f.value, env, fn_kind);
                    if let Some((_, expected_ty)) =
                        expected_fields.iter().find(|(n, _)| n == &f.name)
                    {
                        let type_info = self.type_table.get(&ty_name).unwrap();
                        let is_generic_param = matches!(expected_ty, Ty::Named { name: n, args } if args.is_empty() && type_info.generics.contains(n));

                        if !is_generic_param && !self.ty_compatible(&val_ty, expected_ty) {
                            self.emit(
                                "E-TYPE-001",
                                format!(
                                    "field `{}` type mismatch: expected {:?}, got {:?}",
                                    f.name, expected_ty, val_ty
                                ),
                                f.span.start.line,
                                f.span.start.column,
                            );
                        }
                    } else {
                        self.emit(
                            "E-TYPE-002",
                            format!("unknown field `{}` for `{name}`", f.name),
                            f.span.start.line,
                            f.span.start.column,
                        );
                    }
                }

                let type_info = self.type_table.get(&ty_name).unwrap();
                Ty::Named {
                    name: ty_name,
                    args: vec![Ty::Unknown; type_info.generics.len()],
                }
            }
            ExprKind::Binary { lhs, op, rhs } => {
                let lhs_ty = self.infer_expr(lhs, env, fn_kind);
                let rhs_ty = self.infer_expr(rhs, env, fn_kind);
                if !self.ty_compatible(&lhs_ty, &Ty::Int) || !self.ty_compatible(&rhs_ty, &Ty::Int)
                {
                    self.emit(
                        "E-TYPE-001",
                        format!("binary operator `{:?}` requires Int operands", op),
                        expr.span.start.line,
                        expr.span.start.column,
                    );
                }
                Ty::Int
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(name) = &callee.kind {
                    if self.is_forbidden_side_effect(name) {
                        self.emit_purity_violation(
                            "E-PURE-001",
                            format!("forbidden side-effect function `{name}`"),
                            expr.span.start.line,
                            expr.span.start.column,
                            fn_kind,
                        );
                        for arg in args {
                            let _ = self.infer_expr(arg, env, fn_kind);
                        }
                        return Ty::Unknown;
                    }

                    let fn_info = self.fn_table.get(name).cloned();
                    let Some(info) = fn_info else {
                        self.emit_purity_violation(
                            "E-PURE-002",
                            format!("external function call `{name}` is not allowed"),
                            expr.span.start.line,
                            expr.span.start.column,
                            fn_kind,
                        );
                        for arg in args {
                            let _ = self.infer_expr(arg, env, fn_kind);
                        }
                        return Ty::Unknown;
                    };

                    if args.len() != info.params.len() {
                        self.emit(
                            "E-TYPE-001",
                            format!(
                                "function `{name}` expects {} arguments, got {}",
                                info.params.len(),
                                args.len()
                            ),
                            expr.span.start.line,
                            expr.span.start.column,
                        );
                    }

                    for (idx, arg) in args.iter().enumerate() {
                        let arg_ty = self.infer_expr(arg, env, fn_kind);
                        if let Some(expected) = info.params.get(idx) {
                            if !self.ty_compatible(&arg_ty, expected) {
                                self.emit(
                                    "E-TYPE-001",
                                    format!(
                                        "argument {} of `{name}` expects {:?}, got {:?}",
                                        idx + 1,
                                        expected,
                                        arg_ty
                                    ),
                                    arg.span.start.line,
                                    arg.span.start.column,
                                );
                            }
                        }
                    }

                    info.return_ty
                } else {
                    self.emit_purity_violation(
                        "E-PURE-002",
                        "only named pure function calls are allowed".to_string(),
                        expr.span.start.line,
                        expr.span.start.column,
                        fn_kind,
                    );
                    Ty::Unknown
                }
            }
            ExprKind::Block(block) => self.check_block(block, env, fn_kind),
            ExprKind::Match(match_expr) => self.infer_match(match_expr, env, fn_kind),
        }
    }

    fn infer_match(
        &mut self,
        match_expr: &MatchExpr,
        env: &mut HashMap<String, Ty>,
        fn_kind: FnKind,
    ) -> Ty {
        let subject_ty = self.infer_expr(&match_expr.subject, env, fn_kind);
        let enum_name = match &subject_ty {
            Ty::Named { name, .. } => Some(name.clone()),
            _ => None,
        };

        let mut all_variants = HashSet::new();
        if let Some(name) = &enum_name {
            if let Some(info) = self.type_table.get(name) {
                if matches!(
                    info.kind,
                    TypeDeclKindTag::Enum | TypeDeclKindTag::Event | TypeDeclKindTag::Query
                ) {
                    all_variants.extend(info.variants.keys().cloned());
                }
            }
        }

        let mut covered = HashSet::new();
        let mut wildcard_seen = false;
        let mut arm_ty: Option<Ty> = None;

        for arm in &match_expr.arms {
            if arm.guard.is_some() {
                self.emit(
                    "E-MATCH-003",
                    "match guards are not supported in MVP".to_string(),
                    arm.span.start.line,
                    arm.span.start.column,
                );
            }

            if wildcard_seen {
                self.emit(
                    "E-MATCH-002",
                    "unreachable match arm".to_string(),
                    arm.span.start.line,
                    arm.span.start.column,
                );
            }

            let mut arm_env = env.clone();
            let arm_variant = self.check_pattern(&arm.pattern, &subject_ty, &mut arm_env);

            if let Some(name) = arm_variant {
                if covered.contains(&name) {
                    self.emit(
                        "E-MATCH-002",
                        format!("unreachable match arm for variant `{name}`"),
                        arm.span.start.line,
                        arm.span.start.column,
                    );
                } else {
                    covered.insert(name);
                }
            } else if matches!(arm.pattern.kind, PatternKind::Wildcard) {
                wildcard_seen = true;
            }

            let current_ty = self.infer_expr(&arm.body, &mut arm_env, fn_kind);
            if let Some(prev) = &arm_ty {
                if !self.ty_compatible(prev, &current_ty) {
                    self.emit(
                        "E-TYPE-001",
                        format!(
                            "match arm type mismatch: expected {:?}, got {:?}",
                            prev, current_ty
                        ),
                        arm.body.span.start.line,
                        arm.body.span.start.column,
                    );
                }
            } else {
                arm_ty = Some(current_ty);
            }
        }

        if !all_variants.is_empty() && !wildcard_seen {
            let missing = all_variants
                .difference(&covered)
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.emit(
                    "E-MATCH-001",
                    format!(
                        "non-exhaustive match, missing variants: {}",
                        missing.join(", ")
                    ),
                    match_expr.span.start.line,
                    match_expr.span.start.column,
                );
            }
        }

        arm_ty.unwrap_or(Ty::Unknown)
    }

    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        subject_ty: &Ty,
        env: &mut HashMap<String, Ty>,
    ) -> Option<String> {
        match &pattern.kind {
            PatternKind::Wildcard => None,
            PatternKind::Variant { name, fields } => {
                let Ty::Named {
                    name: ty_name,
                    args: _,
                } = subject_ty
                else {
                    self.emit(
                        "E-TYPE-001",
                        "variant pattern requires enum/event/query subject".to_string(),
                        pattern.span.start.line,
                        pattern.span.start.column,
                    );
                    return None;
                };

                let Some(info) = self.type_table.get(ty_name).cloned() else {
                    self.emit(
                        "E-TYPE-002",
                        format!("unresolved type `{ty_name}`"),
                        pattern.span.start.line,
                        pattern.span.start.column,
                    );
                    return None;
                };

                let Some(payload) = info.variants.get(name).cloned() else {
                    self.emit(
                        "E-TYPE-002",
                        format!("unknown variant `{name}` for type `{ty_name}`"),
                        pattern.span.start.line,
                        pattern.span.start.column,
                    );
                    return None;
                };

                self.bind_pattern_fields(fields, &payload, env);
                Some(name.clone())
            }
        }
    }

    fn bind_pattern_fields(
        &mut self,
        fields: &[PatternField],
        payload: &[(String, Ty)],
        env: &mut HashMap<String, Ty>,
    ) {
        for f in fields {
            let Some((_, field_ty)) = payload.iter().find(|(name, _)| name == &f.name) else {
                self.emit(
                    "E-TYPE-002",
                    format!("unknown payload field `{}`", f.name),
                    f.span.start.line,
                    f.span.start.column,
                );
                continue;
            };

            if let Some(nested) = &f.pattern {
                let _ = self.check_pattern(nested, field_ty, env);
            } else {
                env.insert(f.name.clone(), field_ty.clone());
            }
        }
    }

    fn ty_from_ast(&self, ty: &TypeExpr) -> Ty {
        match &ty.kind {
            TypeExprKind::Tuple { items } => {
                Ty::Tuple(items.iter().map(|item| self.ty_from_ast(item)).collect())
            }
            TypeExprKind::Named { name, generics } => {
                if generics.is_empty() {
                    match name.as_str() {
                        "Int" => Ty::Int,
                        "Bool" => Ty::Bool,
                        "Decimal" => Ty::Decimal,
                        "String" => Ty::String,
                        _ => Ty::Named {
                            name: name.clone(),
                            args: Vec::new(),
                        },
                    }
                } else {
                    Ty::Named {
                        name: name.clone(),
                        args: generics.iter().map(|g| self.ty_from_ast(g)).collect(),
                    }
                }
            }
        }
    }

    fn ty_compatible(&self, a: &Ty, b: &Ty) -> bool {
        if a == b || matches!(a, Ty::Unknown) || matches!(b, Ty::Unknown) {
            return true;
        }

        match (a, b) {
            (Ty::Named { name: na, args: aa }, Ty::Named { name: nb, args: ab }) => {
                if na != nb || aa.len() != ab.len() {
                    return false;
                }
                aa.iter()
                    .zip(ab.iter())
                    .all(|(l, r)| self.ty_compatible(l, r))
            }
            (Ty::Tuple(la), Ty::Tuple(lb)) => {
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

    fn resolve_builtin(&self, name: &str) -> Option<Ty> {
        match name {
            "true" | "false" => Some(Ty::Bool),
            _ => None,
        }
    }

    fn is_forbidden_side_effect(&self, name: &str) -> bool {
        matches!(
            name,
            "now" | "rand" | "random" | "clock" | "io" | "http_get"
        )
    }

    fn is_event_type(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Named { name, .. } => self
                .type_table
                .get(name)
                .is_some_and(|t| t.kind == TypeDeclKindTag::Event),
            _ => false,
        }
    }

    fn is_query_type(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Named { name, .. } => self
                .type_table
                .get(name)
                .is_some_and(|t| t.kind == TypeDeclKindTag::Query),
            _ => false,
        }
    }

    fn is_list_action(&self, ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Named { name, args }
                if name == "List"
                    && args.len() == 1
                    && matches!(&args[0], Ty::Named { name, .. } if name == "Action")
        )
    }

    fn is_result(&self, ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Named { name, args } if name == "Result" && args.len() == 2
        )
    }

    fn emit_purity_violation(
        &mut self,
        code: &'static str,
        message: String,
        line: usize,
        column: usize,
        fn_kind: FnKind,
    ) {
        self.emit(code, message.clone(), line, column);
        if fn_kind == FnKind::User {
            self.emit("E-FN-002", message, line, column);
        }
    }

    fn emit(&mut self, code: &'static str, message: String, line: usize, column: usize) {
        self.diagnostics.push(Diagnostic {
            code,
            message,
            line,
            column,
        });
    }
}

#[cfg(test)]
mod tests {
    use cino_syntax::parse_program;

    use crate::analyze;

    #[test]
    fn reports_forbidden_side_effect_and_fn_violation() {
        let src = r#"
fn side(n: Int) -> Int {
  now(n)
}
"#;

        let program = parse_program(src).expect("parse");
        let result = analyze(&program);
        assert!(result.diagnostics.iter().any(|d| d.code == "E-PURE-001"));
        assert!(result.diagnostics.iter().any(|d| d.code == "E-FN-002"));
    }

    #[test]
    fn reports_return_statement_usage() {
        let src = r#"
fn bad_return(n: Int) -> Int {
  return n
}
"#;

        let program = parse_program(src).expect("parse");
        let result = analyze(&program);
        assert!(result.diagnostics.iter().any(|d| d.code == "E-FN-003"));
    }

    #[test]
    fn reports_match_non_exhaustive_and_unreachable() {
        let src = r#"
enum E =
  | A { x: Int }
  | B { x: Int }

fn bad_match(e: E) -> Int {
  match e {
    A { x } => x
    A { x } => x + 1
  }
}

fn unreachable_arm(e: E) -> Int {
  match e {
    _ => 0
    B { x } => x
  }
}
"#;

        let program = parse_program(src).expect("parse");
        let result = analyze(&program);
        assert!(result.diagnostics.iter().any(|d| d.code == "E-MATCH-001"));
        assert!(result.diagnostics.iter().any(|d| d.code == "E-MATCH-002"));
    }

    #[test]
    fn reports_update_signature_error() {
        let src = r#"
state S { value: Int }
record NotEvent { value: Int }

update(state: S, event: NotEvent) -> S {
  state
}
"#;

        let program = parse_program(src).expect("parse");
        let result = analyze(&program);
        assert!(result.diagnostics.iter().any(|d| d.code == "E-TYPE-004"));
    }

    #[test]
    fn reports_match_guard_unsupported() {
        let src = r#"
enum E =
  | A

fn guarded(e: E) -> Int {
  match e {
    A if true => 1
  }
}
"#;

        let program = parse_program(src).expect("parse");
        let result = analyze(&program);
        assert!(result.diagnostics.iter().any(|d| d.code == "E-MATCH-003"));
    }
}
