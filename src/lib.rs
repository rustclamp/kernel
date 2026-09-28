//! Typed capability resolution for Clamp applications.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;

use rustclamp_core::{Capability, CapabilityId, ModuleId};

/// A capability implementation associated with the module that supplies it.
///
/// This is a graph relationship, not a provider base class. The value may be
/// borrowed from an object owned and constructed by the application.
pub struct Provision<'a, C: Capability> {
    module: ModuleId,
    value: &'a C::Value,
}

impl<'a, C: Capability> Provision<'a, C> {
    /// Associates a module identity with a value implementing capability `C`.
    pub fn new(module: ModuleId, value: &'a C::Value) -> Self {
        Self { module, value }
    }

    /// Returns the module identity associated with this provision.
    pub const fn module(&self) -> ModuleId {
        self.module
    }
}

/// The structured category of a capability composition failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositionErrorKind {
    /// No module provides a required capability.
    MissingProvider,
    /// More than one module provides the capability and none was selected.
    AmbiguousProviders,
    /// The selected module is not among the available provisions.
    ProviderSelectionUnavailable,
}

/// A composition failure with stable identities and candidate context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionError {
    kind: CompositionErrorKind,
    required_by: ModuleId,
    capability: CapabilityId,
    candidates: Vec<ModuleId>,
}

impl CompositionError {
    fn new(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        candidates: Vec<ModuleId>,
    ) -> Self {
        Self {
            kind,
            required_by,
            capability,
            candidates,
        }
    }

    /// Returns the structured failure category.
    pub const fn kind(&self) -> CompositionErrorKind {
        self.kind
    }

    /// Returns the module whose requirement could not be resolved.
    pub const fn required_by(&self) -> ModuleId {
        self.required_by
    }

    /// Returns the stable identity of the missing or ambiguous capability.
    pub const fn capability(&self) -> CapabilityId {
        self.capability
    }

    /// Returns candidate module identities in stable sorted order.
    pub fn candidates(&self) -> &[ModuleId] {
        &self.candidates
    }
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} for capability '{}' required by module '{}'",
            self.kind,
            self.capability.as_str(),
            self.required_by.as_str()
        )?;
        if !self.candidates.is_empty() {
            formatter.write_str("; candidates: ")?;
            for (index, candidate) in self.candidates.iter().enumerate() {
                if index > 0 {
                    formatter.write_str(", ")?;
                }
                formatter.write_str(candidate.as_str())?;
            }
        }
        Ok(())
    }
}

impl Error for CompositionError {}

/// Resolves one typed requirement from the provisions declared for it.
pub struct Resolver;

impl Resolver {
    /// Resolves a unique provision, or the provision named by `selected`.
    ///
    /// Registration order never selects a winner. Multiple candidates without
    /// explicit selection produce a structured ambiguity error. No global
    /// registry, runtime type map, or allocation is needed on the success path.
    pub fn resolve<'value, C: Capability>(
        required_by: ModuleId,
        provisions: &[Provision<'value, C>],
        selected: Option<ModuleId>,
    ) -> Result<&'value C::Value, CompositionError> {
        let capability = C::ID;
        if let Some(selected) = selected {
            let mut matches = provisions
                .iter()
                .filter(|provision| provision.module == selected);
            if let Some(provision) = matches.next() {
                if matches.next().is_none() {
                    return Ok(provision.value);
                }
                return Err(Self::error(
                    CompositionErrorKind::AmbiguousProviders,
                    required_by,
                    capability,
                    provisions,
                ));
            }
            return Err(Self::error(
                CompositionErrorKind::ProviderSelectionUnavailable,
                required_by,
                capability,
                provisions,
            ));
        }

        match provisions {
            [] => Err(Self::error(
                CompositionErrorKind::MissingProvider,
                required_by,
                capability,
                provisions,
            )),
            [provision] => Ok(provision.value),
            _ => Err(Self::error(
                CompositionErrorKind::AmbiguousProviders,
                required_by,
                capability,
                provisions,
            )),
        }
    }

    fn error<C: Capability>(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        provisions: &[Provision<'_, C>],
    ) -> CompositionError {
        let mut candidates = provisions.iter().map(Provision::module).collect::<Vec<_>>();
        candidates.sort_unstable();
        CompositionError::new(kind, required_by, capability, candidates)
    }
}
