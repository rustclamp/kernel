//! Typed capability resolution for Clamp applications.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;

use rustclamp_core::{
    Capability, CapabilityId, Contribution, ContributionId, ContributionTarget,
    ContributionTargetId, Module, ModuleId, Provides, Qualifier, QualifierId, Requires,
};

/// Caller-owned declarations for one typed contribution target and qualifier.
pub struct TargetComposition<T: ContributionTarget, Q: Qualifier> {
    contributions: Vec<(ModuleId, T::Contribution)>,
    target: PhantomData<(T, Q)>,
}

impl<T: ContributionTarget, Q: Qualifier> TargetComposition<T, Q> {
    /// Creates a target composition from contributor identities and declarations.
    pub fn new(contributions: Vec<(ModuleId, T::Contribution)>) -> Self {
        Self {
            contributions,
            target: PhantomData,
        }
    }

    /// Consumes declarations into the selected target's runtime form.
    ///
    /// If no target is selected, required declarations produce a structured
    /// unconsumed-contribution error. Optional declarations are discarded. An
    /// empty composition without a target is a no-op.
    pub fn build(
        self,
        target: Option<&T>,
    ) -> Result<Option<T::Runtime>, TargetCompositionError<T::Error>> {
        let Some(target) = target else {
            let mut contributors = self
                .contributions
                .iter()
                .filter(|_| T::Contribution::REQUIRED)
                .map(|(module, _)| *module)
                .collect::<Vec<_>>();
            if contributors.is_empty() {
                return Ok(None);
            }
            contributors.sort_unstable();
            return Err(TargetCompositionError::UnconsumedRequired {
                target: T::ID,
                contribution: T::Contribution::ID,
                qualifier: Q::ID,
                contributors,
            });
        };

        target
            .build(&self.contributions)
            .map(Some)
            .map_err(TargetCompositionError::Target)
    }
}

/// A target-owned build failure or an unconsumed required declaration.
#[derive(Debug, Eq, PartialEq)]
pub enum TargetCompositionError<E> {
    /// The active composition has required declarations but no matching target.
    UnconsumedRequired {
        /// The target that was expected to consume the declarations.
        target: ContributionTargetId,
        /// The contribution kind that was left unconsumed.
        contribution: ContributionId,
        /// The qualifier whose active composition contains the declarations.
        qualifier: QualifierId,
        /// Modules that contributed required declarations.
        contributors: Vec<ModuleId>,
    },
    /// The selected target rejected the declarations.
    Target(E),
}

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

    /// Creates a provision from a module's additive [`Provides`] contract.
    pub fn from_module<M: Provides<C>>(module: &'a M) -> Self {
        Self::new(<M as Module>::ID, module.provided_value())
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

/// One module's requirement for a single typed capability.
pub struct CapabilityRequirement<C: Capability> {
    required_by: ModuleId,
    selected: Option<ModuleId>,
    capability: PhantomData<C>,
}

impl<C: Capability> Copy for CapabilityRequirement<C> {}

impl<C: Capability> Clone for CapabilityRequirement<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Capability> CapabilityRequirement<C> {
    /// Declares a requirement with an optional explicit provider selection.
    pub const fn new(required_by: ModuleId, selected: Option<ModuleId>) -> Self {
        Self {
            required_by,
            selected,
            capability: PhantomData,
        }
    }

    /// Creates a requirement from a module's additive [`Requires`] contract.
    pub fn from_module<M: Requires<C>>() -> Self {
        Self::new(<M as Module>::ID, M::SELECTED_PROVIDER)
    }

    /// Returns the module that owns this requirement.
    pub const fn required_by(self) -> ModuleId {
        self.required_by
    }

    /// Returns the explicitly selected provider, if one was declared.
    pub const fn selected_provider(self) -> Option<ModuleId> {
        self.selected
    }
}

/// A caller-owned, single-capability composition snapshot.
///
/// This prototype stores declarations only; it does not construct modules or
/// keep global state. Edits return a new snapshot so callers can validate the
/// resulting graph before adopting it.
pub struct CapabilityComposition<'a, C: Capability> {
    provisions: Vec<Provision<'a, C>>,
    requirements: Vec<CapabilityRequirement<C>>,
}

impl<'a, C: Capability> CapabilityComposition<'a, C> {
    /// Creates a composition snapshot from providers and consumer requirements.
    pub fn new(
        provisions: Vec<Provision<'a, C>>,
        requirements: Vec<CapabilityRequirement<C>>,
    ) -> Self {
        Self {
            provisions,
            requirements,
        }
    }

    /// Removes a module's declarations and revalidates the remaining graph.
    ///
    /// Requirements owned by other modules remain, even when the removed module
    /// was their selected provider; validation then reports the broken edge.
    pub fn without_module(&self, module: ModuleId) -> Self {
        Self {
            provisions: self
                .provisions
                .iter()
                .filter(|provision| provision.module != module)
                .map(|provision| Provision::new(provision.module, provision.value))
                .collect(),
            requirements: self
                .requirements
                .iter()
                .copied()
                .filter(|requirement| requirement.required_by != module)
                .collect(),
        }
    }

    /// Returns the active consumer requirements in this snapshot.
    pub fn requirements(&self) -> &[CapabilityRequirement<C>] {
        &self.requirements
    }

    /// Replaces one provider for this capability while retaining module
    /// requirements and redirecting explicit selections to the replacement.
    /// Returns `None` when `replaced` does not currently provide this capability.
    pub fn replace_provider(
        &self,
        replaced: ModuleId,
        replacement: Provision<'a, C>,
    ) -> Option<Self> {
        if !self
            .provisions
            .iter()
            .any(|provision| provision.module == replaced)
        {
            return None;
        }

        let mut provisions = self
            .provisions
            .iter()
            .filter(|provision| provision.module != replaced)
            .map(|provision| Provision::new(provision.module, provision.value))
            .collect::<Vec<_>>();
        let replacement_module = replacement.module;
        provisions.push(replacement);
        let requirements = self
            .requirements
            .iter()
            .map(|requirement| {
                CapabilityRequirement::new(
                    requirement.required_by,
                    (requirement.selected == Some(replaced))
                        .then_some(replacement_module)
                        .or(requirement.selected),
                )
            })
            .collect();

        Some(Self::new(provisions, requirements))
    }

    /// Resolves one declared requirement against this snapshot.
    pub fn resolve_requirement(
        &self,
        requirement: CapabilityRequirement<C>,
    ) -> Result<&'a C::Value, CompositionError> {
        Resolver::resolve::<C>(
            requirement.required_by,
            &self.provisions,
            requirement.selected,
        )
    }

    /// Revalidates every consumer requirement in this snapshot.
    pub fn validate(&self) -> Result<(), CompositionError> {
        for requirement in &self.requirements {
            self.resolve_requirement(*requirement)?;
        }
        Ok(())
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

/// A directed construction dependency from a module to a module it needs.
///
/// An edge `consumer -> dependency` means the dependency must be constructed
/// before the consumer. The graph only validates construction order; it does
/// not construct modules or support lazy/proxy edges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionDependency {
    consumer: ModuleId,
    dependency: ModuleId,
}

impl ConstructionDependency {
    /// Declares that `consumer` requires `dependency` during construction.
    pub const fn new(consumer: ModuleId, dependency: ModuleId) -> Self {
        Self {
            consumer,
            dependency,
        }
    }

    /// Returns the module that has the dependency.
    pub const fn consumer(self) -> ModuleId {
        self.consumer
    }

    /// Returns the module required during construction.
    pub const fn dependency(self) -> ModuleId {
        self.dependency
    }
}

/// A caller-owned set of construction dependencies for cycle validation.
#[derive(Clone, Debug, Default)]
pub struct ConstructionGraph {
    dependencies: Vec<ConstructionDependency>,
}

impl ConstructionGraph {
    /// Creates a graph from module-to-module construction dependencies.
    pub fn new(dependencies: Vec<ConstructionDependency>) -> Self {
        Self { dependencies }
    }

    /// Returns the declared construction dependencies.
    pub fn dependencies(&self) -> &[ConstructionDependency] {
        &self.dependencies
    }

    /// Rejects a construction cycle and returns its closed module path.
    ///
    /// For example, `A -> B -> C -> A` reports `[A, B, C, A]`. Traversal is
    /// sorted by stable module identity, so the selected diagnostic is
    /// independent of dependency registration order.
    pub fn validate(&self) -> Result<(), ConstructionCycle> {
        let mut adjacency = BTreeMap::<ModuleId, Vec<ModuleId>>::new();
        for edge in &self.dependencies {
            adjacency
                .entry(edge.consumer)
                .or_default()
                .push(edge.dependency);
            adjacency.entry(edge.dependency).or_default();
        }

        // Deduplicate and sort once so traversal and diagnostics are stable.
        for dependencies in adjacency.values_mut() {
            dependencies.sort_unstable();
            dependencies.dedup();
        }

        let mut states = BTreeMap::<ModuleId, VisitState>::new();
        let mut path = Vec::new();
        for module in adjacency.keys().copied() {
            if states
                .get(&module)
                .copied()
                .unwrap_or(VisitState::Unvisited)
                == VisitState::Unvisited
            {
                states.insert(module, VisitState::Visiting);
                path.push(module);
                let mut stack = vec![(module, 0usize)];

                while let Some((current, next_index)) = stack.last_mut() {
                    let dependencies = &adjacency[current];
                    if *next_index == dependencies.len() {
                        let (finished, _) = stack.pop().expect("stack is non-empty");
                        path.pop();
                        states.insert(finished, VisitState::Visited);
                        continue;
                    }

                    let dependency = dependencies[*next_index];
                    *next_index += 1;
                    match states
                        .get(&dependency)
                        .copied()
                        .unwrap_or(VisitState::Unvisited)
                    {
                        VisitState::Unvisited => {
                            states.insert(dependency, VisitState::Visiting);
                            path.push(dependency);
                            stack.push((dependency, 0));
                        }
                        VisitState::Visiting => {
                            let start = path
                                .iter()
                                .position(|item| *item == dependency)
                                .expect("visiting modules are in the active path");
                            let mut cycle = path[start..].to_vec();
                            cycle.push(dependency);
                            return Err(ConstructionCycle { path: cycle });
                        }
                        VisitState::Visited => {}
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// A construction cycle with the repeated start module closing its path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionCycle {
    path: Vec<ModuleId>,
}

impl ConstructionCycle {
    /// Returns the dependency path, including the repeated start module.
    pub fn path(&self) -> &[ModuleId] {
        &self.path
    }
}

impl fmt::Display for ConstructionCycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("construction cycle: ")?;
        for (index, module) in self.path.iter().enumerate() {
            if index > 0 {
                formatter.write_str(" -> ")?;
            }
            formatter.write_str(module.as_str())?;
        }
        Ok(())
    }
}

impl Error for ConstructionCycle {}

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
