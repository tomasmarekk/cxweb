//! Durable metadata only. A retry never causes a second browser submission.
use cxweb_domain::{SessionKey, TurnState};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Ledger {
    connection: Arc<Mutex<Connection>>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    New,
    Existing(TurnState),
}

impl Ledger {
    /// Caller supplies an application-owned, permission-protected state path.
    pub async fn open(path: &Path) -> Result<Self, &'static str> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            Self::from_connection(Connection::open(path).map_err(|_| "E_LEDGER_OPEN")?)
        })
        .await
        .map_err(|_| "E_LEDGER_WORKER")?
    }
    fn from_connection(connection: Connection) -> Result<Self, &'static str> {
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS turns (
              request_id TEXT PRIMARY KEY,
              session_hash TEXT NOT NULL,
              input_hash TEXT NOT NULL,
              state TEXT NOT NULL,
              revision INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
            )
            .map_err(|_| "E_LEDGER_SCHEMA")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, &'static str> + Send + 'static,
    ) -> Result<T, &'static str> {
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = connection.lock().map_err(|_| "E_LEDGER_LOCK")?;
            work(&mut connection)
        })
        .await
        .map_err(|_| "E_LEDGER_WORKER")?
    }
    pub async fn admit(
        &self,
        request: &str,
        session: &SessionKey,
        input: &[u8],
    ) -> Result<Admission, &'static str> {
        // Raw request/session IDs may embed upstream data: only hashes persist.
        let request = hash(request.as_bytes());
        let session = hash(&serde_json::to_vec(session).map_err(|_| "E_SESSION_ID")?);
        let input = hash(input);
        self.run(move |connection| {
            let transaction = connection.transaction().map_err(|_| "E_LEDGER_WRITE")?;
            let existing: Option<(String, String, String)> = transaction.query_row("SELECT session_hash,input_hash,state FROM turns WHERE request_id=?1", [&request], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional().map_err(|_| "E_LEDGER_READ")?;
            if let Some((old_session, old_input, state)) = existing {
                if old_session != session || old_input != input { return Err("E_REQUEST_ID_CONFLICT"); }
                return Ok(Admission::Existing(decode(&state)?));
            }
            transaction.execute("INSERT INTO turns(request_id,session_hash,input_hash,state) VALUES(?1,?2,?3,?4)", params![request,session,input,encode(TurnState::Prepared)?]).map_err(|_| "E_LEDGER_WRITE")?;
            transaction.commit().map_err(|_| "E_LEDGER_WRITE")?;
            Ok(Admission::New)
        }).await
    }
    pub async fn transition(&self, request: &str, next: TurnState) -> Result<(), &'static str> {
        let request = hash(request.as_bytes());
        self.run(move |connection| {
            let transaction = connection.transaction().map_err(|_| "E_LEDGER_WRITE")?;
            let encoded: String = transaction
                .query_row(
                    "SELECT state FROM turns WHERE request_id=?1",
                    [&request],
                    |row| row.get(0),
                )
                .map_err(|_| "E_LEDGER_READ")?;
            let mut state = decode(&encoded)?;
            state.transition(next).map_err(|_| "E_TURN_TRANSITION")?;
            transaction
                .execute(
                    "UPDATE turns SET state=?1,revision=revision+1 WHERE request_id=?2",
                    params![encode(state)?, request],
                )
                .map_err(|_| "E_LEDGER_WRITE")?;
            transaction.commit().map_err(|_| "E_LEDGER_WRITE")?;
            Ok(())
        })
        .await
    }
    /// A runtime restart invalidates active browser leases. No request is resent.
    pub async fn recover(&self) -> Result<usize, &'static str> {
        self.run(|connection| {
            let transaction = connection.transaction().map_err(|_| "E_LEDGER_WRITE")?;
            let uncertain = transaction
                .execute(
                    "UPDATE turns SET state=?1,revision=revision+1 WHERE state=?2",
                    params![
                        encode(TurnState::SubmissionUncertain)?,
                        encode(TurnState::Submitting)?
                    ],
                )
                .map_err(|_| "E_LEDGER_WRITE")?;
            let failed = transaction
                .execute(
                    "UPDATE turns SET state=?1,revision=revision+1 WHERE state IN (?2,?3,?4,?5)",
                    params![
                        encode(TurnState::Failed)?,
                        encode(TurnState::Prepared)?,
                        encode(TurnState::ObservedBaseline)?,
                        encode(TurnState::Submitted)?,
                        encode(TurnState::Generating)?
                    ],
                )
                .map_err(|_| "E_LEDGER_WRITE")?;
            transaction.commit().map_err(|_| "E_LEDGER_WRITE")?;
            Ok(uncertain + failed)
        })
        .await
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encode(state: TurnState) -> Result<String, &'static str> {
    serde_json::to_string(&state).map_err(|_| "E_LEDGER_STATE")
}
fn decode(state: &str) -> Result<TurnState, &'static str> {
    serde_json::from_str(state).map_err(|_| "E_LEDGER_STATE")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> SessionKey {
        SessionKey {
            installation: "i".into(),
            native_session: "s".into(),
            account_scope: "a".into(),
            workspace_scope: "w".into(),
            route: "webbridge/test".into(),
            epoch: 0,
        }
    }
    #[tokio::test]
    async fn retry_does_not_readmit_submitted_or_uncertain_work() {
        let ledger = Ledger::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(
            ledger
                .admit("r", &session(), b"sensitive prompt")
                .await
                .unwrap(),
            Admission::New
        );
        ledger
            .transition("r", TurnState::ObservedBaseline)
            .await
            .unwrap();
        ledger.transition("r", TurnState::Submitting).await.unwrap();
        assert_eq!(ledger.recover().await.unwrap(), 1);
        assert_eq!(
            ledger
                .admit("r", &session(), b"sensitive prompt")
                .await
                .unwrap(),
            Admission::Existing(TurnState::SubmissionUncertain)
        );
        assert!(ledger.transition("r", TurnState::Submitted).await.is_err());
        assert!(ledger.admit("r", &session(), b"different").await.is_err());
        let mut other = session();
        other.account_scope = "other".into();
        assert!(
            ledger
                .admit("r", &other, b"sensitive prompt")
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn completed_work_survives_recovery_as_delivery_only() {
        let ledger = Ledger::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        ledger.admit("r", &session(), b"p").await.unwrap();
        for state in [
            TurnState::ObservedBaseline,
            TurnState::Submitting,
            TurnState::Submitted,
            TurnState::Completed,
        ] {
            ledger.transition("r", state).await.unwrap();
        }
        assert_eq!(ledger.recover().await.unwrap(), 0);
        assert_eq!(
            ledger.admit("r", &session(), b"p").await.unwrap(),
            Admission::Existing(TurnState::Completed)
        );
    }
}
