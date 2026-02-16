//! JSONL-based session store — stores transcripts as append-only JSONL files.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::error::{Result, RustyClawError};
use crate::session::{Session, SessionKey, SessionMeta, SessionStore, TranscriptEntry};

/// File-based session store using JSONL for transcripts.
///
/// Layout:
/// - `<base>/sessions.json` — array of `SessionMeta`
/// - `<base>/transcripts/<hash>.jsonl` — one transcript entry per line
pub struct JsonlSessionStore {
    base: PathBuf,
}

impl JsonlSessionStore {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// Default store location: `~/.rusty_claw/sessions/`
    pub fn default_path() -> PathBuf {
        crate::config::data_dir().join("sessions")
    }

    fn index_path(&self) -> PathBuf {
        self.base.join("sessions.json")
    }

    fn transcript_dir(&self) -> PathBuf {
        self.base.join("transcripts")
    }

    fn transcript_path(&self, key: &SessionKey) -> PathBuf {
        self.transcript_dir().join(format!("{}.jsonl", key.hash_key()))
    }

    async fn ensure_dirs(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.base).await?;
        tokio::fs::create_dir_all(self.transcript_dir()).await?;
        Ok(())
    }

    async fn load_index(&self) -> Result<Vec<SessionMeta>> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = tokio::fs::read_to_string(&path).await?;
        let metas: Vec<SessionMeta> = serde_json::from_str(&data)?;
        Ok(metas)
    }

    async fn save_index(&self, metas: &[SessionMeta]) -> Result<()> {
        self.ensure_dirs().await?;
        let data = serde_json::to_string_pretty(metas)?;
        let path = self.index_path();
        // Atomic write: write to temp then rename
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, data.as_bytes()).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    async fn load_transcript(&self, key: &SessionKey) -> Result<Vec<TranscriptEntry>> {
        let path = self.transcript_path(key);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = tokio::fs::read_to_string(&path).await?;
        let mut entries = Vec::new();
        for line in data.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: TranscriptEntry = serde_json::from_str(line).map_err(|e| {
                RustyClawError::Session(format!("corrupt transcript line: {e}"))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

#[async_trait]
impl SessionStore for JsonlSessionStore {
    async fn load(&self, key: &SessionKey) -> Result<Option<Session>> {
        let metas = self.load_index().await?;
        let meta = metas.into_iter().find(|m| &m.key == key);
        match meta {
            Some(meta) => {
                let transcript = self.load_transcript(key).await?;
                debug!(
                    key = %key.hash_key(),
                    entries = transcript.len(),
                    "Loaded session transcript"
                );
                Ok(Some(Session { meta, transcript }))
            }
            None => Ok(None),
        }
    }

    async fn save(&self, session: &Session) -> Result<()> {
        self.ensure_dirs().await?;

        // Update index
        let mut metas = self.load_index().await?;
        if let Some(existing) = metas.iter_mut().find(|m| m.key == session.meta.key) {
            *existing = session.meta.clone();
        } else {
            metas.push(session.meta.clone());
        }
        self.save_index(&metas).await?;

        // Write full transcript
        let path = self.transcript_path(&session.meta.key);
        let mut data = String::new();
        for entry in &session.transcript {
            let line = serde_json::to_string(entry)?;
            data.push_str(&line);
            data.push('\n');
        }
        let tmp = path.with_extension("jsonl.tmp");
        tokio::fs::write(&tmp, data.as_bytes()).await?;
        tokio::fs::rename(&tmp, &path).await?;

        debug!(key = %session.meta.key.hash_key(), "Saved session");
        Ok(())
    }

    async fn append_entry(&self, key: &SessionKey, entry: &TranscriptEntry) -> Result<()> {
        self.ensure_dirs().await?;

        let path = self.transcript_path(key);
        let line = serde_json::to_string(entry)?;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        // Update last_updated_at in index
        let mut metas = self.load_index().await?;
        if let Some(meta) = metas.iter_mut().find(|m| &m.key == key) {
            meta.last_updated_at = chrono::Utc::now();
            self.save_index(&metas).await?;
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<SessionMeta>> {
        self.load_index().await
    }

    async fn delete(&self, key: &SessionKey) -> Result<()> {
        // Remove from index
        let mut metas = self.load_index().await?;
        metas.retain(|m| &m.key != key);
        self.save_index(&metas).await?;

        // Remove transcript file
        let path = self.transcript_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        debug!(key = %key.hash_key(), "Deleted session");
        Ok(())
    }

    async fn reset(&self, key: &SessionKey) -> Result<()> {
        // Clear transcript file
        let path = self.transcript_path(key);
        if path.exists() {
            tokio::fs::write(&path, b"").await?;
        }

        // Update metadata
        let mut metas = self.load_index().await?;
        if let Some(meta) = metas.iter_mut().find(|m| &m.key == key) {
            meta.last_reset_at = Some(chrono::Utc::now());
            meta.last_updated_at = chrono::Utc::now();
            self.save_index(&metas).await?;
        }

        debug!(key = %key.hash_key(), "Reset session");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatType, ContentBlock};

    fn test_key() -> SessionKey {
        SessionKey {
            channel: "test".into(),
            account_id: "acc1".into(),
            chat_type: ChatType::Dm,
            peer_id: "peer1".into(),
            scope: crate::session::SessionScope::PerSender,
        }
    }

    fn test_session() -> Session {
        Session::new(test_key())
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlSessionStore::new(dir.path().to_path_buf());

        let mut session = test_session();
        session.append(TranscriptEntry::User {
            content: vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
            timestamp: chrono::Utc::now(),
        });

        store.save(&session).await.unwrap();
        let loaded = store.load(&test_key()).await.unwrap().unwrap();
        assert_eq!(loaded.transcript.len(), 1);
        assert_eq!(loaded.meta.key, test_key());
    }

    #[tokio::test]
    async fn test_append_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlSessionStore::new(dir.path().to_path_buf());

        // Save empty session first
        let session = test_session();
        store.save(&session).await.unwrap();

        // Append entries
        let entry = TranscriptEntry::User {
            content: vec![ContentBlock::Text {
                text: "Hi".into(),
            }],
            timestamp: chrono::Utc::now(),
        };
        store.append_entry(&test_key(), &entry).await.unwrap();

        let loaded = store.load(&test_key()).await.unwrap().unwrap();
        assert_eq!(loaded.transcript.len(), 1);
    }

    #[tokio::test]
    async fn test_list_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlSessionStore::new(dir.path().to_path_buf());

        let session = test_session();
        store.save(&session).await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 1);

        store.delete(&test_key()).await.unwrap();
        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn test_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlSessionStore::new(dir.path().to_path_buf());

        let mut session = test_session();
        session.append(TranscriptEntry::User {
            content: vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
            timestamp: chrono::Utc::now(),
        });
        store.save(&session).await.unwrap();

        store.reset(&test_key()).await.unwrap();
        let loaded = store.load(&test_key()).await.unwrap().unwrap();
        assert_eq!(loaded.transcript.len(), 0);
        assert!(loaded.meta.last_reset_at.is_some());
    }

    #[test]
    fn test_hash_key_stability() {
        let key1 = test_key();
        let key2 = test_key();
        assert_eq!(key1.hash_key(), key2.hash_key());
    }
}
