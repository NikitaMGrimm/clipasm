use crate::source::{Item, ItemKind, ItemOrigin, OutputBindings, SourceSpan, SurfaceVisibility};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Sugar {
    Clip,
}

impl Sugar {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Clip => "clip",
        }
    }
}

pub(super) fn resolve(name: &str) -> Option<Sugar> {
    match name {
        "clip" => Some(Sugar::Clip),
        _ => None,
    }
}

pub(super) struct Expansion {
    sugar: Sugar,
    authored: ItemOrigin,
}

impl Expansion {
    pub(super) fn new(sugar: Sugar, span: SourceSpan) -> Self {
        Self {
            sugar,
            authored: ItemOrigin::authored(sugar.name(), span),
        }
    }

    pub(super) fn visible(
        &self,
        role: &str,
        kind: ItemKind,
        output_bindings: OutputBindings,
    ) -> Item {
        self.item(role, SurfaceVisibility::Visible, kind, output_bindings)
    }

    pub(super) fn hidden(&self, role: &str, kind: ItemKind) -> Item {
        self.item(role, SurfaceVisibility::Hidden, kind, OutputBindings::None)
    }

    fn item(
        &self,
        role: &str,
        visibility: SurfaceVisibility,
        kind: ItemKind,
        output_bindings: OutputBindings,
    ) -> Item {
        Item {
            kind,
            output_bindings,
            origin: self.authored.expanded(self.sugar.name(), role, visibility),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_registered_sugars() {
        assert_eq!(resolve("clip"), Some(Sugar::Clip));
        assert_eq!(resolve("concat"), None);
    }
}
