//! Authority for account and playback ownership. No UI or platform playback code belongs here.

use goosic_protocol::{Owner, PlaybackState};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("owner must be claimable")]
    InvalidOwner,
    #[error("another playback owner is already active")]
    OwnerConflict,
    #[error("no playback owner is active")]
    NoActiveOwner,
    #[error("playback generation does not match the active lease")]
    GenerationMismatch,
    #[error("sample sequence must increase monotonically")]
    NonMonotonicSampleSequence,
    #[error("sample owner does not match the active lease")]
    OwnerMismatch,
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidOwner => "invalidOwner",
            Self::OwnerConflict => "ownerConflict",
            Self::NoActiveOwner => "noActiveOwner",
            Self::GenerationMismatch => "generationMismatch",
            Self::NonMonotonicSampleSequence => "nonMonotonicSampleSequence",
            Self::OwnerMismatch => "ownerMismatch",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaybackAuthority {
    state: PlaybackState,
}

impl Default for PlaybackAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackAuthority {
    pub fn new() -> Self {
        Self {
            state: PlaybackState {
                account_id: None,
                owner: Owner::None,
                generation: 0,
                sample_sequence: 0,
            },
        }
    }

    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn change_account(
        &mut self,
        account_id: Option<String>,
        expected_generation: u64,
    ) -> Result<PlaybackState, CoreError> {
        if expected_generation != self.state.generation {
            return Err(CoreError::GenerationMismatch);
        }
        self.state.account_id = account_id;
        self.state.owner = Owner::None;
        self.state.generation = self.state.generation.saturating_add(1);
        self.state.sample_sequence = 0;
        Ok(self.state.clone())
    }

    pub fn claim(
        &mut self,
        owner: Owner,
        expected_generation: u64,
    ) -> Result<PlaybackState, CoreError> {
        if !owner.is_claimable() {
            return Err(CoreError::InvalidOwner);
        }
        if self.state.owner != Owner::None {
            return Err(CoreError::OwnerConflict);
        }
        if expected_generation != self.state.generation {
            return Err(CoreError::GenerationMismatch);
        }
        self.state.owner = owner;
        self.state.generation = self.state.generation.saturating_add(1);
        self.state.sample_sequence = 0;
        Ok(self.state.clone())
    }

    pub fn release(
        &mut self,
        owner: Owner,
        expected_generation: u64,
    ) -> Result<PlaybackState, CoreError> {
        self.validate_lease(owner, expected_generation)?;
        self.state.owner = Owner::None;
        self.state.generation = self.state.generation.saturating_add(1);
        self.state.sample_sequence = 0;
        Ok(self.state.clone())
    }

    /// Records both ordinary audio samples and advertisement markers. A marker is metadata, not
    /// a lifecycle signal: accepting one never releases the owner or changes generation.
    pub fn sample(
        &mut self,
        owner: Owner,
        expected_generation: u64,
        sequence: u64,
        _marker: Option<&str>,
    ) -> Result<PlaybackState, CoreError> {
        self.validate_lease(owner, expected_generation)?;
        if sequence <= self.state.sample_sequence {
            return Err(CoreError::NonMonotonicSampleSequence);
        }
        self.state.sample_sequence = sequence;
        Ok(self.state.clone())
    }

    fn validate_lease(&self, owner: Owner, expected_generation: u64) -> Result<(), CoreError> {
        if self.state.owner == Owner::None {
            return Err(CoreError::NoActiveOwner);
        }
        if self.state.owner != owner {
            return Err(CoreError::OwnerMismatch);
        }
        if self.state.generation != expected_generation {
            return Err(CoreError::GenerationMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_owner_can_be_active_and_generation_is_a_lease() {
        let mut authority = PlaybackAuthority::new();
        let claimed = authority.claim(Owner::OfficialWebView, 0).unwrap();
        assert_eq!(claimed.owner, Owner::OfficialWebView);
        assert_eq!(claimed.generation, 1);
        assert_eq!(
            authority.claim(Owner::LocalDownloadedFile, 1),
            Err(CoreError::OwnerConflict)
        );
        assert_eq!(
            authority.release(Owner::OfficialWebView, 0),
            Err(CoreError::GenerationMismatch)
        );
        assert_eq!(
            authority.release(Owner::OfficialWebView, 1).unwrap().owner,
            Owner::None
        );
    }

    #[test]
    fn account_change_resets_owner_generation_and_sequence() {
        let mut authority = PlaybackAuthority::new();
        let claimed = authority.claim(Owner::LocalDownloadedFile, 0).unwrap();
        authority
            .sample(Owner::LocalDownloadedFile, claimed.generation, 9, None)
            .unwrap();
        let reset = authority
            .change_account(Some("account-2".into()), claimed.generation)
            .unwrap();
        assert_eq!(reset.owner, Owner::None);
        assert_eq!(reset.sample_sequence, 0);
        assert_eq!(reset.generation, claimed.generation + 1);
        assert_eq!(reset.account_id.as_deref(), Some("account-2"));
    }

    #[test]
    fn account_change_requires_current_generation_and_handoff_restarts_sequence() {
        let mut authority = PlaybackAuthority::new();
        let first = authority.claim(Owner::OfficialWebView, 0).unwrap();
        authority
            .sample(Owner::OfficialWebView, first.generation, 8, None)
            .unwrap();
        assert_eq!(
            authority.change_account(Some("stale".into()), first.generation - 1),
            Err(CoreError::GenerationMismatch)
        );
        let released = authority
            .release(Owner::OfficialWebView, first.generation)
            .unwrap();
        let second = authority
            .claim(Owner::LocalDownloadedFile, released.generation)
            .unwrap();
        assert_eq!(
            authority
                .sample(Owner::LocalDownloadedFile, second.generation, 1, None)
                .unwrap()
                .sample_sequence,
            1
        );
    }

    #[test]
    fn samples_increase_and_advertisement_marker_does_not_teardown() {
        let mut authority = PlaybackAuthority::new();
        let claimed = authority.claim(Owner::OfficialWebView, 0).unwrap();
        let marked = authority
            .sample(
                Owner::OfficialWebView,
                claimed.generation,
                1,
                Some("advertisement"),
            )
            .unwrap();
        assert_eq!(marked.owner, Owner::OfficialWebView);
        assert_eq!(marked.generation, claimed.generation);
        assert_eq!(
            authority.sample(Owner::OfficialWebView, claimed.generation, 1, None),
            Err(CoreError::NonMonotonicSampleSequence)
        );
    }
}
