//! Which machine `hacer` is carried out on.
//!
//! `vault/09-Notas-Tecnicas/Frontera-de-Plataforma.md`. `exec.rs` decides what
//! a transaction means; this file builds the two machines on Linux it can mean
//! it on, from parts that already existed.
//!
//! ## `linux-current`
//!
//! Exactly what `hacer` was before there was a boundary to go through: the
//! boundary is `thalyx_core::attempt` over a Btrfs snapshot of the subvolume the
//! session stands in, a launch is `run_foreign`'s confinement, evidence is a file
//! renamed into the store, and nothing names a work. It is the default, and it
//! exists so that cutting the boundary cannot quietly erase the baseline every
//! other backend is compared against — `dev/exp13/` holds what the unmodified
//! revision answered, and this is held to it.
//!
//! ## `linux-managed`
//!
//! The same transaction over the managed model: `thalyx_platform::managed` as
//! the client, `thalyx_managed::LinuxStore` as the one writer, work forked into a
//! private workspace, candidates frozen before they are validated, publication by
//! compare-and-swap on a generation with the receipt in the same durable record,
//! evidence as an object named by the log, and a work scope with a fence. The
//! launcher is still `run_foreign`, because what confines a process on Linux is
//! Linux; what is managed is what it is handed and what its verdict is about.
//!
//! Chosen with `THALYX_PLATFORM`, read once by the verb that needs it — rule 11:
//! a variable read where it is used is a global switch some other check's
//! precondition depends on.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thalyx_core::Store;
use thalyx_core::attempt::{self, Authorised, Open};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_managed::LinuxStore;
use thalyx_platform::Platform;
use thalyx_platform::clock::{MonotonicClock, SystemClock};
use thalyx_platform::evidence::{EvidenceSink, Fetched};
use thalyx_platform::launch::{LaunchRequest, Launched, ProgramLaunch};
use thalyx_platform::managed::client::Managed;
use thalyx_platform::profile::{ManagedLocal, Profile};
use thalyx_platform::state::{AbandonFailure, Changes, Kept, Opened, Receipt, VersionedState};
use thalyx_platform::transport::Loopback;
#[cfg(test)]
use thalyx_platform::work::Fence;
use thalyx_platform::work::{Ambient, Scoped, WorkControl};
use thalyx_snapshot::{Snapshots, Volumes};

/// The variable that chooses the backend.
pub const VARIABLE: &str = "THALYX_PLATFORM";

/// Who publishes in a managed store opened by a session.
///
/// One principal, because one session holds one transaction at a time — the
/// same rule `intento` holds. A second consumer of the same store would be a
/// second principal with its own sequence, and the store refuses anybody it does
/// not name.
pub const PRINCIPAL: &str = "session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    LinuxCurrent,
    LinuxManaged,
}

impl Backend {
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "linux-current" => Some(Backend::LinuxCurrent),
            "linux-managed" => Some(Backend::LinuxManaged),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Backend::LinuxCurrent => "linux-current",
            Backend::LinuxManaged => "linux-managed",
        }
    }

    /// The backend this process was asked for.
    ///
    /// A value that names nothing is refused rather than defaulted: a person who
    /// typed `linux-manged` and got the Btrfs path would read every result as a
    /// result about the managed model.
    pub fn chosen() -> Result<Self, String> {
        match std::env::var(VARIABLE) {
            Err(_) => Ok(Backend::LinuxCurrent),
            Ok(value) if value.is_empty() => Ok(Backend::LinuxCurrent),
            Ok(value) => Self::named(&value).ok_or_else(|| {
                format!(
                    "`{VARIABLE}={value}` names no platform this machine has; it has \
                     `linux-current` and `linux-managed`"
                )
            }),
        }
    }
}

pub fn linux_current_profile() -> Profile {
    Profile {
        backend: Backend::LinuxCurrent.word().to_string(),
        managed_local_v1: ManagedLocal {
            immutable_roots: false,
            durable_cas_publication: false,
            defined_grants_and_closure: false,
            explicit_resources: "cgroup_per_confined_launch".to_string(),
            exclusive_store_writer: "no: the workspace is the live tree".to_string(),
            holds: false,
        },
        mutable_files: true,
        type_check: true,
        validation_binding: "nothing_named".to_string(),
        publication: "btrfs_snapshot_released".to_string(),
        rollback: "btrfs_snapshot_exchanged_under_state_witness".to_string(),
        evidence: "store_file_renamed_into_place".to_string(),
        launch: "linux_confined_process".to_string(),
        work: "ambient_process".to_string(),
        transport: "in_process_calls".to_string(),
        power_cut_durability: "filesystem_rename_and_fsync".to_string(),
    }
}

pub fn linux_managed_profile() -> Profile {
    Profile {
        backend: Backend::LinuxManaged.word().to_string(),
        managed_local_v1: ManagedLocal {
            immutable_roots: true,
            durable_cas_publication: true,
            defined_grants_and_closure: true,
            explicit_resources: "cgroup_per_confined_launch".to_string(),
            // Said as the weaker thing it is. Nothing on Linux stops another
            // process of the same user from writing `objects/`; the store
            // re-hashes what it reads and refuses what does not match. That is
            // detection, and a profile that called it exclusivity would be the
            // fallback reported as the stronger guarantee.
            exclusive_store_writer: "detected_by_digest_not_prevented".to_string(),
            holds: false,
        },
        // The private workspace is still a directory on Linux.
        mutable_files: true,
        type_check: true,
        validation_binding: "candidate_content_identity".to_string(),
        publication: "cas_on_generation_prepare_commit_log".to_string(),
        rollback: "private_workspace_discarded".to_string(),
        evidence: "content_addressed_object_named_in_log".to_string(),
        launch: "linux_confined_process".to_string(),
        work: "scope_with_fence".to_string(),
        transport: "loopback_encoded_messages".to_string(),
        power_cut_durability: "log_and_objects_fsync".to_string(),
    }
}

// ── the parts both Linux backends share ────────────────────────────────────

/// `run_foreign`: the confinement a program nobody signed gets.
pub struct Confined<'a> {
    store: &'a Store,
}

impl ProgramLaunch for Confined<'_> {
    fn launch(&mut self, request: &LaunchRequest) -> Result<Launched, String> {
        use thalyx_manifest::{Permission, PermissionKind};

        // Read before write, grant by grant, which is the order `exec.rs`
        // assembled them in before there was a type for a grant.
        let mut grants = Vec::new();
        for grant in &request.grants {
            for (wanted, action) in [(grant.read, "read"), (grant.write, "write")] {
                if wanted {
                    grants.push(Permission {
                        resource: grant.object.display().to_string(),
                        action: action.to_string(),
                        kind: PermissionKind::Session,
                    });
                }
            }
        }

        let outcome = thalyx_core::foreign::run_foreign(
            self.store,
            &thalyx_permd::KernelStore::default_map(),
            thalyx_core::foreign::ForeignRequest {
                program: &request.program,
                args: request
                    .arguments
                    .iter()
                    .map(std::ffi::OsString::from)
                    .collect(),
                grants,
                helper: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("thalyx")),
                request_id: request.request_id.clone(),
                profile: &request.profile,
                environment: request.environment.clone(),
            },
        )
        .map_err(|error| error.to_string())?;

        let mut accounting = serde_json::Map::new();
        accounting.insert("cgroup".to_string(), json!(outcome.cgroup_id));
        accounting.insert("isolated".to_string(), json!(outcome.isolated));
        Ok(Launched {
            exit_code: outcome.exit_code,
            stdout: outcome.wrote.stdout,
            stderr: outcome.wrote.stderr,
            truncated: outcome.wrote.truncated,
            accounting,
        })
    }
}

// ── linux-current ────────────────────────────────────────────────────────────

/// `intento`'s boundary, as the transaction has always used it.
pub struct Current<'a, V: Volumes> {
    store: &'a Store,
    snapshots: Snapshots<V>,
    request_id: String,
    opened: Option<Open>,
}

fn from_difference(difference: thalyx_snapshot::Difference) -> Changes {
    Changes {
        added: difference.added,
        modified: difference.modified,
        removed: difference.removed,
        added_total: difference.added_total,
        modified_total: difference.modified_total,
        removed_total: difference.removed_total,
        unreadable: difference.unreadable,
    }
}

impl<V: Volumes> VersionedState for Current<'_, V> {
    fn workspace(&self) -> &Path {
        self.snapshots.subvolume()
    }

    fn open(&mut self, label: &str, request_id: &str) -> Result<Opened, String> {
        self.request_id = request_id.to_string();
        let opened = attempt::begin(self.store, &self.snapshots, label, request_id)
            .map_err(|error| error.to_string())?;
        let start = thalyx_snapshot::witness(self.snapshots.subvolume());
        let base = opened.snapshot.clone();
        self.opened = Some(opened);
        Ok(Opened {
            base,
            start_state: start.is_complete().then(|| start.id.clone()),
        })
    }

    fn changed(&mut self) -> Changes {
        let Some(open) = &self.opened else {
            return Changes::default();
        };
        match self.snapshots.find(&open.snapshot) {
            Ok(found) => from_difference(thalyx_snapshot::difference(
                self.snapshots.subvolume(),
                &found.path,
            )),
            // The snapshot the boundary named is gone. Nothing is invented for
            // it: the settling reports it, which is the only place that can.
            Err(_) => Changes::default(),
        }
    }

    fn candidate(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn keep(&mut self, _receipt: &Receipt) -> Result<Kept, String> {
        attempt::keep(self.store, &self.snapshots, &self.request_id)
            .map(|_| Kept::default())
            .map_err(|error| error.to_string())
    }

    fn abandon(&mut self, _receipt: &Receipt) -> Result<(), AbandonFailure> {
        let plan = match attempt::what_abandoning_costs(self.store, &self.snapshots) {
            Ok((_, plan)) => plan,
            Err(error) => return Err(AbandonFailure::Unplanned(error.to_string())),
        };
        let Some(opened) = &self.opened else {
            return Err(AbandonFailure::Unplanned(
                "no boundary was opened by this run".to_string(),
            ));
        };
        // Authorised by the state this run itself observed, not by a bare yes.
        attempt::abandon(
            self.store,
            &self.snapshots,
            opened,
            &plan,
            Authorised::ByState(&plan.state.id),
            &self.request_id,
        )
        .map(|_| ())
        .map_err(|error| AbandonFailure::NotPutBack(error.to_string()))
    }

    fn end_state(&mut self) -> Option<String> {
        let end = thalyx_snapshot::witness(self.snapshots.subvolume());
        end.is_complete().then_some(end.id)
    }

    fn describe(&self) -> Value {
        json!({
            "subvolume": self.snapshots.subvolume().display().to_string(),
            "snapshot": self.opened.as_ref().map(|open| open.snapshot.clone()),
        })
    }
}

/// Evidence as a file in the store, renamed into place.
pub struct StoreFiles<'a> {
    store: &'a Store,
}

impl<'a> StoreFiles<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }
}

impl EvidenceSink for StoreFiles<'_> {
    fn record(&mut self, id: &str, body: &[u8]) -> std::io::Result<()> {
        let directory = crate::exec::evidence_directory(self.store);
        std::fs::create_dir_all(&directory)?;
        let path = crate::exec::evidence_path(self.store, id);
        // Written whole and renamed over, the way every other state file in this
        // system is published. A half-written evidence file is the one artefact
        // nobody can reconstruct: the tree it describes has already been rolled
        // back by the time this is written.
        let temporary = directory.join(format!(".{id}.writing"));
        std::fs::write(&temporary, body)?;
        std::fs::rename(&temporary, &path)
    }

    fn fetch(&mut self, id: &str) -> Fetched {
        match std::fs::read_to_string(crate::exec::evidence_path(self.store, id)) {
            Ok(raw) => Fetched::Found(raw.into_bytes()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Fetched::Absent,
            Err(error) => Fetched::Unreadable(error.to_string()),
        }
    }
}

pub struct LinuxCurrent<'a, V: Volumes> {
    profile: Profile,
    clock: SystemClock,
    state: Current<'a, V>,
    launcher: Confined<'a>,
    work: Ambient,
    evidence: StoreFiles<'a>,
}

impl<'a, V: Volumes> LinuxCurrent<'a, V> {
    pub fn new(store: &'a Store, volumes: V, subvolume: &Path) -> Self {
        Self {
            profile: linux_current_profile(),
            clock: SystemClock::new(),
            state: Current {
                store,
                snapshots: Snapshots::of(volumes, subvolume),
                request_id: String::new(),
                opened: None,
            },
            launcher: Confined { store },
            work: Ambient::default(),
            evidence: StoreFiles { store },
        }
    }
}

impl<V: Volumes> Platform for LinuxCurrent<'_, V> {
    fn profile(&self) -> &Profile {
        &self.profile
    }
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
    fn state(&mut self) -> &mut dyn VersionedState {
        &mut self.state
    }
    fn launcher(&mut self) -> &mut dyn ProgramLaunch {
        &mut self.launcher
    }
    fn work(&mut self) -> &mut dyn WorkControl {
        &mut self.work
    }
    fn evidence(&mut self) -> &mut dyn EvidenceSink {
        &mut self.evidence
    }
}

// ── linux-managed ────────────────────────────────────────────────────────────

/// Where a view's managed store and private workspace live.
///
/// In the Thalyx store and **never beside or inside the view**, for the reason
/// evidence lives there: the view is what a person works in, and a store kept
/// inside it would be part of every version of it.
pub fn project_directory(store: &Store, view: &Path) -> PathBuf {
    let digest = Sha256::digest(view.as_os_str().as_encoded_bytes());
    store
        .state_root()
        .join("managed")
        .join(hex::encode(&digest[..12]))
}

/// The tree a managed boundary opened here would be about: exactly where the
/// session stands, and never an ancestor — the rule `intento` learned on
/// 2026-08-10, which does not stop being true because the mechanism changed.
pub fn managed_tree_for(here: &Path) -> Result<PathBuf, (&'static str, String)> {
    let real = here.canonicalize().map_err(|error| {
        (
            "absent",
            format!("{} is not there: {error}", here.display()),
        )
    })?;
    if real == Path::new("/") {
        return Err((
            "the_whole_system",
            "a managed tree of / would make every file on this machine one version of one \
             tree. Nothing was started"
                .to_string(),
        ));
    }
    if !real.is_dir() {
        return Err((
            "not_a_directory",
            format!("{} is not a directory", real.display()),
        ));
    }
    Ok(real)
}

/// The managed client, with Thalyx's journal beside it.
pub struct ManagedState<'a> {
    store: &'a Store,
    inner: Managed<Loopback<LinuxStore>>,
    request_id: String,
    journal_error: Option<String>,
}

impl ManagedState<'_> {
    fn journal(&mut self, operation: &str, notes: Vec<String>) {
        let entry = Entry {
            timestamp: thalyx_journal::now(),
            operation: operation.to_string(),
            module_id: None,
            version: None,
            outcome: Outcome::Success,
            request_id: self.request_id.clone(),
            origin: Origin::UserUtterance,
            snapshot: None,
            notes,
        };
        // Recorded and not raised: by the time this runs the store's own log
        // already holds the effect, and failing the transaction over a copy of
        // the record would report as undone something that is durable.
        if let Err(error) = Journal::open(self.store.journal_path()).and_then(|j| j.append(&entry))
        {
            self.journal_error = Some(error.to_string());
        }
    }
}

impl VersionedState for ManagedState<'_> {
    fn workspace(&self) -> &Path {
        self.inner.workspace()
    }

    fn open(&mut self, label: &str, request_id: &str) -> Result<Opened, String> {
        self.request_id = request_id.to_string();
        let opened = self.inner.open(label, request_id)?;
        let view = self.inner.view().display().to_string();
        self.journal(
            "managed_fork",
            vec![format!("on {view}"), format!("from {}", opened.base)],
        );
        Ok(opened)
    }

    fn changed(&mut self) -> Changes {
        self.inner.changed()
    }

    fn candidate(&mut self) -> Result<Option<String>, String> {
        self.inner.candidate()
    }

    fn keep(&mut self, receipt: &Receipt) -> Result<Kept, String> {
        let kept = self.inner.keep(receipt)?;
        self.journal(
            "managed_publish",
            vec![
                format!("generation {}", kept.generation.unwrap_or(0)),
                format!("root {}", kept.root.clone().unwrap_or_default()),
            ],
        );
        Ok(kept)
    }

    fn abandon(&mut self, receipt: &Receipt) -> Result<(), AbandonFailure> {
        self.inner.abandon(receipt)?;
        let view = self.inner.view().display().to_string();
        self.journal(
            "managed_abandon",
            vec![format!("private work on {view} discarded")],
        );
        Ok(())
    }

    fn end_state(&mut self) -> Option<String> {
        self.inner.end_state()
    }

    fn describe(&self) -> Value {
        let mut described = self.inner.describe();
        if let (Some(object), Some(error)) = (described.as_object_mut(), &self.journal_error) {
            object.insert("journal_error".to_string(), json!(error));
        }
        described
    }
}

impl EvidenceSink for ManagedState<'_> {
    fn record(&mut self, id: &str, body: &[u8]) -> std::io::Result<()> {
        self.inner.record(id, body)
    }

    fn fetch(&mut self, id: &str) -> Fetched {
        self.inner.fetch(id)
    }
}

pub struct LinuxManaged<'a> {
    profile: Profile,
    clock: SystemClock,
    state: ManagedState<'a>,
    launcher: Confined<'a>,
    work: Scoped,
}

impl<'a> LinuxManaged<'a> {
    pub fn open(store: &'a Store, view: &Path, request_id: &str) -> Result<Self, String> {
        let project = project_directory(store, view);
        let service = LinuxStore::open(&project.join("store"), &[PRINCIPAL]).map_err(|error| {
            format!(
                "the managed store for {} could not be opened: {error}",
                view.display()
            )
        })?;
        let workspace = project.join("workspaces").join(PRINCIPAL);
        Ok(Self {
            profile: linux_managed_profile(),
            clock: SystemClock::new(),
            state: ManagedState {
                store,
                inner: Managed::new(Loopback::new(service), view, workspace, PRINCIPAL),
                request_id: request_id.to_string(),
                journal_error: None,
            },
            launcher: Confined { store },
            work: Scoped::new(request_id),
        })
    }

    /// The handle that closes this work.
    ///
    /// Nothing in this binary closes a work mid-transaction yet — a session holds
    /// one at a time and ends it by answering — so only the tests reach for it.
    /// It is the Linux stand-in for the scope fence Thalyx-Kernel's backend will
    /// be handed, and `exec::tests` holds the transaction to it.
    #[cfg(test)]
    pub fn fence(&self) -> Fence {
        self.work.fence()
    }
}

impl Platform for LinuxManaged<'_> {
    fn profile(&self) -> &Profile {
        &self.profile
    }
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
    fn state(&mut self) -> &mut dyn VersionedState {
        &mut self.state
    }
    fn launcher(&mut self) -> &mut dyn ProgramLaunch {
        &mut self.launcher
    }
    fn work(&mut self) -> &mut dyn WorkControl {
        &mut self.work
    }
    fn evidence(&mut self) -> &mut dyn EvidenceSink {
        &mut self.state
    }
}

/// The backend a boundary on this tree is carried out on.
pub fn for_tree<'a>(
    store: &'a Store,
    backend: Backend,
    tree: &Path,
    request_id: &str,
) -> Result<Box<dyn Platform + 'a>, String> {
    match backend {
        Backend::LinuxCurrent => Ok(Box::new(LinuxCurrent::new(
            store,
            thalyx_snapshot::Native,
            tree,
        ))),
        Backend::LinuxManaged => Ok(Box::new(LinuxManaged::open(store, tree, request_id)?)),
    }
}

/// A kept run's record, from wherever this backend keeps them.
///
/// The managed side is read without opening any service: `evidencia` may be
/// asked while a transaction holds one, and a reader has no business waiting
/// for a writer's lock.
pub fn fetch_evidence(store: &Store, backend: Backend, id: &str) -> Fetched {
    match backend {
        Backend::LinuxCurrent => StoreFiles::new(store).fetch(id),
        Backend::LinuxManaged => {
            let projects = store.state_root().join("managed");
            let Ok(entries) = std::fs::read_dir(&projects) else {
                return Fetched::Absent;
            };
            let mut unreadable = None;
            for entry in entries.flatten() {
                match thalyx_managed::read_evidence(&entry.path().join("store"), id) {
                    Fetched::Found(bytes) => return Fetched::Found(bytes),
                    Fetched::Unreadable(why) => unreadable = Some(why),
                    Fetched::Absent => {}
                }
            }
            // Rule 10: a store that could not be read might hold it.
            unreadable.map_or(Fetched::Absent, Fetched::Unreadable)
        }
    }
}
