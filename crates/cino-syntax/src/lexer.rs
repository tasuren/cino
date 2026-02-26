use crate::{
    error::ParseError,
    span::{Position, Span},
    token::{Token, TokenKind},
};

pub(crate) struct Lexer<'a> {
    input: &'a str,
    cursor: usize,
    position: Position,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input,
            cursor: 0,
            position: Position::start(),
        }
    }

    pub(crate) fn tokenize(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.bump_char();
                continue;
            }

            if ch == '/' && self.peek_next_char() == Some('/') {
                while let Some(c) = self.peek_char() {
                    self.bump_char();
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }

            let start = self.position;
            let token = match ch {
                '(' => {
                    self.bump_char();
                    TokenKind::LParen
                }
                ')' => {
                    self.bump_char();
                    TokenKind::RParen
                }
                '{' => {
                    self.bump_char();
                    TokenKind::LBrace
                }
                '}' => {
                    self.bump_char();
                    TokenKind::RBrace
                }
                '[' => {
                    self.bump_char();
                    TokenKind::LBracket
                }
                ']' => {
                    self.bump_char();
                    TokenKind::RBracket
                }
                '<' => {
                    self.bump_char();
                    TokenKind::LAngle
                }
                '>' => {
                    self.bump_char();
                    TokenKind::RAngle
                }
                ',' => {
                    self.bump_char();
                    TokenKind::Comma
                }
                ':' => {
                    self.bump_char();
                    TokenKind::Colon
                }
                '=' => {
                    self.bump_char();
                    if self.peek_char() == Some('>') {
                        self.bump_char();
                        TokenKind::FatArrow
                    } else {
                        TokenKind::Eq
                    }
                }
                '|' => {
                    self.bump_char();
                    TokenKind::Pipe
                }
                ';' => {
                    self.bump_char();
                    TokenKind::Semi
                }
                '+' => {
                    self.bump_char();
                    TokenKind::Plus
                }
                '*' => {
                    self.bump_char();
                    TokenKind::Star
                }
                '/' => {
                    self.bump_char();
                    TokenKind::Slash
                }
                '-' => {
                    self.bump_char();
                    if self.peek_char() == Some('>') {
                        self.bump_char();
                        TokenKind::Arrow
                    } else {
                        TokenKind::Minus
                    }
                }
                c if is_ident_start(c) => self.lex_ident_or_keyword(),
                c if c.is_ascii_digit() => self.lex_int()?,
                other => {
                    return Err(ParseError {
                        message: format!("unexpected character '{other}'"),
                        position: start,
                    });
                }
            };

            let end = self.position;
            tokens.push(Token {
                kind: token,
                span: Span::join(start, end),
            });
        }

        let eof = self.position;
        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::join(eof, eof),
        });

        Ok(tokens)
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.cursor;
        while let Some(ch) = self.peek_char() {
            if is_ident_continue(ch) {
                self.bump_char();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.cursor];
        match text {
            "fn" => TokenKind::Fn,
            "update" => TokenKind::Update,
            "query" => TokenKind::Query,
            "state" => TokenKind::State,
            "event" => TokenKind::Event,
            "enum" => TokenKind::Enum,
            "record" => TokenKind::Record,
            "let" => TokenKind::Let,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "_" => TokenKind::Underscore,
            _ => TokenKind::Ident(text.to_string()),
        }
    }

    fn lex_int(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.cursor;
        let pos = self.position;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.bump_char();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.cursor];
        let value = text.parse::<i64>().map_err(|_| ParseError {
            message: format!("invalid integer literal '{text}'"),
            position: pos,
        })?;
        Ok(TokenKind::Int(value))
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.input[self.cursor..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.cursor += ch.len_utf8();
        self.position.offset += ch.len_utf8();
        if ch == '\n' {
            self.position.line += 1;
            self.position.column = 1;
        } else {
            self.position.column += 1;
        }
        Some(ch)
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}
