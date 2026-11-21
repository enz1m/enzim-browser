use super::{Result, Vault};
use enzui_services::credential_store::{
    CancelHandle, CredentialKey, CredentialOutcome, CredentialStore, PendingCredentialOp, Secret,
    connect_for_app_with_mutation_owner,
    mutation_owner::{CredentialMutationOwner, CredentialMutationStore},
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) type ActiveOperation = Arc<Mutex<Option<CancelHandle>>>;

struct Client {
    store: Arc<dyn CredentialStore>,
    mutations: Option<CredentialMutationStore>,
    _owner: Option<CredentialMutationOwner>,
    runtime: tokio::runtime::Runtime,
}

#[derive(Clone, Default)]
pub(super) struct DesktopContext(Arc<Mutex<ConnectionState>>);
#[derive(Default)]
struct ConnectionState {
    /// When the keyring was last asked without an answer.
    attempted_at: Option<std::time::Instant>,
    client: Option<Arc<Client>>,
}

pub(super) struct DesktopUnlock {
    key: CredentialKey,
    active: ActiveOperation,
    client: Option<Arc<Client>>,
    context: DesktopContext,
    uncertain: bool,
}

impl DesktopUnlock {
    pub(super) fn new(path: &Path, active: ActiveOperation) -> Self {
        Self::for_record(path, "vault", "Password vault unlock", active)
    }

    pub(super) fn for_record(
        path: &Path,
        namespace: &str,
        label: &str,
        active: ActiveOperation,
    ) -> Self {
        let owner = format!(
            "{namespace}-{:x}",
            Sha256::digest(path.as_os_str().as_encoded_bytes())
        );
        Self {
            key: CredentialKey::new("org.enzim.Browser", owner, label),
            active,
            client: None,
            context: DesktopContext::default(),
            uncertain: false,
        }
    }

    pub(super) fn use_context(&mut self, context: DesktopContext) {
        self.context = context;
        self.client = None;
    }
    fn connect(&mut self) -> Option<Arc<Client>> {
        if let Some(client) = &self.client {
            return Some(Arc::clone(client));
        }
        let mut state = self
            .context
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A keyring that did not answer is asked again after a while: one
        // slow start of the desktop's service must not turn the keyring off
        // for the rest of the session.
        let retry = state.client.is_none()
            && state
                .attempted_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(30));
        if retry {
            state.attempted_at = Some(std::time::Instant::now());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            let (owner, store) = runtime
                .block_on(async {
                    tokio::time::timeout(
                        Duration::from_secs(5),
                        connect_for_app_with_mutation_owner("org.enzim.Browser"),
                    )
                    .await
                })
                .ok()?
                .ok()?;
            state.client = Some(Arc::new(Client {
                store: Arc::new(store.clone()),
                mutations: Some(store),
                _owner: Some(owner),
                runtime,
            }));
        }
        self.client.clone_from(&state.client);
        self.client.clone()
    }

    fn run(&self, operation: PendingCredentialOp) -> CredentialOutcome {
        let Some(client) = self.client.as_ref() else {
            return CredentialOutcome::Unavailable;
        };
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(operation.cancel_handle());
        let result = client.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(35), operation)
                .await
                .unwrap_or(CredentialOutcome::Cancelled)
        });
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        result
    }

    pub(super) fn retrieve(&mut self) -> Result<Option<Secret>> {
        let key = self.key.clone();
        let Some(client) = self.connect() else {
            return Ok(None);
        };
        let operation = client.store.get(key);
        match self.run(operation) {
            CredentialOutcome::Retrieved(secret) => Ok(Some(secret)),
            CredentialOutcome::NotFound | CredentialOutcome::Unavailable => Ok(None),
            CredentialOutcome::UnlockCancelled | CredentialOutcome::Cancelled => Ok(None),
            _ => Err("Desktop unlock is unavailable. Use your vault password.".into()),
        }
    }

    pub(super) fn retrieve_required(&mut self) -> Result<Option<Secret>> {
        let key = self.key.clone();
        let client = self.connect().ok_or("The system keyring is unavailable. Install or unlock a Secret Service provider such as GNOME Keyring or oo7.")?;
        let operation = client.store.get(key);
        match self.run(operation) {
            CredentialOutcome::Retrieved(secret) => Ok(Some(secret)),
            CredentialOutcome::NotFound => Ok(None),
            CredentialOutcome::UnlockCancelled | CredentialOutcome::Cancelled => {
                Err("System keyring authorization was cancelled.".into())
            }
            _ => Err("The system keyring could not read this password store.".into()),
        }
    }

    pub(super) fn store_secret(&mut self, secret: Secret) -> bool {
        if !self.reconcile() {
            return false;
        }
        let key = self.key.clone();
        let Some(client) = self.connect() else {
            return false;
        };
        let operation = client.store.set(key, secret);
        let outcome = self.run(operation);
        self.uncertain |= matches!(outcome, CredentialOutcome::MutationUncertain);
        outcome == CredentialOutcome::Stored
    }

    fn reconcile(&mut self) -> bool {
        if !self.uncertain {
            return true;
        }
        let Some(store) = self
            .client
            .as_ref()
            .and_then(|client| client.mutations.as_ref())
        else {
            return false;
        };
        let outcome = self.run(store.reconcile(self.key.clone()));
        if matches!(
            outcome,
            CredentialOutcome::Retrieved(_) | CredentialOutcome::NotFound
        ) {
            self.uncertain = false;
            true
        } else {
            false
        }
    }

    pub(super) fn remember(&mut self, vault: &Vault) -> bool {
        self.store_secret(vault.desktop_secret())
    }

    pub(super) fn forget(&mut self) -> Result<()> {
        if !self.reconcile() {
            return Err(
                "The desktop keyring has an unconfirmed change. Try Forget desktop unlock again."
                    .into(),
            );
        }
        let key = self.key.clone();
        let Some(client) = self.connect() else {
            return Err("Desktop unlock is disabled. The unavailable keyring could not remove its remembered key.".into());
        };
        let operation = client.store.delete(key);
        let outcome = self.run(operation);
        self.uncertain |= matches!(outcome, CredentialOutcome::MutationUncertain);
        match outcome {
            CredentialOutcome::Deleted | CredentialOutcome::NotFound => Ok(()),
            _ => Err("Desktop unlock is disabled. The keyring did not confirm removal of its remembered key; try Forget desktop unlock again.".into()),
        }
    }

    #[cfg(test)]
    pub(super) fn with_record_store(
        path: &Path,
        namespace: &str,
        label: &str,
        store: Arc<dyn CredentialStore>,
        active: ActiveOperation,
    ) -> Self {
        let mut this = Self::for_record(path, namespace, label, active);
        this.client = Some(Arc::new(Client {
            store,
            mutations: None,
            _owner: None,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test keyring runtime"),
        }));
        this
    }

    #[cfg(test)]
    pub(super) fn with_store(path: &Path, store: Arc<dyn CredentialStore>) -> Self {
        Self::with_store_cancellation(path, store, Arc::new(Mutex::new(None)))
    }
    #[cfg(test)]
    pub(super) fn with_store_cancellation(
        path: &Path,
        store: Arc<dyn CredentialStore>,
        active: ActiveOperation,
    ) -> Self {
        let mut this = Self::new(path, active);
        this.client = Some(Arc::new(Client {
            store,
            mutations: None,
            _owner: None,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test keyring runtime"),
        }));
        this
    }
}
