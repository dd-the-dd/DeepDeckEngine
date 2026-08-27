use rusqlite::{Connection, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_BATCH_SIZE: usize = 128;
const MAX_BATCH_BYTES: usize = 1_048_576;
const BATCH_MAX_DELAY: Duration = Duration::from_millis(20);
const WRITER_MAX_DELAY: Duration = Duration::from_millis(2);
const WRITER_CHANNEL_CAPACITY: usize = 128;
const DEFAULT_SPOOL_MAX_BYTES: u64 = 2 * 1_024 * 1_024 * 1_024;
const RETRY_MINIMUM: Duration = Duration::from_millis(100);
const RETRY_MAXIMUM: Duration = Duration::from_secs(5);
const CONFIGURATION_RETRY: Duration = Duration::from_secs(30);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub event_id: String,
    pub stream_id: String,
    pub sequence_number: u64,
    pub event_type: String,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    pub payload: Value,
    pub occurred_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQueueStatus {
    pub max_bytes: u64,
    pub path: String,
    pub pending_bytes: u64,
    pub pending_events: u64,
    pub quarantined_bytes: u64,
    pub quarantined_events: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeliveryErrorKind {
    Configuration,
    Payload,
    Transient,
}

#[derive(Debug)]
struct DeliveryError {
    kind: DeliveryErrorKind,
    message: String,
}

impl DeliveryError {
    fn new(kind: DeliveryErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

trait HistorySink: Send + 'static {
    fn deliver(&mut self, events: &[HistoryEvent]) -> Result<(), DeliveryError>;
}

struct HttpHistorySink {
    agent: ureq::Agent,
    endpoint: String,
    token: String,
}

impl HttpHistorySink {
    fn new(endpoint: String, token: String) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(2))
                .timeout_read(Duration::from_secs(15))
                .timeout_write(Duration::from_secs(15))
                .build(),
            endpoint,
            token,
        }
    }
}

impl HistorySink for HttpHistorySink {
    fn deliver(&mut self, events: &[HistoryEvent]) -> Result<(), DeliveryError> {
        self.agent
            .post(&self.endpoint)
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(json!({ "events": events }))
            .map(|_| ())
            .map_err(classify_delivery_error)
    }
}

fn classify_delivery_error(error: ureq::Error) -> DeliveryError {
    match error {
        ureq::Error::Status(status, response) => {
            let message = format!("HTTP {status}: {}", response.status_text());
            let kind = match status {
                400 | 413 | 422 => DeliveryErrorKind::Payload,
                401 | 403 | 404 | 405 => DeliveryErrorKind::Configuration,
                408 | 425 | 429 | 500..=599 => DeliveryErrorKind::Transient,
                _ if (400..500).contains(&status) => DeliveryErrorKind::Payload,
                _ => DeliveryErrorKind::Transient,
            };
            DeliveryError::new(kind, message)
        }
        ureq::Error::Transport(error) => {
            DeliveryError::new(DeliveryErrorKind::Transient, error.to_string())
        }
    }
}

#[derive(Clone)]
struct HistorySpool {
    max_bytes: u64,
    path: Arc<PathBuf>,
}

struct StoredEvent {
    id: i64,
    encoded_bytes: u64,
    event: HistoryEvent,
}

struct StoredBatch {
    encoded_bytes: u64,
    events: Vec<StoredEvent>,
}

impl HistorySpool {
    fn prepare(path: PathBuf, max_bytes: u64) -> Result<Self, String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "history spool directory {} cannot be created: {error}",
                    parent.display()
                )
            })?;
        }
        let spool = Self {
            max_bytes,
            path: Arc::new(path),
        };
        let connection = spool.connect()?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("history spool WAL mode cannot be enabled: {error}"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| format!("history spool durability cannot be configured: {error}"))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS history_outbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    stream_id TEXT NOT NULL,
                    sequence_number INTEGER NOT NULL,
                    event_json BLOB NOT NULL,
                    encoded_bytes INTEGER NOT NULL,
                    enqueued_at_ms INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS history_outbox_stream_sequence
                    ON history_outbox (stream_id, sequence_number);
                 CREATE TABLE IF NOT EXISTS history_quarantine (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    stream_id TEXT NOT NULL,
                    sequence_number INTEGER NOT NULL,
                    event_json BLOB NOT NULL,
                    encoded_bytes INTEGER NOT NULL,
                    delivery_error TEXT NOT NULL,
                    quarantined_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS history_spool_state (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    pending_bytes INTEGER NOT NULL,
                    quarantined_bytes INTEGER NOT NULL
                 );
                 INSERT OR IGNORE INTO history_spool_state
                    (id, pending_bytes, quarantined_bytes) VALUES (1, 0, 0);
                 UPDATE history_spool_state SET
                    pending_bytes = COALESCE((SELECT sum(encoded_bytes) FROM history_outbox), 0),
                    quarantined_bytes = COALESCE((SELECT sum(encoded_bytes) FROM history_quarantine), 0)
                 WHERE id = 1;",
            )
            .map_err(|error| format!("history spool schema cannot be prepared: {error}"))?;
        Ok(spool)
    }

    fn connect(&self) -> Result<Connection, String> {
        let connection = Connection::open(self.path.as_ref()).map_err(|error| {
            format!(
                "history spool {} cannot be opened: {error}",
                self.path.display()
            )
        })?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| format!("history spool busy timeout cannot be configured: {error}"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| format!("history spool durability cannot be configured: {error}"))?;
        Ok(connection)
    }

    fn persist(&self, connection: &mut Connection, events: &[HistoryEvent]) -> Result<(), String> {
        let encoded = events
            .iter()
            .map(|event| {
                serde_json::to_vec(event)
                    .map(|bytes| (event, bytes))
                    .map_err(|error| format!("history event cannot be serialized: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("history spool transaction cannot start: {error}"))?;
        let stored_bytes = transaction
            .query_row(
                "SELECT pending_bytes + quarantined_bytes
                 FROM history_spool_state WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| u64::try_from(value).unwrap_or(0))
            .map_err(|error| format!("history spool size cannot be read: {error}"))?;
        let mut added_bytes = 0_u64;
        for (event, bytes) in encoded {
            let encoded_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            let inserted = transaction
                .execute(
                    "INSERT OR IGNORE INTO history_outbox (
                        event_id, stream_id, sequence_number, event_json,
                        encoded_bytes, enqueued_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        event.event_id,
                        event.stream_id,
                        i64::try_from(event.sequence_number).unwrap_or(i64::MAX),
                        bytes,
                        i64::try_from(encoded_bytes).unwrap_or(i64::MAX),
                        i64::try_from(now_millis()).unwrap_or(i64::MAX),
                    ],
                )
                .map_err(|error| format!("history event cannot be spooled: {error}"))?;
            if inserted == 1 {
                added_bytes = added_bytes.saturating_add(encoded_bytes);
            }
        }
        if stored_bytes.saturating_add(added_bytes) > self.max_bytes {
            return Err(format!(
                "history spool quota reached ({} + {} > {} bytes); official game progress is paused",
                stored_bytes, added_bytes, self.max_bytes
            ));
        }
        transaction
            .execute(
                "UPDATE history_spool_state
                 SET pending_bytes = pending_bytes + ?1 WHERE id = 1",
                [i64::try_from(added_bytes).unwrap_or(i64::MAX)],
            )
            .map_err(|error| format!("history spool size cannot be updated: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("history spool transaction cannot commit: {error}"))
    }

    fn load_batch(
        &self,
        connection: &Connection,
        max_events: usize,
    ) -> Result<StoredBatch, String> {
        let mut statement = connection
            .prepare(
                "SELECT id, event_json, encoded_bytes
                 FROM history_outbox ORDER BY id LIMIT ?1",
            )
            .map_err(|error| format!("history spool batch cannot be prepared: {error}"))?;
        let rows = statement
            .query_map([i64::try_from(max_events).unwrap_or(i64::MAX)], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|error| format!("history spool batch cannot be read: {error}"))?;
        let mut batch = StoredBatch {
            encoded_bytes: 0,
            events: Vec::new(),
        };
        for row in rows {
            let (id, bytes, encoded_bytes) =
                row.map_err(|error| format!("history spool row cannot be read: {error}"))?;
            let encoded_bytes = u64::try_from(encoded_bytes)
                .map_err(|_| "history spool row has a negative byte size".to_string())?;
            if !batch.events.is_empty()
                && batch.encoded_bytes.saturating_add(encoded_bytes) > MAX_BATCH_BYTES as u64
            {
                break;
            }
            let event = serde_json::from_slice(&bytes)
                .map_err(|error| format!("spooled history event is invalid: {error}"))?;
            batch.encoded_bytes = batch.encoded_bytes.saturating_add(encoded_bytes);
            batch.events.push(StoredEvent {
                id,
                encoded_bytes,
                event,
            });
        }
        Ok(batch)
    }

    fn acknowledge(&self, connection: &mut Connection, batch: &StoredBatch) -> Result<(), String> {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("history acknowledgement cannot start: {error}"))?;
        let mut removed_bytes = 0_u64;
        for event in &batch.events {
            let removed = transaction
                .execute("DELETE FROM history_outbox WHERE id = ?1", [event.id])
                .map_err(|error| format!("acknowledged history cannot be removed: {error}"))?;
            if removed == 1 {
                removed_bytes = removed_bytes.saturating_add(event.encoded_bytes);
            }
        }
        transaction
            .execute(
                "UPDATE history_spool_state SET pending_bytes = max(0, pending_bytes - ?1)
                 WHERE id = 1",
                [i64::try_from(removed_bytes).unwrap_or(i64::MAX)],
            )
            .map_err(|error| format!("history spool size cannot be updated: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("history acknowledgement cannot commit: {error}"))
    }

    fn quarantine(
        &self,
        connection: &mut Connection,
        event: &StoredEvent,
        error: &str,
    ) -> Result<(), String> {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|failure| format!("history quarantine cannot start: {failure}"))?;
        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO history_quarantine (
                    event_id, stream_id, sequence_number, event_json, encoded_bytes,
                    delivery_error, quarantined_at_ms
                 )
                 SELECT event_id, stream_id, sequence_number, event_json, encoded_bytes, ?2, ?3
                 FROM history_outbox WHERE id = ?1",
                params![
                    event.id,
                    error,
                    i64::try_from(now_millis()).unwrap_or(i64::MAX)
                ],
            )
            .map_err(|failure| format!("history event cannot be quarantined: {failure}"))?;
        let removed = transaction
            .execute("DELETE FROM history_outbox WHERE id = ?1", [event.id])
            .map_err(|failure| format!("quarantined history cannot leave the outbox: {failure}"))?;
        transaction
            .execute(
                "UPDATE history_spool_state SET
                    pending_bytes = max(0, pending_bytes - ?1),
                    quarantined_bytes = quarantined_bytes + ?2
                 WHERE id = 1",
                params![
                    if removed == 1 {
                        i64::try_from(event.encoded_bytes).unwrap_or(i64::MAX)
                    } else {
                        0
                    },
                    if inserted == 1 {
                        i64::try_from(event.encoded_bytes).unwrap_or(i64::MAX)
                    } else {
                        0
                    }
                ],
            )
            .map_err(|failure| format!("history quarantine size cannot be updated: {failure}"))?;
        transaction
            .commit()
            .map_err(|failure| format!("history quarantine cannot commit: {failure}"))
    }

    fn status(&self) -> Result<HistoryQueueStatus, String> {
        let connection = self.connect()?;
        let (pending_events, pending_bytes, quarantined_events, quarantined_bytes) = connection
            .query_row(
                "SELECT
                    (SELECT count(*) FROM history_outbox),
                    pending_bytes,
                    (SELECT count(*) FROM history_quarantine),
                    quarantined_bytes
                 FROM history_spool_state WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(|error| format!("history spool status cannot be read: {error}"))?;
        Ok(HistoryQueueStatus {
            max_bytes: self.max_bytes,
            path: self.path.display().to_string(),
            pending_bytes: u64::try_from(pending_bytes).unwrap_or(0),
            pending_events: u64::try_from(pending_events).unwrap_or(0),
            quarantined_bytes: u64::try_from(quarantined_bytes).unwrap_or(0),
            quarantined_events: u64::try_from(quarantined_events).unwrap_or(0),
        })
    }

    #[cfg(test)]
    fn pending_count(&self) -> u64 {
        self.connect()
            .and_then(|connection| {
                connection
                    .query_row("SELECT count(*) FROM history_outbox", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(|error| error.to_string())
            })
            .map(|count| u64::try_from(count).unwrap_or(0))
            .unwrap()
    }
}

struct PersistRequest {
    event: HistoryEvent,
    persisted: mpsc::SyncSender<()>,
}

type WorkSignal = Arc<(Mutex<u64>, Condvar)>;

#[derive(Clone)]
pub struct HistoryQueue {
    sender: mpsc::SyncSender<PersistRequest>,
    spool: HistorySpool,
    writer_id: Arc<str>,
}

impl HistoryQueue {
    pub fn from_env() -> Option<Self> {
        let Some(token) = std::env::var("DDL_ENGINE_INGEST_TOKEN")
            .ok()
            .filter(|value| !value.is_empty())
        else {
            eprintln!("history publisher disabled: DDL_ENGINE_INGEST_TOKEN is not configured");
            return None;
        };
        let endpoint = std::env::var("DDL_HISTORY_INGEST_URL")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                let base = std::env::var("DDL_PUBLIC_API_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:8790".to_string());
                format!("{}/v1/history/events", base.trim_end_matches('/'))
            });
        let path = std::env::var_os("DDL_HISTORY_SPOOL_PATH")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(".local-app").join("history-outbox.sqlite3"));
        let max_bytes = std::env::var("DDL_HISTORY_SPOOL_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value >= MAX_BATCH_BYTES as u64)
            .unwrap_or(DEFAULT_SPOOL_MAX_BYTES);
        eprintln!(
            "history publisher enabled: {endpoint} (durable spool {}, {} byte quota)",
            path.display(),
            max_bytes
        );
        Some(
            Self::with_sink_at(HttpHistorySink::new(endpoint, token), path, max_bytes)
                .unwrap_or_else(|error| panic!("official history spool is unavailable: {error}")),
        )
    }

    fn with_sink_at(sink: impl HistorySink, path: PathBuf, max_bytes: u64) -> Result<Self, String> {
        let spool = HistorySpool::prepare(path, max_bytes)?;
        let (sender, receiver) = mpsc::sync_channel(WRITER_CHANNEL_CAPACITY);
        let writer_id: Arc<str> = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
        .into();
        let signal = Arc::new((Mutex::new(0), Condvar::new()));
        let writer_spool = spool.clone();
        let writer_signal = Arc::clone(&signal);
        thread::Builder::new()
            .name("mtg-history-spool-writer".to_string())
            .spawn(move || history_spool_writer(receiver, writer_spool, writer_signal))
            .map_err(|error| format!("history spool writer cannot start: {error}"))?;
        let upload_spool = spool.clone();
        thread::Builder::new()
            .name("mtg-history-uploader".to_string())
            .spawn(move || history_upload_worker(upload_spool, signal, sink))
            .map_err(|error| format!("history uploader cannot start: {error}"))?;
        Ok(Self {
            sender,
            spool,
            writer_id,
        })
    }

    pub fn stream(&self, session_id: impl Into<String>) -> HistoryStream {
        HistoryStream {
            next_sequence: Arc::new(Mutex::new(1)),
            previous_observations: Arc::new(Mutex::new(serde_json::Map::new())),
            queue: self.clone(),
            stream_id: format!("{}:{}", self.writer_id, session_id.into()),
        }
    }

    pub fn status(&self) -> Result<HistoryQueueStatus, String> {
        self.spool.status()
    }

    fn persist(&self, event: HistoryEvent) {
        let (persisted, confirmation) = mpsc::sync_channel(1);
        if self
            .sender
            .send(PersistRequest { event, persisted })
            .is_err()
        {
            panic!("official history spool writer stopped unexpectedly");
        }
        if confirmation.recv().is_err() {
            panic!("official history event could not be persisted");
        }
    }
}

#[derive(Clone)]
pub struct HistoryStream {
    queue: HistoryQueue,
    stream_id: String,
    next_sequence: Arc<Mutex<u64>>,
    previous_observations: Arc<Mutex<serde_json::Map<String, Value>>>,
}

impl HistoryStream {
    pub fn publish_authoritative(&self, event_type: &str, payload: Value) {
        self.publish(event_type, "authoritative", None, payload);
    }

    pub fn publish_player_event(&self, event_type: &str, player_id: &str, payload: Value) {
        self.publish(event_type, "player", Some(player_id.to_string()), payload);
    }

    pub fn publish_player_observation(
        &self,
        event_type: &str,
        player_id: &str,
        observation: Value,
        mut payload: serde_json::Map<String, Value>,
    ) {
        let observation = compact_persisted_observation(observation);
        let observation_update = {
            let mut previous = self
                .previous_observations
                .lock()
                .expect("history observations lock");
            let update = previous.get(player_id).map_or_else(
                || {
                    json!({
                        "kind": "fullObservation",
                        "observation": observation,
                    })
                },
                |old| {
                    json!({
                        "kind": "observationDelta",
                        "patch": crate::observation_delta::merge_patch(old, &observation),
                    })
                },
            );
            previous.insert(player_id.to_string(), observation);
            update
        };
        payload.insert("observationUpdate".to_string(), observation_update);
        self.publish(
            event_type,
            "player",
            Some(player_id.to_string()),
            Value::Object(payload),
        );
    }

    fn publish(
        &self,
        event_type: &str,
        visibility: &str,
        player_id: Option<String>,
        payload: Value,
    ) {
        let mut next_sequence = self
            .next_sequence
            .lock()
            .expect("history stream sequence lock");
        let sequence_number = *next_sequence;
        *next_sequence = (*next_sequence).saturating_add(1);
        self.queue.persist(HistoryEvent {
            event_id: format!("{}:{sequence_number}", self.stream_id),
            stream_id: self.stream_id.clone(),
            sequence_number,
            event_type: event_type.to_string(),
            visibility: visibility.to_string(),
            player_id,
            payload,
            occurred_at_ms: now_millis(),
        });
    }
}

fn compact_persisted_observation(mut observation: Value) -> Value {
    let Some(state) = observation.as_object_mut() else {
        return observation;
    };

    // Events are cumulative, while the authoritative history already records each
    // transition separately. Libraries and sideboards are immutable deck data and
    // are available through the persisted deck version. Keeping them in every
    // array-based merge patch caused hundreds of kilobytes of duplicate JSON per
    // decision in Commander games.
    state.remove("events");
    if let Some(players) = state.get_mut("players").and_then(Value::as_array_mut) {
        for player in players {
            let Some(player) = player.as_object_mut() else {
                continue;
            };
            if let Some(library) = player.get_mut("library") {
                let card_count = library.as_array().map(Vec::len).unwrap_or_default();
                *library = Value::Array(vec![Value::Null; card_count]);
            }
            if let Some(sideboard) = player.get_mut("sideboard") {
                *sideboard = Value::Array(Vec::new());
            }
        }
    }
    observation
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn history_spool_writer(
    receiver: mpsc::Receiver<PersistRequest>,
    spool: HistorySpool,
    signal: WorkSignal,
) {
    let mut connection = connect_with_retry(&spool, "writer");
    while let Ok(first) = receiver.recv() {
        let mut requests = vec![first];
        let deadline = Instant::now() + WRITER_MAX_DELAY;
        while requests.len() < MAX_BATCH_SIZE {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(request) => requests.push(request),
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
        let events = requests
            .iter()
            .map(|request| request.event.clone())
            .collect::<Vec<_>>();
        let mut retry_delay = RETRY_MINIMUM;
        loop {
            match spool.persist(&mut connection, &events) {
                Ok(()) => break,
                Err(error) => {
                    eprintln!(
                        "history spool write failed ({} events); retrying in {} ms: {error}",
                        events.len(),
                        retry_delay.as_millis()
                    );
                    thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
                    if let Ok(reopened) = spool.connect() {
                        connection = reopened;
                    }
                }
            }
        }
        for request in requests {
            let _ = request.persisted.send(());
        }
        let (generation, changed) = &*signal;
        *generation.lock().expect("history work signal lock") += 1;
        changed.notify_one();
    }
}

fn history_upload_worker(spool: HistorySpool, signal: WorkSignal, mut sink: impl HistorySink) {
    let mut connection = connect_with_retry(&spool, "uploader");
    let mut retry_delay = RETRY_MINIMUM;
    let mut batch_limit = MAX_BATCH_SIZE;
    loop {
        let batch = match spool.load_batch(&connection, batch_limit) {
            Ok(batch) if batch.events.is_empty() => {
                wait_for_history_work(&signal);
                batch_limit = MAX_BATCH_SIZE;
                continue;
            }
            Ok(batch) => batch,
            Err(error) => {
                eprintln!("history spool read failed; retrying: {error}");
                thread::sleep(retry_delay);
                retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
                if let Ok(reopened) = spool.connect() {
                    connection = reopened;
                }
                continue;
            }
        };
        let events = batch
            .events
            .iter()
            .map(|stored| stored.event.clone())
            .collect::<Vec<_>>();
        match sink.deliver(&events) {
            Ok(()) => match spool.acknowledge(&mut connection, &batch) {
                Ok(()) => {
                    retry_delay = RETRY_MINIMUM;
                    batch_limit = MAX_BATCH_SIZE;
                }
                Err(error) => {
                    eprintln!("history acknowledgement failed; delivery will be retried: {error}");
                    thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
                }
            },
            Err(error) if error.kind == DeliveryErrorKind::Payload && batch.events.len() > 1 => {
                batch_limit = (batch.events.len() / 2).max(1);
            }
            Err(error) if error.kind == DeliveryErrorKind::Payload => {
                let event = &batch.events[0];
                eprintln!(
                    "history event {} was rejected and moved to durable quarantine: {}",
                    event.event.event_id, error.message
                );
                if let Err(failure) = spool.quarantine(&mut connection, event, &error.message) {
                    eprintln!("history quarantine failed; event remains pending: {failure}");
                    thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
                } else {
                    retry_delay = RETRY_MINIMUM;
                    batch_limit = MAX_BATCH_SIZE;
                }
            }
            Err(error) => {
                let delay = if error.kind == DeliveryErrorKind::Configuration {
                    CONFIGURATION_RETRY
                } else {
                    retry_delay
                };
                eprintln!(
                    "history batch delivery failed ({} events, {} bytes); retrying in {} ms: {}",
                    batch.events.len(),
                    batch.encoded_bytes,
                    delay.as_millis(),
                    error.message
                );
                thread::sleep(delay);
                if error.kind == DeliveryErrorKind::Transient {
                    retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
                }
            }
        }
    }
}

fn connect_with_retry(spool: &HistorySpool, role: &str) -> Connection {
    let mut retry_delay = RETRY_MINIMUM;
    loop {
        match spool.connect() {
            Ok(connection) => return connection,
            Err(error) => {
                eprintln!(
                    "history spool {role} connection failed; retrying in {} ms: {error}",
                    retry_delay.as_millis()
                );
                thread::sleep(retry_delay);
                retry_delay = (retry_delay * 2).min(RETRY_MAXIMUM);
            }
        }
    }
}

fn wait_for_history_work(signal: &WorkSignal) {
    let (generation, changed) = &**signal;
    let guard = generation.lock().expect("history work signal lock");
    let current = *guard;
    let (_guard, timeout) = changed
        .wait_timeout_while(guard, IDLE_POLL_INTERVAL, |value| *value == current)
        .expect("history work signal wait");
    if !timeout.timed_out() {
        thread::sleep(BATCH_MAX_DELAY);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SPOOL_MAX_BYTES, DeliveryError, DeliveryErrorKind, HistoryEvent, HistoryQueue,
        HistorySink, HistorySpool, compact_persisted_observation,
    };
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn temp_spool(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mtg-history-{name}-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    struct RecordingSink(Arc<Mutex<Vec<Vec<HistoryEvent>>>>);

    impl HistorySink for RecordingSink {
        fn deliver(&mut self, events: &[HistoryEvent]) -> Result<(), DeliveryError> {
            self.0.lock().unwrap().push(events.to_vec());
            Ok(())
        }
    }

    struct FailingSink;

    impl HistorySink for FailingSink {
        fn deliver(&mut self, _events: &[HistoryEvent]) -> Result<(), DeliveryError> {
            Err(DeliveryError::new(
                DeliveryErrorKind::Transient,
                "platform offline",
            ))
        }
    }

    struct SelectiveSink(Arc<Mutex<Vec<String>>>);

    #[test]
    fn persisted_observations_do_not_duplicate_deck_zones_or_cumulative_events() {
        let card = json!({
            "instanceId": "card-1",
            "definition": {"id": "oracle-1", "name": "Card", "rules": [{"kind": "effect"}]}
        });
        let observation = json!({
            "turnNumber": 4,
            "events": [{"kind": "draw"}],
            "players": [{
                "library": [card.clone(), card.clone()],
                "sideboard": [card.clone()],
                "hand": [card]
            }]
        });

        let compact = compact_persisted_observation(observation);

        assert!(compact.get("events").is_none());
        assert_eq!(
            compact.pointer("/players/0/library"),
            Some(&json!([null, null]))
        );
        assert_eq!(compact.pointer("/players/0/sideboard"), Some(&json!([])));
        assert_eq!(
            compact.pointer("/players/0/hand/0/definition/name"),
            Some(&json!("Card"))
        );
    }

    impl HistorySink for SelectiveSink {
        fn deliver(&mut self, events: &[HistoryEvent]) -> Result<(), DeliveryError> {
            if events
                .iter()
                .any(|event| event.payload.get("reject") == Some(&json!(true)))
            {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::Payload,
                    "invalid event payload",
                ));
            }
            self.0
                .lock()
                .unwrap()
                .extend(events.iter().map(|event| event.event_id.clone()));
            Ok(())
        }
    }

    #[test]
    fn concurrent_events_are_durably_micro_batched_and_keep_stream_order() {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let queue = HistoryQueue::with_sink_at(
            RecordingSink(Arc::clone(&delivered)),
            temp_spool("batch"),
            DEFAULT_SPOOL_MAX_BYTES,
        )
        .unwrap();
        let stream = queue.stream("game-a");
        let publishers = (0..32)
            .map(|index| {
                let stream = stream.clone();
                thread::spawn(move || {
                    stream.publish_authoritative("decision.resolved", json!({ "index": index }));
                })
            })
            .collect::<Vec<_>>();
        for publisher in publishers {
            publisher.join().unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        while delivered
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            < 32
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        let batches = delivered.lock().unwrap();
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 32);
        assert!(batches.iter().any(|batch| batch.len() > 1));
        let sequences = batches
            .iter()
            .flatten()
            .map(|event| event.sequence_number)
            .collect::<Vec<_>>();
        assert_eq!(sequences, (1..=32).collect::<Vec<_>>());
    }

    #[test]
    fn parallel_sessions_sustain_a_bounded_normal_backlog() {
        const SESSION_COUNT: usize = 16;
        const EVENTS_PER_SESSION: usize = 32;
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let queue = HistoryQueue::with_sink_at(
            RecordingSink(Arc::clone(&delivered)),
            temp_spool("throughput"),
            DEFAULT_SPOOL_MAX_BYTES,
        )
        .unwrap();
        let started = Instant::now();
        let publishers = (0..SESSION_COUNT)
            .map(|session| {
                let stream = queue.stream(format!("game-{session}"));
                thread::spawn(move || {
                    for event in 0..EVENTS_PER_SESSION {
                        stream.publish_authoritative(
                            "decision.resolved",
                            json!({ "session": session, "event": event }),
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        for publisher in publishers {
            publisher.join().unwrap();
        }
        let expected = SESSION_COUNT * EVENTS_PER_SESSION;
        let deadline = Instant::now() + Duration::from_secs(5);
        while delivered
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            < expected
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        let batches = delivered.lock().unwrap();
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), expected);
        assert!(batches.len() < expected / 4);
        eprintln!(
            "history throughput probe: {expected} events in {} ms across {} HTTP batches",
            started.elapsed().as_millis(),
            batches.len()
        );
    }

    #[test]
    fn platform_outage_retains_events_on_disk_without_growing_a_memory_queue() {
        let queue =
            HistoryQueue::with_sink_at(FailingSink, temp_spool("offline"), DEFAULT_SPOOL_MAX_BYTES)
                .unwrap();
        let stream = queue.stream("game-a");
        stream.publish_authoritative("game.started", json!({}));
        stream.publish_authoritative("turn.completed", json!({}));
        stream.publish_authoritative("game.completed", json!({}));
        assert_eq!(queue.spool.pending_count(), 3);
    }

    #[test]
    fn a_rejected_event_is_quarantined_without_blocking_later_events() {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let queue = HistoryQueue::with_sink_at(
            SelectiveSink(Arc::clone(&delivered)),
            temp_spool("quarantine"),
            DEFAULT_SPOOL_MAX_BYTES,
        )
        .unwrap();
        let stream = queue.stream("game-a");
        stream.publish_authoritative("game.started", json!({}));
        stream.publish_authoritative("decision.resolved", json!({ "reject": true }));
        stream.publish_authoritative("game.completed", json!({}));

        let deadline = Instant::now() + Duration::from_secs(3);
        while queue.status().unwrap().pending_events > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let status = queue.status().unwrap();
        assert_eq!(status.pending_events, 0);
        assert_eq!(status.quarantined_events, 1);
        assert_eq!(delivered.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_reopened_spool_preserves_unacknowledged_events() {
        let path = temp_spool("recovery");
        let spool = HistorySpool::prepare(path.clone(), DEFAULT_SPOOL_MAX_BYTES).unwrap();
        let mut connection = spool.connect().unwrap();
        spool
            .persist(
                &mut connection,
                &[HistoryEvent {
                    event_id: "writer:game:1".to_string(),
                    stream_id: "writer:game".to_string(),
                    sequence_number: 1,
                    event_type: "game.started".to_string(),
                    visibility: "authoritative".to_string(),
                    player_id: None,
                    payload: json!({ "seed": 29 }),
                    occurred_at_ms: 1,
                }],
            )
            .unwrap();
        drop(connection);

        let delivered = Arc::new(Mutex::new(Vec::new()));
        let queue = HistoryQueue::with_sink_at(
            RecordingSink(Arc::clone(&delivered)),
            path,
            DEFAULT_SPOOL_MAX_BYTES,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while delivered.lock().unwrap().is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(delivered.lock().unwrap()[0][0].event_id, "writer:game:1");
        assert_eq!(queue.status().unwrap().pending_events, 0);
    }

    #[test]
    fn spool_quota_rejects_a_batch_atomically() {
        let spool = HistorySpool::prepare(temp_spool("quota"), 64).unwrap();
        let mut connection = spool.connect().unwrap();
        let error = spool
            .persist(
                &mut connection,
                &[HistoryEvent {
                    event_id: "writer:game:1".to_string(),
                    stream_id: "writer:game".to_string(),
                    sequence_number: 1,
                    event_type: "game.started".to_string(),
                    visibility: "authoritative".to_string(),
                    player_id: None,
                    payload: json!({ "state": "payload larger than the quota" }),
                    occurred_at_ms: 1,
                }],
            )
            .unwrap_err();
        assert!(error.contains("quota reached"));
        assert_eq!(spool.pending_count(), 0);
    }
}
