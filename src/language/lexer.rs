use crate::diagnostic::{Diagnostic, Result};
use crate::source::{SourceFile, SourceSpan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TokenKind {
    Identifier(String),
    Bare(String),
    String(String),
    Newline,
    Dollar,
    At,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    Colon,
    Comma,
    Equal,
    End,
}

pub(crate) fn lex(source: SourceFile) -> Result<Vec<Token>> {
    Lexer::new(source).lex()
}

struct Lexer {
    source: SourceFile,
    offset: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    fn new(source: SourceFile) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        while let Some(character) = self.peek() {
            match character {
                ' ' | '\t' | '\r' => self.advance(),
                '\n' => {
                    let span = self.span();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        span,
                    });
                }
                '#' => self.skip_comment(),
                '"' => tokens.push(self.string()?),
                character if is_identifier_start(character) => tokens.push(self.identifier()),
                character if is_bare_start(character) => tokens.push(self.bare()),
                _ => tokens.push(self.punctuation()?),
            }
        }
        tokens.push(Token {
            kind: TokenKind::End,
            span: self.span(),
        });
        Ok(tokens)
    }

    fn identifier(&mut self) -> Token {
        let span = self.span();
        let start = self.offset;
        self.advance();
        while self.peek().is_some_and(is_identifier_continue) {
            self.advance();
        }
        Token {
            kind: TokenKind::Identifier(self.source.text()[start..self.offset].to_owned()),
            span,
        }
    }

    fn bare(&mut self) -> Token {
        let span = self.span();
        let start = self.offset;
        while self
            .peek()
            .is_some_and(|character| !is_delimiter(character))
        {
            self.advance();
        }
        Token {
            kind: TokenKind::Bare(self.source.text()[start..self.offset].to_owned()),
            span,
        }
    }

    fn string(&mut self) -> Result<Token> {
        let span = self.span();
        self.advance();
        let mut value = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.advance();
                    return Ok(Token {
                        kind: TokenKind::String(value),
                        span,
                    });
                }
                Some('\\') => {
                    self.advance();
                    let escaped = match self.peek() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some(character) => {
                            return Err(Diagnostic::new(
                                "E_INVALID_ESCAPE",
                                format!("unsupported string escape `\\{character}`"),
                                self.span(),
                            ));
                        }
                        None => return Err(Self::unterminated_string(span)),
                    };
                    self.advance();
                    value.push(escaped);
                }
                Some('\n') | None => return Err(Self::unterminated_string(span)),
                Some(character) => {
                    self.advance();
                    value.push(character);
                }
            }
        }
    }

    fn punctuation(&mut self) -> Result<Token> {
        let span = self.span();
        let character = self.peek().expect("punctuation begins at a character");
        let kind = match character {
            '$' => TokenKind::Dollar,
            '@' => TokenKind::At,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            '<' => TokenKind::LeftAngle,
            '>' => TokenKind::RightAngle,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '=' => TokenKind::Equal,
            _ => {
                return Err(Diagnostic::new(
                    "E_INVALID_TOKEN",
                    format!("unexpected character `{character}`"),
                    span,
                ));
            }
        };
        self.advance();
        Ok(Token { kind, span })
    }

    fn skip_comment(&mut self) {
        while self.peek().is_some_and(|character| character != '\n') {
            self.advance();
        }
    }

    fn unterminated_string(span: SourceSpan) -> Diagnostic {
        Diagnostic::new(
            "E_UNTERMINATED_STRING",
            "string literal is missing its closing quote",
            span,
        )
    }

    fn span(&self) -> SourceSpan {
        SourceSpan::at(self.source.clone(), self.line, self.column)
    }

    fn peek(&self) -> Option<char> {
        self.source.text()[self.offset..].chars().next()
    }

    fn advance(&mut self) {
        let character = self.peek().expect("advance requires a character");
        self.offset += character.len_utf8();
        if character == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    matches!(character, '_' | '-') || character.is_ascii_alphanumeric()
}

fn is_bare_start(character: char) -> bool {
    character.is_ascii_digit() || character == '-' || character == '.'
}

fn is_delimiter(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '#' | '"' | '$' | '@' | '(' | ')' | '{' | '}' | '[' | ']' | '<' | '>' | ':' | ',' | '='
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(SourceFile::new("test.clipasm", source))
            .expect("valid source")
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_native_surface_without_program_knowledge() {
        assert_eq!(
            kinds("clipasm 1\n@visible clip<Audio> {\n  trim(300ms..800ms) as result # note\n}\n"),
            vec![
                TokenKind::Identifier("clipasm".to_owned()),
                TokenKind::Bare("1".to_owned()),
                TokenKind::Newline,
                TokenKind::At,
                TokenKind::Identifier("visible".to_owned()),
                TokenKind::Identifier("clip".to_owned()),
                TokenKind::LeftAngle,
                TokenKind::Identifier("Audio".to_owned()),
                TokenKind::RightAngle,
                TokenKind::LeftBrace,
                TokenKind::Newline,
                TokenKind::Identifier("trim".to_owned()),
                TokenKind::LeftParen,
                TokenKind::Bare("300ms..800ms".to_owned()),
                TokenKind::RightParen,
                TokenKind::Identifier("as".to_owned()),
                TokenKind::Identifier("result".to_owned()),
                TokenKind::Newline,
                TokenKind::RightBrace,
                TokenKind::Newline,
                TokenKind::End,
            ]
        );

        assert_eq!(
            kinds("clipasm 1\nmy-program as output-name\n"),
            vec![
                TokenKind::Identifier("clipasm".to_owned()),
                TokenKind::Bare("1".to_owned()),
                TokenKind::Newline,
                TokenKind::Identifier("my-program".to_owned()),
                TokenKind::Identifier("as".to_owned()),
                TokenKind::Identifier("output-name".to_owned()),
                TokenKind::Newline,
                TokenKind::End,
            ]
        );
    }

    #[test]
    fn decodes_strings_and_tracks_locations() {
        let tokens = lex(SourceFile::new(
            "test.clipasm",
            "\nimage(\"a\\n\\\"b.png\")\n",
        ))
        .expect("valid source");
        assert_eq!(tokens[1].span.line, 2);
        assert_eq!(tokens[1].span.column, 1);
        assert_eq!(tokens[3].kind, TokenKind::String("a\n\"b.png".to_owned()));
    }

    #[test]
    fn rejects_unknown_characters_and_unterminated_strings() {
        let invalid =
            lex(SourceFile::new("test.clipasm", "image(!)")).expect_err("invalid character");
        assert_eq!(invalid.code, "E_INVALID_TOKEN");
        assert_eq!(invalid.span.column, 7);

        let unterminated = lex(SourceFile::new("test.clipasm", "image(\"missing)"))
            .expect_err("unterminated string");
        assert_eq!(unterminated.code, "E_UNTERMINATED_STRING");
        assert_eq!(unterminated.span.column, 7);
    }
}
