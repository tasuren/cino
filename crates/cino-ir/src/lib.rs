#![forbid(unsafe_code)]

mod ir;
mod lower;
mod validate;

pub use ir::*;
pub use lower::lower_program;
pub use validate::validate_program;

/// IR crate entry point for the cino MVP workspace.
pub fn crate_name() -> &'static str {
    "cino-ir"
}

#[cfg(test)]
mod tests {
    use cino_syntax::{FnKind, parse_program};

    use crate::{
        Diagnostic, IrExpr, IrExprKind, IrFunction, IrParam, IrProgram, IrType, SourceLoc,
        lower_program, validate_program,
    };

    #[test]
    fn lowers_minimum_sample_to_typed_ir() {
        let source = r#"
enum BillingEvent =
  | InvoiceIssued { amount: Int }
  | PaymentReceived { amount: Int }

fn score(event: BillingEvent) -> Int {
  match event {
    InvoiceIssued { amount } => amount + 1
    PaymentReceived { amount } => amount
  }
}
"#;
        let program = parse_program(source).expect("parse");
        let lowered = lower_program(&program);
        assert!(
            lowered.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            lowered.diagnostics
        );

        let ir = lowered.program.expect("ir program");
        assert_eq!(ir.functions.len(), 1);
        assert_eq!(ir.functions[0].kind, FnKind::User);
        assert_eq!(ir.functions[0].return_type, IrType::Int);
    }

    #[test]
    fn validates_lowered_ir_consistency() {
        let source = r#"
enum E =
  | A { value: Int }
  | B { value: Int }

fn pick(e: E) -> Int {
  match e {
    A { value } => value
    B { value } => value + 1
  }
}
"#;
        let program = parse_program(source).expect("parse");
        let lowered = lower_program(&program);
        let ir = lowered.program.expect("ir program");
        assert!(validate_program(&ir).is_empty());
    }

    #[test]
    fn reports_diagnostic_on_lowering_failure() {
        let source = r#"
fn bad(x: Int) -> Int {
  missing(x)
}
"#;
        let program = parse_program(source).expect("parse");
        let lowered = lower_program(&program);
        assert!(lowered.program.is_none());
        assert!(
            lowered
                .diagnostics
                .iter()
                .any(|d| d.code == "E-PURE-002" || d.code == "E-TYPE-002")
        );
    }

    #[test]
    fn validator_detects_structural_ir_errors() {
        let ir = IrProgram {
            functions: vec![IrFunction {
                kind: FnKind::User,
                name: "bad".to_string(),
                params: vec![IrParam {
                    name: "x".to_string(),
                    ty: IrType::Int,
                    span: SourceLoc { line: 1, column: 1 },
                }],
                return_type: IrType::Int,
                body: IrExpr {
                    kind: IrExprKind::Bool(true),
                    ty: IrType::Bool,
                    span: SourceLoc { line: 1, column: 1 },
                },
                span: SourceLoc { line: 1, column: 1 },
            }],
        };

        let diagnostics = validate_program(&ir);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d: &Diagnostic| d.code == "E-IR-VAL-001")
        );
    }
}
