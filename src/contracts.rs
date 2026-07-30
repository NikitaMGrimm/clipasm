//! Canonical versions and support policy for machine-readable boundaries.
//!
//! Serialization owners consume the constants in this module. The public
//! [`crate::reference`] view exposes only descriptive, read-only metadata.

/// Current compiled inspection JSON format version.
pub(crate) const COMPILED_INSPECTION_FORMAT_VERSION: u32 = 22;
/// Current prepared inspection JSON format version.
pub(crate) const PREPARED_INSPECTION_FORMAT_VERSION: u32 = 13;
/// Current render manifest format version.
pub(crate) const RENDER_MANIFEST_FORMAT_VERSION: u32 = 1;
/// Current external-program request protocol version.
pub(crate) const EXTERNAL_PROGRAM_PROTOCOL_VERSION: u32 = 1;
/// Current browser render-plan version.
pub(crate) const BROWSER_RENDER_PLAN_VERSION: u32 = 1;
/// Current browser `FFmpeg` recipe contract version.
pub(crate) const BROWSER_RECIPE_CONTRACT_VERSION: u32 = 2;

/// A machine-readable boundary produced by `ClipAsm`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MachineContract {
    /// JSON printed by `clipasm inspect` and [`crate::compiler::CompiledProgram::compiled_json`].
    CompiledInspection,
    /// JSON returned by native `PreparedPlan::prepared_json`.
    PreparedInspection,
    /// JSON manifest published beside a rendered MP4.
    RenderManifest,
    /// JSON request sent to an external program over standard input.
    ExternalProgramRequest,
    /// JSON plan consumed by the shipped browser render host.
    BrowserRenderPlan,
}

impl MachineContract {
    /// Contracts in stable reference order.
    pub const ALL: [Self; 5] = [
        Self::CompiledInspection,
        Self::RenderManifest,
        Self::ExternalProgramRequest,
        Self::PreparedInspection,
        Self::BrowserRenderPlan,
    ];

    /// Return this contract's immutable reference metadata.
    #[must_use]
    pub fn reference(self) -> &'static MachineContractReference {
        reference(self)
    }
}

/// Intended consumer of a machine-readable contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MachineContractAudience {
    /// User tooling and integrations.
    UserTooling,
    /// Executables implementing authored external programs.
    ExternalPrograms,
    /// A host adapter shipped and updated together with `ClipAsm`.
    ShippedHostAdapter,
}

impl MachineContractAudience {
    /// Return a concise human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UserTooling => "User tooling",
            Self::ExternalPrograms => "External programs",
            Self::ShippedHostAdapter => "Shipped host adapter",
        }
    }
}

/// Compatibility promise attached to a machine-readable contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MachineContractStability {
    /// The shape is versioned for consumers outside the `ClipAsm` implementation.
    Versioned,
    /// The shape is an implementation detail of components released together.
    HostInternal,
}

impl MachineContractStability {
    /// Return a concise human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Versioned => "Versioned integration contract",
            Self::HostInternal => "Host-internal contract",
        }
    }
}

/// One version discriminator carried by a machine-readable document.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MachineContractVersion {
    field: &'static str,
    value: u32,
    meaning: &'static str,
}

impl MachineContractVersion {
    /// Return the JSON field containing this version.
    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }

    /// Return the current version value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }

    /// Return what this discriminator versions.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        self.meaning
    }
}

/// Read-only reference facts for one machine-readable contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineContractReference {
    contract: MachineContract,
    slug: &'static str,
    title: &'static str,
    summary: &'static str,
    audience: MachineContractAudience,
    stability: MachineContractStability,
    versions: &'static [MachineContractVersion],
}

impl MachineContractReference {
    /// Return the typed contract identifier.
    #[must_use]
    pub const fn contract(self) -> MachineContract {
        self.contract
    }

    /// Return the stable documentation anchor slug.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        self.slug
    }

    /// Return the user-facing title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        self.title
    }

    /// Return the contract's concise purpose.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.summary
    }

    /// Return the intended consumer group.
    #[must_use]
    pub const fn audience(self) -> MachineContractAudience {
        self.audience
    }

    /// Return the compatibility policy.
    #[must_use]
    pub const fn stability(self) -> MachineContractStability {
        self.stability
    }

    /// Return every version discriminator carried by this contract.
    #[must_use]
    pub const fn versions(self) -> &'static [MachineContractVersion] {
        self.versions
    }

    /// Return the generated-guide route for this contract.
    #[must_use]
    pub fn documentation_route(self) -> String {
        format!("reference/machine-contracts.html#{}", self.slug)
    }
}

const COMPILED_VERSIONS: [MachineContractVersion; 1] = [MachineContractVersion {
    field: "format_version",
    value: COMPILED_INSPECTION_FORMAT_VERSION,
    meaning: "compiled inspection document shape",
}];
const PREPARED_VERSIONS: [MachineContractVersion; 1] = [MachineContractVersion {
    field: "format_version",
    value: PREPARED_INSPECTION_FORMAT_VERSION,
    meaning: "prepared inspection document shape",
}];
const MANIFEST_VERSIONS: [MachineContractVersion; 1] = [MachineContractVersion {
    field: "format_version",
    value: RENDER_MANIFEST_FORMAT_VERSION,
    meaning: "render manifest shape",
}];
const EXTERNAL_VERSIONS: [MachineContractVersion; 1] = [MachineContractVersion {
    field: "protocol_version",
    value: EXTERNAL_PROGRAM_PROTOCOL_VERSION,
    meaning: "external-program request protocol",
}];
const BROWSER_VERSIONS: [MachineContractVersion; 2] = [
    MachineContractVersion {
        field: "version",
        value: BROWSER_RENDER_PLAN_VERSION,
        meaning: "browser render-plan envelope",
    },
    MachineContractVersion {
        field: "recipe_contract",
        value: BROWSER_RECIPE_CONTRACT_VERSION,
        meaning: "FFmpeg argument and artifact recipe contract",
    },
];

static REFERENCES: [MachineContractReference; 5] = [
    MachineContractReference {
        contract: MachineContract::CompiledInspection,
        slug: "compiled-inspection-json",
        title: "Compiled inspection JSON",
        summary: "A source-independent view of compiled semantics for inspection and tooling.",
        audience: MachineContractAudience::UserTooling,
        stability: MachineContractStability::Versioned,
        versions: &COMPILED_VERSIONS,
    },
    MachineContractReference {
        contract: MachineContract::RenderManifest,
        slug: "render-manifest",
        title: "Render manifest",
        summary: "Path-free metadata published beside a rendered MP4.",
        audience: MachineContractAudience::UserTooling,
        stability: MachineContractStability::Versioned,
        versions: &MANIFEST_VERSIONS,
    },
    MachineContractReference {
        contract: MachineContract::ExternalProgramRequest,
        slug: "external-program-request",
        title: "External-program request",
        summary: "The JSON request sent to a trusted external program over standard input.",
        audience: MachineContractAudience::ExternalPrograms,
        stability: MachineContractStability::Versioned,
        versions: &EXTERNAL_VERSIONS,
    },
    MachineContractReference {
        contract: MachineContract::PreparedInspection,
        slug: "prepared-inspection-json",
        title: "Prepared inspection JSON",
        summary: "A local debugging view of resolved assets, tools, and renderer primitives.",
        audience: MachineContractAudience::ShippedHostAdapter,
        stability: MachineContractStability::HostInternal,
        versions: &PREPARED_VERSIONS,
    },
    MachineContractReference {
        contract: MachineContract::BrowserRenderPlan,
        slug: "browser-render-plan",
        title: "Browser render plan",
        summary: "Virtual paths, FFmpeg argument arrays, and artifact contracts for the shipped browser host.",
        audience: MachineContractAudience::ShippedHostAdapter,
        stability: MachineContractStability::HostInternal,
        versions: &BROWSER_VERSIONS,
    },
];

pub(crate) const fn references() -> &'static [MachineContractReference] {
    &REFERENCES
}

pub(crate) fn reference(contract: MachineContract) -> &'static MachineContractReference {
    REFERENCES
        .iter()
        .find(|reference| reference.contract == contract)
        .expect("every machine contract has one reference entry")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{MachineContract, MachineContractStability, references};

    #[test]
    fn catalog_is_complete_unique_and_well_formed() {
        let references = references();
        assert_eq!(references.len(), MachineContract::ALL.len());

        let mut contracts = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        for reference in references {
            assert!(contracts.insert(reference.contract()));
            assert!(slugs.insert(reference.slug()));
            assert!(!reference.title().trim().is_empty());
            assert!(!reference.summary().trim().is_empty());
            assert!(!reference.versions().is_empty());
            for version in reference.versions() {
                assert!(!version.field().trim().is_empty());
                assert!(version.value() > 0);
                assert!(!version.meaning().trim().is_empty());
            }
        }
        assert_eq!(
            contracts,
            MachineContract::ALL.into_iter().collect::<BTreeSet<_>>()
        );
        assert_eq!(
            references
                .iter()
                .filter(|reference| {
                    reference.stability() == MachineContractStability::Versioned
                })
                .count(),
            3
        );
    }
}
