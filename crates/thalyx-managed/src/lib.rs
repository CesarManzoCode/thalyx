//! `managed-local-v1` on Linux, as a store with one writer.
//!
//! `vault/09-Notas-Tecnicas/Frontera-de-Plataforma.md`, and the contract it
//! implements is Thalyx-Kernel's `vault/architecture/persistence.md`: immutable
//! objects named by digest, private work forked from a generation, publication
//! as a compare-and-swap on that generation, a log in which a publication is
//! `PREPARE`d and `COMMIT`ted durably with the receipt that decided it, request
//! identities that make a retry an answer rather than a second effect, and a
//! recovery that resolves a prepared publication nobody committed as aborted
//! before admitting anything else.
//!
//! This is the service side of `thalyx_platform::managed`. The client never
//! reaches in here: it sends encoded requests through a transport, and this
//! answers them — so `linux-managed` is the same client over this store that the
//! Thalyx-Kernel backend is meant to be over the kernel's.
//!
//! ## On disk
//!
//! ```text
//! <store>/service.lock          held for as long as the service is open
//! <store>/log.jsonl             one record per line, each naming the digest of
//!                               the line before it
//! <store>/objects/ab/<digest>.<kind>
//! ```
//!
//! ## What this does not promise, said here rather than found later
//!
//! **That nothing else writes the store.** Linux gives this process no way to
//! stop another one with the same user from opening `objects/`. What it does is
//! re-hash every object it hands out and refuse one that does not match, and
//! refuse the whole log from the first record whose chain does not hold — so a
//! foreign writer is *detected* rather than *prevented*, and the profile says
//! exactly that. Preventing it is what a kernel that owns the medium is for.
//!
//! **That `fsync` means what it says.** The protocol assumes the stack below
//! honours a flush. Thalyx-Kernel's K4 records the same assumption about its own
//! driver.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thalyx_platform::authority::RequestId;
use thalyx_platform::content::{self, Manifest};
use thalyx_platform::evidence::Fetched;
use thalyx_platform::managed::protocol::{self, FORMAT, ObjectKind, Reply, Request};
use thalyx_platform::transport::{Service, TransportError};

pub const LOG: &str = "log.jsonl";
pub const OBJECTS: &str = "objects";
pub const LOCK: &str = "service.lock";

/// Where to stop, for a test that needs to see what a stop leaves behind.
///
/// A stop is modelled as the process going away: the call does not answer, and
/// every later call fails until the store is opened again — which is when
/// recovery runs. A fake that answered after "crashing" would not model the
/// property under test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Faults {
    /// The `PREPARE` is durable and nothing after it is.
    pub stop_after_prepare: bool,
    /// The `COMMIT` is durable and the reply is lost.
    pub stop_after_commit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum Record {
    Format {
        format: String,
        store: String,
        epoch: u64,
        publishers: Vec<String>,
    },
    Seed {
        request: RequestId,
        asked: String,
        root: String,
        receipt: String,
    },
    Fork {
        request: RequestId,
        asked: String,
        transaction: String,
        generation: u64,
    },
    Prepare {
        request: RequestId,
        asked: String,
        transaction: String,
        expected_generation: u64,
        candidate: String,
        receipt: String,
    },
    Commit {
        request: RequestId,
        prepared: u64,
        generation: u64,
        root: String,
        receipt: String,
    },
    Abort {
        request: RequestId,
        asked: String,
        reason: String,
        /// `(expected, current)` when the reason was a generation that moved.
        #[serde(default)]
        stale: Option<(u64, u64)>,
    },
    Abandon {
        request: RequestId,
        asked: String,
        transaction: String,
        candidate: Option<String>,
        receipt: String,
    },
    Evidence {
        transaction: String,
        digest: String,
    },
}

#[derive(Serialize, Deserialize)]
struct Line {
    seq: u64,
    prev: String,
    body: Record,
}

#[derive(Debug, Clone)]
enum Outcome {
    Committed {
        generation: u64,
        root: String,
    },
    Forked {
        generation: u64,
    },
    Abandoned,
    Aborted {
        reason: String,
        stale: Option<(u64, u64)>,
    },
}

impl Outcome {
    fn reply(&self) -> Reply {
        match self {
            Outcome::Committed { generation, root } => Reply::Committed {
                generation: *generation,
                root: root.clone(),
            },
            Outcome::Forked { generation } => Reply::Forked {
                generation: *generation,
            },
            Outcome::Abandoned => Reply::Abandoned,
            Outcome::Aborted {
                stale: Some((expected, current)),
                ..
            } => Reply::Stale {
                expected: *expected,
                current: *current,
            },
            Outcome::Aborted { reason, .. } => Reply::Aborted {
                reason: reason.clone(),
            },
        }
    }
}

#[derive(Debug, Default)]
struct Principal {
    high_water: u64,
    /// Sequence → what it came to, and the digest of what was asked.
    results: BTreeMap<u64, (String, Outcome)>,
    /// The open work, by transaction, and the generation it forked.
    fork: Option<(String, u64)>,
}

#[derive(Debug, Default)]
struct State {
    store: String,
    publishers: Vec<String>,
    generation: u64,
    root: Option<String>,
    principals: BTreeMap<String, Principal>,
    /// A `PREPARE` whose request has not been resolved.
    prepared: Option<(u64, RequestId, String)>,
    evidence: BTreeMap<String, String>,
    /// Every published generation and its root, oldest first.
    history: Vec<(u64, String)>,
}

impl State {
    fn principal(&mut self, name: &str) -> &mut Principal {
        self.principals.entry(name.to_string()).or_default()
    }

    fn resolve(&mut self, request: &RequestId, asked: &str, outcome: Outcome) {
        let principal = self.principal(&request.principal);
        principal.high_water = principal.high_water.max(request.sequence);
        principal
            .results
            .insert(request.sequence, (asked.to_string(), outcome));
    }

    fn apply(&mut self, seq: u64, record: &Record) {
        match record {
            Record::Format {
                store, publishers, ..
            } => {
                self.store = store.clone();
                self.publishers = publishers.clone();
            }
            Record::Seed {
                request,
                asked,
                root,
                ..
            } => {
                self.generation = 1;
                self.root = Some(root.clone());
                self.history.push((1, root.clone()));
                self.resolve(
                    request,
                    asked,
                    Outcome::Committed {
                        generation: 1,
                        root: root.clone(),
                    },
                );
            }
            Record::Fork {
                request,
                asked,
                transaction,
                generation,
            } => {
                self.resolve(
                    request,
                    asked,
                    Outcome::Forked {
                        generation: *generation,
                    },
                );
                self.principal(&request.principal).fork = Some((transaction.clone(), *generation));
            }
            Record::Prepare { request, asked, .. } => {
                let principal = self.principal(&request.principal);
                principal.high_water = principal.high_water.max(request.sequence);
                self.prepared = Some((seq, request.clone(), asked.clone()));
            }
            Record::Commit {
                request,
                generation,
                root,
                ..
            } => {
                let asked = self
                    .prepared
                    .take()
                    .map(|(_, _, asked)| asked)
                    .unwrap_or_default();
                self.generation = *generation;
                self.root = Some(root.clone());
                self.history.push((*generation, root.clone()));
                self.resolve(
                    request,
                    &asked,
                    Outcome::Committed {
                        generation: *generation,
                        root: root.clone(),
                    },
                );
                self.principal(&request.principal).fork = None;
            }
            Record::Abort {
                request,
                asked,
                reason,
                stale,
            } => {
                if self
                    .prepared
                    .as_ref()
                    .is_some_and(|(_, prepared, _)| prepared == request)
                {
                    self.prepared = None;
                }
                self.resolve(
                    request,
                    asked,
                    Outcome::Aborted {
                        reason: reason.clone(),
                        stale: *stale,
                    },
                );
                // A work whose generation moved on can never publish; keeping it
                // open would refuse every later fork for a work that is over.
                if stale.is_some() {
                    self.principal(&request.principal).fork = None;
                }
            }
            Record::Abandon { request, asked, .. } => {
                self.resolve(request, asked, Outcome::Abandoned);
                self.principal(&request.principal).fork = None;
            }
            Record::Evidence {
                transaction,
                digest,
            } => {
                self.evidence.insert(transaction.clone(), digest.clone());
            }
        }
    }
}

/// What opening the store found and did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Recovery {
    pub records: u64,
    /// Bytes of a torn last record that were cut off.
    pub truncated_bytes: u64,
    /// A publication that was prepared and never committed, resolved as aborted.
    pub aborted_prepare: bool,
    /// Set when the log is damaged before its end. Every request is refused.
    pub integrity: Option<String>,
}

pub struct LinuxStore {
    dir: PathBuf,
    _lock: File,
    log: File,
    state: State,
    next_seq: u64,
    last_digest: String,
    faults: Faults,
    stopped: Option<String>,
    recovery: Recovery,
}

const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Read {
    records: Vec<(u64, String, Record)>,
    valid_len: u64,
    total_len: u64,
    integrity: Option<String>,
}

fn read_log(path: &Path) -> std::io::Result<Read> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    let mut read = Read {
        records: Vec::new(),
        valid_len: 0,
        total_len: bytes.len() as u64,
        integrity: None,
    };
    let mut offset = 0usize;
    let mut previous = GENESIS.to_string();
    while offset < bytes.len() {
        let Some(length) = bytes[offset..].iter().position(|byte| *byte == b'\n') else {
            // No newline: the last write did not finish. A torn tail.
            break;
        };
        let end = offset + length;
        let line = &bytes[offset..end];
        let is_last = end + 1 >= bytes.len();
        let parsed = serde_json::from_slice::<Line>(line).ok().filter(|parsed| {
            parsed.seq == read.records.len() as u64 + 1 && parsed.prev == previous
        });
        let Some(parsed) = parsed else {
            if !is_last {
                // Damage before the end is not a crash: something after it was
                // written as if this had been fine. Refuse the whole store rather
                // than skip to a later record.
                read.integrity = Some(format!(
                    "record {} of the log, at byte {offset}, is not the record that should be there",
                    read.records.len() + 1
                ));
            }
            break;
        };
        let digest = hex::encode(Sha256::digest(line));
        read.records.push((parsed.seq, digest.clone(), parsed.body));
        previous = digest;
        offset = end + 1;
        read.valid_len = offset as u64;
    }
    Ok(read)
}

fn object_path(dir: &Path, digest: &str, kind: ObjectKind) -> PathBuf {
    dir.join(OBJECTS)
        .join(&digest[..2])
        .join(format!("{digest}.{}", kind.word()))
}

fn read_object(dir: &Path, digest: &str) -> Result<(ObjectKind, Vec<u8>), (String, String)> {
    if !content::is_digest(digest) {
        return Err(("invalid".to_string(), format!("`{digest}` is not a digest")));
    }
    for kind in ObjectKind::ALL {
        let path = object_path(dir, digest, kind);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let actual = content::object_digest(kind.word(), &bytes);
                if actual != digest {
                    return Err((
                        "integrity".to_string(),
                        format!(
                            "the object filed as `{digest}` holds `{actual}`: something other than \
                             the service wrote the store"
                        ),
                    ));
                }
                return Ok((kind, bytes));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err((
                    "unreadable".to_string(),
                    format!("`{digest}` is there and could not be read: {error}"),
                ));
            }
        }
    }
    Err((
        "absent".to_string(),
        format!("the store has no object `{digest}`"),
    ))
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

/// Read a kept evidence record without opening the service.
///
/// For `evidencia`, which fetches by handle and may run while a transaction
/// holds the service. The log is append-only, so a reader that ignores a torn
/// tail sees a prefix of the truth; every object is re-hashed.
pub fn read_evidence(dir: &Path, transaction: &str) -> Fetched {
    let read = match read_log(&dir.join(LOG)) {
        Ok(read) => read,
        Err(error) => {
            return Fetched::Unreadable(format!("the store's log could not be read: {error}"));
        }
    };
    if let Some(why) = read.integrity {
        return Fetched::Unreadable(why);
    }
    let digest = read
        .records
        .iter()
        .rev()
        .find_map(|(_, _, record)| match record {
            Record::Evidence {
                transaction: kept,
                digest,
            } if kept == transaction => Some(digest.clone()),
            _ => None,
        });
    let Some(digest) = digest else {
        return Fetched::Absent;
    };
    match read_object(dir, &digest) {
        Ok((ObjectKind::Evidence, bytes)) => Fetched::Found(bytes),
        Ok((kind, _)) => Fetched::Unreadable(format!(
            "`{digest}` is a {} object, not evidence",
            kind.word()
        )),
        Err((_, message)) => Fetched::Unreadable(message),
    }
}

impl LinuxStore {
    /// Open a store, creating it if there is none, and recover it.
    ///
    /// Blocks while another service holds it: one writer.
    pub fn open(dir: &Path, publishers: &[&str]) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir.join(OBJECTS))?;
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dir.join(LOCK))?;
        lock.lock()?;

        let log_path = dir.join(LOG);
        let read = read_log(&log_path)?;
        let mut recovery = Recovery {
            records: read.records.len() as u64,
            integrity: read.integrity.clone(),
            ..Recovery::default()
        };

        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&log_path)?;
        if read.integrity.is_none() && read.total_len > read.valid_len {
            // The tail of a write that never finished. Cut here, flushed, before
            // anything is appended after it.
            log.set_len(read.valid_len)?;
            log.sync_all()?;
            recovery.truncated_bytes = read.total_len - read.valid_len;
        }

        let mut state = State::default();
        let mut last_digest = GENESIS.to_string();
        for (seq, digest, record) in &read.records {
            state.apply(*seq, record);
            last_digest = digest.clone();
        }

        let mut store = Self {
            dir: dir.to_path_buf(),
            _lock: lock,
            log,
            state,
            next_seq: read.records.len() as u64 + 1,
            last_digest,
            faults: Faults::default(),
            stopped: None,
            recovery,
        };
        if store.recovery.integrity.is_some() {
            return Ok(store);
        }

        if read.records.is_empty() {
            let name = {
                let mut hasher = Sha256::new();
                hasher.update(dir.as_os_str().as_encoded_bytes());
                hasher.update(format!("{:?}", std::time::SystemTime::now()).as_bytes());
                hex::encode(&hasher.finalize()[..16])
            };
            store
                .append(Record::Format {
                    format: FORMAT.to_string(),
                    store: name,
                    epoch: 1,
                    publishers: publishers.iter().map(|name| name.to_string()).collect(),
                })
                .map_err(std::io::Error::other)?;
            sync_directory(dir)?;
        }

        // Before admitting anything: a publication that was prepared and never
        // committed did not happen, and the log says so durably.
        if let Some((_, request, asked)) = store.state.prepared.clone() {
            store
                .append(Record::Abort {
                    request,
                    asked,
                    reason: "recovered: it was prepared and never committed".to_string(),
                    stale: None,
                })
                .map_err(std::io::Error::other)?;
            store.recovery.aborted_prepare = true;
        }
        Ok(store)
    }

    pub fn with_faults(mut self, faults: Faults) -> Self {
        self.faults = faults;
        self
    }

    pub fn recovery(&self) -> &Recovery {
        &self.recovery
    }

    pub fn generation(&self) -> u64 {
        self.state.generation
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    fn append(&mut self, record: Record) -> Result<(), TransportError> {
        if let Some(why) = &self.stopped {
            return Err(TransportError::Gone(why.clone()));
        }
        let line = Line {
            seq: self.next_seq,
            prev: self.last_digest.clone(),
            body: record.clone(),
        };
        let mut bytes = serde_json::to_vec(&line).expect("a record is always JSON");
        let digest = hex::encode(Sha256::digest(&bytes));
        bytes.push(b'\n');
        let written = self
            .log
            .write_all(&bytes)
            .and_then(|()| self.log.sync_data());
        if let Err(error) = written {
            // A flush that failed is a log whose end nobody knows. Stop, and let
            // the next open decide what is durable, rather than publish on top of
            // a state this process only believes.
            let why = format!("the log could not be written, so the store stopped: {error}");
            self.stopped = Some(why.clone());
            return Err(TransportError::Gone(why));
        }
        self.state.apply(self.next_seq, &record);
        self.last_digest = digest;
        self.next_seq += 1;
        Ok(())
    }

    fn stop(&mut self, why: &str) -> TransportError {
        let why = format!("the store stopped {why}");
        self.stopped = Some(why.clone());
        TransportError::Gone(why)
    }

    fn has(&self, digest: &str, kind: ObjectKind) -> bool {
        content::is_digest(digest) && object_path(&self.dir, digest, kind).exists()
    }

    fn write_object(&self, kind: ObjectKind, digest: &str, bytes: &[u8]) -> std::io::Result<()> {
        let path = object_path(&self.dir, digest, kind);
        if path.exists() {
            return Ok(());
        }
        let parent = path.parent().expect("an object has a directory");
        std::fs::create_dir_all(parent)?;
        let writing = parent.join(format!(".{digest}.{}.writing", kind.word()));
        {
            let mut file = File::create(&writing)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        std::fs::rename(&writing, &path)?;
        sync_directory(parent)
    }

    /// Whether a root names a tree whose every file the store holds.
    fn whole_tree(&self, root: &str) -> Result<(), String> {
        let digest = content::digest_of_id(root)
            .ok_or_else(|| format!("`{root}` is not a version identity"))?;
        let bytes = match read_object(&self.dir, digest) {
            Ok((ObjectKind::Tree, bytes)) => bytes,
            Ok((kind, _)) => return Err(format!("`{root}` names a {} object", kind.word())),
            Err((_, message)) => return Err(message),
        };
        let manifest = Manifest::decode(&bytes)?;
        if let Some(other) = manifest.others().first() {
            return Err(format!("`{other}` is not something a version can hold"));
        }
        for file in manifest.file_digests() {
            if !self.has(&file, ObjectKind::Bytes) {
                return Err(format!(
                    "the tree names `{file}`, which the store does not hold"
                ));
            }
        }
        Ok(())
    }

    /// The reply to a sequenced request that is not new, if it is not.
    fn already(&self, request: &RequestId, asked: &str) -> Option<Reply> {
        let principal = self.state.principals.get(&request.principal);
        let high_water = principal.map(|p| p.high_water).unwrap_or(0);
        if request.sequence <= high_water {
            return Some(
                match principal.and_then(|p| p.results.get(&request.sequence)) {
                    Some((recorded, _)) if recorded != asked => Reply::Refused {
                        word: "conflict".to_string(),
                        message: format!(
                            "sequence {} of `{}` was already used for a different request",
                            request.sequence, request.principal
                        ),
                    },
                    Some((_, outcome)) => outcome.reply(),
                    None => Reply::Unknown,
                },
            );
        }
        if request.sequence != high_water + 1 {
            return Some(Reply::Refused {
                word: "out_of_order".to_string(),
                message: format!(
                    "`{}` sent sequence {} and the next one is {}",
                    request.principal,
                    request.sequence,
                    high_water + 1
                ),
            });
        }
        None
    }

    /// Resolve a request as refused, durably, and say why.
    fn refuse(
        &mut self,
        request: RequestId,
        asked: &str,
        word: &str,
        message: String,
    ) -> Result<Reply, TransportError> {
        self.append(Record::Abort {
            request,
            asked: asked.to_string(),
            reason: message.clone(),
            stale: None,
        })?;
        Ok(Reply::Refused {
            word: word.to_string(),
            message,
        })
    }

    fn handle(&mut self, request: Request, asked: &str) -> Result<Reply, TransportError> {
        if let Some(why) = &self.stopped {
            return Err(TransportError::Gone(why.clone()));
        }
        if let Some(why) = &self.recovery.integrity {
            return Ok(Reply::Refused {
                word: "integrity".to_string(),
                message: why.clone(),
            });
        }
        let refused = |word: &str, message: String| Reply::Refused {
            word: word.to_string(),
            message,
        };

        Ok(match request {
            Request::Hello => Reply::Hello {
                format: FORMAT.to_string(),
                store: self.state.store.clone(),
                epoch: 1,
            },
            Request::Published => Reply::Version {
                generation: self.state.generation,
                root: self.state.root.clone(),
            },
            Request::History => Reply::History {
                roots: self.state.history.clone(),
            },
            Request::Missing { digests } => Reply::Missing {
                digests: digests
                    .into_iter()
                    .filter(|digest| !self.has(digest, ObjectKind::Bytes))
                    .collect(),
            },
            Request::Put { kind, hex } => {
                let Ok(bytes) = hex::decode(&hex) else {
                    return Ok(refused("invalid", "an object is not hex".to_string()));
                };
                if kind == ObjectKind::Tree
                    && let Err(why) = Manifest::decode(&bytes)
                {
                    return Ok(refused("invalid", why));
                }
                let digest = content::object_digest(kind.word(), &bytes);
                match self.write_object(kind, &digest, &bytes) {
                    Ok(()) => Reply::Stored { digest },
                    Err(error) => refused(
                        "unwritable",
                        format!("`{digest}` could not be stored: {error}"),
                    ),
                }
            }
            Request::Get { digest } => match read_object(&self.dir, &digest) {
                Ok((kind, bytes)) => Reply::Object {
                    kind,
                    hex: hex::encode(bytes),
                },
                Err((word, message)) => refused(&word, message),
            },
            Request::Sequence { principal } => Reply::Next {
                sequence: self
                    .state
                    .principals
                    .get(&principal)
                    .map(|p| p.high_water)
                    .unwrap_or(0)
                    + 1,
            },
            Request::Result { request } => {
                let principal = self.state.principals.get(&request.principal);
                match principal.and_then(|p| p.results.get(&request.sequence)) {
                    Some((_, outcome)) => outcome.reply(),
                    None => Reply::Unknown,
                }
            }
            Request::Seed {
                request,
                root,
                receipt,
            } => {
                if let Some(reply) = self.already(&request, asked) {
                    return Ok(reply);
                }
                if !self.state.publishers.contains(&request.principal) {
                    let message = format!("`{}` may not publish in this store", request.principal);
                    return self.refuse(request, asked, "forbidden", message);
                }
                if self.state.generation != 0 {
                    let message = format!(
                        "the store already publishes generation {}, so there is nothing to seed",
                        self.state.generation
                    );
                    return self.refuse(request, asked, "already_seeded", message);
                }
                if let Err(why) = self.whole_tree(&root) {
                    return self.refuse(request, asked, "invalid", why);
                }
                if !self.has(&receipt, ObjectKind::Receipt) {
                    let message = format!("the receipt `{receipt}` is not in the store");
                    return self.refuse(request, asked, "invalid", message);
                }
                self.append(Record::Seed {
                    request,
                    asked: asked.to_string(),
                    root: root.clone(),
                    receipt,
                })?;
                Reply::Committed {
                    generation: 1,
                    root,
                }
            }
            Request::Fork {
                request,
                transaction,
                generation,
            } => {
                if let Some(reply) = self.already(&request, asked) {
                    return Ok(reply);
                }
                if let Some((open, _)) = self
                    .state
                    .principals
                    .get(&request.principal)
                    .and_then(|p| p.fork.clone())
                {
                    let message = format!(
                        "an attempt is already open for `{}`: `{open}`. Settle it before starting another",
                        request.principal
                    );
                    return self.refuse(request, asked, "already_open", message);
                }
                if generation == 0 || generation != self.state.generation {
                    let current = self.state.generation;
                    self.append(Record::Abort {
                        request,
                        asked: asked.to_string(),
                        reason: format!("generation {generation} is not the published one"),
                        stale: Some((generation, current)),
                    })?;
                    return Ok(Reply::Stale {
                        expected: generation,
                        current,
                    });
                }
                self.append(Record::Fork {
                    request,
                    asked: asked.to_string(),
                    transaction,
                    generation,
                })?;
                Reply::Forked { generation }
            }
            Request::Publish {
                request,
                transaction,
                expected_generation,
                candidate,
                receipt,
            } => {
                if let Some(reply) = self.already(&request, asked) {
                    return Ok(reply);
                }
                if !self.state.publishers.contains(&request.principal) {
                    let message = format!("`{}` may not publish in this store", request.principal);
                    return self.refuse(request, asked, "forbidden", message);
                }
                let open = self
                    .state
                    .principals
                    .get(&request.principal)
                    .and_then(|p| p.fork.clone());
                if open.as_ref().map(|(t, _)| t.as_str()) != Some(transaction.as_str()) {
                    let message = format!(
                        "`{transaction}` is not the work open for `{}`, so it has nothing to publish",
                        request.principal
                    );
                    return self.refuse(request, asked, "not_open", message);
                }
                if expected_generation != self.state.generation {
                    let current = self.state.generation;
                    self.append(Record::Abort {
                        request,
                        asked: asked.to_string(),
                        reason: format!(
                            "generation {expected_generation} is no longer the published one"
                        ),
                        stale: Some((expected_generation, current)),
                    })?;
                    return Ok(Reply::Stale {
                        expected: expected_generation,
                        current,
                    });
                }
                if let Err(why) = self.whole_tree(&candidate) {
                    return self.refuse(request, asked, "invalid", why);
                }
                if !self.has(&receipt, ObjectKind::Receipt) {
                    let message = format!("the receipt `{receipt}` is not in the store");
                    return self.refuse(request, asked, "invalid", message);
                }

                let prepared = self.next_seq;
                self.append(Record::Prepare {
                    request: request.clone(),
                    asked: asked.to_string(),
                    transaction,
                    expected_generation,
                    candidate: candidate.clone(),
                    receipt: receipt.clone(),
                })?;
                if self.faults.stop_after_prepare {
                    return Err(self.stop("after the prepare was durable"));
                }
                let generation = expected_generation + 1;
                self.append(Record::Commit {
                    request,
                    prepared,
                    generation,
                    root: candidate.clone(),
                    receipt,
                })?;
                if self.faults.stop_after_commit {
                    return Err(self.stop("after the commit was durable and before answering"));
                }
                Reply::Committed {
                    generation,
                    root: candidate,
                }
            }
            Request::Abandon {
                request,
                transaction,
                candidate,
                receipt,
            } => {
                if let Some(reply) = self.already(&request, asked) {
                    return Ok(reply);
                }
                let open = self
                    .state
                    .principals
                    .get(&request.principal)
                    .and_then(|p| p.fork.clone());
                if open.as_ref().map(|(t, _)| t.as_str()) != Some(transaction.as_str()) {
                    let message = format!(
                        "`{transaction}` is not the work open for `{}`, so there is nothing to abandon",
                        request.principal
                    );
                    return self.refuse(request, asked, "not_open", message);
                }
                if !receipt.is_empty() && !self.has(&receipt, ObjectKind::Receipt) {
                    let message = format!("the receipt `{receipt}` is not in the store");
                    return self.refuse(request, asked, "invalid", message);
                }
                self.append(Record::Abandon {
                    request,
                    asked: asked.to_string(),
                    transaction,
                    candidate,
                    receipt,
                })?;
                Reply::Abandoned
            }
            Request::KeepEvidence {
                transaction,
                digest,
            } => {
                let shaped = !transaction.is_empty()
                    && transaction.len() <= 128
                    && transaction
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte));
                if !shaped {
                    return Ok(refused(
                        "invalid",
                        format!("`{transaction}` is not the shape of a handle"),
                    ));
                }
                if !self.has(&digest, ObjectKind::Evidence) {
                    return Ok(refused(
                        "invalid",
                        format!("the evidence `{digest}` is not in the store"),
                    ));
                }
                self.append(Record::Evidence {
                    transaction,
                    digest,
                })?;
                Reply::Recorded
            }
            Request::FindEvidence { transaction } => match self.state.evidence.get(&transaction) {
                Some(digest) => Reply::Found {
                    digest: digest.clone(),
                },
                None => Reply::Absent,
            },
        })
    }
}

impl Service for LinuxStore {
    fn serve(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        let reply = match protocol::decode::<Request>(request) {
            Ok(decoded) => {
                let asked = hex::encode(Sha256::digest(request));
                self.handle(decoded, &asked)?
            }
            Err(why) => Reply::Refused {
                word: "unintelligible".to_string(),
                message: why,
            },
        };
        Ok(protocol::encode(&reply))
    }
}
