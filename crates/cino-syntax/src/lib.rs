#![forbid(unsafe_code)]

mod ast;
mod error;
mod lexer;
mod parser;
mod span;
mod token;

pub use ast::{
    BinaryOp, BlockExpr, Expr, ExprKind, FieldDecl, FnDecl, FnKind, LetStmt, MatchArm, MatchExpr,
    Param, Pattern, PatternField, PatternKind, Program, RecordBody, ReturnStmt, Stmt, TopDecl,
    TypeDecl, TypeDeclKind, TypeExpr, TypeExprKind, VariantDecl, VariantList,
};
pub use error::ParseError;
pub use parser::parse_program;
pub use span::{Position, Span};
