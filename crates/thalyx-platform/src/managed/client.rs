//! The managed model, as a Thalyx transaction uses it.
//!
//! ## The three trees
//!
//! - **The published root** is in the store and nowhere else. Nobody writes it.
//! - **The view** is where the session stands: a checkout of the published root,
//!   brought up to each new generation by this client after a publication. It is
//!   verified against the root before any work is opened and again before any
//!   publication, and a view that holds something the store did not publish is
//!   refused, not overwritten — `managed-local-v1` requires that nothing but the
//!   service writes the store, and a publication that silently clobbered a
//!   person's edit would be that requirement broken from the other side.
//! - **The private workspace** is where the work's requests act. It is forked
//!   from a generation, nothing published can see it, and abandoning puts it
//!   back to the generation it came from.
//!
//! ## What a candidate is
//!
//! The content identity of the private workspace at the moment it was frozen,
//! with every object it names held by the store. A verdict names one and a
//! publication publishes one; the transaction above refuses to publish a
//! candidate on the strength of a verdict about another.
//!
//! ## What this client does not trust
//!
//! The service. Every object it hands back is re-hashed here before it is
//! written anywhere, because a transport to another machine is a transport, and
//! a digest that was only checked on the far side is a claim the near side
//! accepted.

use super::protocol::{self, FORMAT, ObjectKind, Reply, Request};
use crate::authority::RequestId;
use crate::content::{self, Manifest, Node};
use crate::evidence::{EvidenceSink, Fetched};
use crate::state::{AbandonFailure, Changes, Decision, Kept, Opened, Receipt, VersionedState};
use crate::transport::{Carried, Transport, TransportError};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

struct Fork {
    generation: u64,
    root: String,
    manifest: Manifest,
    transaction: String,
}

enum CallError {
    Transport(TransportError),
    Protocol(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Transport(error) => write!(f, "{error}"),
            CallError::Protocol(message) => write!(f, "{message}"),
        }
    }
}

pub struct Managed<T: Transport> {
    transport: T,
    view: PathBuf,
    workspace: PathBuf,
    principal: String,
    fork: Option<Fork>,
    candidate: Option<(String, Manifest)>,
    generation_before: Option<u64>,
    generation_after: Option<u64>,
    published: Option<String>,
    discarded: bool,
    /// The earlier published root a view was found holding and brought up from.
    caught_up_from: Option<String>,
}

impl<T: Transport> Managed<T> {
    pub fn new(
        transport: T,
        view: impl Into<PathBuf>,
        workspace: impl Into<PathBuf>,
        principal: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            view: view.into(),
            workspace: workspace.into(),
            principal: principal.into(),
            fork: None,
            candidate: None,
            generation_before: None,
            generation_after: None,
            published: None,
            discarded: false,
            caught_up_from: None,
        }
    }

    /// Whether this store ever published a root with exactly this identity.
    fn was_published(&mut self, id: &str) -> Result<bool, String> {
        match self.ask(&Request::History)? {
            Reply::History { roots } => Ok(roots.iter().any(|(_, root)| root == id)),
            other => Err(refusal(&other)),
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn view(&self) -> &Path {
        &self.view
    }

    /// The generation the store publishes now, and its root.
    pub fn published(&mut self) -> Result<(u64, Option<String>), String> {
        match self.ask(&Request::Published)? {
            Reply::Version { generation, root } => Ok((generation, root)),
            other => Err(refusal(&other)),
        }
    }

    /// The tree a root names, re-hashed here.
    pub fn tree(&mut self, root: &str) -> Result<Manifest, String> {
        let digest = content::digest_of_id(root)
            .ok_or_else(|| format!("`{root}` is not a version this client can name"))?
            .to_string();
        let bytes = self.get(&digest, ObjectKind::Tree)?;
        Manifest::decode(&bytes)
    }

    fn call(&mut self, request: &Request) -> Result<Reply, CallError> {
        let bytes = self
            .transport
            .call(&protocol::encode(request))
            .map_err(CallError::Transport)?;
        protocol::decode(&bytes).map_err(CallError::Protocol)
    }

    fn ask(&mut self, request: &Request) -> Result<Reply, String> {
        self.call(request).map_err(|error| error.to_string())
    }

    fn next_request(&mut self) -> Result<RequestId, String> {
        let principal = self.principal.clone();
        match self.ask(&Request::Sequence {
            principal: principal.clone(),
        })? {
            Reply::Next { sequence } => Ok(RequestId {
                principal,
                sequence,
            }),
            other => Err(refusal(&other)),
        }
    }

    fn put(&mut self, kind: ObjectKind, bytes: &[u8]) -> Result<String, String> {
        let expected = content::object_digest(kind.word(), bytes);
        match self.ask(&Request::Put {
            kind,
            hex: hex::encode(bytes),
        })? {
            Reply::Stored { digest } if digest == expected => Ok(digest),
            Reply::Stored { digest } => Err(format!(
                "the store filed {} bytes as `{digest}` and they are `{expected}`",
                bytes.len()
            )),
            other => Err(refusal(&other)),
        }
    }

    fn get(&mut self, digest: &str, kind: ObjectKind) -> Result<Vec<u8>, String> {
        match self.ask(&Request::Get {
            digest: digest.to_string(),
        })? {
            Reply::Object { kind: found, hex } if found == kind => {
                let bytes = hex::decode(hex)
                    .map_err(|_| format!("the store sent `{digest}` in a form that is not hex"))?;
                let actual = content::object_digest(kind.word(), &bytes);
                if actual != digest {
                    return Err(format!(
                        "the store sent `{actual}` when asked for `{digest}`; nothing was written"
                    ));
                }
                Ok(bytes)
            }
            Reply::Object { kind: found, .. } => Err(format!(
                "`{digest}` is a {} object and a {} was needed",
                found.word(),
                kind.word()
            )),
            other => Err(refusal(&other)),
        }
    }

    /// Put every object a tree needs into the store, and name the tree.
    fn upload(&mut self, root: &Path, manifest: &Manifest) -> Result<String, String> {
        let id = manifest.id().ok_or_else(|| {
            format!(
                "{} path(s) could not be read, so this tree has no identity",
                manifest.unreadable.len()
            )
        })?;
        if let Some(other) = manifest.others().first() {
            return Err(format!(
                "`{other}` is not a file, a link or a directory, and a managed version cannot hold one"
            ));
        }
        let missing = match self.ask(&Request::Missing {
            digests: manifest.file_digests(),
        })? {
            Reply::Missing { digests } => digests,
            other => return Err(refusal(&other)),
        };
        for digest in missing {
            let Some(path) = manifest.nodes.iter().find_map(|(path, node)| match node {
                Node::File { digest: d, .. } if *d == digest => Some(path.clone()),
                _ => None,
            }) else {
                return Err(format!(
                    "the store asked for `{digest}`, which this tree does not name"
                ));
            };
            let bytes = std::fs::read(root.join(&path))
                .map_err(|error| format!("`{path}` could not be read to be frozen: {error}"))?;
            // Frozen means these bytes and no others. A file rewritten between the
            // walk and this read would otherwise be stored under the digest of
            // what it used to hold.
            if content::object_digest("bytes", &bytes) != digest {
                return Err(format!(
                    "`{path}` changed while it was being frozen, so there is no candidate"
                ));
            }
            self.put(ObjectKind::Bytes, &bytes)?;
        }
        let tree = manifest.encode();
        let digest = self.put(ObjectKind::Tree, &tree)?;
        debug_assert_eq!(content::digest_of_id(&id), Some(digest.as_str()));
        Ok(id)
    }

    /// Make a directory hold exactly a tree, touching only what differs.
    ///
    /// Only what differs, because an unchanged file keeping its modification
    /// time is what makes the next `cargo check` over it incremental — a
    /// materialisation that rewrote everything would turn every validation into
    /// a cold build, and the cost would be charged to the managed model when it
    /// belongs to this function.
    fn materialize(&mut self, target: &Path, wanted: &Manifest) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let io = |what: &str, path: &Path, error: std::io::Error| {
            format!("could not {what} {}: {error}", path.display())
        };

        std::fs::create_dir_all(target).map_err(|e| io("create", target, e))?;
        let have = Manifest::of(target);
        if !have.is_complete() {
            return Err(format!(
                "{} path(s) of {} could not be read, so it cannot be made to hold a version",
                have.unreadable.len(),
                target.display()
            ));
        }

        // Deepest first, so a directory is empty by the time it is its turn.
        for (path, node) in have.nodes.iter().rev() {
            let stays = match wanted.nodes.get(path) {
                None => false,
                Some(Node::Directory) => matches!(node, Node::Directory),
                Some(_) => !matches!(node, Node::Directory),
            };
            if stays {
                continue;
            }
            let full = target.join(path);
            if matches!(node, Node::Directory) {
                std::fs::remove_dir_all(&full).map_err(|e| io("remove", &full, e))?;
            } else {
                std::fs::remove_file(&full).map_err(|e| io("remove", &full, e))?;
            }
        }

        // Parents first.
        for (path, node) in &wanted.nodes {
            let full = target.join(path);
            if have.nodes.get(path) == Some(node) {
                continue;
            }
            match node {
                Node::Directory => {
                    if !full.is_dir() {
                        std::fs::create_dir(&full).map_err(|e| io("create", &full, e))?;
                    }
                }
                Node::File {
                    digest, executable, ..
                } => {
                    let bytes = self.get(digest, ObjectKind::Bytes)?;
                    let parent = full.parent().unwrap_or(target);
                    let name = full
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    // Written whole beside the file and renamed over it, so a
                    // reader never sees half of one version and half of another.
                    let writing = parent.join(format!(".{name}.thalyx-writing"));
                    std::fs::write(&writing, &bytes).map_err(|e| io("write", &writing, e))?;
                    let mode = if *executable { 0o755 } else { 0o644 };
                    std::fs::set_permissions(&writing, std::fs::Permissions::from_mode(mode))
                        .map_err(|e| io("set the mode of", &writing, e))?;
                    std::fs::rename(&writing, &full).map_err(|e| io("replace", &full, e))?;
                }
                Node::Symlink { target: points_at } => {
                    if std::fs::symlink_metadata(&full).is_ok() {
                        std::fs::remove_file(&full).map_err(|e| io("replace", &full, e))?;
                    }
                    std::os::unix::fs::symlink(points_at, &full)
                        .map_err(|e| io("link", &full, e))?;
                }
                Node::Other => {
                    return Err(format!(
                        "`{path}` is not a file, a link or a directory, and cannot be made"
                    ));
                }
            }
        }
        Ok(())
    }

    fn put_receipt(
        &mut self,
        receipt: &Receipt,
        base_generation: Option<u64>,
        candidate: Option<&str>,
    ) -> Result<String, String> {
        let body = json!({
            "format": "thalyx-receipt-v1",
            "principal": self.principal,
            "base_generation": base_generation,
            "candidate": candidate,
            "receipt": receipt,
        });
        let bytes = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
        self.put(ObjectKind::Receipt, &bytes)
    }
}

fn refusal(reply: &Reply) -> String {
    match reply {
        Reply::Refused { message, .. } => message.clone(),
        Reply::Aborted { reason } => {
            format!("the store resolved the request without effect: {reason}")
        }
        other => format!(
            "the store answered `{}` where that is not an answer",
            String::from_utf8_lossy(&protocol::encode(other))
        ),
    }
}

impl<T: Transport> VersionedState for Managed<T> {
    fn workspace(&self) -> &Path {
        &self.workspace
    }

    fn open(&mut self, label: &str, request_id: &str) -> Result<Opened, String> {
        match self.ask(&Request::Hello)? {
            Reply::Hello { format, .. } if format == FORMAT => {}
            Reply::Hello { format, .. } => {
                return Err(format!(
                    "the store speaks `{format}` and this client speaks `{FORMAT}`"
                ));
            }
            other => return Err(refusal(&other)),
        }

        let (generation, root) = self.published()?;
        let view_path = self.view.clone();
        let view = Manifest::of(&view_path);
        if !view.is_complete() {
            return Err(format!(
                "{} path(s) of {} could not be read, so it cannot be named as a version and \
                 nothing was started",
                view.unreadable.len(),
                view_path.display()
            ));
        }

        let (generation, root, manifest) = if generation == 0 {
            // The tree as it was found, published once so that there is a version
            // to work against — and its receipt says it was never validated.
            let root = self.upload(&view_path, &view)?;
            let seed = Receipt {
                transaction: format!("{request_id}-seed"),
                label: format!("seed for {label}"),
                decision: Decision::Seed,
                succeeded: false,
                checks: Vec::new(),
                program: None,
            };
            let receipt = self.put_receipt(&seed, None, Some(&root))?;
            let request = self.next_request()?;
            match self.ask(&Request::Seed {
                request,
                root: root.clone(),
                receipt,
            })? {
                Reply::Committed { generation, root } => (generation, root, view),
                other => return Err(refusal(&other)),
            }
        } else {
            let root = root.ok_or_else(|| {
                format!("the store says generation {generation} and names no root for it")
            })?;
            let view_id = view.id().unwrap_or_default();
            if view_id != root && self.was_published(&view_id)? {
                // Behind, not written: the view holds exactly a version this
                // store published earlier, which is what a publication whose
                // reply was lost after its commit leaves behind. Bringing it up
                // loses nothing, because nothing in it is anybody's but the
                // store's.
                let manifest = self.tree(&root)?;
                self.materialize(&view_path, &manifest)
                    .map_err(|error| format!("{} is behind the store and could not be brought up to generation {generation}: {error}", view_path.display()))?;
                self.caught_up_from = Some(view_id.clone());
            } else if view_id != root {
                return Err(format!(
                    "{} has been written outside the store: it holds `{}` and generation \
                     {generation} is `{root}`. A managed tree changes by publishing, so nothing \
                     was started",
                    view_path.display(),
                    view.id().unwrap_or_default()
                ));
            }
            let manifest = self.tree(&root)?;
            (generation, root, manifest)
        };

        let request = self.next_request()?;
        match self.ask(&Request::Fork {
            request,
            transaction: request_id.to_string(),
            generation,
        })? {
            Reply::Forked { .. } => {}
            other => return Err(refusal(&other)),
        }

        let workspace = self.workspace.clone();
        if let Err(error) = self.materialize(&workspace, &manifest) {
            // The store holds an open work nobody can reach. Closing it is the
            // only thing that keeps the next request from being refused for
            // a work that never started.
            if let Ok(request) = self.next_request() {
                let _ = self.ask(&Request::Abandon {
                    request,
                    transaction: request_id.to_string(),
                    candidate: None,
                    receipt: String::new(),
                });
            }
            return Err(format!("the private workspace could not be made: {error}"));
        }

        self.generation_before = Some(generation);
        self.published = Some(root.clone());
        self.candidate = None;
        self.fork = Some(Fork {
            generation,
            root: root.clone(),
            manifest,
            transaction: request_id.to_string(),
        });
        Ok(Opened {
            base: format!("generation-{generation}"),
            start_state: Some(root),
        })
    }

    fn changed(&mut self) -> Changes {
        match &self.fork {
            Some(fork) => Manifest::of(&self.workspace).changes_from(&fork.manifest),
            None => Changes::default(),
        }
    }

    fn candidate(&mut self) -> Result<Option<String>, String> {
        if self.fork.is_none() {
            return Err("no work is open, so there is nothing to freeze".to_string());
        }
        let workspace = self.workspace.clone();
        let manifest = Manifest::of(&workspace);
        let Some(id) = manifest.id() else {
            return Err(format!(
                "{} path(s) of the workspace could not be read, so there is no candidate",
                manifest.unreadable.len()
            ));
        };
        if self
            .candidate
            .as_ref()
            .is_some_and(|(known, _)| *known == id)
        {
            return Ok(Some(id));
        }
        let root = self.upload(&workspace, &manifest)?;
        self.candidate = Some((root.clone(), manifest));
        Ok(Some(root))
    }

    fn keep(&mut self, receipt: &Receipt) -> Result<Kept, String> {
        let Some((generation, base, transaction)) = self
            .fork
            .as_ref()
            .map(|fork| (fork.generation, fork.root.clone(), fork.transaction.clone()))
        else {
            return Err("no work is open, so there is nothing to publish".to_string());
        };
        let candidate = self
            .candidate()?
            .ok_or_else(|| "there is no candidate to publish".to_string())?;
        let manifest = self
            .candidate
            .as_ref()
            .map(|(_, manifest)| manifest.clone())
            .unwrap_or_default();

        let view_path = self.view.clone();
        let view = Manifest::of(&view_path);
        if view.id().as_deref() != Some(base.as_str()) {
            return Err(format!(
                "{} was written outside the store while the work was open, and publishing would \
                 overwrite what was written. Nothing was published",
                view_path.display()
            ));
        }

        let receipt = self.put_receipt(receipt, Some(generation), Some(&candidate))?;
        let request = self.next_request()?;
        let publish = Request::Publish {
            request: request.clone(),
            transaction,
            expected_generation: generation,
            candidate: candidate.clone(),
            receipt,
        };
        let reply = match self.call(&publish) {
            Ok(reply) => reply,
            // A reply that never came is not an abort. Ask what became of it.
            Err(CallError::Transport(lost)) => match self.call(&Request::Result { request }) {
                Ok(reply) => reply,
                Err(second) => {
                    return Err(format!(
                        "whether generation {} was published is not known: {lost}, and asking \
                         about it failed too: {second}",
                        generation + 1
                    ));
                }
            },
            Err(CallError::Protocol(message)) => return Err(message),
        };

        match reply {
            Reply::Committed {
                generation: published,
                root,
            } => {
                self.fork = None;
                self.generation_after = Some(published);
                self.published = Some(root.clone());
                self.materialize(&view_path, &manifest).map_err(|error| {
                    format!(
                        "generation {published} was published and {} could not be brought up to \
                         it: {error}",
                        view_path.display()
                    )
                })?;
                Ok(Kept {
                    generation: Some(published),
                    root: Some(root),
                })
            }
            Reply::Stale { expected, current } => {
                self.fork = None;
                Err(format!(
                    "generation {expected} is no longer the published one — it is {current} now — \
                     so nothing was published and the work was closed"
                ))
            }
            other => Err(refusal(&other)),
        }
    }

    fn abandon(&mut self, receipt: &Receipt) -> Result<(), AbandonFailure> {
        let Some((generation, transaction, base)) = self.fork.as_ref().map(|fork| {
            (
                fork.generation,
                fork.transaction.clone(),
                fork.manifest.clone(),
            )
        }) else {
            return Err(AbandonFailure::Unplanned(
                "no work is open, so there is nothing to put back".to_string(),
            ));
        };

        // Authorised by the state this client last froze, checked as the last
        // thing before the private work is destroyed — the rule
        // `thalyx_core::attempt::abandon` holds a rollback to, applied to a
        // workspace that is private but still a directory somebody could write.
        let now = Manifest::of(&self.workspace);
        let Some(now_id) = now.id() else {
            return Err(AbandonFailure::NotPutBack(format!(
                "{} path(s) under the workspace could not be read, so what a rollback would \
                 destroy cannot be established exactly. Nothing was changed",
                now.unreadable.len()
            )));
        };
        if let Some((observed, _)) = &self.candidate
            && *observed != now_id
        {
            return Err(AbandonFailure::NotPutBack(format!(
                "the workspace has been written to since this rollback was authorised: it was \
                 `{observed}` and it is `{now_id}` now. Nothing was changed"
            )));
        }

        let recorded = self
            .put_receipt(receipt, Some(generation), Some(&now_id))
            .and_then(|digest| Ok((digest, self.next_request()?)))
            .and_then(|(digest, request)| {
                self.ask(&Request::Abandon {
                    request,
                    transaction,
                    candidate: Some(now_id.clone()),
                    receipt: digest,
                })
            });
        match recorded {
            Ok(Reply::Abandoned) => {}
            Ok(other) => return Err(AbandonFailure::Unplanned(refusal(&other))),
            Err(error) => return Err(AbandonFailure::Unplanned(error)),
        }

        self.fork = None;
        self.generation_after = Some(generation);
        self.discarded = true;
        let workspace = self.workspace.clone();
        self.materialize(&workspace, &base).map_err(|error| {
            AbandonFailure::NotPutBack(format!(
                "the store closed the work, and its private workspace could not be emptied: \
                 {error}"
            ))
        })
    }

    fn end_state(&mut self) -> Option<String> {
        self.published().ok().and_then(|(_, root)| root)
    }

    fn describe(&self) -> Value {
        json!({
            "view": self.view.display().to_string(),
            "workspace": self.workspace.display().to_string(),
            "principal": self.principal,
            "generation_before": self.generation_before,
            "generation_after": self.generation_after,
            "base_root": self.fork.as_ref().map(|fork| fork.root.clone()).or(self.published.clone()),
            "candidate": self.candidate.as_ref().map(|(id, _)| id.clone()),
            "discarded": self.discarded,
            "view_caught_up_from": self.caught_up_from,
            "transport": self.transport.carried(),
        })
    }
}

impl<T: Transport> EvidenceSink for Managed<T> {
    fn record(&mut self, id: &str, body: &[u8]) -> std::io::Result<()> {
        let digest = self
            .put(ObjectKind::Evidence, body)
            .map_err(std::io::Error::other)?;
        match self.ask(&Request::KeepEvidence {
            transaction: id.to_string(),
            digest,
        }) {
            Ok(Reply::Recorded) => Ok(()),
            Ok(other) => Err(std::io::Error::other(refusal(&other))),
            Err(error) => Err(std::io::Error::other(error)),
        }
    }

    fn fetch(&mut self, id: &str) -> Fetched {
        match self.ask(&Request::FindEvidence {
            transaction: id.to_string(),
        }) {
            Ok(Reply::Found { digest }) => match self.get(&digest, ObjectKind::Evidence) {
                Ok(bytes) => Fetched::Found(bytes),
                Err(error) => Fetched::Unreadable(error),
            },
            Ok(Reply::Absent) => Fetched::Absent,
            Ok(other) => Fetched::Unreadable(refusal(&other)),
            Err(error) => Fetched::Unreadable(error),
        }
    }
}

/// What the transport carried, for a caller holding the client by value.
pub fn carried<T: Transport>(managed: &Managed<T>) -> Carried {
    managed.transport.carried()
}
