//! Configuration and credential storage.
//!
//! Owns non-secret settings (TOML under the XDG config directory) and the
//! Feedbin password, stored in the OS keychain via the `keyring` crate.
//! Implemented in TASK-2.
