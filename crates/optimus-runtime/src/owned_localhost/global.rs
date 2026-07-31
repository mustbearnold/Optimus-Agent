//! Process-wide registry sharing and failed-owner quarantine.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use optimus_store::Store;

use super::{OwnedLocalhostLeaseRegistry, OwnedLocalhostRevocationReason, RetainedOwnedServer};

type SharedRegistry = Arc<Mutex<OwnedLocalhostLeaseRegistry>>;
type FailedOwners = HashMap<String, Vec<Box<dyn RetainedOwnedServer>>>;

struct RegistryEntry {
    registry: Weak<Mutex<OwnedLocalhostLeaseRegistry>>,
    clients: usize,
}

static REGISTRIES: OnceLock<Mutex<HashMap<String, RegistryEntry>>> = OnceLock::new();
static FAILED_OWNERS: OnceLock<Mutex<FailedOwners>> = OnceLock::new();

pub(super) fn registry_for(scope_key: String, workspace_sha256: String) -> SharedRegistry {
    let registries = REGISTRIES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registries = registries
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(entry) = registries.get_mut(&scope_key) {
        if let Some(registry) = entry.registry.upgrade() {
            entry.clients = entry.clients.saturating_add(1);
            return registry;
        }
    }
    let registry = Arc::new(Mutex::new(OwnedLocalhostLeaseRegistry::new(
        scope_key.clone(),
        workspace_sha256,
    )));
    registries.insert(
        scope_key,
        RegistryEntry {
            registry: Arc::downgrade(&registry),
            clients: 1,
        },
    );
    registry
}

pub(super) fn release_registry(scope_key: &str, registry: &SharedRegistry, store: &Store) {
    let registries = REGISTRIES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registries = registries
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(entry) = registries.get_mut(scope_key) else {
        return;
    };
    entry.clients = entry.clients.saturating_sub(1);
    if entry.clients != 0 {
        return;
    }
    registries.remove(scope_key);
    let mut registry = registry.lock().unwrap_or_else(|poison| poison.into_inner());
    let _ = registry.revoke_all(store, OwnedLocalhostRevocationReason::RuntimeDrop);
}

pub(super) fn retain_failed_owner(key: String, owner: Box<dyn RetainedOwnedServer>) {
    FAILED_OWNERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(key)
        .or_default()
        .push(owner);
}

pub(super) fn has_failed_owner(key: &str) -> bool {
    FAILED_OWNERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(key)
        .is_some_and(|owners| !owners.is_empty())
}

pub(super) fn retry_failed_owners() -> usize {
    let owners = FAILED_OWNERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut owners = owners.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut still_pending = FailedOwners::new();
    let mut cleaned = 0;
    for (key, pending) in owners.drain() {
        for mut owner in pending {
            if owner.terminate_and_confirm().is_ok() {
                cleaned += 1;
            } else {
                still_pending.entry(key.clone()).or_default().push(owner);
            }
        }
    }
    *owners = still_pending;
    cleaned
}
