//! Task-driven pack rotation (spec-006 R5).
//!
//! [`super::CapabilitySession::provision`] rotates the least-recently-used
//! on-demand pack out at the count ceiling so a task can reach any pack
//! without holding more than the footprint; `release` frees a slot and its
//! schema tokens. The swap is atomic: a schema-budget failure leaves the
//! session untouched.
//!
//! Recency is measured on USE, not activation: [`super::CapabilitySession::touch`]
//! is called by the kernel after every successful tool dispatch, so a pack
//! that is actively working is never the victim. `activations` is the
//! LRU-order list, maintained by activation (deduped push), use (touch
//! moves to the end) and eviction (retain-remove).
//!
//! Packs with no available tools (ADR-0068 placeholders such as `devex`,
//! `social`, `home`, `office`, and the all-unavailable `desktop`) cannot be
//! provisioned: rotating a real pack out for zero capability is
//! destructive, so the agent-facing path refuses with a typed
//! [`PackError::EmptyPack`]. Direct `activate` stays permissive so
//! persisted prefs from older sessions still restore.
//!
//! Split out of `lib.rs` to keep the baselined module shrinking (module
//! ratchet: baselined modules may only shrink; new code lands in new
//! modules).

use super::{PackError, PackId};

impl super::CapabilitySession {
    pub fn provision_str(&mut self, name: &str) -> Result<(), PackError> {
        let pack = PackId::parse(name).ok_or_else(|| PackError::UnknownPack(name.into()))?;
        self.provision(pack)
    }

    /// Record a use of the given pack: move it to the most-recent end of the
    /// LRU order. The kernel calls this after every successful tool dispatch
    /// (see `tool_dispatch.rs`), so eviction reflects working-set recency.
    pub fn touch(&mut self, pack: PackId) {
        if self.loaded.contains(&pack) {
            self.activations.retain(|p| *p != pack);
            self.activations.push(pack);
        }
    }

    /// Touch the pack that owns `tool` (no-op when the tool is not loaded).
    pub fn touch_pack_of(&mut self, tool: &str) {
        let owner = self.loaded.iter().find(|p| {
            self.catalog
                .get(*p)
                .is_some_and(|desc| desc.tools.iter().any(|t| t.id.as_str() == tool))
        });
        if let Some(pack) = owner {
            self.touch(*pack);
        }
    }

    pub fn deactivate(&mut self, pack: PackId) -> Result<(), PackError> {
        if pack.is_core() {
            return Err(PackError::CorePinned);
        }
        if !self.loaded.remove(&pack) {
            return Err(PackError::NotLoaded(pack.as_str().into()));
        }
        // Prune the LRU order so a released pack cannot shadow recency.
        self.activations.retain(|p| *p != pack);
        Ok(())
    }

    pub fn deactivate_str(&mut self, name: &str) -> Result<(), PackError> {
        let pack = PackId::parse(name).ok_or_else(|| PackError::UnknownPack(name.into()))?;
        self.deactivate(pack)
    }

    fn pack_has_available_tools(&self, pack: &PackId) -> bool {
        self.catalog
            .get(pack)
            .is_some_and(|desc| desc.tools.iter().any(|t| t.is_available()))
    }

    /// Provision a pack for the current task, rotating the least-recently
    /// used on-demand pack out when the count ceiling is reached.
    ///
    /// The rotation is atomic: either the eviction AND activation both
    /// happen, or neither does. When the new pack would exceed the schema
    /// budget even with the LRU victim evicted, the session is left
    /// untouched and the typed [`PackError::SchemaBudget`] is returned.
    pub fn provision(&mut self, pack: PackId) -> Result<(), PackError> {
        if !self.catalog.contains_key(&pack) {
            return Err(PackError::UnknownPack(pack.as_str().into()));
        }
        if self.loaded.contains(&pack) {
            return Ok(());
        }
        // Empty packs would evict a real one for zero capability.
        if !pack.is_core() && !self.pack_has_available_tools(&pack) {
            return Err(PackError::EmptyPack(pack.as_str().into()));
        }
        if !pack.is_core() && self.on_demand_count() >= self.config.max_on_demand_packs {
            let victim = self
                .activations
                .iter()
                .find(|p| self.loaded.contains(p) && !p.is_core())
                .cloned()
                .ok_or_else(|| PackError::PackLimit {
                    loaded: self.on_demand_count(),
                    max: self.config.max_on_demand_packs,
                })?;
            let add = self
                .catalog
                .get(&pack)
                .ok_or_else(|| PackError::UnknownPack(pack.as_str().into()))?
                .checked_schema_tokens()?;
            let victim_tokens = self
                .catalog
                .get(&victim)
                .ok_or_else(|| PackError::UnknownPack(victim.as_str().into()))?
                .checked_schema_tokens()?;
            let would_be = self
                .checked_loaded_schema_tokens()?
                .checked_sub(victim_tokens)
                .and_then(|tokens| tokens.checked_add(add))
                .ok_or_else(|| PackError::SchemaTokenOverflow {
                    scope: "provision swap".into(),
                })?;
            if would_be > self.config.max_schema_tokens {
                return Err(PackError::SchemaBudget {
                    would_be,
                    max: self.config.max_schema_tokens,
                });
            }
            self.loaded.remove(&victim);
            self.activations.retain(|p| *p != victim);
        }
        self.activate(pack)
    }
}
