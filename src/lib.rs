//! Typed capability resolution for Clamp applications.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;
use std::marker::PhantomData;

use rustclamp_core::{Capability, CapabilityId, ModuleId, Qualifier, QualifierId};

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

    /// Returns the provided capability value.
    pub const fn value(&self) -> &'a C::Value {
        self.value
    }
}

/// A capability provision distinguished by a compile-time qualifier type.
pub struct QualifiedProvision<'a, C: Capability, Q: Qualifier> {
    module: ModuleId,
    value: &'a C::Value,
    qualifier: PhantomData<Q>,
}

impl<'a, C: Capability, Q: Qualifier> QualifiedProvision<'a, C, Q> {
    /// Associates a module and a typed qualifier with a capability value.
    pub fn new(module: ModuleId, value: &'a C::Value) -> Self {
        Self {
            module,
            value,
            qualifier: PhantomData,
        }
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
    qualifier: Option<QualifierId>,
    candidates: Vec<ModuleId>,
}

impl CompositionError {
    fn new(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        candidates: Vec<ModuleId>,
    ) -> Self {
        Self {
            kind,
            required_by,
            capability,
            qualifier,
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

    /// Returns the qualifier identity when the failed requirement was qualified.
    pub const fn qualifier(&self) -> Option<QualifierId> {
        self.qualifier
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
        if let Some(qualifier) = self.qualifier {
            write!(formatter, " qualified as '{}'", qualifier.as_str())?;
        }
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
        Self::resolve_candidates(
            required_by,
            capability,
            None,
            provisions
                .iter()
                .map(|provision| (provision.module, provision.value)),
            provisions.len(),
            selected,
        )
    }

    /// Resolves an optional requirement without installing a default provider.
    /// Zero candidates returns `Ok(None)`, one returns its value, and multiple
    /// candidates remain ambiguous.
    pub fn resolve_optional<'value, C: Capability>(
        required_by: ModuleId,
        provisions: &[Provision<'value, C>],
    ) -> Result<Option<&'value C::Value>, CompositionError> {
        match provisions {
            [] => Ok(None),
            [provision] => Ok(Some(provision.value)),
            _ => Err(Self::error(
                CompositionErrorKind::AmbiguousProviders,
                required_by,
                C::ID,
                None,
                provisions
                    .iter()
                    .map(|provision| (provision.module, provision.value)),
            )),
        }
    }

    /// Returns every provider for a many-valued requirement.
    ///
    /// Results are sorted by module identity for stable inspection only. This
    /// order does not define provider execution or dependency order.
    pub fn resolve_many<'value, C: Capability>(
        provisions: &[Provision<'value, C>],
    ) -> Vec<Provision<'value, C>> {
        let mut resolved = provisions
            .iter()
            .map(|provision| Provision::new(provision.module, provision.value))
            .collect::<Vec<_>>();
        resolved.sort_unstable_by_key(Provision::module);
        resolved
    }

    /// Resolves a provision for one typed qualifier, excluding other qualifiers
    /// from candidate counting and diagnostics.
    pub fn resolve_qualified<'value, C: Capability, Q: Qualifier>(
        required_by: ModuleId,
        provisions: &[QualifiedProvision<'value, C, Q>],
        selected: Option<ModuleId>,
    ) -> Result<&'value C::Value, CompositionError> {
        Self::resolve_candidates(
            required_by,
            C::ID,
            Some(Q::ID),
            provisions
                .iter()
                .map(|provision| (provision.module, provision.value)),
            provisions.len(),
            selected,
        )
    }

    fn resolve_candidates<'value, V: ?Sized + 'value>(
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        mut provisions: impl Iterator<Item = (ModuleId, &'value V)> + Clone,
        count: usize,
        selected: Option<ModuleId>,
    ) -> Result<&'value V, CompositionError> {
        if let Some(selected) = selected {
            let mut matches = provisions.clone().filter(|(module, _)| *module == selected);
            if let Some((_, value)) = matches.next() {
                if matches.next().is_none() {
                    return Ok(value);
                }
                return Err(Self::error(
                    CompositionErrorKind::AmbiguousProviders,
                    required_by,
                    capability,
                    qualifier,
                    provisions,
                ));
            }
            return Err(Self::error(
                CompositionErrorKind::ProviderSelectionUnavailable,
                required_by,
                capability,
                qualifier,
                provisions,
            ));
        }

        match count {
            0 => Err(Self::error(
                CompositionErrorKind::MissingProvider,
                required_by,
                capability,
                qualifier,
                provisions,
            )),
            1 => Ok(provisions.next().expect("count checked").1),
            _ => Err(Self::error(
                CompositionErrorKind::AmbiguousProviders,
                required_by,
                capability,
                qualifier,
                provisions,
            )),
        }
    }

    fn error<'value, V: ?Sized + 'value>(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        provisions: impl Iterator<Item = (ModuleId, &'value V)>,
    ) -> CompositionError {
        let mut candidates = provisions.map(|(module, _)| module).collect::<Vec<_>>();
        candidates.sort_unstable();
        CompositionError::new(kind, required_by, capability, qualifier, candidates)
    }
}
