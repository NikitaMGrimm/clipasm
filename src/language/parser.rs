use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{ParameterType, StackAccess};
use crate::source::{SourceFile, SourceSpan, Spanned};

use super::lexer::{Token, TokenKind, lex};
use super::syntax::{
    Argument, Block, ConfigDeclaration, Declaration, Expression, ExternalDeclaration,
    InputDeclaration, Invocation, OutputBindings, ParameterDeclaration, PathDeclaration, Scalar,
    SourceFileSyntax, Statement, VideoConfigDeclaration,
};

pub(crate) fn parse(source: SourceFile) -> Result<SourceFileSyntax> {
    let span = SourceSpan::source_start(source.clone());
    Parser::new(lex(source)?).parse_file(span)
}

pub(crate) fn accepts_invocation_name(name: &str) -> bool {
    let source = SourceFile::new("<program-name>", format!("clipasm 1\n{name}()\n"));
    parse(source).is_ok()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    syntax_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            syntax_depth: 0,
        }
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

        let mut declarations = Vec::new();
        let mut statements = Vec::new();
        let mut saw_statement = false;
        self.skip_newlines();
        while !self.at(&TokenKind::End) {
            if self.starts_declaration() {
                if saw_statement {
                    return Err(Diagnostic::new(
                        "E_DECLARATION_AFTER_STATEMENT",
                        "file declarations must appear before executable statements",
                        self.current().span.clone(),
                    ));
                }
                declarations.push(self.parse_declaration()?);
                self.expect_statement_end("declaration")?;
            } else {
                saw_statement = true;
                statements.push(self.parse_statement()?);
            }
            self.skip_newlines();
        }
        Ok(SourceFileSyntax {
            version,
            declarations,
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

    fn parse_declaration(&mut self) -> Result<Declaration> {
        match self.current_identifier() {
            Some("config") => self.parse_config().map(Declaration::Config),
            Some("import") => self
                .parse_path_declaration("import")
                .map(Declaration::Import),
            Some("external") => self.parse_external().map(Declaration::External),
            Some("input") => self.parse_input().map(Declaration::Input),
            Some("param") => self.parse_parameter().map(Declaration::Parameter),
            _ => Err(self.expected("a file declaration")),
        }
    }

    fn parse_config(&mut self) -> Result<ConfigDeclaration> {
        let span = self.current().span.clone();
        self.expect_keyword("config", "E_EXPECTED_TOKEN", "expected `config`")?;
        self.expect(&TokenKind::LeftBrace, "`{` after `config`")?;
        self.skip_newlines();

        let mut video = None;
        let mut output = None;
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::End) {
                return Err(Diagnostic::new(
                    "E_UNTERMINATED_CONFIG",
                    "config block is missing its closing `}`",
                    span,
                ));
            }
            let field = self.expect_identifier("a config field")?;
            match field.value.as_str() {
                "video" => {
                    if video.is_some() {
                        return Err(duplicate_declaration_field("config", "video", field.span));
                    }
                    video = Some(self.parse_video_config(field.span)?);
                }
                "output" => {
                    if output.is_some() {
                        return Err(duplicate_declaration_field("config", "output", field.span));
                    }
                    self.expect(&TokenKind::Equal, "`=` after `output`")?;
                    output = Some(self.expect_string("an output path")?);
                }
                _ => {
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_CONFIG_FIELD",
                        format!("unknown config field `{}`", field.value),
                        field.span,
                    ));
                }
            }
            self.expect_statement_end("config field")?;
        }
        self.advance();
        Ok(ConfigDeclaration {
            video,
            output,
            span,
        })
    }

    fn parse_video_config(&mut self, span: SourceSpan) -> Result<VideoConfigDeclaration> {
        self.expect(&TokenKind::LeftBrace, "`{` after `video`")?;
        self.skip_newlines();
        let mut width = None;
        let mut height = None;
        let mut fps = None;
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::End) {
                return Err(Diagnostic::new(
                    "E_UNTERMINATED_CONFIG",
                    "video config block is missing its closing `}`",
                    span,
                ));
            }
            let field = self.expect_identifier("a video config field")?;
            self.expect(&TokenKind::Equal, "`=` after the video config field")?;
            let value = self.expect_scalar_text("a video config value")?;
            let target = match field.value.as_str() {
                "width" => &mut width,
                "height" => &mut height,
                "fps" => &mut fps,
                _ => {
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_VIDEO_FIELD",
                        format!("unknown video config field `{}`", field.value),
                        field.span,
                    ));
                }
            };
            if target.replace(value).is_some() {
                return Err(duplicate_declaration_field(
                    "video config",
                    &field.value,
                    field.span,
                ));
            }
            self.expect_statement_end("video config field")?;
        }
        self.advance();
        Ok(VideoConfigDeclaration {
            width,
            height,
            fps,
            span,
        })
    }

    fn parse_path_declaration(&mut self, keyword: &str) -> Result<PathDeclaration> {
        let span = self.current().span.clone();
        self.advance();
        let path = self.expect_string(&format!("a path after `{keyword}`"))?;
        self.expect_keyword(
            "as",
            "E_MISSING_IMPORT_ALIAS",
            &format!("`{keyword}` requires `as alias`"),
        )?;
        let alias = self.expect_identifier("an import alias")?;
        Ok(PathDeclaration { path, alias, span })
    }

    fn parse_external(&mut self) -> Result<ExternalDeclaration> {
        let span = self.current().span.clone();
        self.expect_keyword("external", "E_EXPECTED_TOKEN", "expected `external`")?;
        self.expect(&TokenKind::LeftBrace, "`{` after `external`")?;
        self.skip_newlines();

        let mut command = None;
        let mut semantic_version = None;
        let mut preserve = None;
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::End) {
                return Err(Diagnostic::new(
                    "E_UNTERMINATED_EXTERNAL",
                    "external block is missing its closing `}`",
                    span,
                ));
            }
            let field = self.expect_identifier("an external field")?;
            self.expect(&TokenKind::Equal, "`=` after the external field")?;
            match field.value.as_str() {
                "command" => {
                    if command.is_some() {
                        return Err(duplicate_declaration_field(
                            "external", "command", field.span,
                        ));
                    }
                    command = Some(self.expect_string("an executable path or name")?);
                }
                "semantic_version" => {
                    if semantic_version.is_some() {
                        return Err(duplicate_declaration_field(
                            "external",
                            "semantic_version",
                            field.span,
                        ));
                    }
                    semantic_version = Some(self.expect_scalar_text("an unsigned integer")?);
                }
                "preserve" => {
                    if preserve.is_some() {
                        return Err(duplicate_declaration_field(
                            "external", "preserve", field.span,
                        ));
                    }
                    preserve = Some(self.expect_identifier("an input name")?);
                }
                _ => {
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_EXTERNAL_FIELD",
                        format!("unknown external field `{}`", field.value),
                        field.span,
                    ));
                }
            }
            self.expect_statement_end("external field")?;
        }
        self.advance();
        Ok(ExternalDeclaration {
            command,
            semantic_version,
            preserve,
            span,
        })
    }

    fn parse_input(&mut self) -> Result<InputDeclaration> {
        let span = self.current().span.clone();
        self.advance();
        let name = self.expect_identifier("an input name")?;
        self.expect(&TokenKind::Colon, "`:` after the input name")?;
        let value_type = self.parse_value_type("an input type")?;
        Ok(InputDeclaration {
            name,
            value_type,
            span,
        })
    }

    fn parse_parameter(&mut self) -> Result<ParameterDeclaration> {
        let span = self.current().span.clone();
        self.advance();
        let name = self.expect_identifier("a parameter name")?;
        self.expect(&TokenKind::Colon, "`:` after the parameter name")?;
        let parameter_type = self.parse_parameter_type()?;
        let default = if self.consume(&TokenKind::Equal) {
            Some(self.parse_scalar()?)
        } else {
            None
        };
        Ok(ParameterDeclaration {
            name,
            parameter_type,
            default,
            span,
        })
    }

    fn parse_value_type(&mut self, expected: &str) -> Result<Spanned<ValueType>> {
        let name = self.expect_identifier(expected)?;
        let value = ValueType::from_source_name(&name.value).ok_or_else(|| {
            Diagnostic::new(
                "E_UNKNOWN_VALUE_TYPE",
                format!(
                    "unknown value type `{}`; expected `Video` or `Audio`",
                    name.value
                ),
                name.span.clone(),
            )
        })?;
        Ok(Spanned::new(value, name.span))
    }

    fn parse_parameter_type(&mut self) -> Result<Spanned<ParameterType>> {
        let name = self.expect_identifier("a parameter type")?;
        let keyword_values = if name.value == "Keyword" {
            self.expect(&TokenKind::LeftParen, "`(` after `Keyword`")?;
            self.skip_newlines();
            if self.at(&TokenKind::RightParen) {
                return Err(Diagnostic::new(
                    "E_MISSING_KEYWORD_VALUES",
                    "a `Keyword` parameter requires at least one allowed value",
                    name.span,
                ));
            }
            let mut values = Vec::new();
            loop {
                values.push(self.expect_identifier("an allowed keyword value")?.value);
                self.skip_newlines();
                if self.consume(&TokenKind::RightParen) {
                    break;
                }
                self.expect(&TokenKind::Comma, "`,` between keyword values")?;
                self.skip_newlines();
            }
            Some(values)
        } else {
            None
        };
        let parameter_type = ParameterType::from_source_name(&name.value, keyword_values)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_UNKNOWN_PARAMETER_TYPE",
                    format!("unknown parameter type `{}`", name.value),
                    name.span.clone(),
                )
            })?;
        Ok(Spanned::new(parameter_type, name.span))
    }

    fn parse_scalar(&mut self) -> Result<Scalar> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::String(value) => Ok(Scalar::String(Spanned::new(value, token.span))),
            TokenKind::Bare(value) | TokenKind::Identifier(value) => {
                Ok(Scalar::Atom(Spanned::new(value, token.span)))
            }
            _ => Err(Diagnostic::new(
                "E_EXPECTED_TOKEN",
                "expected a scalar default value",
                token.span,
            )),
        }
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
        self.with_syntax_nesting(span.clone(), |parser| {
            parser.parse_invocation_inner(access, span)
        })
    }

    fn parse_invocation_inner(
        &mut self,
        access: Option<Spanned<StackAccess>>,
        span: SourceSpan,
    ) -> Result<Invocation> {
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
        let value_type = ValueType::from_source_name(&value.value).ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_TYPE_ARGUMENT",
                "type argument must be `Video` or `Audio`",
                value.span.clone(),
            )
        })?;
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
        self.with_syntax_nesting(span.clone(), |parser| {
            parser.parse_block_inner(access, span)
        })
    }

    fn parse_block_inner(
        &mut self,
        access: Option<Spanned<StackAccess>>,
        span: SourceSpan,
    ) -> Result<Block> {
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

    fn with_syntax_nesting<T>(
        &mut self,
        span: SourceSpan,
        parse: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if self.syntax_depth >= crate::source::MAX_SYNTAX_NESTING {
            return Err(Diagnostic::new(
                "E_SYNTAX_NESTING_DEPTH",
                format!(
                    "source syntax nesting exceeds the supported depth of {}",
                    crate::source::MAX_SYNTAX_NESTING
                ),
                span,
            ));
        }
        self.syntax_depth += 1;
        let result = parse(self);
        self.syntax_depth -= 1;
        result
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

    fn starts_declaration(&self) -> bool {
        matches!(
            self.current_identifier(),
            Some("config" | "import" | "external" | "input" | "param")
        )
    }

    fn current_identifier(&self) -> Option<&str> {
        match &self.current().kind {
            TokenKind::Identifier(value) => Some(value),
            _ => None,
        }
    }

    fn expect_string(&mut self, expected: &str) -> Result<Spanned<String>> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::String(value) => Ok(Spanned::new(value, token.span)),
            _ => Err(Diagnostic::new(
                "E_EXPECTED_TOKEN",
                format!("expected {expected}"),
                token.span,
            )),
        }
    }

    fn expect_scalar_text(&mut self, expected: &str) -> Result<Spanned<String>> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::String(value) | TokenKind::Bare(value) | TokenKind::Identifier(value) => {
                Ok(Spanned::new(value, token.span))
            }
            _ => Err(Diagnostic::new(
                "E_EXPECTED_TOKEN",
                format!("expected {expected}"),
                token.span,
            )),
        }
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

fn duplicate_declaration_field(owner: &str, field: &str, span: SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_DUPLICATE_DECLARATION_FIELD",
        format!("duplicate {owner} field `{field}`"),
        span,
    )
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
    fn indentation_is_ignored_but_newlines_separate_statements() {
        let compact = parse_text("clipasm 1\nclip { image(\"card.png\", 1s) } as card\n$card\n");
        assert_eq!(compact.statements.len(), 2);

        let irregular =
            parse_text("clipasm 1\n\tclip {\nimage(\"card.png\", 1s)\n        zoom(8)\n}\n");
        let Expression::Invocation(clip) = &irregular.statements[0].expression else {
            panic!("clip invocation");
        };
        assert_eq!(clip.body.as_ref().expect("clip body").statements.len(), 2);

        let error = parse(SourceFile::new(
            "test.clipasm",
            "clipasm 1\nclip { image(\"card.png\", 1s) zoom(8) }\n",
        ))
        .expect_err("two statements require a newline");
        assert_eq!(error.code, "E_EXPECTED_STATEMENT_END");
    }

    #[test]
    fn rejects_syntax_nesting_before_recursive_descent_overflows() {
        let mut source = String::from("clipasm 1\n");
        for _ in 0..=crate::source::MAX_SYNTAX_NESTING {
            source.push_str("{\n");
        }
        source.push_str("image(\"card.png\", 1s)\n");
        for _ in 0..=crate::source::MAX_SYNTAX_NESTING {
            source.push_str("}\n");
        }

        let error =
            parse(SourceFile::new("deep.clipasm", source)).expect_err("excessive body nesting");
        assert_eq!(error.code, "E_SYNTAX_NESTING_DEPTH");
    }

    #[test]
    fn rejects_deeply_nested_invocation_arguments() {
        let mut expression = String::from("image(\"card.png\", 1s)");
        for _ in 0..=crate::source::MAX_SYNTAX_NESTING {
            expression = format!("repeat({expression}, 1)");
        }
        let source = format!("clipasm 1\n{expression}\n");

        let error = parse(SourceFile::new("deep-expression.clipasm", source))
            .expect_err("excessive expression nesting");
        assert_eq!(error.code, "E_SYNTAX_NESTING_DEPTH");
    }

    #[test]
    fn parses_file_declarations_before_execution() {
        let syntax = parse_text(
            "clipasm 1\n\nconfig {\n  video {\n    width = 1920\n    height = 1080\n    fps = 30000/1001\n  }\n  output = \"generated/final.mp4\"\n}\n\nimport \"programs/polish.clipasm\" as polish\nexternal {\n  command = \"./brighten.py\"\n  semantic_version = 1\n  preserve = source\n}\ninput source: Video\nparam title: File = \"assets/title.png\"\nparam duration: Duration = 2s\nparam fit: Keyword(contain, cover, stretch) = contain\n",
        );
        assert_eq!(syntax.declarations.len(), 7);
        let Declaration::Config(config) = &syntax.declarations[0] else {
            panic!("config declaration");
        };
        let video = config.video.as_ref().expect("video config");
        assert_eq!(
            video.width.as_ref().map(|value| value.value.as_str()),
            Some("1920")
        );
        assert_eq!(
            video.fps.as_ref().map(|value| value.value.as_str()),
            Some("30000/1001")
        );
        assert_eq!(
            config.output.as_ref().map(|value| value.value.as_str()),
            Some("generated/final.mp4")
        );

        let Declaration::Parameter(parameter) = &syntax.declarations[6] else {
            panic!("keyword parameter");
        };
        assert_eq!(
            parameter.parameter_type.value,
            ParameterType::Keyword(vec![
                "contain".to_owned(),
                "cover".to_owned(),
                "stretch".to_owned(),
            ])
        );
        assert!(matches!(parameter.default, Some(Scalar::Atom(_))));
        assert!(syntax.statements.is_empty());
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

        let late_declaration = parse(SourceFile::new(
            "test.clipasm",
            "clipasm 1\ndrop\nparam count: Integer = 2\n",
        ))
        .expect_err("declaration after execution");
        assert_eq!(late_declaration.code, "E_DECLARATION_AFTER_STATEMENT");

        let empty_keywords = parse(SourceFile::new(
            "test.clipasm",
            "clipasm 1\nparam fit: Keyword()\n",
        ))
        .expect_err("empty keyword declaration");
        assert_eq!(empty_keywords.code, "E_MISSING_KEYWORD_VALUES");
    }
}
