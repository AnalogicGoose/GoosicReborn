//! Durable, metadata-only account profiles.
//!
//! This crate deliberately has no WebKit, cookie, credential, authentication, or network
//! integration. It stores the identifiers and display metadata needed by the shell to choose a
//! profile; the platform UI owns the actual WebKit profile data.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use goosic_protocol::{AccountSummary, AccountUpsert, AccountsSnapshot};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const DOCUMENT_VERSION: u32 = 1;
pub const MAX_ACCOUNTS: usize = 128;
pub const MAX_DISPLAY_NAME: usize = 128;
pub const MAX_EMAIL: usize = 320;
pub const MAX_CHANNEL: usize = 128;
pub const MAX_AVATAR_URL: usize = 2048;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account file could not be read: {0}")]
    Unreadable(#[source] std::io::Error),
    #[error("account file could not be written: {0}")]
    Unwritable(#[source] std::io::Error),
    #[error("account file is not valid JSON: {0}")]
    Malformed(String),
    #[error("account file was written by a newer version ({0})")]
    TooNew(u32),
    #[error("no account location is available on this platform")]
    NoLocation,
    #[error("account id must be a canonical UUID")]
    InvalidId,
    #[error("WebKit profile id must be a canonical UUID")]
    InvalidProfileId,
    #[error("account metadata is invalid: {0}")]
    InvalidMetadata(String),
    #[error("too many accounts")]
    TooManyAccounts,
    #[error("account does not exist")]
    NotFound,
    #[error("an account already uses that WebKit profile")]
    DuplicateProfile,
    #[error("account epoch is exhausted")]
    EpochExhausted,
    #[error("account file committed, but directory durability is uncertain: {0}")]
    CommittedDurabilityUncertain(#[source] std::io::Error),
}

impl AccountError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable(_) | Self::Unwritable(_) => "accountsIoError",
            Self::Malformed(_) => "accountsMalformed",
            Self::TooNew(_) => "accountsTooNew",
            Self::NoLocation => "accountsNoLocation",
            Self::InvalidId | Self::InvalidProfileId | Self::InvalidMetadata(_) => "invalidAccount",
            Self::TooManyAccounts => "accountsLimitExceeded",
            Self::NotFound => "accountNotFound",
            Self::DuplicateProfile => "duplicateProfile",
            Self::EpochExhausted => "accountsEpochExhausted",
            Self::CommittedDurabilityUncertain(_) => "accountsDurabilityUncertain",
        }
    }

    /// Whether the destination inode was replaced before this error was returned.
    pub fn is_committed(&self) -> bool {
        matches!(self, Self::CommittedDurabilityUncertain(_))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountsDocument {
    pub version: u32,
    #[serde(default)]
    pub epoch: u64,
    #[serde(default)]
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub accounts: Vec<AccountSummary>,
}

impl Default for AccountsDocument {
    fn default() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            epoch: 0,
            active_account_id: None,
            accounts: Vec::new(),
        }
    }
}

impl AccountsDocument {
    fn validate_and_sanitize(mut self) -> Result<Self, AccountError> {
        if self.version > DOCUMENT_VERSION {
            return Err(AccountError::TooNew(self.version));
        }
        if self.accounts.len() > MAX_ACCOUNTS {
            return Err(AccountError::TooManyAccounts);
        }
        let mut profiles = Vec::with_capacity(self.accounts.len());
        let mut ids = Vec::with_capacity(self.accounts.len());
        for account in &mut self.accounts {
            sanitize_summary(account)?;
            if ids.iter().any(|existing| existing == &account.id) {
                return Err(AccountError::Malformed("duplicate account id".into()));
            }
            ids.push(account.id.clone());
            if profiles
                .iter()
                .any(|profile| profile == &account.webkit_profile_id)
            {
                return Err(AccountError::DuplicateProfile);
            }
            profiles.push(account.webkit_profile_id.clone());
        }
        if let Some(id) = self.active_account_id.as_mut() {
            *id = canonical_uuid(id, AccountError::InvalidId)?.to_string();
            if !self.accounts.iter().any(|account| account.id == *id) {
                return Err(AccountError::NotFound);
            }
        }
        self.version = DOCUMENT_VERSION;
        Ok(self)
    }
}

/// The account file and its in-memory, validated representation.
#[derive(Debug, Clone)]
pub struct AccountStore {
    path: PathBuf,
    document: AccountsDocument,
    post_persist_sync_failure: bool,
}

impl AccountStore {
    pub fn default_path() -> Result<PathBuf, AccountError> {
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| Path::new(&home).join("Library/Application Support"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        };
        base.map(|base| base.join("goosic").join("accounts.json"))
            .ok_or(AccountError::NoLocation)
    }

    /// On Unix, opens the leaf without following symlinks. Parent-directory symlinks are
    /// intentionally allowed for user-configured locations such as XDG_CONFIG_HOME; only the
    /// account-file leaf is protected. The non-Unix fallback rejects ordinary pre-existing leaf
    /// symlinks, but cannot provide the same race-free no-follow guarantee until a platform
    /// adapter is added. This store assumes one service process; concurrent writers are
    /// last-writer wins, but each writer uses an independent random temporary file.
    pub fn open(path: PathBuf) -> Result<Self, AccountError> {
        let document = match read_account_file(&path)? {
            Some(bytes) => {
                let envelope: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|error| AccountError::Malformed(error.to_string()))?;
                let version = envelope
                    .get("version")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(DOCUMENT_VERSION as u64);
                if version > DOCUMENT_VERSION as u64 {
                    return Err(AccountError::TooNew(version.min(u32::MAX as u64) as u32));
                }
                let document: AccountsDocument = serde_json::from_value(envelope)
                    .map_err(|error| AccountError::Malformed(error.to_string()))?;
                document.validate_and_sanitize()?
            }
            None => AccountsDocument::default(),
        };
        Ok(Self {
            path,
            document,
            post_persist_sync_failure: false,
        })
    }

    pub fn document(&self) -> &AccountsDocument {
        &self.document
    }

    pub fn snapshot(&self) -> AccountsSnapshot {
        AccountsSnapshot {
            accounts: self.document.accounts.clone(),
            active_account_id: self.document.active_account_id.clone(),
            epoch: self.document.epoch,
        }
    }

    pub fn active_account_id(&self) -> Option<&str> {
        self.document.active_account_id.as_deref()
    }

    pub fn is_active_account(&self, id: &str) -> bool {
        let Ok(uuid) = canonical_uuid(id, AccountError::InvalidId) else {
            return false;
        };
        let canonical = uuid.hyphenated().to_string();
        self.document.active_account_id.as_deref() == Some(canonical.as_str())
    }

    /// Test-only fault injection for validating post-rename durability uncertainty handling.
    #[doc(hidden)]
    pub fn inject_post_persist_sync_failure(&mut self, enabled: bool) {
        self.post_persist_sync_failure = enabled;
    }

    pub fn account_exists(&self, id: &str) -> bool {
        canonical_uuid(id, AccountError::InvalidId)
            .map(|uuid| {
                self.document
                    .accounts
                    .iter()
                    .any(|account| account.id == uuid.to_string())
            })
            .unwrap_or(false)
    }

    /// Upserts metadata and returns the resulting account. An omitted id creates a fresh local
    /// UUID; callers should persist and use the returned snapshot as the source of truth.
    pub fn upsert(&mut self, input: AccountUpsert) -> Result<AccountSummary, AccountError> {
        let account = summary_from_upsert(input)?;
        let previous = self.document.clone();
        ensure_epoch_available(self.document.epoch)?;
        if let Some(index) = self
            .document
            .accounts
            .iter()
            .position(|existing| existing.id == account.id)
        {
            if self.document.accounts.iter().any(|other| {
                other.id != account.id && other.webkit_profile_id == account.webkit_profile_id
            }) {
                return Err(AccountError::DuplicateProfile);
            }
            self.document.accounts[index] = account.clone();
        } else {
            if self.document.accounts.len() >= MAX_ACCOUNTS {
                return Err(AccountError::TooManyAccounts);
            }
            if self
                .document
                .accounts
                .iter()
                .any(|other| other.webkit_profile_id == account.webkit_profile_id)
            {
                return Err(AccountError::DuplicateProfile);
            }
            self.document.accounts.push(account.clone());
        }
        self.document.epoch = self
            .document
            .epoch
            .checked_add(1)
            .ok_or(AccountError::EpochExhausted)?;
        if let Err(error) = self.save() {
            if !error.is_committed() {
                self.document = previous;
            }
            return Err(error);
        }
        Ok(account)
    }

    /// Makes an account active. This only changes durable account state; playback authority
    /// coordination belongs to `goosic-service`.
    pub fn activate(&mut self, id: Option<&str>) -> Result<bool, AccountError> {
        let canonical = match id {
            Some(id) => Some(canonical_uuid(id, AccountError::InvalidId)?.to_string()),
            None => None,
        };
        if let Some(id) = canonical.as_deref() {
            if !self
                .document
                .accounts
                .iter()
                .any(|account| account.id == id)
            {
                return Err(AccountError::NotFound);
            }
        }
        if self.document.active_account_id == canonical {
            return Ok(false);
        }
        ensure_epoch_available(self.document.epoch)?;
        let previous = self.document.clone();
        self.document.active_account_id = canonical;
        self.document.epoch = self
            .document
            .epoch
            .checked_add(1)
            .ok_or(AccountError::EpochExhausted)?;
        if let Err(error) = self.save() {
            if !error.is_committed() {
                self.document = previous;
            }
            return Err(error);
        }
        Ok(true)
    }

    pub fn remove(&mut self, id: &str) -> Result<bool, AccountError> {
        let id = canonical_uuid(id, AccountError::InvalidId)?.to_string();
        let Some(index) = self
            .document
            .accounts
            .iter()
            .position(|account| account.id == id)
        else {
            return Err(AccountError::NotFound);
        };
        ensure_epoch_available(self.document.epoch)?;
        let previous = self.document.clone();
        self.document.accounts.remove(index);
        let was_active = self.document.active_account_id.as_deref() == Some(id.as_str());
        if was_active {
            self.document.active_account_id = None;
        }
        self.document.epoch = self
            .document
            .epoch
            .checked_add(1)
            .ok_or(AccountError::EpochExhausted)?;
        if let Err(error) = self.save() {
            if !error.is_committed() {
                self.document = previous;
            }
            return Err(error);
        }
        Ok(was_active)
    }

    /// Replaces this store's in-memory document and persists it. Used only to roll back a
    /// cross-component account/playback handoff after a later operation fails.
    pub fn restore(&mut self, document: AccountsDocument) -> Result<(), AccountError> {
        let document = document.validate_and_sanitize()?;
        let previous = self.document.clone();
        self.document = document;
        if let Err(error) = self.save() {
            if !error.is_committed() {
                self.document = previous;
            }
            return Err(error);
        }
        Ok(())
    }

    fn save(&self) -> Result<(), AccountError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(AccountError::Unwritable)?;
        }
        reject_symlink(&self.path, true)?;
        let encoded = serde_json::to_vec_pretty(&self.document)
            .map_err(|error| AccountError::Unwritable(std::io::Error::other(error)))?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        #[cfg(unix)]
        let directory = std::fs::File::open(parent).map_err(AccountError::Unwritable)?;
        #[cfg(unix)]
        let directory_sync_supported = match directory.sync_all() {
            Ok(()) => true,
            Err(error) if directory_sync_unsupported(&error) => false,
            Err(error) => return Err(AccountError::Unwritable(error)),
        };
        // Persist is an atomic same-directory rename. Sync the directory entry as well so a
        // crash cannot leave a successfully returned mutation pointing at the old inode. Some
        // platforms reject directory fsync; only that narrowly classified case is skipped.
        #[cfg(not(unix))]
        let directory_sync_supported = false;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(AccountError::Unwritable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o600))
                .map_err(AccountError::Unwritable)?;
        }
        temporary
            .as_file_mut()
            .write_all(&encoded)
            .map_err(AccountError::Unwritable)?;
        temporary
            .as_file_mut()
            .flush()
            .map_err(AccountError::Unwritable)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(AccountError::Unwritable)?;
        temporary
            .persist(&self.path)
            .map_err(|error| AccountError::Unwritable(error.error))?;
        if self.post_persist_sync_failure {
            return Err(AccountError::CommittedDurabilityUncertain(
                std::io::Error::other("injected parent directory fsync failure"),
            ));
        }
        #[cfg(unix)]
        if directory_sync_supported {
            if let Err(error) = directory.sync_all() {
                return Err(AccountError::CommittedDurabilityUncertain(error));
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn directory_sync_unsupported(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::Unsupported
        || matches!(error.raw_os_error(), Some(22 | 38 | 95))
}

fn read_account_file(path: &Path) -> Result<Option<Vec<u8>>, AccountError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).custom_flags(libc::O_NOFOLLOW);
        let mut file = match options.open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AccountError::Unreadable(error)),
        };
        if !file.metadata().map_err(AccountError::Unreadable)?.is_file() {
            return Err(AccountError::Unreadable(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "account path is not a regular file",
            )));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(AccountError::Unreadable)?;
        return Ok(Some(bytes));
    }
    #[cfg(not(unix))]
    {
        reject_symlink(path, false)?;
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AccountError::Unreadable(error)),
        };
        if !file.metadata().map_err(AccountError::Unreadable)?.is_file() {
            return Err(AccountError::Unreadable(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "account path is not a regular file",
            )));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(AccountError::Unreadable)?;
        Ok(Some(bytes))
    }
}

fn ensure_epoch_available(epoch: u64) -> Result<(), AccountError> {
    if epoch == u64::MAX {
        Err(AccountError::EpochExhausted)
    } else {
        Ok(())
    }
}

fn reject_symlink(path: &Path, write: bool) -> Result<(), AccountError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(if write {
                AccountError::Unwritable(error)
            } else {
                AccountError::Unreadable(error)
            })
        }
    };
    if metadata.file_type().is_symlink() {
        let error =
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "symlink account path");
        return Err(if write {
            AccountError::Unwritable(error)
        } else {
            AccountError::Unreadable(error)
        });
    }
    Ok(())
}

fn canonical_uuid(value: &str, error: AccountError) -> Result<Uuid, AccountError> {
    let uuid = match Uuid::parse_str(value) {
        Ok(uuid) => uuid,
        Err(_) => return Err(error),
    };
    if value != uuid.hyphenated().to_string() {
        return Err(error);
    }
    Ok(uuid)
}

fn summary_from_upsert(input: AccountUpsert) -> Result<AccountSummary, AccountError> {
    let id = input
        .id
        .map(|id| canonical_uuid(&id, AccountError::InvalidId).map(|uuid| uuid.to_string()))
        .transpose()?
        .unwrap_or_else(|| Uuid::new_v4().hyphenated().to_string());
    let profile = canonical_uuid(&input.webkit_profile_id, AccountError::InvalidProfileId)?;
    let mut account = AccountSummary {
        id,
        webkit_profile_id: profile.to_string(),
        display_name: input.display_name,
        email: input.email,
        channel: input.channel,
        avatar_url: input.avatar_url,
    };
    sanitize_summary(&mut account)?;
    Ok(account)
}

fn sanitize_summary(account: &mut AccountSummary) -> Result<(), AccountError> {
    account.id = canonical_uuid(&account.id, AccountError::InvalidId)?.to_string();
    account.webkit_profile_id =
        canonical_uuid(&account.webkit_profile_id, AccountError::InvalidProfileId)?.to_string();
    account.display_name = clean_required(
        account.display_name.clone(),
        MAX_DISPLAY_NAME,
        "display name",
    )?;
    account.email = clean_optional(account.email.take(), MAX_EMAIL, "email")?;
    account.channel = clean_optional(account.channel.take(), MAX_CHANNEL, "channel")?;
    account.avatar_url = clean_avatar(account.avatar_url.take())?;
    Ok(())
}

fn clean_required(value: String, limit: usize, field: &str) -> Result<String, AccountError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(AccountError::InvalidMetadata(format!(
            "{field} is required"
        )));
    }
    if value.chars().count() > limit || value.chars().any(|character| character.is_control()) {
        return Err(AccountError::InvalidMetadata(format!(
            "{field} is out of bounds"
        )));
    }
    Ok(value)
}

fn clean_optional(
    value: Option<String>,
    limit: usize,
    field: &str,
) -> Result<Option<String>, AccountError> {
    value
        .map(|value| clean_required(value, limit, field))
        .transpose()
}

fn clean_avatar(value: Option<String>) -> Result<Option<String>, AccountError> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim().to_owned();
    let parsed = Url::parse(&value)
        .map_err(|_| AccountError::InvalidMetadata("avatar must be a valid HTTPS URL".into()))?;
    if value.chars().count() > MAX_AVATAR_URL
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || parsed.scheme() != "https"
        || parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(AccountError::InvalidMetadata(
            "avatar must be an HTTPS URL without userinfo".into(),
        ));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: Option<&str>) -> AccountUpsert {
        AccountUpsert {
            id: id.map(str::to_owned),
            webkit_profile_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            display_name: "  Alice  ".into(),
            email: Some(" alice@example.com ".into()),
            channel: Some("Music".into()),
            avatar_url: Some("https://example.com/a.png".into()),
        }
    }

    #[test]
    fn missing_file_is_not_created_and_upsert_generates_uuid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("accounts.json");
        let mut store = AccountStore::open(path.clone()).unwrap();
        assert!(!path.exists());
        let account = store.upsert(input(None)).unwrap();
        assert!(Uuid::parse_str(&account.id).is_ok());
        assert_eq!(account.display_name, "Alice");
        assert_eq!(account.email.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn rejects_noncanonical_ids_and_non_https_avatars() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = AccountStore::open(directory.path().join("accounts.json")).unwrap();
        let mut value = input(Some("123E4567-E89B-12D3-A456-426614174000"));
        assert_eq!(
            store.upsert(value.clone()).unwrap_err().code(),
            "invalidAccount"
        );
        value.id = Some("123e4567-e89b-12d3-a456-426614174000".into());
        value.avatar_url = Some("http://example.com/a.png".into());
        assert_eq!(
            store.upsert(value.clone()).unwrap_err().code(),
            "invalidAccount"
        );
        value.avatar_url = Some("https://user:pass@example.com/a.png".into());
        assert_eq!(
            store.upsert(value.clone()).unwrap_err().code(),
            "invalidAccount"
        );
        value.avatar_url = Some("https://".into());
        assert_eq!(store.upsert(value).unwrap_err().code(), "invalidAccount");
    }

    #[test]
    fn too_new_documents_are_refused_and_epoch_changes_only_on_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("accounts.json");
        std::fs::write(&path, r#"{"version":99,"accounts":[]}"#).unwrap();
        assert!(matches!(
            AccountStore::open(path),
            Err(AccountError::TooNew(99))
        ));
        let mut store = AccountStore::open(directory.path().join("other.json")).unwrap();
        assert_eq!(store.snapshot().epoch, 0);
        assert!(!store.activate(None).unwrap());
        assert_eq!(store.snapshot().epoch, 0);
    }

    #[test]
    fn account_set_and_active_identity_changes_increment_epoch() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = AccountStore::open(directory.path().join("accounts.json")).unwrap();
        let account = store.upsert(input(None)).unwrap();
        assert_eq!(store.snapshot().epoch, 1);
        assert!(store.activate(Some(&account.id)).unwrap());
        assert_eq!(store.snapshot().epoch, 2);
        assert!(!store.activate(Some(&account.id)).unwrap());
        assert_eq!(store.snapshot().epoch, 2);
        assert!(store.remove(&account.id).unwrap());
        assert_eq!(store.snapshot().epoch, 3);
    }

    #[test]
    fn epoch_exhaustion_is_typed_and_does_not_mutate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("accounts.json");
        std::fs::write(
            &path,
            r#"{"version":1,"epoch":18446744073709551615,"accounts":[]}"#,
        )
        .unwrap();
        let mut store = AccountStore::open(path).unwrap();
        let before = store.document().clone();
        assert!(matches!(
            store.upsert(input(None)),
            Err(AccountError::EpochExhausted)
        ));
        assert_eq!(store.document(), &before);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_account_path_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real.json");
        std::fs::write(&real, r#"{"version":1,"accounts":[]}"#).unwrap();
        let link = directory.path().join("accounts.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(matches!(
            AccountStore::open(link),
            Err(AccountError::Unreadable(_))
        ));
    }

    #[test]
    fn failed_forward_persist_restores_the_in_memory_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("accounts.json");
        let mut store = AccountStore::open(path.clone()).unwrap();
        std::fs::create_dir(&path).unwrap();
        let before = store.document().clone();
        assert!(matches!(
            store.upsert(input(None)),
            Err(AccountError::Unwritable(_))
        ));
        assert_eq!(store.document(), &before);
    }

    #[test]
    fn failed_compensating_persist_never_installs_the_candidate_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("accounts.json");
        let mut store = AccountStore::open(path.clone()).unwrap();
        let account = store.upsert(input(None)).unwrap();
        let before = store.document().clone();
        let mut candidate = before.clone();
        candidate.active_account_id = Some(account.id);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(
            store.restore(candidate),
            Err(AccountError::Unwritable(_))
        ));
        assert_eq!(store.document(), &before);
    }
}
