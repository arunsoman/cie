//! Guarded contract registry (TASK-038 authority API, §8 global rule 4).
//!
//! The registry is the single write path into the contract store. Its guards
//! make the §8 invariants *unrepresentable* rather than merely discouraged:
//!
//! - `Naming`/`Protocol` obligations can only enter as
//!   `Authority::InferredCandidate` — rule-mining modules have no API that
//!   could insert them with any other authority (FR-10/FR-28).
//! - `LlmCandidate` is inert (acceptance #28): it can never be upgraded to
//!   `Explicit` or `Verified` — not by `insert`, not by `upgrade_to_verified`.
//! - `InferredCandidate → Verified` flows only through
//!   [`ContractRegistry::upgrade_to_verified`], which demands a mechanical
//!   proof string (the checker run that survived); the upgrade is recorded
//!   in the contract's `expr` audit trail.
//! - `Explicit` requires a determinate source: `Docstring` or `Claim(_)` —
//!   a `NamingConvention` source can never pose as established.

use crate::{Authority, Contract, ContractSource, ObligationKind};
use cie_core::protocol::CieError;
use cie_core::types::{ContractId, SymbolId};
use std::collections::BTreeMap;

/// Next free contract id; ids are assigned densely from 1.
pub type NextId = u64;

/// The guarded registry over [`Contract`] values (§11).
#[derive(Debug, Clone, Default)]
pub struct ContractRegistry {
    contracts: Vec<Contract>,
    /// symbol → contract indices that mention it (for `contracts_for`).
    by_symbol: BTreeMap<SymbolId, Vec<usize>>,
    next_id: u64,
}

impl ContractRegistry {
    pub fn new() -> Self {
        Self { contracts: vec![], by_symbol: BTreeMap::new(), next_id: 0 }
    }

    /// §8/global rule 4 guard: the single insertion path.
    ///
    /// Rejects:
    /// - `Naming`/`Protocol` obligations whose authority is anything but
    ///   `InferredCandidate` (rule-mining must stay a hypothesis);
    /// - `Explicit` authority from a `NamingConvention` source;
    /// - `Explicit`/`Verified` authority arriving from an `LlmCandidate`
    ///   construction site (the inert-proposal rule, acceptance #28).
    pub fn insert(&mut self, mut contract: Contract) -> Result<ContractId, CieError> {
        if matches!(contract.obligation, ObligationKind::Naming | ObligationKind::Protocol)
            && contract.authority != Authority::InferredCandidate
        {
            return Err(CieError::UnsupportedConstruct(format!(
                "obligation `{}` may only enter the registry as InferredCandidate \
                 (§8 rule 4), got `{}`",
                contract.obligation.as_str(),
                contract.authority.as_str()
            )));
        }
        if contract.authority == Authority::Explicit
            && matches!(contract.source, ContractSource::NamingConvention)
        {
            return Err(CieError::UnsupportedConstruct(
                "NamingConvention source can never be Explicit (FR-10): naming \
                 rules are inferred candidates, not established requirements"
                    .into(),
            ));
        }
        if matches!(contract.source, ContractSource::Llm)
            && contract.authority.is_decisive()
        {
            return Err(CieError::UnsupportedConstruct(
                "LlmCandidate is inert (acceptance #28): an LLM proposal can \
                 never be inserted as Explicit/Verified; re-derive \
                 deterministically instead"
                    .into(),
            ));
        }
        self.next_id += 1;
        contract.id = self.next_id as ContractId;
        let idx = self.contracts.len();
        for sym in &contract.symbols {
            self.by_symbol.entry(*sym).or_default().push(idx);
        }
        let id = contract.id;
        self.contracts.push(contract);
        Ok(id)
    }

    /// The TASK-038/§8 guarded transition: the ONLY way an
    /// `InferredCandidate` becomes `Verified`. Requires the mechanical
    /// proof (the checker/bounded-check evidence string); empty proofs are
    /// rejected, and `LlmCandidate` sources are refused outright.
    pub fn upgrade_to_verified(
        &mut self,
        id: ContractId,
        mechanical_proof: &str,
    ) -> Result<Authority, CieError> {
        if mechanical_proof.trim().is_empty() {
            return Err(CieError::UnsupportedConstruct(
                "upgrade_to_verified requires a mechanical proof string \
                 (the checker run that survived)"
                    .into(),
            ));
        }
        let c = self
            .contracts
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(CieError::DanglingReference { what: format!("contract {id:?}") })?;
        match c.authority {
            Authority::LlmCandidate => Err(CieError::UnsupportedConstruct(
                "LlmCandidate is inert (acceptance #28): it can never become \
                 Verified; re-derive the rule deterministically instead"
                    .into(),
            )),
            Authority::InferredCandidate => {
                c.authority = Authority::Verified;
                c.expr = format!("{} [verified: {}]", c.expr, mechanical_proof);
                Ok(c.authority)
            }
            other => Ok(other), // Explicit/Verified stay as they are (idempotent)
        }
    }

    /// All contracts that mention `sym` (sketch: `contracts_for`).
    pub fn contracts_for(&self, sym: SymbolId) -> Vec<&Contract> {
        self.by_symbol
            .get(&sym)
            .map(|idx| idx.iter().map(|i| &self.contracts[*i]).collect())
            .unwrap_or_default()
    }

    pub fn all(&self) -> &[Contract] {
        &self.contracts
    }

    pub fn get(&self, id: ContractId) -> Option<&Contract> {
        self.contracts.iter().find(|c| c.id == id)
    }

    pub fn len(&self) -> usize {
        self.contracts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }
}
