pub(crate) const PUBLIC_NAME_GRAMMAR: &str = "[A-Za-z_][A-Za-z0-9_-]*";

pub(crate) fn is_valid_public_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}
