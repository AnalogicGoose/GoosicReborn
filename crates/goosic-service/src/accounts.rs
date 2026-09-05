//! Account command handling and playback handoff coordination.
//!
//! Account metadata is persisted by `goosic-accounts`; this module is the only place that may
//! combine an account-store mutation with the playback authority. UI callers must quiesce and
//! release their renderer first, then provide the current playback generation.

use goosic_accounts::{AccountError, AccountStore};
use goosic_core::{CoreError, PlaybackAuthority};
use goosic_protocol::{ErrorObject, Owner, RequestPayload, ResponseEnvelope, ResponsePayload};
use std::path::PathBuf;

pub struct Accounts {
    store: Option<AccountStore>,
    unavailable: Option<AccountError>,
    authority_failure_injected: bool,
}

impl Default for Accounts {
    fn default() -> Self {
        Self::new()
    }
}

impl Accounts {
    pub fn new() -> Self {
        match AccountStore::default_path().and_then(AccountStore::open) {
            Ok(store) => Self {
                store: Some(store),
                unavailable: None,
                authority_failure_injected: false,
            },
            Err(error) => Self {
                store: None,
                unavailable: Some(error),
                authority_failure_injected: false,
            },
        }
    }

    /// Opens an account store at an explicit path. Tests use this to keep real user files intact.
    pub fn at(path: PathBuf) -> Self {
        match AccountStore::open(path) {
            Ok(store) => Self {
                store: Some(store),
                unavailable: None,
                authority_failure_injected: false,
            },
            Err(error) => Self {
                store: None,
                unavailable: Some(error),
                authority_failure_injected: false,
            },
        }
    }

    pub fn snapshot(&self) -> Option<goosic_protocol::AccountsSnapshot> {
        self.store.as_ref().map(AccountStore::snapshot)
    }

    /// Test-only fault injection for validating committed-but-uncertain handoffs.
    #[doc(hidden)]
    pub fn inject_post_persist_sync_failure(&mut self, enabled: bool) {
        if let Some(store) = self.store.as_mut() {
            store.inject_post_persist_sync_failure(enabled);
        }
    }

    /// Test-only fault injection for exercising authority/store compensation.
    #[doc(hidden)]
    pub fn inject_authority_failure(&mut self, enabled: bool) {
        self.authority_failure_injected = enabled;
    }

    /// Applies a persisted active identity to a fresh authority at process startup.
    pub fn synchronize_authority(
        &self,
        authority: &mut PlaybackAuthority,
    ) -> Result<(), CoreError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let Some(id) = store.active_account_id() else {
            return Ok(());
        };
        authority.change_account(Some(id.to_owned()), authority.state().generation)?;
        Ok(())
    }

    fn unavailable(&self, request_id: String) -> ResponseEnvelope {
        let (code, message) = match &self.unavailable {
            Some(error) => (error.code(), error.to_string()),
            None => (
                "accountsUnavailable",
                "accounts are not available".to_owned(),
            ),
        };
        failure(request_id, code, message)
    }

    fn snapshot_response(&self, request_id: String) -> ResponseEnvelope {
        match self.snapshot() {
            Some(snapshot) => ResponseEnvelope::success(
                request_id,
                ResponsePayload {
                    accounts: Some(snapshot),
                    ..Default::default()
                },
            ),
            None => self.unavailable(request_id),
        }
    }

    fn transition_response(
        &self,
        request_id: String,
        state: goosic_protocol::PlaybackState,
    ) -> ResponseEnvelope {
        ResponseEnvelope::success(
            request_id,
            ResponsePayload {
                state: Some(state),
                accounts: self.snapshot(),
                ..Default::default()
            },
        )
    }

    fn uncertain_transition_response(
        &self,
        request_id: String,
        state: goosic_protocol::PlaybackState,
        message: String,
    ) -> ResponseEnvelope {
        ResponseEnvelope::success(
            request_id,
            ResponsePayload {
                message: Some(message),
                state: Some(state),
                accounts: self.snapshot(),
                ..Default::default()
            },
        )
    }

    fn change_authority(
        &self,
        authority: &mut PlaybackAuthority,
        account_id: Option<String>,
        generation: u64,
    ) -> Result<goosic_protocol::PlaybackState, CoreError> {
        if self.authority_failure_injected {
            return Err(CoreError::OwnerConflict);
        }
        authority.change_account(account_id, generation)
    }
}

/// Handles every `accounts.*` command. Returns `None` for unrelated commands.
pub fn handle(
    accounts: &mut Accounts,
    authority: &mut PlaybackAuthority,
    command: &str,
    request_id: &str,
    payload: &RequestPayload,
) -> Option<ResponseEnvelope> {
    if !matches!(
        command,
        "accounts.get" | "accounts.upsert" | "accounts.activate" | "accounts.remove"
    ) {
        return None;
    }
    let id = request_id.to_owned();
    if accounts.store.is_none() {
        return Some(accounts.unavailable(id));
    }
    Some(match command {
        "accounts.get" => accounts.snapshot_response(id),
        "accounts.upsert" => {
            let Some(input) = payload.account.clone() else {
                return Some(failure(
                    id,
                    "invalidRequest",
                    "accounts.upsert requires account metadata",
                ));
            };
            let store = accounts.store.as_mut().expect("checked above");
            match store.upsert(input) {
                Ok(_) => accounts.snapshot_response(id),
                Err(error) => account_failure(id, error),
            }
        }
        "accounts.activate" => {
            let generation = match payload.generation {
                Some(generation) => generation,
                None => {
                    return Some(failure(
                        id,
                        "invalidRequest",
                        "accounts.activate requires generation",
                    ))
                }
            };
            let account_id = match payload.account_id.as_deref() {
                Some(account_id) => account_id,
                None => {
                    return Some(failure(
                        id,
                        "invalidRequest",
                        "accounts.activate requires accountId",
                    ))
                }
            };
            transition(accounts, authority, id, Some(account_id), generation)
        }
        "accounts.remove" => {
            let Some(account_id) = payload.account_id.as_deref() else {
                return Some(failure(
                    id,
                    "invalidRequest",
                    "accounts.remove requires accountId",
                ));
            };
            remove(accounts, authority, id, account_id, payload.generation)
        }
        _ => unreachable!(),
    })
}

/// Compatibility path for the old `account.change` command. It is intentionally routed through
/// the same store and quiescence checks, so it cannot create an authority/store split.
pub fn handle_compat_change(
    accounts: &mut Accounts,
    authority: &mut PlaybackAuthority,
    request_id: String,
    account_id: Option<&str>,
    generation: Option<u64>,
) -> ResponseEnvelope {
    let Some(generation) = generation else {
        return failure(
            request_id,
            "invalidRequest",
            "account change requires generation",
        );
    };
    transition(accounts, authority, request_id, account_id, generation)
}

fn transition(
    accounts: &mut Accounts,
    authority: &mut PlaybackAuthority,
    request_id: String,
    account_id: Option<&str>,
    generation: u64,
) -> ResponseEnvelope {
    if authority.state().owner != Owner::None {
        return failure(
            request_id,
            "playbackBusy",
            "release playback before changing accounts",
        );
    }
    if authority.state().generation != generation {
        return core_failure(request_id, CoreError::GenerationMismatch);
    }
    let Some(store) = accounts.store.as_mut() else {
        return accounts.unavailable(request_id);
    };
    if let Some(id) = account_id {
        if !store.account_exists(id) {
            return account_failure(request_id, AccountError::NotFound);
        }
    }
    let previous_document = store.document().clone();
    let previous_authority = authority.clone();
    let target = account_id.map(str::to_owned);
    let activation = store.activate(account_id);
    if let Err(error) = activation {
        if error.is_committed() {
            return match accounts.change_authority(authority, target.clone(), generation) {
                Ok(state) => accounts.uncertain_transition_response(
                    request_id,
                    state,
                    format!(
                        "{}; playback state aligned with the committed account",
                        error
                    ),
                ),
                Err(authority_error) => compensate(
                    accounts,
                    authority,
                    request_id,
                    previous_document,
                    previous_authority,
                    format!("{error}; could not align playback authority: {authority_error}"),
                    None,
                ),
            };
        }
        return account_failure(request_id, error);
    }
    match accounts.change_authority(authority, target, generation) {
        Ok(state) => {
            if accounts.snapshot().is_some() {
                accounts.transition_response(request_id, state)
            } else {
                accounts.unavailable(request_id)
            }
        }
        Err(error) => compensate(
            accounts,
            authority,
            request_id,
            previous_document,
            previous_authority,
            format!("account transition failed ({error})"),
            Some(error),
        ),
    }
}

fn remove(
    accounts: &mut Accounts,
    authority: &mut PlaybackAuthority,
    request_id: String,
    account_id: &str,
    generation: Option<u64>,
) -> ResponseEnvelope {
    let Some(store) = accounts.store.as_mut() else {
        return accounts.unavailable(request_id);
    };
    let is_active = store.is_active_account(account_id);
    // Inactive profile deletion is independent of playback and does not need a lease generation.
    if !is_active {
        return match store.remove(account_id) {
            Ok(_) => accounts.snapshot_response(request_id),
            Err(error) => account_failure(request_id, error),
        };
    }
    let Some(generation) = generation else {
        return failure(
            request_id,
            "invalidRequest",
            "removing the active account requires generation",
        );
    };
    if authority.state().owner != Owner::None {
        return failure(
            request_id,
            "playbackBusy",
            "release playback before removing accounts",
        );
    }
    if authority.state().generation != generation {
        return core_failure(request_id, CoreError::GenerationMismatch);
    }
    let previous_document = store.document().clone();
    let previous_authority = authority.clone();
    let removal = store.remove(account_id);
    let was_active = match removal {
        Ok(was_active) => was_active,
        Err(error) if error.is_committed() => {
            return match accounts.change_authority(authority, None, generation) {
                Ok(state) => accounts.uncertain_transition_response(
                    request_id,
                    state,
                    format!("{error}; playback state aligned with the committed guest switch"),
                ),
                Err(authority_error) => compensate(
                    accounts,
                    authority,
                    request_id,
                    previous_document,
                    previous_authority,
                    format!("{error}; could not align playback authority: {authority_error}"),
                    None,
                ),
            }
        }
        Err(error) => return account_failure(request_id, error),
    };
    if !was_active {
        return accounts.snapshot_response(request_id);
    }
    match accounts.change_authority(authority, None, generation) {
        Ok(state) => {
            if accounts.snapshot().is_some() {
                accounts.transition_response(request_id, state)
            } else {
                accounts.unavailable(request_id)
            }
        }
        Err(error) => compensate(
            accounts,
            authority,
            request_id,
            previous_document,
            previous_authority,
            format!("account removal failed ({error})"),
            Some(error),
        ),
    }
}

fn account_failure(request_id: String, error: AccountError) -> ResponseEnvelope {
    failure(request_id, error.code(), error.to_string())
}

fn compensate(
    accounts: &mut Accounts,
    authority: &mut PlaybackAuthority,
    request_id: String,
    previous_document: goosic_accounts::AccountsDocument,
    previous_authority: PlaybackAuthority,
    reason: String,
    authority_error: Option<CoreError>,
) -> ResponseEnvelope {
    *authority = previous_authority;
    let rollback = accounts
        .store
        .as_mut()
        .map(|store| store.restore(previous_document));
    match rollback {
        Some(Ok(())) => match authority_error {
            Some(error) => core_failure(request_id, error),
            None => failure(request_id, "accountsCoordinationFailed", reason),
        },
        Some(Err(error)) if error.is_committed() => failure(
            request_id,
            "accountsDurabilityUncertain",
            format!(
                "{reason}; previous account document committed with uncertain durability ({error})"
            ),
        ),
        Some(Err(error)) => failure(
            request_id,
            "accountsRollbackFailed",
            format!("{reason}; rollback failed ({error})"),
        ),
        None => failure(
            request_id,
            "accountsCoordinationFailed",
            format!("{reason}; account store unavailable during compensation"),
        ),
    }
}

fn core_failure(request_id: String, error: CoreError) -> ResponseEnvelope {
    failure(request_id, error.code(), error.to_string())
}

fn failure(
    request_id: String,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ResponseEnvelope {
    ResponseEnvelope::failure(
        request_id,
        ErrorObject {
            code: code.into(),
            message: message.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::{AccountUpsert, PROTOCOL_VERSION};

    fn payload(account: Option<AccountUpsert>) -> RequestPayload {
        RequestPayload {
            account,
            ..Default::default()
        }
    }

    fn input() -> AccountUpsert {
        AccountUpsert {
            id: None,
            webkit_profile_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            display_name: "Alice".into(),
            email: None,
            channel: None,
            avatar_url: None,
        }
    }

    #[test]
    fn upsert_get_activate_and_remove_are_typed_and_coordinated() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let upsert = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let account_id = upsert.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        let activated = handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "2",
            &RequestPayload {
                account_id: Some(account_id.clone()),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(activated.ok);
        assert_eq!(
            activated
                .payload
                .unwrap()
                .state
                .unwrap()
                .account_id
                .as_deref(),
            Some(account_id.as_str())
        );
        assert_eq!(authority.state().generation, 1);
        let removed = handle(
            &mut accounts,
            &mut authority,
            "accounts.remove",
            "3",
            &RequestPayload {
                account_id: Some(account_id),
                generation: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(removed.ok);
        assert_eq!(removed.payload.unwrap().state.unwrap().account_id, None);
    }

    #[test]
    fn active_transition_requires_an_idle_authority_and_current_generation() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "1",
            &RequestPayload::default(),
        )
        .unwrap();
        assert_eq!(response.error.unwrap().code, "invalidRequest");
        let _ = PROTOCOL_VERSION;
    }

    #[test]
    fn removing_an_inactive_account_does_not_require_or_change_playback_lease() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let first = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let first_id = first.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        let mut second_input = input();
        second_input.webkit_profile_id = "123e4567-e89b-12d3-a456-426614174001".into();
        let second = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "2",
            &payload(Some(second_input)),
        )
        .unwrap();
        let second_id = second.payload.unwrap().accounts.unwrap().accounts[1]
            .id
            .clone();
        handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "3",
            &RequestPayload {
                account_id: Some(first_id),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        authority
            .claim(Owner::OfficialWebView, authority.state().generation)
            .unwrap();
        let removed = handle(
            &mut accounts,
            &mut authority,
            "accounts.remove",
            "4",
            &RequestPayload {
                account_id: Some(second_id),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(removed.ok);
        assert_eq!(authority.state().owner, Owner::OfficialWebView);
    }

    #[test]
    fn post_persist_sync_uncertainty_keeps_disk_memory_and_authority_aligned() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let account_id = response.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        accounts.inject_post_persist_sync_failure(true);
        let activated = handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "2",
            &RequestPayload {
                account_id: Some(account_id.clone()),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(activated.ok);
        assert!(activated
            .payload
            .as_ref()
            .and_then(|payload| payload.message.as_deref())
            .is_some_and(|message| message.contains("durability")));
        assert_eq!(
            authority.state().account_id.as_deref(),
            Some(account_id.as_str())
        );
        assert_eq!(
            accounts.snapshot().unwrap().active_account_id.as_deref(),
            Some(account_id.as_str())
        );
        let reopened =
            goosic_accounts::AccountStore::open(directory.path().join("accounts.json")).unwrap();
        assert_eq!(reopened.active_account_id(), Some(account_id.as_str()));
    }

    #[test]
    fn activate_authority_failure_compensates_committed_uncertainty() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let id = response.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        accounts.inject_post_persist_sync_failure(true);
        accounts.inject_authority_failure(true);
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "2",
            &RequestPayload {
                account_id: Some(id.clone()),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "accountsDurabilityUncertain");
        assert_eq!(authority.state().account_id, None);
        assert_eq!(accounts.snapshot().unwrap().active_account_id, None);
        assert_eq!(
            goosic_accounts::AccountStore::open(directory.path().join("accounts.json"))
                .unwrap()
                .active_account_id(),
            None
        );
    }

    #[test]
    fn active_remove_authority_failure_compensates_committed_uncertainty() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let id = response.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        handle(
            &mut accounts,
            &mut authority,
            "accounts.activate",
            "2",
            &RequestPayload {
                account_id: Some(id.clone()),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        accounts.inject_post_persist_sync_failure(true);
        accounts.inject_authority_failure(true);
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.remove",
            "3",
            &RequestPayload {
                account_id: Some(id.clone()),
                generation: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "accountsDurabilityUncertain");
        assert_eq!(authority.state().account_id.as_deref(), Some(id.as_str()));
        assert_eq!(
            accounts.snapshot().unwrap().active_account_id.as_deref(),
            Some(id.as_str())
        );
        assert_eq!(accounts.snapshot().unwrap().accounts.len(), 1);
    }

    #[test]
    fn compat_account_change_uses_the_same_committed_uncertainty_compensation() {
        let directory = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::at(directory.path().join("accounts.json"));
        let mut authority = PlaybackAuthority::new();
        let response = handle(
            &mut accounts,
            &mut authority,
            "accounts.upsert",
            "1",
            &payload(Some(input())),
        )
        .unwrap();
        let id = response.payload.unwrap().accounts.unwrap().accounts[0]
            .id
            .clone();
        accounts.inject_post_persist_sync_failure(true);
        accounts.inject_authority_failure(true);
        let response = handle_compat_change(
            &mut accounts,
            &mut authority,
            "2".into(),
            Some(&id),
            Some(0),
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "accountsDurabilityUncertain");
        assert_eq!(authority.state().account_id, None);
        assert_eq!(accounts.snapshot().unwrap().active_account_id, None);
    }
}
