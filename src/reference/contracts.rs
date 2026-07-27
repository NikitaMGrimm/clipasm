//! Sanitized public access to machine-contract versions and support policy.

pub use crate::contracts::{
    MachineContract, MachineContractAudience, MachineContractReference, MachineContractStability,
    MachineContractVersion,
};

/// Return every machine-readable contract in stable reference order.
#[must_use]
pub fn machine_contracts() -> &'static [MachineContractReference] {
    crate::contracts::references()
}

/// Return one machine-readable contract reference.
#[must_use]
pub fn machine_contract(contract: MachineContract) -> &'static MachineContractReference {
    crate::contracts::reference(contract)
}
