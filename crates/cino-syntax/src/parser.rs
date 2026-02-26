use crate::{
    ast::{
        BinaryOp, BlockExpr, Expr, ExprKind, FieldDecl, FnDecl, FnKind, LetStmt, MatchArm,
        MatchExpr, Param, Pattern, PatternField, PatternKind, Program, RecordBody, RecordField,
        ReturnStmt, Stmt, TopDecl, TypeDecl, TypeDeclKind, TypeExpr, TypeExprKind, VariantDecl,
        VariantList,
    },
    error::ParseError,
    lexer::Lexer,
    span::{Position, Span},
    token::{Token, TokenKind, TokenTag, display_tag},
};

pub fn parse_program(input: &str) -> Result<Program, ParseError> {
    let tokens = Lexer::new(input).tokenize()?;
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut decls = Vec::new();

        while !self.at_eof() {
            decls.push(self.parse_top_decl()?);
        }

        Ok(Program { decls })
    }

    fn parse_top_decl(&mut self) -> Result<TopDecl, ParseError> {
        match TokenTag::from(&self.current().kind) {
            TokenTag::Fn => Ok(TopDecl::Function(self.parse_user_fn_decl()?)),
            TokenTag::Update => Ok(TopDecl::Function(self.parse_update_fn_decl()?)),
            TokenTag::Query => {
                if self.peek_tag(1) == TokenTag::LParen {
                    Ok(TopDecl::Function(self.parse_query_fn_decl()?))
                } else {
                    Ok(TopDecl::Type(self.parse_query_decl()?))
                }
            }
            TokenTag::State => Ok(TopDecl::Type(self.parse_state_decl()?)),
            TokenTag::Event => Ok(TopDecl::Type(self.parse_event_decl()?)),
            TokenTag::Enum => Ok(TopDecl::Type(self.parse_enum_decl()?)),
            TokenTag::Record => Ok(TopDecl::Type(self.parse_record_decl()?)),
            other => Err(ParseError {
                message: format!(
                    "expected top-level declaration, found {}",
                    display_tag(&other)
                ),
                position: self.current().span.start,
            }),
        }
    }

    fn parse_state_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.expect(TokenTag::State)?.span.start;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let body = self.parse_record_body()?;
        Ok(TypeDecl {
            name,
            generics,
            kind: TypeDeclKind::State(body.clone()),
            span: Span::join(start, body.span.end),
        })
    }

    fn parse_record_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.expect(TokenTag::Record)?.span.start;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let body = self.parse_record_body()?;
        Ok(TypeDecl {
            name,
            generics,
            kind: TypeDeclKind::Record(body.clone()),
            span: Span::join(start, body.span.end),
        })
    }

    fn parse_event_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.expect(TokenTag::Event)?.span.start;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenTag::Eq)?;
        let variants = self.parse_variant_list()?;
        Ok(TypeDecl {
            name,
            generics,
            kind: TypeDeclKind::Event(variants.clone()),
            span: Span::join(start, variants.span.end),
        })
    }

    fn parse_query_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.expect(TokenTag::Query)?.span.start;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenTag::Eq)?;
        let variants = self.parse_variant_list()?;
        Ok(TypeDecl {
            name,
            generics,
            kind: TypeDeclKind::Query(variants.clone()),
            span: Span::join(start, variants.span.end),
        })
    }

    fn parse_enum_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.expect(TokenTag::Enum)?.span.start;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenTag::Eq)?;
        let variants = self.parse_variant_list()?;
        Ok(TypeDecl {
            name,
            generics,
            kind: TypeDeclKind::Enum(variants.clone()),
            span: Span::join(start, variants.span.end),
        })
    }

    fn parse_generic_params(&mut self) -> Result<Vec<String>, ParseError> {
        if self.consume_if(TokenTag::LAngle).is_none() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        loop {
            out.push(self.expect_ident()?);
            if self.consume_if(TokenTag::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenTag::RAngle)?;
        Ok(out)
    }

    fn parse_variant_list(&mut self) -> Result<VariantList, ParseError> {
        let start = self.current().span.start;
        let mut variants = Vec::new();

        while self.check(TokenTag::Pipe) {
            variants.push(self.parse_variant_decl()?);
        }

        if variants.is_empty() {
            return Err(ParseError {
                message: "variant list requires at least one `|` arm".to_string(),
                position: start,
            });
        }

        let end = variants.last().map(|v| v.span.end).unwrap_or(start);

        Ok(VariantList {
            variants,
            span: Span::join(start, end),
        })
    }

    fn parse_variant_decl(&mut self) -> Result<VariantDecl, ParseError> {
        let start = self.expect(TokenTag::Pipe)?.span.start;
        let name = self.expect_ident()?;
        let payload = if self.check(TokenTag::LBrace) {
            self.parse_record_body()?.fields
        } else {
            Vec::new()
        };
        let end = if self.previous_tag() == TokenTag::RBrace {
            self.previous().span.end
        } else if let Some(last) = payload.last() {
            last.span.end
        } else {
            self.previous().span.end
        };

        Ok(VariantDecl {
            name,
            payload,
            span: Span::join(start, end),
        })
    }

    fn parse_record_body(&mut self) -> Result<RecordBody, ParseError> {
        let start = self.expect(TokenTag::LBrace)?.span.start;
        let mut fields = Vec::new();

        while !self.check(TokenTag::RBrace) {
            fields.push(self.parse_field_decl()?);
            let _ = self.consume_if(TokenTag::Comma);
            let _ = self.consume_if(TokenTag::Semi);
        }

        let end = self.expect(TokenTag::RBrace)?.span.end;

        Ok(RecordBody {
            fields,
            span: Span::join(start, end),
        })
    }

    fn parse_field_decl(&mut self) -> Result<FieldDecl, ParseError> {
        let start = self.current().span.start;
        let name = self.expect_ident()?;
        self.expect(TokenTag::Colon)?;
        let type_expr = self.parse_type_expr()?;
        Ok(FieldDecl {
            name,
            span: Span::join(start, type_expr.span.end),
            type_expr,
        })
    }

    fn parse_user_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let start = self.expect(TokenTag::Fn)?.span.start;
        let name = self.expect_ident()?;
        self.parse_fn_decl_tail(FnKind::User, name, start)
    }

    fn parse_update_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let start = self.expect(TokenTag::Update)?.span.start;
        self.parse_fn_decl_tail(FnKind::Update, "update".to_string(), start)
    }

    fn parse_query_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let start = self.expect(TokenTag::Query)?.span.start;
        self.parse_fn_decl_tail(FnKind::Query, "query".to_string(), start)
    }

    fn parse_fn_decl_tail(
        &mut self,
        kind: FnKind,
        name: String,
        start: Position,
    ) -> Result<FnDecl, ParseError> {
        self.expect(TokenTag::LParen)?;

        let mut params = Vec::new();
        if !self.check(TokenTag::RParen) {
            loop {
                params.push(self.parse_param()?);
                if self.consume_if(TokenTag::Comma).is_none() {
                    break;
                }
            }
        }

        self.expect(TokenTag::RParen)?;
        self.expect(TokenTag::Arrow)?;
        let return_type = self.parse_type_expr()?;
        let body = self.parse_block_expr()?;

        Ok(FnDecl {
            kind,
            name,
            params,
            return_type,
            span: Span::join(start, body.span.end),
            body,
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let start = self.current().span.start;
        let name = self.expect_ident()?;
        self.expect(TokenTag::Colon)?;
        let type_expr = self.parse_type_expr()?;
        Ok(Param {
            name,
            span: Span::join(start, type_expr.span.end),
            type_expr,
        })
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        if self.consume_if(TokenTag::LParen).is_some() {
            let start = self.previous().span.start;
            let mut items = Vec::new();
            if !self.check(TokenTag::RParen) {
                loop {
                    items.push(self.parse_type_expr()?);
                    if self.consume_if(TokenTag::Comma).is_none() {
                        break;
                    }
                }
            }
            let end = self.expect(TokenTag::RParen)?.span.end;
            return Ok(TypeExpr {
                kind: TypeExprKind::Tuple { items },
                span: Span::join(start, end),
            });
        }

        let (name, start) = self.expect_ident_with_span()?;
        let mut generics = Vec::new();
        let mut end = self.previous().span.end;

        if self.consume_if(TokenTag::LAngle).is_some() {
            loop {
                generics.push(self.parse_type_expr()?);
                if self.consume_if(TokenTag::Comma).is_none() {
                    break;
                }
            }
            end = self.expect(TokenTag::RAngle)?.span.end;
        }

        Ok(TypeExpr {
            kind: TypeExprKind::Named { name, generics },
            span: Span::join(start, end),
        })
    }

    fn parse_block_expr(&mut self) -> Result<BlockExpr, ParseError> {
        let start = self.expect(TokenTag::LBrace)?.span.start;
        let mut statements = Vec::new();

        loop {
            match TokenTag::from(&self.current().kind) {
                TokenTag::Let => {
                    statements.push(Stmt::Let(self.parse_let_stmt()?));
                    let _ = self.consume_if(TokenTag::Semi);
                }
                TokenTag::Return => {
                    statements.push(Stmt::Return(self.parse_return_stmt()?));
                    let _ = self.consume_if(TokenTag::Semi);
                }
                _ => break,
            }
        }

        let tail = if self.check(TokenTag::RBrace) {
            None
        } else {
            Some(Box::new(self.parse_expr(0)?))
        };

        let end = self.expect(TokenTag::RBrace)?.span.end;
        Ok(BlockExpr {
            statements,
            tail,
            span: Span::join(start, end),
        })
    }

    fn parse_let_stmt(&mut self) -> Result<LetStmt, ParseError> {
        let start = self.expect(TokenTag::Let)?.span.start;
        let name = self.expect_ident()?;
        self.expect(TokenTag::Eq)?;
        let value = self.parse_expr(0)?;

        Ok(LetStmt {
            name,
            span: Span::join(start, value.span.end),
            value,
        })
    }

    fn parse_return_stmt(&mut self) -> Result<ReturnStmt, ParseError> {
        let start = self.expect(TokenTag::Return)?.span.start;
        let value = self.parse_expr(0)?;
        Ok(ReturnStmt {
            span: Span::join(start, value.span.end),
            value,
        })
    }

    fn parse_expr(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_postfix_expr()?;

        while let Some((op, prec)) = self.current_binary_op() {
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(prec + 1)?;
            let span = Span::join(lhs.span.start, rhs.span.end);
            lhs = Expr {
                kind: ExprKind::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                },
                span,
            };
        }

        Ok(lhs)
    }

    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            if self.consume_if(TokenTag::LParen).is_some() {
                let mut args = Vec::new();
                if !self.check(TokenTag::RParen) {
                    loop {
                        args.push(self.parse_expr(0)?);
                        if self.consume_if(TokenTag::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenTag::RParen)?.span.end;
                let start = expr.span.start;
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span: Span::join(start, end),
                };
                continue;
            }
            break;
        }

        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        if self.check(TokenTag::LBrace) {
            let block = self.parse_block_expr()?;
            return Ok(Expr {
                span: block.span,
                kind: ExprKind::Block(block),
            });
        }

        if self.check(TokenTag::Match) {
            let m = self.parse_match_expr()?;
            return Ok(Expr {
                span: m.span,
                kind: ExprKind::Match(m),
            });
        }

        if self.consume_if(TokenTag::LBracket).is_some() {
            let start = self.previous().span.start;
            let mut items = Vec::new();
            if !self.check(TokenTag::RBracket) {
                loop {
                    items.push(self.parse_expr(0)?);
                    if self.consume_if(TokenTag::Comma).is_none() {
                        break;
                    }
                }
            }
            let end = self.expect(TokenTag::RBracket)?.span.end;
            return Ok(Expr {
                span: Span::join(start, end),
                kind: ExprKind::List(items),
            });
        }

        if self.consume_if(TokenTag::LParen).is_some() {
            let start = self.previous().span.start;
            let first = self.parse_expr(0)?;
            if self.consume_if(TokenTag::Comma).is_some() {
                let mut items = vec![first];
                loop {
                    items.push(self.parse_expr(0)?);
                    if self.consume_if(TokenTag::Comma).is_none() {
                        break;
                    }
                }
                let end = self.expect(TokenTag::RParen)?.span.end;
                return Ok(Expr {
                    span: Span::join(start, end),
                    kind: ExprKind::Tuple(items),
                });
            }

            let end = self.expect(TokenTag::RParen)?.span.end;
            return Ok(Expr {
                span: Span::join(start, end),
                kind: first.kind,
            });
        }

        let token = self.bump();
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Ident(name) => {
                if self.check(TokenTag::LBrace) && self.looks_like_record_expr() {
                    self.bump();
                    let mut fields = Vec::new();
                    while !self.check(TokenTag::RBrace) {
                        let f_start = self.current().span.start;
                        let f_name = self.expect_ident()?;
                        self.expect(TokenTag::Colon)?;
                        let f_value = self.parse_expr(0)?;
                        fields.push(RecordField {
                            name: f_name,
                            value: f_value,
                            span: Span::join(f_start, self.previous().span.end),
                        });
                        let _ = self.consume_if(TokenTag::Comma);
                        let _ = self.consume_if(TokenTag::Semi);
                    }
                    self.expect(TokenTag::RBrace)?;
                    ExprKind::Record { name, fields }
                } else {
                    ExprKind::Ident(name)
                }
            }
            TokenKind::State => ExprKind::Ident("state".to_string()),
            TokenKind::Event => ExprKind::Ident("event".to_string()),
            TokenKind::Query => ExprKind::Ident("query".to_string()),
            TokenKind::Update => ExprKind::Ident("update".to_string()),
            TokenKind::Enum => ExprKind::Ident("enum".to_string()),
            TokenKind::Record => ExprKind::Ident("record".to_string()),
            TokenKind::Let => ExprKind::Ident("let".to_string()),
            TokenKind::Match => ExprKind::Ident("match".to_string()),
            TokenKind::Return => ExprKind::Ident("return".to_string()),
            TokenKind::If => ExprKind::Ident("if".to_string()),
            TokenKind::Fn => ExprKind::Ident("fn".to_string()),
            TokenKind::Int(v) => ExprKind::Int(v),
            TokenKind::True => ExprKind::Bool(true),
            TokenKind::False => ExprKind::Bool(false),
            other => {
                return Err(ParseError {
                    message: format!(
                        "expected expression, found {}",
                        display_tag(&TokenTag::from(&other))
                    ),
                    position: span.start,
                });
            }
        };

        Ok(Expr { kind, span })
    }

    fn parse_match_expr(&mut self) -> Result<MatchExpr, ParseError> {
        let start = self.expect(TokenTag::Match)?.span.start;
        let subject = self.parse_expr(0)?;
        self.expect(TokenTag::LBrace)?;

        let mut arms = Vec::new();
        while !self.check(TokenTag::RBrace) {
            arms.push(self.parse_match_arm()?);
            let _ = self.consume_if(TokenTag::Comma);
            let _ = self.consume_if(TokenTag::Semi);
        }

        let end = self.expect(TokenTag::RBrace)?.span.end;
        Ok(MatchExpr {
            subject: Box::new(subject),
            arms,
            span: Span::join(start, end),
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.current().span.start;
        let pattern = self.parse_pattern()?;
        let guard = if self.consume_if(TokenTag::If).is_some() {
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        self.expect(TokenTag::FatArrow)?;
        let body = self.parse_expr(0)?;
        Ok(MatchArm {
            pattern,
            guard,
            span: Span::join(start, body.span.end),
            body,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.consume_if(TokenTag::Underscore).is_some() {
            let span = self.previous().span;
            return Ok(Pattern {
                kind: PatternKind::Wildcard,
                span,
            });
        }

        let (name, start) = self.expect_ident_with_span()?;
        let mut fields = Vec::new();
        let mut end = self.previous().span.end;

        if self.consume_if(TokenTag::LBrace).is_some() {
            while !self.check(TokenTag::RBrace) {
                fields.push(self.parse_pattern_field()?);
                let _ = self.consume_if(TokenTag::Comma);
            }
            end = self.expect(TokenTag::RBrace)?.span.end;
        }

        Ok(Pattern {
            kind: PatternKind::Variant { name, fields },
            span: Span::join(start, end),
        })
    }

    fn parse_pattern_field(&mut self) -> Result<PatternField, ParseError> {
        let start = self.current().span.start;
        let name = self.expect_ident()?;
        let pattern = if self.consume_if(TokenTag::Colon).is_some() {
            Some(Box::new(self.parse_pattern()?))
        } else {
            None
        };
        let end = pattern
            .as_ref()
            .map_or(self.previous().span.end, |p| p.span.end);
        Ok(PatternField {
            name,
            pattern,
            span: Span::join(start, end),
        })
    }

    fn current_binary_op(&self) -> Option<(BinaryOp, u8)> {
        match self.current().kind {
            TokenKind::Plus => Some((BinaryOp::Add, 10)),
            TokenKind::Minus => Some((BinaryOp::Sub, 10)),
            TokenKind::Star => Some((BinaryOp::Mul, 20)),
            TokenKind::Slash => Some((BinaryOp::Div, 20)),
            _ => None,
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let token = self.bump();
        self.token_to_ident(&token.kind).ok_or_else(|| ParseError {
            message: format!(
                "expected identifier, found {}",
                display_tag(&TokenTag::from(&token.kind))
            ),
            position: token.span.start,
        })
    }

    fn expect_ident_with_span(&mut self) -> Result<(String, Position), ParseError> {
        let token = self.bump();
        let start = token.span.start;
        self.token_to_ident(&token.kind)
            .map(|name| (name, start))
            .ok_or_else(|| ParseError {
                message: format!(
                    "expected identifier, found {}",
                    display_tag(&TokenTag::from(&token.kind))
                ),
                position: token.span.start,
            })
    }

    fn expect(&mut self, expected: TokenTag) -> Result<Token, ParseError> {
        let token = self.bump();
        let actual = TokenTag::from(&token.kind);
        if actual == expected {
            Ok(token)
        } else {
            Err(ParseError {
                message: format!(
                    "expected {}, found {}",
                    display_tag(&expected),
                    display_tag(&actual)
                ),
                position: token.span.start,
            })
        }
    }

    fn consume_if(&mut self, expected: TokenTag) -> Option<Token> {
        if self.check(expected) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn check(&self, tag: TokenTag) -> bool {
        TokenTag::from(&self.current().kind) == tag
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.cursor.saturating_sub(1)]
    }

    fn previous_tag(&self) -> TokenTag {
        TokenTag::from(&self.previous().kind)
    }

    fn looks_like_record_expr(&self) -> bool {
        matches!(self.peek_tag(1), TokenTag::RBrace) || matches!(self.peek_tag(2), TokenTag::Colon)
    }

    fn peek_tag(&self, offset: usize) -> TokenTag {
        let idx = self
            .cursor
            .saturating_add(offset)
            .min(self.tokens.len().saturating_sub(1));
        TokenTag::from(&self.tokens[idx].kind)
    }

    fn bump(&mut self) -> Token {
        let idx = self.cursor;
        if !self.at_eof() {
            self.cursor += 1;
        }
        self.tokens[idx].clone()
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn token_to_ident(&self, token: &TokenKind) -> Option<String> {
        match token {
            TokenKind::Ident(name) => Some(name.clone()),
            TokenKind::State => Some("state".to_string()),
            TokenKind::Event => Some("event".to_string()),
            TokenKind::Query => Some("query".to_string()),
            TokenKind::Update => Some("update".to_string()),
            TokenKind::Enum => Some("enum".to_string()),
            TokenKind::Record => Some("record".to_string()),
            TokenKind::Let => Some("let".to_string()),
            TokenKind::Match => Some("match".to_string()),
            TokenKind::Return => Some("return".to_string()),
            TokenKind::If => Some("if".to_string()),
            TokenKind::Fn => Some("fn".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ExprKind, FnKind, PatternKind, TopDecl, TypeDeclKind, parse_program};

    #[test]
    fn parse_core_decls_and_match() {
        let src = r#"
state BillingState {
  balance: Int
}

event BillingEvent =
  | InvoiceIssued { amount: Int }
  | PaymentReceived { amount: Int }

update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) {
  match event {
    InvoiceIssued { amount } => (state, noop(amount))
    PaymentReceived { amount } => (state, noop(amount))
  }
}
"#;

        let program = parse_program(src).expect("parses");
        assert_eq!(program.decls.len(), 3);

        match &program.decls[0] {
            TopDecl::Type(td) => assert!(matches!(td.kind, TypeDeclKind::State(_))),
            _ => panic!("expected type decl"),
        }

        match &program.decls[1] {
            TopDecl::Type(td) => assert!(matches!(td.kind, TypeDeclKind::Event(_))),
            _ => panic!("expected type decl"),
        }

        let func = match &program.decls[2] {
            TopDecl::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert_eq!(func.kind, FnKind::Update);
        let tail = func.body.tail.as_ref().expect("tail");
        match &tail.kind {
            ExprKind::Match(m) => {
                assert_eq!(m.arms.len(), 2);
                assert!(matches!(
                    m.arms[0].pattern.kind,
                    PatternKind::Variant { .. }
                ));
            }
            other => panic!("unexpected tail: {other:?}"),
        }
    }

    #[test]
    fn parse_query_declaration_and_query_function() {
        let src = r#"
query BillingQuery =
  | CurrentBalance

query(state: BillingState, q: BillingQuery) -> Result<Int, DomainError> {
  1
}
"#;
        let program = parse_program(src).expect("parses");
        assert_eq!(program.decls.len(), 2);

        match &program.decls[0] {
            TopDecl::Type(td) => assert!(matches!(td.kind, TypeDeclKind::Query(_))),
            _ => panic!("expected query type decl"),
        }

        match &program.decls[1] {
            TopDecl::Function(f) => assert_eq!(f.kind, FnKind::Query),
            _ => panic!("expected query function"),
        }
    }

    #[test]
    fn parse_match_guard_ast() {
        let src = r#"
fn test(x: Int) -> Int {
  match x {
    _ if true => 1
  }
}
"#;
        let program = parse_program(src).expect("parses");
        let func = match &program.decls[0] {
            TopDecl::Function(f) => f,
            _ => panic!("expected fn"),
        };
        let tail = func.body.tail.as_ref().expect("tail");
        match &tail.kind {
            ExprKind::Match(m) => {
                assert!(m.arms[0].guard.is_some());
            }
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn parse_return_statement() {
        let src = r#"
fn f(x: Int) -> Int {
  return x
}
"#;
        let program = parse_program(src).expect("parses");
        let func = match &program.decls[0] {
            TopDecl::Function(f) => f,
            _ => panic!("expected fn"),
        };
        assert!(func.body.tail.is_none());
        assert_eq!(func.body.statements.len(), 1);
    }

    #[test]
    fn parse_reports_line_and_column_on_error() {
        let src = r#"
fn broken(a: Int -> Int {
  a
}
"#;
        let err = parse_program(src).expect_err("must fail");
        assert!(err.message.contains("expected `)`"));
        assert_eq!(err.position.line, 2);
        assert_eq!(err.position.column, 18);
    }
}
