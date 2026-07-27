use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::{ScalarExpression, Spanned};

use super::draft::{BodyId, DraftBody, DraftInput, DraftItemKind};
use super::ids::ScalarAliasId;

#[derive(Debug)]
pub(super) struct ScalarAliasDeclaration {
    pub(super) scope: BodyId,
    pub(super) name: Spanned<String>,
    pub(super) expression: ScalarExpression,
}

#[derive(Debug)]
struct ScalarScope {
    parent: Option<BodyId>,
    aliases: BTreeMap<String, ScalarAliasId>,
    ordered_aliases: Vec<ScalarAliasId>,
}

#[derive(Debug)]
pub(super) struct ScalarScopes {
    scopes: Vec<ScalarScope>,
    aliases: Vec<ScalarAliasDeclaration>,
}

impl ScalarScopes {
    pub(super) fn build(
        root: &DraftBody,
        body_count: usize,
        reserved_names: &BTreeSet<String>,
    ) -> Result<Self> {
        let mut builder = ScalarScopeBuilder {
            scopes: std::iter::repeat_with(|| None).take(body_count).collect(),
            aliases: Vec::new(),
            reserved_names,
        };
        builder.collect_body(root, None)?;
        Ok(Self {
            scopes: builder
                .scopes
                .into_iter()
                .enumerate()
                .map(|(index, scope)| {
                    scope.unwrap_or_else(|| panic!("draft body {index} has no scalar scope"))
                })
                .collect(),
            aliases: builder.aliases,
        })
    }

    pub(super) fn resolve(&self, mut scope: BodyId, name: &str) -> Option<ScalarAliasId> {
        loop {
            let current = &self.scopes[scope.index()];
            if let Some(alias) = current.aliases.get(name) {
                return Some(*alias);
            }
            scope = current.parent?;
        }
    }

    pub(super) fn local_aliases(&self, scope: BodyId) -> &[ScalarAliasId] {
        &self.scopes[scope.index()].ordered_aliases
    }

    pub(super) fn declaration(&self, alias: ScalarAliasId) -> &ScalarAliasDeclaration {
        &self.aliases[alias.index()]
    }

    pub(super) fn alias_count(&self) -> usize {
        self.aliases.len()
    }
}

struct ScalarScopeBuilder<'a> {
    scopes: Vec<Option<ScalarScope>>,
    aliases: Vec<ScalarAliasDeclaration>,
    reserved_names: &'a BTreeSet<String>,
}

impl ScalarScopeBuilder<'_> {
    fn collect_body(&mut self, body: &DraftBody, parent: Option<BodyId>) -> Result<()> {
        let mut aliases = BTreeMap::new();
        let mut ordered_aliases = Vec::new();
        for item in &body.items {
            let DraftItemKind::ScalarBinding { name, value } = &item.kind else {
                continue;
            };
            if self.reserved_names.contains(&name.value) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_NAME",
                    format!(
                        "scalar alias `{}` conflicts with a program input, parameter, or named graph value",
                        name.value
                    ),
                    name.span.clone(),
                ));
            }
            if aliases.contains_key(&name.value) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_NAME",
                    format!("duplicate scalar alias `{}` in the same body", name.value),
                    name.span.clone(),
                ));
            }
            if parent.is_some_and(|scope| self.resolve(scope, &name.value).is_some()) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_NAME",
                    format!("scalar alias `{}` shadows a visible alias", name.value),
                    name.span.clone(),
                ));
            }
            let id = ScalarAliasId(u32::try_from(self.aliases.len()).map_err(|_| {
                Diagnostic::new(
                    "E_GRAPH_TOO_LARGE",
                    "too many scalar aliases were declared",
                    name.span.clone(),
                )
            })?);
            aliases.insert(name.value.clone(), id);
            ordered_aliases.push(id);
            self.aliases.push(ScalarAliasDeclaration {
                scope: body.id,
                name: name.clone(),
                expression: value.clone(),
            });
        }
        let previous = self.scopes[body.id.index()].replace(ScalarScope {
            parent,
            aliases,
            ordered_aliases,
        });
        assert!(
            previous.is_none(),
            "draft body scalar scope was built twice"
        );

        for item in &body.items {
            match &item.kind {
                DraftItemKind::Reference(_) | DraftItemKind::ScalarBinding { .. } => {}
                DraftItemKind::Invocation(invocation) => {
                    for input in invocation.inputs.iter().flatten() {
                        if let DraftInput::Body(child) = input {
                            self.collect_body(child, Some(body.id))?;
                        }
                    }
                    if let Some(child) = invocation.body.as_deref() {
                        self.collect_body(child, Some(body.id))?;
                    }
                }
                DraftItemKind::StackBlock(block) => {
                    self.collect_body(&block.body, Some(body.id))?;
                }
            }
        }
        Ok(())
    }

    fn resolve(&self, mut scope: BodyId, name: &str) -> Option<ScalarAliasId> {
        loop {
            let current = self.scopes[scope.index()]
                .as_ref()
                .expect("parent scalar scope is built before child scopes");
            if let Some(alias) = current.aliases.get(name) {
                return Some(*alias);
            }
            scope = current.parent?;
        }
    }
}
