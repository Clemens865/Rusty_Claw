//! DM pairing — approve/reject unknown senders before they can chat.
//!
//! When an unknown sender messages from a channel, a pairing request is created
//! with a short code. The owner approves/rejects via CLI.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::data_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequest {
    pub channel: String,
    pub sender_id: String,
    pub display_name: Option<String>,
    pub code: String,
    pub status: PairingStatus,
    pub created_at: DateTime<Utc>,
}

/// Persistent store for pairing requests.
pub struct PairingStore {
    path: PathBuf,
}

impl PairingStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        data_dir().join("pairing.json")
    }

    fn load_all(&self) -> HashMap<String, PairingRequest> {
        if !self.path.exists() {
            return HashMap::new();
        }
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_all(&self, data: &HashMap<String, PairingRequest>) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(data)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Generate a unique pairing key from channel + sender_id.
    fn pairing_key(channel: &str, sender_id: &str) -> String {
        format!("{channel}:{sender_id}")
    }

    /// Generate a short random code.
    fn generate_code() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        let code: u32 = rng.random_range(100_000..999_999);
        code.to_string()
    }

    /// Check if a sender is approved.
    pub fn is_approved(&self, channel: &str, sender_id: &str) -> bool {
        let data = self.load_all();
        let key = Self::pairing_key(channel, sender_id);
        data.get(&key)
            .is_some_and(|r| r.status == PairingStatus::Approved)
    }

    /// Create a pairing request for an unknown sender. Returns the code.
    pub fn create_request(
        &self,
        channel: &str,
        sender_id: &str,
        display_name: Option<String>,
    ) -> anyhow::Result<String> {
        let mut data = self.load_all();
        let key = Self::pairing_key(channel, sender_id);

        // If already exists and pending, return existing code
        if let Some(existing) = data.get(&key) {
            if existing.status == PairingStatus::Pending {
                return Ok(existing.code.clone());
            }
        }

        let code = Self::generate_code();
        let request = PairingRequest {
            channel: channel.to_string(),
            sender_id: sender_id.to_string(),
            display_name,
            code: code.clone(),
            status: PairingStatus::Pending,
            created_at: Utc::now(),
        };
        data.insert(key, request);
        self.save_all(&data)?;
        Ok(code)
    }

    /// Approve a pairing request by channel + code.
    pub fn approve(&self, channel: &str, code: &str) -> anyhow::Result<bool> {
        let mut data = self.load_all();
        let found = data.values_mut().find(|r| {
            r.channel == channel && r.code == code && r.status == PairingStatus::Pending
        });

        if let Some(req) = found {
            req.status = PairingStatus::Approved;
            self.save_all(&data)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reject a pairing request by channel + code.
    pub fn reject(&self, channel: &str, code: &str) -> anyhow::Result<bool> {
        let mut data = self.load_all();
        let found = data.values_mut().find(|r| {
            r.channel == channel && r.code == code && r.status == PairingStatus::Pending
        });

        if let Some(req) = found {
            req.status = PairingStatus::Rejected;
            self.save_all(&data)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all pairing requests.
    pub fn list(&self) -> Vec<PairingRequest> {
        self.load_all().into_values().collect()
    }

    /// List only pending pairing requests.
    pub fn list_pending(&self) -> Vec<PairingRequest> {
        self.load_all()
            .into_values()
            .filter(|r| r.status == PairingStatus::Pending)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_flow() {
        let dir = tempfile::tempdir().unwrap();
        let store = PairingStore::new(dir.path().join("pairing.json"));

        // Unknown sender creates request
        let code = store
            .create_request("telegram", "user123", Some("Alice".into()))
            .unwrap();
        assert_eq!(code.len(), 6);

        // Not yet approved
        assert!(!store.is_approved("telegram", "user123"));

        // List pending
        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sender_id, "user123");

        // Approve
        assert!(store.approve("telegram", &code).unwrap());
        assert!(store.is_approved("telegram", "user123"));

        // No more pending
        assert!(store.list_pending().is_empty());
    }

    #[test]
    fn test_reject_pairing() {
        let dir = tempfile::tempdir().unwrap();
        let store = PairingStore::new(dir.path().join("pairing.json"));

        let code = store
            .create_request("discord", "user456", None)
            .unwrap();
        assert!(store.reject("discord", &code).unwrap());
        assert!(!store.is_approved("discord", "user456"));
    }

    #[test]
    fn test_duplicate_request_returns_same_code() {
        let dir = tempfile::tempdir().unwrap();
        let store = PairingStore::new(dir.path().join("pairing.json"));

        let code1 = store
            .create_request("telegram", "user789", None)
            .unwrap();
        let code2 = store
            .create_request("telegram", "user789", None)
            .unwrap();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_approve_wrong_code() {
        let dir = tempfile::tempdir().unwrap();
        let store = PairingStore::new(dir.path().join("pairing.json"));

        store
            .create_request("telegram", "user000", None)
            .unwrap();
        assert!(!store.approve("telegram", "000000").unwrap());
    }
}
