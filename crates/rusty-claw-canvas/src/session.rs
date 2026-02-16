//! Canvas session management.

use chrono::{DateTime, Utc};

/// A canvas session tracks components and connected clients.
#[derive(Debug, Clone)]
pub struct CanvasSession {
    pub session_id: String,
    pub components: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

impl CanvasSession {
    pub fn new(session_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            components: Vec::new(),
            created_at: now,
            last_updated: now,
        }
    }

    pub fn push(&mut self, html: String) {
        self.components.push(html);
        self.last_updated = Utc::now();
    }

    pub fn reset(&mut self) {
        self.components.clear();
        self.last_updated = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_session() {
        let mut session = CanvasSession::new("test-1".to_string());
        assert!(session.components.is_empty());

        session.push("<h1>Hello</h1>".to_string());
        assert_eq!(session.components.len(), 1);

        session.push("<p>World</p>".to_string());
        assert_eq!(session.components.len(), 2);

        session.reset();
        assert!(session.components.is_empty());
    }
}
