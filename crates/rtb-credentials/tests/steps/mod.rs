//! Step definitions for `tests/features/credentials.feature`.

pub mod cred_steps;

use std::sync::Arc;

use cucumber::World;
use rtb_credentials::{CredentialError, CredentialRef, MemoryStore};
use secrecy::SecretString;

#[derive(Debug, Default, World)]
pub struct CredWorld {
    pub literal: Option<SecretString>,
    pub memory: Option<Arc<MemoryStore>>,
    pub cref: Option<CredentialRef>,
    pub resolved: Option<String>, // exposed secret for assertions
    pub debug_rendering: Option<String>,
    pub got_value: Option<String>,
    pub last_error: Option<CredentialError>,
}
