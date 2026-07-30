//! Monotonic type-variable storage and unification.

use crate::model::ValueType;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct TypeVarId(u32);

impl TypeVarId {
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct TypeDomain(u8);

impl TypeDomain {
    const VIDEO: u8 = 0b01;
    const AUDIO: u8 = 0b10;
    pub(super) const TIMELINE: Self = Self(Self::VIDEO | Self::AUDIO);

    #[must_use]
    pub(super) const fn from_value_type(value_type: ValueType) -> Self {
        match value_type {
            ValueType::Video => Self(Self::VIDEO),
            ValueType::Audio => Self(Self::AUDIO),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn contains(self, value_type: ValueType) -> bool {
        let candidate = Self::from_value_type(value_type);
        candidate.0 != 0 && self.0 & candidate.0 == candidate.0
    }

    #[must_use]
    pub(super) const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub(super) const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    #[must_use]
    pub(super) const fn overlaps(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub(super) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub(super) const fn concrete(self) -> Option<ValueType> {
        match self.0 {
            Self::VIDEO => Some(ValueType::Video),
            Self::AUDIO => Some(ValueType::Audio),
            _ => None,
        }
    }
}

impl From<ValueType> for TypeDomain {
    fn from(value_type: ValueType) -> Self {
        Self::from_value_type(value_type)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TypeConflict {
    pub(super) left: TypeDomain,
    pub(super) right: TypeDomain,
}

#[derive(Clone, Debug)]
struct TypeNode {
    parent: TypeVarId,
    domain: TypeDomain,
    rank: u8,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TypeArena {
    nodes: Vec<TypeNode>,
    revision: u64,
}

impl TypeArena {
    #[must_use]
    pub(super) fn allocate(&mut self) -> TypeVarId {
        self.allocate_domain(TypeDomain::TIMELINE)
    }

    #[must_use]
    pub(super) fn allocate_exact(&mut self, value_type: ValueType) -> TypeVarId {
        self.allocate_domain(value_type.into())
    }

    pub(super) fn equate(
        &mut self,
        left: TypeVarId,
        right: TypeVarId,
    ) -> std::result::Result<(), TypeConflict> {
        let left_root = self.root(left);
        let right_root = self.root(right);
        let left_domain = self.nodes[left_root.index()].domain;
        let right_domain = self.nodes[right_root.index()].domain;
        let intersection = left_domain.intersection(right_domain);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: left_domain,
                right: right_domain,
            });
        }
        if left_root == right_root {
            return Ok(());
        }

        let left_rank = self.nodes[left_root.index()].rank;
        let right_rank = self.nodes[right_root.index()].rank;
        let (parent, child) = if left_rank < right_rank {
            (right_root, left_root)
        } else {
            (left_root, right_root)
        };

        self.nodes[child.index()].parent = parent;
        self.nodes[parent.index()].domain = intersection;
        if left_rank == right_rank {
            self.nodes[parent.index()].rank = self.nodes[parent.index()]
                .rank
                .checked_add(1)
                .expect("type arena rank overflow");
        }
        self.bump_revision();
        Ok(())
    }

    pub(super) fn constrain(
        &mut self,
        variable: TypeVarId,
        value_type: ValueType,
    ) -> std::result::Result<(), TypeConflict> {
        let root = self.root(variable);
        let current = self.nodes[root.index()].domain;
        let required = TypeDomain::from(value_type);
        let intersection = current.intersection(required);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: current,
                right: required,
            });
        }
        if intersection != current {
            self.nodes[root.index()].domain = intersection;
            self.bump_revision();
        }
        Ok(())
    }

    pub(super) fn constrain_domain(
        &mut self,
        variable: TypeVarId,
        required: TypeDomain,
    ) -> std::result::Result<(), TypeConflict> {
        let root = self.root(variable);
        let current = self.nodes[root.index()].domain;
        let intersection = current.intersection(required);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: current,
                right: required,
            });
        }
        if intersection != current {
            self.nodes[root.index()].domain = intersection;
            self.bump_revision();
        }
        Ok(())
    }

    #[must_use]
    pub(super) fn domain(&self, variable: TypeVarId) -> TypeDomain {
        self.nodes[self.root(variable).index()].domain
    }

    #[must_use]
    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    fn allocate_domain(&mut self, domain: TypeDomain) -> TypeVarId {
        let raw = u32::try_from(self.nodes.len()).expect("too many type variables");
        let variable = TypeVarId(raw);
        self.nodes.push(TypeNode {
            parent: variable,
            domain,
            rank: 0,
        });
        self.bump_revision();
        variable
    }

    fn root(&self, variable: TypeVarId) -> TypeVarId {
        let mut current = variable;
        loop {
            let parent = self.nodes[current.index()].parent;
            if parent == current {
                return current;
            }
            current = parent;
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("type arena revision overflow");
    }
}
