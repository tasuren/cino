#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};

use cino_ir::{IrExpr, IrExprKind, IrFunction, IrPattern, IrPatternKind, IrProgram};
use cino_syntax::{BinaryOp, FnKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmValue {
    Unit,
    Int(i64),
    Bool(bool),
    String(String),
    List(Vec<VmValue>),
    Tuple(Vec<VmValue>),
    Map(BTreeMap<String, VmValue>),
    Enum {
        tag: String,
        fields: BTreeMap<String, VmValue>,
    },
}

pub type VmState = VmValue;
pub type VmAction = VmValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLimits {
    pub max_steps: u64,
    pub max_memory_bytes: usize,
    pub max_recursion_depth: u32,
}

impl Default for VmLimits {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_memory_bytes: 64 * 1024 * 1024,
            max_recursion_depth: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmErrorCode {
    StepLimitExceeded,
    MemoryLimitExceeded,
    RecursionLimitExceeded,
    InvalidInput,
    Trap,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmError {
    pub code: VmErrorCode,
    pub message: String,
}

impl VmError {
    pub fn new(code: VmErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for VmError {}

pub trait VmProgram: Send + Sync {
    fn update(
        &self,
        state: &VmState,
        event: &VmValue,
        limits: &VmLimits,
    ) -> Result<(VmState, Vec<VmAction>), VmError>;

    fn query(
        &self,
        state: &VmState,
        query: &VmValue,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError>;
}

pub type UpdateFn = dyn Fn(&VmState, &VmValue, &VmLimits) -> Result<(VmState, Vec<VmAction>), VmError>
    + Send
    + Sync;
pub type QueryFn = dyn Fn(&VmState, &VmValue, &VmLimits) -> Result<VmValue, VmError> + Send + Sync;

pub struct NativeProgram {
    update_fn: Arc<UpdateFn>,
    query_fn: Arc<QueryFn>,
}

impl NativeProgram {
    pub fn new(update_fn: Arc<UpdateFn>, query_fn: Arc<QueryFn>) -> Self {
        Self {
            update_fn,
            query_fn,
        }
    }
}

impl VmProgram for NativeProgram {
    fn update(
        &self,
        state: &VmState,
        event: &VmValue,
        limits: &VmLimits,
    ) -> Result<(VmState, Vec<VmAction>), VmError> {
        (self.update_fn)(state, event, limits)
    }

    fn query(
        &self,
        state: &VmState,
        query: &VmValue,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError> {
        (self.query_fn)(state, query, limits)
    }
}

pub struct IrVmProgram {
    functions: HashMap<String, IrFunction>,
    update_function: String,
    query_function: String,
}

impl IrVmProgram {
    pub fn from_ir(program: IrProgram) -> Result<Self, VmError> {
        let mut functions = HashMap::new();
        let mut update_function = None;
        let mut query_function = None;

        for function in program.functions {
            if functions.contains_key(&function.name) {
                return Err(VmError::new(
                    VmErrorCode::InvalidInput,
                    format!("duplicate function `{}` in IR program", function.name),
                ));
            }

            match function.kind {
                FnKind::Update => {
                    if update_function.is_some() {
                        return Err(VmError::new(
                            VmErrorCode::InvalidInput,
                            "IR program must contain exactly one update function",
                        ));
                    }
                    update_function = Some(function.name.clone());
                }
                FnKind::Query => {
                    if query_function.is_some() {
                        return Err(VmError::new(
                            VmErrorCode::InvalidInput,
                            "IR program must contain exactly one query function",
                        ));
                    }
                    query_function = Some(function.name.clone());
                }
                FnKind::User => {}
            }

            functions.insert(function.name.clone(), function);
        }

        Ok(Self {
            functions,
            update_function: update_function.ok_or_else(|| {
                VmError::new(
                    VmErrorCode::InvalidInput,
                    "IR program is missing update function",
                )
            })?,
            query_function: query_function.ok_or_else(|| {
                VmError::new(
                    VmErrorCode::InvalidInput,
                    "IR program is missing query function",
                )
            })?,
        })
    }

    fn execute_entry(
        &self,
        entry_name: &str,
        args: Vec<VmValue>,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError> {
        wrap_panic(|| {
            let mut evaluator = Evaluator::new(self, limits);
            evaluator.call_function(entry_name, args)
        })
    }
}

impl VmProgram for IrVmProgram {
    fn update(
        &self,
        state: &VmState,
        event: &VmValue,
        limits: &VmLimits,
    ) -> Result<(VmState, Vec<VmAction>), VmError> {
        let result = self.execute_entry(
            &self.update_function,
            vec![state.clone(), event.clone()],
            limits,
        )?;

        let VmValue::Tuple(items) = result else {
            return Err(VmError::new(
                VmErrorCode::Trap,
                "update must return a tuple (state, actions)",
            ));
        };

        let [next_state, actions_value]: [VmValue; 2] = items.try_into().map_err(|_| {
            VmError::new(
                VmErrorCode::Trap,
                "update must return exactly 2 tuple items",
            )
        })?;

        let VmValue::List(actions) = actions_value else {
            return Err(VmError::new(
                VmErrorCode::Trap,
                "update second tuple item must be List<Action>",
            ));
        };

        Ok((next_state, actions))
    }

    fn query(
        &self,
        state: &VmState,
        query: &VmValue,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError> {
        self.execute_entry(
            &self.query_function,
            vec![state.clone(), query.clone()],
            limits,
        )
    }
}

struct Evaluator<'a> {
    program: &'a IrVmProgram,
    limits: &'a VmLimits,
    remaining_steps: u64,
    current_depth: u32,
    memory_bytes: usize,
}

impl<'a> Evaluator<'a> {
    fn new(program: &'a IrVmProgram, limits: &'a VmLimits) -> Self {
        Self {
            program,
            limits,
            remaining_steps: limits.max_steps,
            current_depth: 0,
            memory_bytes: 0,
        }
    }

    fn call_function(&mut self, name: &str, args: Vec<VmValue>) -> Result<VmValue, VmError> {
        self.consume_step()?;

        if self.current_depth >= self.limits.max_recursion_depth {
            return Err(VmError::new(
                VmErrorCode::RecursionLimitExceeded,
                format!("max recursion depth exceeded while calling `{name}`"),
            ));
        }

        let function =
            self.program.functions.get(name).cloned().ok_or_else(|| {
                VmError::new(VmErrorCode::Trap, format!("unknown function `{name}`"))
            })?;

        if args.len() != function.params.len() {
            return Err(VmError::new(
                VmErrorCode::Trap,
                format!(
                    "arity mismatch for `{name}`: expected {}, got {}",
                    function.params.len(),
                    args.len()
                ),
            ));
        }

        self.current_depth += 1;
        let result = {
            let mut env = BTreeMap::new();
            for (param, arg) in function.params.iter().zip(args.into_iter()) {
                env.insert(param.name.clone(), arg);
            }
            self.eval_expr(&function.body, &mut env)
        };
        self.current_depth -= 1;
        result
    }

    fn eval_expr(
        &mut self,
        expr: &IrExpr,
        env: &mut BTreeMap<String, VmValue>,
    ) -> Result<VmValue, VmError> {
        self.consume_step()?;

        let value = match &expr.kind {
            IrExprKind::LocalRef { name } => env.get(name).cloned().ok_or_else(|| {
                VmError::new(
                    VmErrorCode::Trap,
                    format!("unknown local reference `{name}`"),
                )
            })?,
            IrExprKind::Int(v) => VmValue::Int(*v),
            IrExprKind::Bool(v) => VmValue::Bool(*v),
            IrExprKind::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr(item, env)?);
                }
                VmValue::Tuple(out)
            }
            IrExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.eval_expr(lhs, env)?;
                let rhs = self.eval_expr(rhs, env)?;
                self.eval_binary(*op, lhs, rhs)?
            }
            IrExprKind::Call { callee, args } => {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.eval_expr(arg, env)?);
                }
                self.call_function(callee, evaluated_args)?
            }
            IrExprKind::Let { name, value, body } => {
                let evaluated = self.eval_expr(value, env)?;
                let previous = env.insert(name.clone(), evaluated);
                let result = self.eval_expr(body, env);
                if let Some(prev) = previous {
                    env.insert(name.clone(), prev);
                } else {
                    env.remove(name);
                }
                result?
            }
            IrExprKind::Match { subject, arms } => {
                let subject_value = self.eval_expr(subject, env)?;
                let mut matched = None;

                for arm in arms {
                    let mut arm_env = env.clone();
                    if self.pattern_matches(&arm.pattern, &subject_value, &mut arm_env)? {
                        matched = Some(self.eval_expr(&arm.body, &mut arm_env)?);
                        break;
                    }
                }

                matched.ok_or_else(|| {
                    VmError::new(VmErrorCode::Trap, "non-exhaustive match reached at runtime")
                })?
            }
        };

        self.charge_memory(&value)?;
        Ok(value)
    }

    fn pattern_matches(
        &mut self,
        pattern: &IrPattern,
        subject: &VmValue,
        env: &mut BTreeMap<String, VmValue>,
    ) -> Result<bool, VmError> {
        self.consume_step()?;

        match &pattern.kind {
            IrPatternKind::Wildcard => Ok(true),
            IrPatternKind::Binding { name } => {
                env.insert(name.clone(), subject.clone());
                Ok(true)
            }
            IrPatternKind::Variant { name, fields } => {
                let VmValue::Enum {
                    tag,
                    fields: payload,
                } = subject
                else {
                    return Err(VmError::new(
                        VmErrorCode::InvalidInput,
                        "variant pattern expects enum-like VM value",
                    ));
                };

                if tag != name {
                    return Ok(false);
                }

                for field in fields {
                    let value = payload.get(&field.name).ok_or_else(|| {
                        VmError::new(
                            VmErrorCode::InvalidInput,
                            format!("missing enum payload field `{}`", field.name),
                        )
                    })?;

                    if !self.pattern_matches(&field.pattern, value, env)? {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
        }
    }

    fn eval_binary(&self, op: BinaryOp, lhs: VmValue, rhs: VmValue) -> Result<VmValue, VmError> {
        let (VmValue::Int(lhs), VmValue::Int(rhs)) = (lhs, rhs) else {
            return Err(VmError::new(
                VmErrorCode::InvalidInput,
                "binary operations require Int operands",
            ));
        };

        let value = match op {
            BinaryOp::Add => lhs.checked_add(rhs),
            BinaryOp::Sub => lhs.checked_sub(rhs),
            BinaryOp::Mul => lhs.checked_mul(rhs),
            BinaryOp::Div => {
                if rhs == 0 {
                    return Err(VmError::new(VmErrorCode::Trap, "division by zero"));
                }
                lhs.checked_div(rhs)
            }
        }
        .ok_or_else(|| VmError::new(VmErrorCode::Trap, "integer arithmetic overflow"))?;

        Ok(VmValue::Int(value))
    }

    fn consume_step(&mut self) -> Result<(), VmError> {
        if self.remaining_steps == 0 {
            return Err(VmError::new(
                VmErrorCode::StepLimitExceeded,
                "step limit exceeded",
            ));
        }
        self.remaining_steps -= 1;
        Ok(())
    }

    fn charge_memory(&mut self, value: &VmValue) -> Result<(), VmError> {
        self.memory_bytes = self
            .memory_bytes
            .saturating_add(estimate_value_bytes(value));

        if self.memory_bytes > self.limits.max_memory_bytes {
            return Err(VmError::new(
                VmErrorCode::MemoryLimitExceeded,
                "memory budget exceeded",
            ));
        }

        Ok(())
    }
}

fn wrap_panic<T, F>(f: F) -> Result<T, VmError>
where
    F: FnOnce() -> Result<T, VmError>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(_) => Err(VmError::new(
            VmErrorCode::Panic,
            "vm converted panic to structured error",
        )),
    }
}

fn estimate_value_bytes(value: &VmValue) -> usize {
    const BASE: usize = 16;

    match value {
        VmValue::Unit => BASE,
        VmValue::Int(_) | VmValue::Bool(_) => BASE,
        VmValue::String(s) => BASE.saturating_add(s.len()),
        VmValue::List(items) | VmValue::Tuple(items) => items.iter().fold(BASE, |acc, item| {
            acc.saturating_add(estimate_value_bytes(item))
        }),
        VmValue::Map(entries) => entries.iter().fold(BASE, |acc, (k, v)| {
            acc.saturating_add(k.len())
                .saturating_add(estimate_value_bytes(v))
        }),
        VmValue::Enum { tag, fields } => {
            fields
                .iter()
                .fold(BASE.saturating_add(tag.len()), |acc, (k, v)| {
                    acc.saturating_add(k.len())
                        .saturating_add(estimate_value_bytes(v))
                })
        }
    }
}

/// VM crate entry point for the cino MVP workspace.
pub fn crate_name() -> &'static str {
    "cino-vm"
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use cino_ir::{
        IrExpr, IrExprKind, IrFunction, IrMatchArm, IrParam, IrPattern, IrPatternField,
        IrPatternKind, IrProgram, IrType, SourceLoc,
    };
    use cino_syntax::FnKind;

    use crate::{IrVmProgram, VmErrorCode, VmLimits, VmProgram, VmValue};

    fn loc() -> SourceLoc {
        SourceLoc { line: 1, column: 1 }
    }

    #[test]
    fn ir_vm_executes_minimum_update_and_query() {
        let update = IrFunction {
            kind: FnKind::Update,
            name: "update".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "event".to_string(),
                    ty: IrType::Named {
                        name: "Event".to_string(),
                        args: vec![],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Tuple(vec![
                IrType::Int,
                IrType::Named {
                    name: "List".to_string(),
                    args: vec![IrType::Named {
                        name: "Action".to_string(),
                        args: vec![],
                    }],
                },
            ]),
            body: IrExpr {
                kind: IrExprKind::Match {
                    subject: Box::new(IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "event".to_string(),
                        },
                        ty: IrType::Named {
                            name: "Event".to_string(),
                            args: vec![],
                        },
                        span: loc(),
                    }),
                    arms: vec![IrMatchArm {
                        pattern: IrPattern {
                            kind: IrPatternKind::Variant {
                                name: "Add".to_string(),
                                fields: vec![
                                    IrPatternField {
                                        name: "delta".to_string(),
                                        pattern: IrPattern {
                                            kind: IrPatternKind::Binding {
                                                name: "delta".to_string(),
                                            },
                                            ty: IrType::Int,
                                            span: loc(),
                                        },
                                        span: loc(),
                                    },
                                    IrPatternField {
                                        name: "actions".to_string(),
                                        pattern: IrPattern {
                                            kind: IrPatternKind::Binding {
                                                name: "actions".to_string(),
                                            },
                                            ty: IrType::Named {
                                                name: "List".to_string(),
                                                args: vec![IrType::Named {
                                                    name: "Action".to_string(),
                                                    args: vec![],
                                                }],
                                            },
                                            span: loc(),
                                        },
                                        span: loc(),
                                    },
                                ],
                            },
                            ty: IrType::Named {
                                name: "Event".to_string(),
                                args: vec![],
                            },
                            span: loc(),
                        },
                        body: IrExpr {
                            kind: IrExprKind::Tuple(vec![
                                IrExpr {
                                    kind: IrExprKind::Binary {
                                        lhs: Box::new(IrExpr {
                                            kind: IrExprKind::LocalRef {
                                                name: "state".to_string(),
                                            },
                                            ty: IrType::Int,
                                            span: loc(),
                                        }),
                                        op: cino_syntax::BinaryOp::Add,
                                        rhs: Box::new(IrExpr {
                                            kind: IrExprKind::LocalRef {
                                                name: "delta".to_string(),
                                            },
                                            ty: IrType::Int,
                                            span: loc(),
                                        }),
                                    },
                                    ty: IrType::Int,
                                    span: loc(),
                                },
                                IrExpr {
                                    kind: IrExprKind::LocalRef {
                                        name: "actions".to_string(),
                                    },
                                    ty: IrType::Named {
                                        name: "List".to_string(),
                                        args: vec![IrType::Named {
                                            name: "Action".to_string(),
                                            args: vec![],
                                        }],
                                    },
                                    span: loc(),
                                },
                            ]),
                            ty: IrType::Tuple(vec![
                                IrType::Int,
                                IrType::Named {
                                    name: "List".to_string(),
                                    args: vec![IrType::Named {
                                        name: "Action".to_string(),
                                        args: vec![],
                                    }],
                                },
                            ]),
                            span: loc(),
                        },
                        span: loc(),
                    }],
                },
                ty: IrType::Tuple(vec![
                    IrType::Int,
                    IrType::Named {
                        name: "List".to_string(),
                        args: vec![IrType::Named {
                            name: "Action".to_string(),
                            args: vec![],
                        }],
                    },
                ]),
                span: loc(),
            },
            span: loc(),
        };

        let query = IrFunction {
            kind: FnKind::Query,
            name: "query".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "q".to_string(),
                    ty: IrType::Named {
                        name: "Query".to_string(),
                        args: vec![],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Int,
            body: IrExpr {
                kind: IrExprKind::LocalRef {
                    name: "state".to_string(),
                },
                ty: IrType::Int,
                span: loc(),
            },
            span: loc(),
        };

        let vm = IrVmProgram::from_ir(IrProgram {
            functions: vec![update, query],
        })
        .expect("build vm from ir");

        let mut fields = BTreeMap::new();
        fields.insert("delta".to_string(), VmValue::Int(7));
        fields.insert(
            "actions".to_string(),
            VmValue::List(vec![VmValue::String("notify".to_string())]),
        );

        let event = VmValue::Enum {
            tag: "Add".to_string(),
            fields,
        };

        let (next_state, actions) = vm
            .update(&VmValue::Int(10), &event, &VmLimits::default())
            .expect("update should succeed");
        assert_eq!(next_state, VmValue::Int(17));
        assert_eq!(actions, vec![VmValue::String("notify".to_string())]);

        let queried = vm
            .query(&next_state, &VmValue::Unit, &VmLimits::default())
            .expect("query should succeed");
        assert_eq!(queried, VmValue::Int(17));
    }

    #[test]
    fn ir_vm_returns_structured_error_on_step_limit() {
        let recurse = IrFunction {
            kind: FnKind::User,
            name: "loop_forever".to_string(),
            params: vec![IrParam {
                name: "x".to_string(),
                ty: IrType::Int,
                span: loc(),
            }],
            return_type: IrType::Int,
            body: IrExpr {
                kind: IrExprKind::Call {
                    callee: "loop_forever".to_string(),
                    args: vec![IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "x".to_string(),
                        },
                        ty: IrType::Int,
                        span: loc(),
                    }],
                },
                ty: IrType::Int,
                span: loc(),
            },
            span: loc(),
        };

        let update = IrFunction {
            kind: FnKind::Update,
            name: "update".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "event".to_string(),
                    ty: IrType::Named {
                        name: "List".to_string(),
                        args: vec![IrType::Named {
                            name: "Action".to_string(),
                            args: vec![],
                        }],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Tuple(vec![
                IrType::Int,
                IrType::Named {
                    name: "List".to_string(),
                    args: vec![IrType::Named {
                        name: "Action".to_string(),
                        args: vec![],
                    }],
                },
            ]),
            body: IrExpr {
                kind: IrExprKind::Tuple(vec![
                    IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "state".to_string(),
                        },
                        ty: IrType::Int,
                        span: loc(),
                    },
                    IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "event".to_string(),
                        },
                        ty: IrType::Named {
                            name: "List".to_string(),
                            args: vec![IrType::Named {
                                name: "Action".to_string(),
                                args: vec![],
                            }],
                        },
                        span: loc(),
                    },
                ]),
                ty: IrType::Tuple(vec![
                    IrType::Int,
                    IrType::Named {
                        name: "List".to_string(),
                        args: vec![IrType::Named {
                            name: "Action".to_string(),
                            args: vec![],
                        }],
                    },
                ]),
                span: loc(),
            },
            span: loc(),
        };

        let query = IrFunction {
            kind: FnKind::Query,
            name: "query".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "q".to_string(),
                    ty: IrType::Named {
                        name: "Query".to_string(),
                        args: vec![],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Int,
            body: IrExpr {
                kind: IrExprKind::Call {
                    callee: "loop_forever".to_string(),
                    args: vec![IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "state".to_string(),
                        },
                        ty: IrType::Int,
                        span: loc(),
                    }],
                },
                ty: IrType::Int,
                span: loc(),
            },
            span: loc(),
        };

        let vm = IrVmProgram::from_ir(IrProgram {
            functions: vec![recurse, update, query],
        })
        .expect("build vm from ir");

        let error = vm
            .query(
                &VmValue::Int(1),
                &VmValue::Unit,
                &VmLimits {
                    max_steps: 32,
                    max_memory_bytes: 1_000_000,
                    max_recursion_depth: 10_000,
                },
            )
            .expect_err("query must fail on step limit");

        assert_eq!(error.code, VmErrorCode::StepLimitExceeded);
    }

    #[test]
    fn ir_vm_is_deterministic_for_same_input() {
        let update = IrFunction {
            kind: FnKind::Update,
            name: "update".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "event".to_string(),
                    ty: IrType::Named {
                        name: "List".to_string(),
                        args: vec![IrType::Named {
                            name: "Action".to_string(),
                            args: vec![],
                        }],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Tuple(vec![
                IrType::Int,
                IrType::Named {
                    name: "List".to_string(),
                    args: vec![IrType::Named {
                        name: "Action".to_string(),
                        args: vec![],
                    }],
                },
            ]),
            body: IrExpr {
                kind: IrExprKind::Tuple(vec![
                    IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "state".to_string(),
                        },
                        ty: IrType::Int,
                        span: loc(),
                    },
                    IrExpr {
                        kind: IrExprKind::LocalRef {
                            name: "event".to_string(),
                        },
                        ty: IrType::Named {
                            name: "List".to_string(),
                            args: vec![IrType::Named {
                                name: "Action".to_string(),
                                args: vec![],
                            }],
                        },
                        span: loc(),
                    },
                ]),
                ty: IrType::Tuple(vec![
                    IrType::Int,
                    IrType::Named {
                        name: "List".to_string(),
                        args: vec![IrType::Named {
                            name: "Action".to_string(),
                            args: vec![],
                        }],
                    },
                ]),
                span: loc(),
            },
            span: loc(),
        };

        let query = IrFunction {
            kind: FnKind::Query,
            name: "query".to_string(),
            params: vec![
                IrParam {
                    name: "state".to_string(),
                    ty: IrType::Int,
                    span: loc(),
                },
                IrParam {
                    name: "q".to_string(),
                    ty: IrType::Named {
                        name: "Query".to_string(),
                        args: vec![],
                    },
                    span: loc(),
                },
            ],
            return_type: IrType::Int,
            body: IrExpr {
                kind: IrExprKind::LocalRef {
                    name: "state".to_string(),
                },
                ty: IrType::Int,
                span: loc(),
            },
            span: loc(),
        };

        let vm = IrVmProgram::from_ir(IrProgram {
            functions: vec![update, query],
        })
        .expect("build vm from ir");

        let event = VmValue::List(vec![VmValue::String("same".to_string())]);
        let limits = VmLimits::default();

        let first = vm
            .update(&VmValue::Int(42), &event, &limits)
            .expect("first update should succeed");
        let second = vm
            .update(&VmValue::Int(42), &event, &limits)
            .expect("second update should succeed");

        assert_eq!(first, second);
    }
}
