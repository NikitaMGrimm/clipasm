use crate::diagnostic::{Diagnostic, Result};
use crate::model::{SourceTime, SourceTimeRange};
use crate::program::{ParameterType, ParameterValue};
use crate::source::{Literal, SourceSpan};

pub(super) fn from_literal(
    program: &str,
    parameter: &str,
    parameter_type: &ParameterType,
    argument: &Literal,
) -> Result<ParameterValue> {
    match (parameter_type, argument) {
        (ParameterType::Integer, Literal::Integer(value, _)) => Ok(ParameterValue::Integer(*value)),
        (ParameterType::File, Literal::String(value, _)) => Ok(ParameterValue::File(value.into())),
        (ParameterType::Duration, Literal::String(value, span)) => {
            Ok(ParameterValue::Duration(SourceTime::parse(value, span)?))
        }
        (ParameterType::TimeRange, Literal::String(value, span)) => Ok(ParameterValue::TimeRange(
            SourceTimeRange::parse(value, span)?,
        )),
        (ParameterType::Keyword(allowed), Literal::String(value, span)) => {
            keyword(program, parameter, allowed, value, span)
        }
        _ => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!("parameter `{program}.{parameter}` has the wrong value type"),
            argument.span().clone(),
        )),
    }
}

pub(super) fn from_text(
    program: &str,
    parameter: &str,
    parameter_type: &ParameterType,
    value: &str,
    span: &SourceSpan,
) -> Result<ParameterValue> {
    match parameter_type {
        ParameterType::Integer => value
            .parse::<i64>()
            .map(ParameterValue::Integer)
            .map_err(|_| {
                Diagnostic::new(
                    "E_INVALID_ARGUMENT_TYPE",
                    format!("parameter `{program}.{parameter}` must be an integer"),
                    span.clone(),
                )
            }),
        ParameterType::File => Ok(ParameterValue::File(value.into())),
        ParameterType::Duration => Ok(ParameterValue::Duration(SourceTime::parse(value, span)?)),
        ParameterType::TimeRange => Ok(ParameterValue::TimeRange(SourceTimeRange::parse(
            value, span,
        )?)),
        ParameterType::Keyword(allowed) => keyword(program, parameter, allowed, value, span),
    }
}

fn keyword(
    program: &str,
    parameter: &str,
    allowed: &[String],
    value: &str,
    span: &SourceSpan,
) -> Result<ParameterValue> {
    allowed
        .iter()
        .find(|candidate| candidate.as_str() == value)
        .cloned()
        .map(ParameterValue::Keyword)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_ARGUMENT_VALUE",
                format!(
                    "parameter `{program}.{parameter}` must be one of: {}",
                    allowed.join(", ")
                ),
                span.clone(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::ParameterValue;

    fn span() -> SourceSpan {
        SourceSpan::file_start("test.yaml")
    }

    #[test]
    fn converts_authored_literals_and_caller_text() {
        assert_eq!(
            from_literal(
                "repeat",
                "count",
                &ParameterType::Integer,
                &Literal::Integer(3, span()),
            )
            .expect("integer literal"),
            ParameterValue::Integer(3)
        );
        assert_eq!(
            from_text("repeat", "count", &ParameterType::Integer, "3", &span())
                .expect("integer text"),
            ParameterValue::Integer(3)
        );
        assert!(matches!(
            from_text(
                "image",
                "path",
                &ParameterType::File,
                "card.png",
                &span(),
            )
            .expect("file text"),
            ParameterValue::File(path) if path == std::path::Path::new("card.png")
        ));
    }

    #[test]
    fn rejects_invalid_text_and_keywords() {
        let integer = from_text("repeat", "count", &ParameterType::Integer, "many", &span())
            .expect_err("invalid integer");
        assert_eq!(integer.code, "E_INVALID_ARGUMENT_TYPE");

        let keyword = from_text(
            "image",
            "fit",
            &ParameterType::Keyword(vec!["cover".to_owned(), "contain".to_owned()]),
            "crop",
            &span(),
        )
        .expect_err("invalid keyword");
        assert_eq!(keyword.code, "E_INVALID_ARGUMENT_VALUE");
    }
}
