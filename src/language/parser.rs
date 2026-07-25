use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::StackAccess;
use crate::source::{SourceFile, SourceSpan, Spanned};

use super::lexer::{Token, TokenKind, lex};
use super::syntax::{
    Argument, Block, Expression, Invocation, OutputBindings, SourceFileSyntax, Statement,
};

pub(crate) fn parse(source: SourceFile) -> Result<SourceFileSyntax> {
    let span = SourceSpan::source_start(source.clone());
    Parser::new(lex(source)?).parse_file(span)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse_file(mut self, span: SourceSpan) -> Result<SourceFileSyntax> {
        self.skip_newlines();
        self.expect_keyword(
            "clipasm",
            "E_MISSING_VERSION",
            "source must begin with `clipasm 1`",
        )?;
        let version = self.parse_version()?;
        self.expect_statement_end("version declaration")?;

        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::End) {
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }
        Ok(SourceFileSyntax {
            version,
            statements,
            span,
        })
    }

    fn parse_version(&mut self) -> Result<Spanned<u32>> {
        let token = self.advance().clone();
        let TokenKind::Bare(value) = token.kind else {
            return Err(Diagnostic::new(
                "E_INVALID_VERSION",
                "`clipasm` must be followed by language version `1`",
                token.span,
            ));
        };
        let version = value.parse::<u32>().map_err(|_| {
            Diagnostic::new(
                "E_INVALID_VERSION",
                "the language version must be an unsigned integer",
                token.span.clone(),
            )
        })?;
        if version != 1 {
            return Err(Diagnostic::new(
                "E_UNSUPPORTED_VERSION",
                format!("unsupported ClipAsm language version `{version}`; expected `1`"),
                token.span,
            ));
        }
        Ok(Spanned::new(version, token.span))
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        let span = self.current().span.clone();
        let expression = self.parse_statement_expression()?;
        let output_bindings = self.parse_output_bindings()?;
        self.expect_statement_end("statement")?;
        Ok(Statement {
            expression,
            output_bindings,
            span,
        })
    }

    fn parse_statement_expression(&mut self) -> Result<Expression> {
        let access = self.parse_access()?;
        match &self.current().kind {
            TokenKind::LeftBrace => Ok(Expression::Block(self.parse_block(access)?)),
            TokenKind::Identifier(_) => Ok(Expression::Invocation(self.parse_invocation(access)?)),
            TokenKind::Dollar if access.is_none() => self.parse_reference(),
            TokenKind::Dollar => Err(Diagnostic::new(
                "E_INVALID_ACCESS_TARGET",
                "stack access may modify an invocation or stack block, not a reference",
                self.current().span.clone(),
            )),
            _ => Err(self.expected("an invocation, reference, or stack block")),
        }
    }

    fn parse_expression(&mut self) -> Result<Expression> {
        let access = self.parse_access()?;
        match &self.current().kind {
            TokenKind::Dollar if access.is_none() => self.parse_reference(),
            TokenKind::Dollar => Err(Diagnostic::new(
                "E_INVALID_ACCESS_TARGET",
                "stack access may modify an invocation or block, not a reference",
                self.current().span.clone(),
            )),
            TokenKind::LeftBrace => Ok(Expression::Block(self.parse_block(access)?)),
            TokenKind::String(value) if access.is_none() => {
                let value = value.clone();
                let span = self.advance().span.clone();
                Ok(Expression::String(Spanned::new(value, span)))
            }
            TokenKind::Bare(value) if access.is_none() => {
                let value = value.clone();
                let span = self.advance().span.clone();
                Ok(Expression::Atom(Spanned::new(value, span)))
            }
            TokenKind::Identifier(value)
                if access.is_none() && !self.identifier_starts_invocation() =>
            {
                let value = value.clone();
                let span = self.advance().span.clone();
                Ok(Expression::Atom(Spanned::new(value, span)))
            }
            TokenKind::Identifier(_) => Ok(Expression::Invocation(self.parse_invocation(access)?)),
            _ => Err(self.expected("an argument expression")),
        }
    }

    fn parse_reference(&mut self) -> Result<Expression> {
        self.expect(&TokenKind::Dollar, "`$`")?;
        let name = self.expect_identifier("a reference name")?;
        Ok(Expression::Reference(name))
    }

    fn parse_invocation(&mut self, access: Option<Spanned<StackAccess>>) -> Result<Invocation> {
        let span = access
            .as_ref()
            .map_or_else(|| self.current().span.clone(), |access| access.span.clone());
        let name = self.expect_identifier("a program name")?;
        let type_argument = if self.consume(&TokenKind::LeftAngle) {
            let argument = self.parse_type_argument()?;
            self.expect(&TokenKind::RightAngle, "`>` after the type argument")?;
            Some(argument)
        } else {
            None
        };
        let arguments = if self.consume(&TokenKind::LeftParen) {
            self.parse_arguments()?
        } else {
            Vec::new()
        };
        let body = if self.at(&TokenKind::LeftBrace) {
            Some(self.parse_block(None)?)
        } else {
            None
        };
        Ok(Invocation {
            access,
            name,
            type_argument,
            arguments,
            body,
            span,
        })
    }

    fn parse_type_argument(&mut self) -> Result<Spanned<ValueType>> {
        let value = self.expect_identifier("`Video` or `Audio`")?;
        let value_type = match value.value.as_str() {
            "Video" => ValueType::Video,
            "Audio" => ValueType::Audio,
            _ => {
                return Err(Diagnostic::new(
                    "E_INVALID_TYPE_ARGUMENT",
                    "type argument must be `Video` or `Audio`",
                    value.span,
                ));
            }
        };
        Ok(Spanned::new(value_type, value.span))
    }

    fn parse_arguments(&mut self) -> Result<Vec<Argument>> {
        let mut arguments = Vec::new();
        let mut saw_named = false;
        self.skip_newlines();
        if self.consume(&TokenKind::RightParen) {
            return Ok(arguments);
        }
        loop {
            let argument = if matches!(self.current().kind, TokenKind::Identifier(_))
                && self.peek_is(1, &TokenKind::Equal)
            {
                saw_named = true;
                let name = self.expect_identifier("an argument name")?;
                self.expect(&TokenKind::Equal, "`=` after the argument name")?;
                Argument::Named {
                    name,
                    value: self.parse_expression()?,
                }
            } else {
                if saw_named {
                    return Err(Diagnostic::new(
                        "E_POSITIONAL_AFTER_NAMED",
                        "positional arguments must appear before named arguments",
                        self.current().span.clone(),
                    ));
                }
                Argument::Positional(self.parse_expression()?)
            };
            arguments.push(argument);
            self.skip_newlines();
            if self.consume(&TokenKind::RightParen) {
                break;
            }
            self.expect(&TokenKind::Comma, "`,` between arguments")?;
            self.skip_newlines();
            if self.consume(&TokenKind::RightParen) {
                break;
            }
        }
        Ok(arguments)
    }

    fn parse_block(&mut self, access: Option<Spanned<StackAccess>>) -> Result<Block> {
        let span = access
            .as_ref()
            .map_or_else(|| self.current().span.clone(), |access| access.span.clone());
        self.expect(&TokenKind::LeftBrace, "`{`")?;
        self.skip_newlines();
        let mut statements = Vec::new();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::End) {
                return Err(Diagnostic::new(
                    "E_UNTERMINATED_BLOCK",
                    "block is missing its closing `}`",
                    span,
                ));
            }
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }
        self.advance();
        Ok(Block {
            access,
            statements,
            span,
        })
    }

    fn parse_output_bindings(&mut self) -> Result<OutputBindings> {
        if !self.consume_keyword("as") {
            return Ok(OutputBindings::None);
        }
        if !self.consume(&TokenKind::LeftParen) {
            return Ok(OutputBindings::One(
                self.expect_identifier("an output name after `as`")?,
            ));
        }

        let span = self.previous().span.clone();
        self.skip_newlines();
        let mut names = Vec::new();
        loop {
            names.push(self.expect_identifier("an output name")?);
            self.skip_newlines();
            if self.consume(&TokenKind::RightParen) {
                break;
            }
            self.expect(&TokenKind::Comma, "`,` between output names")?;
            self.skip_newlines();
        }
        if names.len() < 2 {
            return Err(Diagnostic::new(
                "E_INVALID_OUTPUT_BINDING",
                "parenthesized output binding must contain at least two names",
                span,
            ));
        }
        Ok(OutputBindings::Many(names, span))
    }

    fn parse_access(&mut self) -> Result<Option<Spanned<StackAccess>>> {
        if !self.consume(&TokenKind::At) {
            return Ok(None);
        }
        let at_span = self.previous().span.clone();
        let access = self.expect_identifier("`owned` or `visible` after `@`")?;
        let value = match access.value.as_str() {
            "owned" => StackAccess::Owned,
            "visible" => StackAccess::Visible,
            _ => {
                return Err(Diagnostic::new(
                    "E_INVALID_STACK_ACCESS",
                    "stack access must be `@owned` or `@visible`",
                    access.span,
                ));
            }
        };
        Ok(Some(Spanned::new(value, at_span)))
    }

    fn expect_statement_end(&mut self, owner: &str) -> Result<()> {
        if self.at(&TokenKind::Newline) {
            self.skip_newlines();
            return Ok(());
        }
        if self.at(&TokenKind::RightBrace) || self.at(&TokenKind::End) {
            return Ok(());
        }
        Err(Diagnostic::new(
            "E_EXPECTED_STATEMENT_END",
            format!("{owner} must end before the next token"),
            self.current().span.clone(),
        ))
    }

    fn identifier_starts_invocation(&self) -> bool {
        self.peek_is(1, &TokenKind::LeftParen)
            || self.peek_is(1, &TokenKind::LeftAngle)
            || self.peek_is(1, &TokenKind::LeftBrace)
    }

    fn expect_identifier(&mut self, expected: &str) -> Result<Spanned<String>> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Identifier(value) => Ok(Spanned::new(value, token.span)),
            _ => Err(Diagnostic::new(
                "E_EXPECTED_TOKEN",
                format!("expected {expected}"),
                token.span,
            )),
        }
    }

    fn expect_keyword(&mut self, keyword: &str, code: &'static str, message: &str) -> Result<()> {
        if self.consume_keyword(keyword) {
            Ok(())
        } else {
            Err(Diagnostic::new(code, message, self.current().span.clone()))
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        if matches!(&self.current().kind, TokenKind::Identifier(value) if value == keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> Result<()> {
        if self.consume(kind) {
            Ok(())
        } else {
            Err(self.expected(expected))
        }
    }

    fn expected(&self, expected: &str) -> Diagnostic {
        Diagnostic::new(
            "E_EXPECTED_TOKEN",
            format!("expected {expected}"),
            self.current().span.clone(),
        )
    }

    fn skip_newlines(&mut self) {
        while self.consume(&TokenKind::Newline) {}
    }

    fn consume(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn peek_is(&self, distance: usize, kind: &TokenKind) -> bool {
        self.tokens.get(self.index + distance).is_some_and(|token| {
            std::mem::discriminant(&token.kind) == std::mem::discriminant(kind)
        })
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn advance(&mut self) -> &Token {
        let current = self.index;
        if !matches!(self.tokens[current].kind, TokenKind::End) {
            self.index += 1;
        }
        &self.tokens[current]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(source: &str) -> SourceFileSyntax {
        parse(SourceFile::new("test.clipasm", source)).expect("valid source")
    }

    #[test]
    fn parses_invocations_references_bodies_and_bindings() {
        let syntax = parse_text(
            "clipasm 1\n\n@visible clip<Audio> {\n  $source_audio\n  trim(0s..45s)\n} as soundtrack\n\nflash(\n  {\n    $opening\n    trim(0s..1s)\n  },\n  after=$main_edit,\n  frames=3,\n) as flashed\n",
        );
        assert_eq!(syntax.version.value, 1);
        assert_eq!(syntax.statements.len(), 2);

        let Expression::Invocation(clip) = &syntax.statements[0].expression else {
            panic!("clip invocation");
        };
        assert_eq!(clip.name.value, "clip");
        assert_eq!(
            clip.access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
        assert_eq!(
            clip.type_argument.as_ref().map(|value| value.value),
            Some(ValueType::Audio)
        );
        assert_eq!(clip.body.as_ref().expect("clip body").statements.len(), 2);
        assert!(matches!(
            syntax.statements[0].output_bindings,
            OutputBindings::One(_)
        ));

        let Expression::Invocation(flash) = &syntax.statements[1].expression else {
            panic!("flash invocation");
        };
        assert_eq!(flash.arguments.len(), 3);
        assert!(matches!(
            flash.arguments[0],
            Argument::Positional(Expression::Block(_))
        ));
        assert!(matches!(flash.arguments[1], Argument::Named { .. }));
    }

    #[test]
    fn parses_structural_stack_blocks_with_ordered_outputs() {
        let syntax = parse_text(
            "clipasm 1\n@visible {\n  image(\"a.png\", 1s)\n  audio(\"b.wav\")\n} as (picture, sound)\n",
        );
        let Expression::Block(block) = &syntax.statements[0].expression else {
            panic!("stack block");
        };
        assert_eq!(
            block.access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
        assert_eq!(block.statements.len(), 2);
        let OutputBindings::Many(names, _) = &syntax.statements[0].output_bindings else {
            panic!("multiple bindings");
        };
        assert_eq!(
            names
                .iter()
                .map(|name| name.value.as_str())
                .collect::<Vec<_>>(),
            vec!["picture", "sound"]
        );
    }

    #[test]
    fn rejects_invalid_ordering_and_incomplete_structure() {
        let positional = parse(SourceFile::new(
            "test.clipasm",
            "clipasm 1\nflash(frames=3, $after)\n",
        ))
        .expect_err("positional after named");
        assert_eq!(positional.code, "E_POSITIONAL_AFTER_NAMED");

        let block = parse(SourceFile::new("test.clipasm", "clipasm 1\n{\n  drop\n"))
            .expect_err("unterminated block");
        assert_eq!(block.code, "E_UNTERMINATED_BLOCK");

        let version = parse(SourceFile::new("test.clipasm", "clipasm 2\ndrop\n"))
            .expect_err("unsupported version");
        assert_eq!(version.code, "E_UNSUPPORTED_VERSION");
    }
}
