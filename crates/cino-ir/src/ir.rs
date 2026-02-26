use cino_syntax::{BinaryOp, FnKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweringResult {
    pub program: Option<IrProgram>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrFunction {
    pub kind: FnKind,
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: IrType,
    pub body: IrExpr,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrParam {
    pub name: String,
    pub ty: IrType,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrExpr {
    pub kind: IrExprKind,
    pub ty: IrType,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrExprKind {
    LocalRef {
        name: String,
    },
    Int(i64),
    Bool(bool),
    Tuple(Vec<IrExpr>),
    List(Vec<IrExpr>),
    Record {
        name: String,
        fields: Vec<IrRecordField>,
    },
    Binary {
        lhs: Box<IrExpr>,
        op: BinaryOp,
        rhs: Box<IrExpr>,
    },
    Call {
        callee: String,
        args: Vec<IrExpr>,
    },
    Let {
        name: String,
        value: Box<IrExpr>,
        body: Box<IrExpr>,
    },
    Match {
        subject: Box<IrExpr>,
        arms: Vec<IrMatchArm>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRecordField {
    pub name: String,
    pub value: IrExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMatchArm {
    pub pattern: IrPattern,
    pub body: IrExpr,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrPattern {
    pub kind: IrPatternKind,
    pub ty: IrType,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrPatternKind {
    Wildcard,
    Binding {
        name: String,
    },
    Variant {
        name: String,
        fields: Vec<IrPatternField>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrPatternField {
    pub name: String,
    pub pattern: IrPattern,
    pub span: SourceLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrType {
    Int,
    Bool,
    Decimal,
    String,
    Named { name: String, args: Vec<IrType> },
    Tuple(Vec<IrType>),
    Unknown,
}

impl IrType {
    pub fn is_compatible(&self, other: &IrType) -> bool {
        if self == other || matches!(self, IrType::Unknown) || matches!(other, IrType::Unknown) {
            return true;
        }

        match (self, other) {
            (
                IrType::Named {
                    name: na,
                    args: aa,
                },
                IrType::Named {
                    name: nb,
                    args: ab,
                },
            ) => {
                if na != nb || aa.len() != ab.len() {
                    return false;
                }
                aa.iter()
                    .zip(ab.iter())
                    .all(|(l, r)| l.is_compatible(r))
            }
            (IrType::Tuple(la), IrType::Tuple(lb)) => {
                if la.len() != lb.len() {
                    return false;
                }
                la.iter()
                    .zip(lb.iter())
                    .all(|(l, r)| l.is_compatible(r))
            }
            _ => false,
        }
    }
}
