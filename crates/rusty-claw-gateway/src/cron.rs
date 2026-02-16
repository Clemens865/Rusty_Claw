//! Cron scheduler — runs scheduled agent tasks.
//!
//! Uses `croner` for cron expression parsing and a background tokio task
//! that checks job schedules every 30 seconds.

use std::sync::Arc;

use chrono::Utc;
use croner::Cron;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use rusty_claw_core::config::CronJob;
use rusty_claw_core::types::InboundMessage;

use crate::state::GatewayState;

/// A running cron scheduler.
pub struct CronScheduler {
    jobs: Arc<RwLock<Vec<CronJob>>>,
    /// Timestamp of the last check, to avoid re-triggering.
    last_check: Arc<RwLock<chrono::DateTime<Utc>>>,
}

impl CronScheduler {
    /// Create a new scheduler with the given initial jobs.
    pub fn new(jobs: Vec<CronJob>) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(jobs)),
            last_check: Arc::new(RwLock::new(Utc::now())),
        }
    }

    /// Start the background scheduler loop.
    pub fn start(self: Arc<Self>, state: Arc<GatewayState>) {
        let scheduler = self.clone();
        tokio::spawn(async move {
            info!("Cron scheduler started");
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                scheduler.check_and_run(&state).await;
            }
        });
    }

    /// Check which jobs are due and execute them.
    async fn check_and_run(&self, state: &Arc<GatewayState>) {
        let now = Utc::now();
        let last = *self.last_check.read().await;

        let jobs = self.jobs.read().await;
        for job in jobs.iter() {
            if !job.enabled {
                continue;
            }

            let cron = match Cron::new(&job.schedule).parse() {
                Ok(c) => c,
                Err(e) => {
                    warn!(job_id = %job.id, %e, "Invalid cron expression");
                    continue;
                }
            };

            // Check if there's a trigger time between last_check and now
            let has_trigger = cron
                .iter_after(last)
                .take(1)
                .any(|t| t <= now);

            if has_trigger {
                info!(job_id = %job.id, task = %job.task, "Cron job triggered");
                self.execute_job(job, state).await;
            }
        }

        *self.last_check.write().await = now;
    }

    /// Execute a single cron job by sending its task as an inbound message.
    async fn execute_job(&self, job: &CronJob, state: &Arc<GatewayState>) {
        let message = InboundMessage::from_cli_text(&job.task);

        let key = rusty_claw_core::session::SessionKey {
            channel: "cron".into(),
            account_id: "scheduler".into(),
            chat_type: rusty_claw_core::types::ChatType::Dm,
            peer_id: job.session_key.clone().unwrap_or_else(|| job.id.clone()),
            scope: rusty_claw_core::session::SessionScope::PerSender,
        };

        let mut session = match state.sessions.load(&key).await {
            Ok(Some(s)) => s,
            Ok(None) => rusty_claw_core::session::Session::new(key.clone()),
            Err(e) => {
                error!(job_id = %job.id, %e, "Failed to load cron session");
                return;
            }
        };

        let (provider, credentials) = match state.providers.default() {
            Some(pc) => pc,
            None => {
                error!("No default provider for cron job");
                return;
            }
        };

        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        // Read config snapshot
        let config = std::sync::Arc::new(state.read_config().await);

        match rusty_claw_agent::run_agent(
            &mut session,
            message,
            &config,
            &state.tools,
            provider,
            credentials,
            event_tx,
            &state.hooks,
        )
        .await
        {
            Ok(result) => {
                debug!(job_id = %job.id, "Cron job completed");
                if let Some(err) = &result.meta.error {
                    warn!(job_id = %job.id, error = %err.message, "Cron job had error");
                }
            }
            Err(e) => {
                error!(job_id = %job.id, %e, "Cron job failed");
            }
        }

        // Save session
        if let Err(e) = state.sessions.save(&session).await {
            error!(job_id = %job.id, %e, "Failed to save cron session");
        }
    }

    /// Add a new job.
    pub async fn add_job(&self, job: CronJob) -> Result<(), String> {
        // Validate cron expression
        Cron::new(&job.schedule)
            .parse()
            .map_err(|e| format!("Invalid cron expression: {e}"))?;

        let mut jobs = self.jobs.write().await;
        if jobs.iter().any(|j| j.id == job.id) {
            return Err(format!("Job '{}' already exists", job.id));
        }
        jobs.push(job);
        Ok(())
    }

    /// Remove a job by ID.
    pub async fn remove_job(&self, id: &str) -> bool {
        let mut jobs = self.jobs.write().await;
        let len_before = jobs.len();
        jobs.retain(|j| j.id != id);
        jobs.len() < len_before
    }

    /// List all jobs.
    pub async fn list_jobs(&self) -> Vec<CronJob> {
        self.jobs.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_expression_parsing() {
        // Valid expression
        let result = Cron::new("0 9 * * *").parse();
        assert!(result.is_ok());

        // Invalid expression
        let result = Cron::new("invalid").parse();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_remove_job() {
        let scheduler = CronScheduler::new(vec![]);

        let job = CronJob {
            id: "test-job".into(),
            schedule: "0 9 * * *".into(),
            task: "Good morning".into(),
            session_key: None,
            enabled: true,
        };

        scheduler.add_job(job).await.unwrap();
        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "test-job");

        // Duplicate add should fail
        let dup = CronJob {
            id: "test-job".into(),
            schedule: "0 10 * * *".into(),
            task: "Another".into(),
            session_key: None,
            enabled: true,
        };
        assert!(scheduler.add_job(dup).await.is_err());

        // Remove
        assert!(scheduler.remove_job("test-job").await);
        assert!(scheduler.list_jobs().await.is_empty());

        // Remove nonexistent
        assert!(!scheduler.remove_job("nonexistent").await);
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let scheduler = CronScheduler::new(vec![]);
        let job = CronJob {
            id: "bad".into(),
            schedule: "not a cron".into(),
            task: "test".into(),
            session_key: None,
            enabled: true,
        };
        assert!(scheduler.add_job(job).await.is_err());
    }
}
