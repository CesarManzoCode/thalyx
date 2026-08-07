//! `thalyx session` — what Thalyx is when Thalyx is the only thing there.
//!
//! On the image this is what init starts. There is no login before it and no
//! shell behind it, because `vault/05-Decisiones-y-Debates/Decision-Capa-vs-SO-Nuevo.md`
//! decrees that **Thalyx owns the boot** — and a system you reach by logging
//! into somebody else's session and running a command does not own anything.
//! The login that would otherwise be there was never decreed by anyone; it is
//! what an Alpine base hands you for free if nobody says otherwise.
//!
//! ## The one rule this file exists to keep
//!
//! **Every line is read from the running machine, now, and nothing claims what
//! it could not confirm.**
//!
//! A first screen is the easiest place in the whole system to put on a show:
//! nobody checks a banner. So this one is built the way `verify.sh` is built,
//! and turned on itself — each reading says whether it could be taken, and a
//! machine where nothing works produces a screen that says so, plainly, on
//! first boot. That is the point. A first impression that asserts things it did
//! not verify is worse than an Alpine login prompt, because it lies in the one
//! place nobody thinks to doubt.
//!
//! The same discipline the `hola` module learned: it never says whether it is
//! confined, because it cannot know. This never says it is the machine unless
//! it is.

use std::io::Write;
use std::path::Path;
use thalyx_agent::recollection::RecollectionError;
use thalyx_core::Store;

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// One thing Thalyx tried to find out about the machine it is running on.
struct Reading {
    subject: &'static str,
    /// What was found, or why it could not be.
    outcome: Outcome,
}

enum Outcome {
    /// Checked, and this is what it says.
    Found(String),
    /// Checked, and the answer is that it is not there.
    Absent(String),
    /// Could not be checked at all, which is not the same as absent.
    ///
    /// Rule 10: a failure to read is not a failure to exist, and saying which
    /// one happened is the difference between "go fix this" and "nothing to do
    /// here". Every version of this that collapsed the two has been a bug.
    Unreadable(String),
}

impl Reading {
    fn mark(&self) -> &'static str {
        match self.outcome {
            Outcome::Found(_) => "  ok ",
            Outcome::Absent(_) => "  no ",
            Outcome::Unreadable(_) => "  ?  ",
        }
    }

    fn text(&self) -> &str {
        match &self.outcome {
            Outcome::Found(t) | Outcome::Absent(t) | Outcome::Unreadable(t) => t,
        }
    }
}

/// How this process came to be running.
///
/// The distinction is the whole answer to "is this an operating system", and it
/// is read rather than assumed. Running as the machine's own session and
/// running as a program somebody started are different facts, and a session
/// that claimed the first while being the second would be doing exactly the
/// theatre this project refuses.
enum Standing {
    /// Started by init. Leaving means the machine stops, because there is
    /// nothing else here.
    TheMachine,
    /// Started from a shell on somebody else's system. Leaving goes back there.
    AProgram { under: String },
}

fn standing() -> Standing {
    // The parent is init if this is the machine's session. Nothing else needs
    // to be true, and nothing else is claimed.
    let ppid = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| {
            // Field 4 is ppid, but field 2 is the command name and may contain
            // spaces inside parentheses. Everything after the last ')' is safe.
            let tail = stat.rsplit(')').next()?.trim().to_string();
            tail.split_whitespace().nth(1)?.parse::<u32>().ok()
        });

    match ppid {
        Some(1) => Standing::TheMachine,
        _ => {
            let under = std::env::var("SHELL")
                .ok()
                .and_then(|s| {
                    Path::new(&s)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "another system".to_string());
            Standing::AProgram { under }
        }
    }
}

fn read_line(path: &str) -> Outcome {
    match std::fs::read_to_string(path) {
        Ok(text) => Outcome::Found(text.trim().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Outcome::Absent(format!("{path} is not there"))
        }
        Err(error) => Outcome::Unreadable(format!("{path}: {error}")),
    }
}

/// What filesystem the store actually sits on.
fn filesystem_of(root: &Path) -> Outcome {
    let mounts = match std::fs::read_to_string("/proc/mounts") {
        Ok(text) => text,
        Err(error) => return Outcome::Unreadable(format!("/proc/mounts: {error}")),
    };

    // The longest mount point that is a prefix of the path is the one it is on.
    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (_, point, kind) = match (fields.next(), fields.next(), fields.next()) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => continue,
        };
        if root.starts_with(point) && point.len() >= best.as_ref().map_or(0, |(n, _)| *n) {
            best = Some((point.len(), kind.to_string()));
        }
    }

    match best {
        Some((_, kind)) if kind == "btrfs" => Outcome::Found("btrfs".to_string()),
        Some((_, kind)) => Outcome::Absent(format!(
            "{kind} — snapshots and restore need btrfs and will not work here"
        )),
        None => Outcome::Unreadable("no mount point matched the store".to_string()),
    }
}

fn lsm_order() -> Outcome {
    match read_line("/sys/kernel/security/lsm") {
        Outcome::Found(order) => {
            if order.split(',').any(|name| name == "bpf") {
                Outcome::Found(order)
            } else {
                Outcome::Absent(format!("bpf is not in the LSM order ({order})"))
            }
        }
        // The file being missing has two entirely different causes, and the
        // first version of this function reported both as "not there".
        //
        // If securityfs is not mounted, nothing can be said about the LSM order
        // at all — the kernel may well have bpf first. Calling that absent
        // sends someone to edit a kernel command line over a missing mount.
        // Rule 10, broken in the same file that quotes it, which is about as
        // clear a demonstration as the rule could ask for.
        Outcome::Absent(_) if !mounted("securityfs") => {
            Outcome::Unreadable("securityfs is not mounted, so the LSM order cannot be read".into())
        }
        other => other,
    }
}

fn mounted(kind: &str) -> bool {
    std::fs::read_to_string("/proc/mounts").is_ok_and(|text| {
        text.lines()
            .any(|line| line.split_whitespace().nth(2) == Some(kind))
    })
}

fn cgroup2() -> Outcome {
    match std::fs::read_to_string("/proc/mounts") {
        Ok(text) => match text.lines().find_map(|l| {
            let mut f = l.split_whitespace();
            let (_, point, kind) = (f.next()?, f.next()?, f.next()?);
            (kind == "cgroup2").then(|| point.to_string())
        }) {
            Some(point) => match handed_down(Path::new(&point)) {
                Ok(()) => Outcome::Found(format!("mounted at {point}")),
                // Mounted and useless is not "mounted". This line said only
                // `mounted at /sys/fs/cgroup` on a machine where no module
                // could be given the limits its profile declares, so the boot
                // screen was clean and the first `correr` was not — which is
                // the failure with no symptom, in the one place built to have
                // none.
                Err(reason) => Outcome::Absent(format!("mounted at {point}, but {reason}")),
            },
            None => Outcome::Absent("no cgroup2 filesystem".to_string()),
        },
        Err(error) => Outcome::Unreadable(format!("/proc/mounts: {error}")),
    }
}

/// Whether a module can be given a root filesystem of its own.
///
/// Here for the same reason the cgroup reading grew a second half: a machine
/// that cannot pivot a module into its own root cannot run a module at all,
/// and until this line existed the screen said nothing about it. The boot said
/// `ok` to seven mounts and the first `correr` said `Invalid argument`.
fn sandbox_root() -> Outcome {
    let mountinfo = match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(text) => text,
        Err(error) => return Outcome::Unreadable(format!("/proc/self/mountinfo: {error}")),
    };

    match thalyx_sandbox::rootfs::root_mount_has_a_parent(&mountinfo) {
        Some(true) => Outcome::Found("a module can be pivoted into a root of its own".to_string()),
        Some(false) => Outcome::Absent(
            "the root has no parent mount, so pivot_root refuses every module".to_string(),
        ),
        None => Outcome::Unreadable("no mount for / is listed, so this cannot be told".to_string()),
    }
}

/// Whether the cgroup root hands down what a module's profile needs.
///
/// Read rather than assumed, and read from the profile rather than from a list
/// beside it. `thalyx_sandbox::limits::delegate` would *enable* them; this only
/// looks, because a reading that changes the machine is not a reading.
fn handed_down(root: &Path) -> Result<(), String> {
    let profile = thalyx_sandbox::profile::module_standard();
    let needed = profile.limits.controllers();

    let enabled = thalyx_sandbox::limits::enabled_controllers(root)
        .map_err(|error| format!("its controllers cannot be read: {error}"))?;

    let missing: Vec<&str> = needed
        .iter()
        .copied()
        .filter(|c| !enabled.iter().any(|e| e == c))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "it hands down no {} — a module could not be given the limits its profile declares",
        missing.join(" or ")
    ))
}

/// Is anything actually enforcing?
///
/// This used to answer by asking whether the **policy map** was pinned, through
/// `bpftool` — wrong twice over. The image has no `bpftool`, so inside the
/// machine the answer was always "absent" whatever the truth was; and a pinned
/// map is a place to put permissions, not something that reads them. A machine
/// with every map pinned and no program linked would have reported enforcement.
///
/// The honest question is which of thalyx-lsm's programs a live **link** runs,
/// and `thalyx_bpf::attachment` asks the kernel exactly that. The names come
/// out of the same object this binary carries, so there is no second list to
/// drift.
fn enforcement() -> Outcome {
    let Some(object) = crate::init::embedded::OBJECT else {
        return Outcome::Absent(
            "no BPF object was built into me, so there is nothing to attach".to_string(),
        );
    };

    match thalyx_bpf::attachment(object) {
        Ok(state) if state.is_absent() => Outcome::Absent(format!(
            "{}, so no permission would be enforced",
            state.describe()
        )),
        // A partial attachment is Found rather than Absent because something
        // *is* in the decision path — and `describe` says in as many words that
        // it enforces less than it looks like it does.
        Ok(state) => Outcome::Found(state.describe()),
        // Listing the kernel's links needs CAP_SYS_ADMIN. Unreadable, not
        // absent: a session run by a human who is not root would otherwise
        // report a machine with no enforcement, which is the same lie in the
        // opposite direction.
        Err(error) => Outcome::Unreadable(error.to_string()),
    }
}

fn modules(store: &Store) -> Outcome {
    match store.installed() {
        Ok(list) if list.is_empty() => Outcome::Absent("nothing installed yet".to_string()),
        Ok(list) => Outcome::Found(format!(
            "{}: {}",
            list.len(),
            list.iter()
                .map(|(id, version)| format!("{id} {version}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Err(error) => Outcome::Unreadable(error.to_string()),
    }
}

fn gather(store: &Store) -> Vec<Reading> {
    vec![
        Reading {
            subject: "kernel",
            outcome: read_line("/proc/sys/kernel/osrelease"),
        },
        Reading {
            subject: "filesystem",
            outcome: filesystem_of(store.root()),
        },
        Reading {
            subject: "cgroup v2",
            outcome: cgroup2(),
        },
        Reading {
            subject: "sandbox root",
            outcome: sandbox_root(),
        },
        Reading {
            subject: "lsm order",
            outcome: lsm_order(),
        },
        Reading {
            subject: "enforcement",
            outcome: enforcement(),
        },
        Reading {
            subject: "modules",
            outcome: modules(store),
        },
    ]
}

/// Turn the machine off.
///
/// It exists because the `salir` branch has been telling people to type
/// `apagar` since the first boot, and nothing understood the word. A machine
/// whose only documented way out is a verb it does not implement leaves
/// `Ctrl-a x` — which is QEMU's, not Thalyx's, and would be nothing at all on
/// real hardware.
///
/// The kernel is asked to power off directly. There is no `shutdown` to call
/// and nothing else running that would need to be told first: the session is
/// the only child, and PID 1 is what invoked it.
fn power_off(standing: &Standing) {
    match standing {
        Standing::TheMachine => {
            println!();
            println!("  Turning off. Anything not written to the store is gone,");
            println!("  because the root filesystem is memory and always was.");
            println!();
            let _ = std::io::stdout().flush();
            // Only returns on failure: on success the machine is already off.
            // Reported rather than swallowed, because a poweroff that silently
            // did nothing leaves a prompt that looks like it ignored the human.
            let error = thalyx_syscall::reboot(thalyx_syscall::RebootCommand::PowerOff);
            println!("  the kernel refused to power off: {error}");
            println!("  that needs privilege I do not have here.");
            println!();
        }
        Standing::AProgram { under } => {
            println!();
            println!("  No. I am a program under {under}, and turning this machine");
            println!("  off would turn off something that is not mine. `salir`");
            println!("  leaves; on the image this same word powers the machine down.");
            println!();
        }
    }
}

/// What the kernel has been saying, since PID 1 stopped it saying it here.
///
/// This exists because turning the console down without giving the messages
/// back would be hiding them, and on a machine with no shell there is no
/// `dmesg` to fall back on. `nucleo` shows what went wrong; `nucleo todo` shows
/// everything, which is usually a lot and occasionally the only thing that
/// helps.
fn show_kernel(everything: bool) {
    println!();
    let messages = match thalyx_syscall::kernel_messages() {
        Ok(messages) => messages,
        Err(error) => {
            // Rule 10 at the one place a human would read silence as calm.
            println!("  I could not read what the kernel said: {error}");
            println!("  That is not the same as it having said nothing.");
            println!();
            return;
        }
    };

    let shown: Vec<_> = messages
        .iter()
        .filter(|m| everything || m.is_trouble())
        .collect();

    if shown.is_empty() {
        if everything {
            println!("  The kernel's buffer is empty, which is stranger than it sounds.");
        } else {
            println!("  The kernel has reported nothing at warning level or worse.");
            println!(
                "  `nucleo todo` shows everything it said, which is {} lines.",
                messages.len()
            );
        }
        println!();
        return;
    }

    for message in &shown {
        // The marker says how bad the kernel thought it was, not how bad this
        // thinks it is. Thalyx does not re-grade somebody else's report.
        let marker = if message.priority <= 3 {
            "x"
        } else if message.is_trouble() {
            "!"
        } else {
            " "
        };
        println!("  {marker} [{:>9.6}] {}", message.seconds, message.text);
    }

    println!();
    if !everything {
        println!(
            "  {} of {} lines. `nucleo todo` for all of them.",
            shown.len(),
            messages.len()
        );
        println!();
    }
}

/// What is installed, read from the store rather than from anything remembered.
fn list_modules(store: &Store) {
    println!();
    match store.installed() {
        Ok(list) if list.is_empty() => {
            println!("  Nothing is installed.");
            println!();
            println!("  If a store was expected here, the first lines of the boot say");
            println!("  whether one was mounted. An empty store and an absent one look");
            println!("  the same from this list, and only the boot told them apart.");
        }
        Ok(list) => {
            for (id, version) in &list {
                println!("  {id} {version}");
            }
            println!();
            println!("  `correr <id>` runs one.");
        }
        Err(error) => {
            // Not "nothing is installed". Rule 10 again, at the one place a
            // human is most likely to read the answer as an inventory.
            println!("  I could not read the store: {error}");
            println!("  That is not the same as it being empty, and I will not");
            println!("  report it as empty.");
        }
    }
    println!();
}

/// What is in the repository and could be installed.
///
/// Separate verb from `modulos` because they answer different questions, and
/// conflating them is how a person ends up believing something is installed
/// because they saw its name. `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`
/// step 2 is installing from a local repository, and inside the machine there
/// is no shell to hand a path to — so the repository has to be findable.
fn list_available(store: &Store) {
    println!();
    let repo = store.repo_root();
    match thalyx_core::repo::scan(&repo) {
        Ok(scan) if scan.candidates.is_empty() && scan.rejected.is_empty() => {
            println!("  The repository is empty.");
            println!();
            println!("  It is {}, on the store.", repo.display());
        }
        Ok(scan) => {
            for candidate in &scan.candidates {
                println!("  {} {}", candidate.module_id, candidate.version);
            }
            // Named, never silently dropped. A bundle whose signature does not
            // check out is the single most important thing this list can say,
            // and a resolver that only prints what passed would hide exactly
            // the file somebody needs to look at.
            for rejected in &scan.rejected {
                println!();
                println!(
                    "  refused  {}",
                    rejected
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| rejected.path.display().to_string())
                );
                println!("           {}", rejected.reason);
            }
            if !scan.candidates.is_empty() {
                println!();
                println!("  `instalar <id>` installs one, and shows what it asks for");
                println!("  before anything is written.");
            }
        }
        Err(error) => {
            println!("  I could not read the repository: {error}");
            println!("  That is not the same as it being empty.");
        }
    }
    println!();
}

/// Install from the repository, through the trusted path.
///
/// The confirmation is `TerminalConfirmer`, which prints a prompt the **core**
/// generated and can neither reword nor skip — `vault/11-Seguridad/Camino-Confiable.md`
/// decrees that the request is generated and rendered by the core, and this is
/// the machine's end of it. It is step 3 of the exit criterion, and it is the
/// same code path the host CLI uses, not a copy of it.
fn install_module(store: &Store, name: &str, utterance: &str) {
    println!();
    let candidate = match thalyx_core::repo::resolve(&store.repo_root(), name, None) {
        Ok(candidate) => candidate,
        Err(error) => {
            println!("  {error}");
            println!();
            println!("  `disponibles` lists what the repository holds.");
            println!();
            return;
        }
    };

    println!("  {} {}", candidate.module_id, candidate.version);
    println!("  from {}", candidate.path.display());
    println!();

    let request = thalyx_core::InstallRequest {
        bundle_path: &candidate.path,
        contract: crate::install_contract(&candidate.path),
    };

    // Never `--yes` from here. This is the one place in the whole system where
    // a human is being asked to grant something, and a session that assumed
    // consent would make the trusted path a formality.
    let mut confirmer = crate::render::TerminalConfirmer::new(false);

    match thalyx_core::install(store, request, &mut confirmer) {
        Ok(outcome) => {
            println!();
            match &outcome.replaced {
                Some(previous) => println!(
                    "  {} upgraded from {} to {}",
                    outcome.module_id, previous, outcome.version
                ),
                None => println!("  {} {} installed", outcome.module_id, outcome.version),
            }
            println!(
                "  {} file(s), {} permission(s) now in force",
                outcome.files.len(),
                outcome.granted
            );

            // After the commit, never before. A memory written first would
            // describe an installation that a refusal at the trusted path then
            // stopped, and the person who said no would find the machine
            // remembering that they had said yes.
            //
            // The `current` link and not the version directory: it is the one
            // point that decides whether the module is installed at all, so
            // `revertir` makes this record stop being assertable — which is
            // how undoing shows up in what the machine remembers.
            let module_id = outcome.module_id.clone();
            let version = outcome.version.clone();
            let installed_at = store.current_link(&module_id);
            remembering(store, "The install", |memory| {
                thalyx_agent::recollection::record_install(
                    memory,
                    SESSION_TASK,
                    utterance,
                    &module_id,
                    &version,
                    &installed_at,
                )
            });

            println!();
            println!(
                "  `correr {}` runs it. `revertir` undoes this.",
                outcome.module_id
            );
            println!("  `recuerdos` says what I will still know after a restart.");
        }
        Err(error) => {
            println!();
            println!("  not installed: {error}");
            println!();
            println!("  Nothing was written. An install that stops before the commit");
            println!("  leaves the machine exactly as it was.");
        }
    }
    println!();
}

/// What is granted, and to whom.
fn show_permissions(store: &Store) {
    println!();
    if let Err(error) = crate::render::permissions(store) {
        println!("  I could not read the permission registry: {error}");
    }
    println!();
}

/// Undo the last thing Thalyx published.
///
/// Step 4 of the exit criterion. Deliberately the cheap one: `rollback` takes
/// back what Thalyx itself put on disk and touches nothing the human made,
/// which is why it does not ask first. The destructive one is `restore`, it has
/// its own name, and it is not a verb here.
fn revert(store: &Store, utterance: &str) {
    println!();
    let plan = match thalyx_core::rollback::plan(store, None) {
        Ok(plan) => plan,
        Err(error) => {
            println!("  {error}");
            println!();
            return;
        }
    };

    println!("  {}", plan.describe());
    println!("  published by request {}", plan.request_id);
    if plan.permissions_revoked > 0 {
        println!(
            "  {} permission(s) stop being effective",
            plan.permissions_revoked
        );
    }
    if let Some(uid) = plan.uid_retired {
        println!("  user {uid} is retired, and never handed to another module");
    }
    println!();
    println!("  Nothing outside what Thalyx published is touched.");

    match thalyx_core::rollback::apply(store, &plan, &crate::new_request_id()) {
        Ok(()) => {
            // Only what was asked, and nothing about the world. The install's
            // own record witnesses the `current` link this just removed, so the
            // undo already shows up there — as that record becoming
            // unconfirmable, in those words. See `recollection.rs`.
            remembering(store, "The rollback", |memory| {
                thalyx_agent::recollection::record_utterance(memory, SESSION_TASK, utterance)
            });

            println!();
            println!("  undone.");
        }
        Err(error) => {
            println!();
            println!("  not undone: {error}");
        }
    }
    println!();
}

/// What everything done through the session is remembered under.
///
/// Step 6 of `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` is
/// restarting the machine and finding that the agent still knows what was being
/// done. On the host CLI the task is named by `--task`; inside the machine
/// there is nobody to name one, and inventing a scheme — a task per boot, a
/// task per module — would put a decision in code that the vault has not made,
/// and a task per boot would lose the record at exactly the reboot the step is
/// about.
///
/// So there is one, it is named for what it is, and it deliberately outlives
/// the session that wrote it. That is the whole demonstration: the word is
/// `session` and the memory is not one.
const SESSION_TASK: &str = "session";

/// The word that makes running without enforcement a thing somebody typed.
const UNCONFINED_WORD: &str = "sin-confinar";

/// The profile a module gets when it is started from the prompt.
///
/// Taken from `thalyx-sandbox` rather than written out here. It was written
/// out here once, as `"default"`, which is not the name of any profile — and
/// the run failed with that on the machine's own console after installing
/// correctly, which is the worst place to find it.
///
/// Nothing caught it, and the reason is structural rather than an oversight:
/// the name is only looked up once the kernel side is found present, so every
/// machine that cannot enforce reported the honest gap instead and never got
/// as far as the name. The one machine that could enforce was the image. See
/// `thalyx_core::run` — the lookup now happens before that gate, so a name no
/// profile has is a wrong name everywhere.
const SESSION_PROFILE: &str = thalyx_sandbox::profile::MODULE_STANDARD;

/// What the machine still knows, re-checked against the disk right now.
///
/// The same function `thalyx agent recall` runs, indented to match the session.
/// `Coherencia-Doble-Ruta.md` is why it is not written twice: the two routes
/// have to agree about a memory, and the way they stop agreeing is one of them
/// being edited.
fn show_memory(store: &Store) {
    println!();
    if let Err(error) = crate::agent::recall(store, SESSION_TASK, "  ") {
        // Rule 10. An unreadable memory and an empty one are different facts
        // about the machine, and `recall` already says the second in its own
        // words — so this branch can only be the first, and says so.
        println!("  I could not read what I remember: {error}");
        println!("  That is not the same as remembering nothing.");
    }
    println!();
}

/// Write down what was just done, and never let that failure look like the
/// operation's.
///
/// The install has already happened when this runs. If the memory cannot be
/// written, the module is still installed and the person has to be told which
/// of the two is true — a message that read as though the install had failed
/// would send them to undo something that worked.
fn remembering(
    store: &Store,
    what: &str,
    write: impl FnOnce(&Path) -> Result<(), RecollectionError>,
) {
    if let Err(error) = write(&crate::agent::memory_path(store)) {
        println!();
        println!("  {what} happened, and I could not write it down: {error}");
        println!("  The machine is as this said it is. What will be missing after");
        println!("  a restart is my record of it, not the thing itself.");
    }
}

/// Run an installed module from the session.
///
/// The whole run goes through `thalyx_core::run` by way of the same CLI code
/// `thalyx module run` uses. `Coherencia-Doble-Ruta.md` is the reason it is not
/// written a second time here: two orchestrations of the same operation drift,
/// and the drift shows up as the human's route and the agent's route leaving
/// the machine in different states.
///
/// ## Why there is no fallback
///
/// Confined is the default and the core refuses it outright when nothing can
/// enforce a permission. A session that noticed the refusal and quietly ran the
/// module unconfined would be doing the one thing `RunRequest::unconfined` was
/// written to prevent: reaching the degraded state by accident instead of
/// deliberately. So the refusal is printed, the word that means it is named,
/// and the human types it or does not.
fn start_module(store: &Store, rest: &str) {
    let (id, unconfined) = match rest.split_once(' ') {
        Some((id, tail)) => (id.trim(), tail.trim() == UNCONFINED_WORD),
        None => (rest, false),
    };

    if id.is_empty() {
        println!();
        println!("  Which one. `modulos` lists them.");
        println!();
        return;
    }

    if !store.is_installed(id) {
        println!();
        println!("  `{id}` is not installed. `modulos` lists what is.");
        println!();
        return;
    }

    let Err(error) = crate::run::run(
        store.root(),
        id,
        SESSION_PROFILE,
        thalyx_core::run::DEFAULT_ENTRYPOINT,
        Vec::new(),
        unconfined,
        crate::new_request_id(),
    ) else {
        return;
    };

    println!();
    println!("  {id} did not run: {error}");
    if !unconfined {
        println!();
        println!("  If that is the kernel side being absent, it is the known gap and");
        println!("  not this module: nothing here can enforce a permission yet, and");
        println!("  Thalyx will not pretend to. To run it anyway, knowing that:");
        println!();
        println!("      correr {id} {UNCONFINED_WORD}");
        println!();
        println!("  The journal records that run as degraded, because it is.");
    }
    println!();
}

pub fn run(store: &Store, once: bool) -> Fallible {
    let standing = standing();
    let readings = gather(store);

    println!();
    match &standing {
        Standing::TheMachine => {
            println!("  Thalyx.");
            println!();
            println!("  This is the machine. There is no shell behind this and nothing");
            println!("  to return to — not because it is hidden, but because it was");
            println!("  never installed.");
        }
        Standing::AProgram { under } => {
            println!("  Thalyx, running as a program under {under}.");
            println!();
            println!("  This is not the machine. Something else booted it and started");
            println!("  me, and leaving returns you there. On the image this paragraph");
            println!("  reads differently, and that difference is the whole claim.");
        }
    }

    println!();
    println!("  What I can tell you about where I am:");
    println!();
    for reading in &readings {
        println!(
            "{} {:<12} {}",
            reading.mark(),
            reading.subject,
            reading.text()
        );
    }

    let unreadable = readings
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Unreadable(_)))
        .count();
    let absent = readings
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Absent(_)))
        .count();

    println!();
    if unreadable > 0 {
        // "1 of those" reads fine; the count below did not, and the machine's
        // own voice is the last place to leave an obvious slip.
        println!("  {unreadable} of those I could not check at all. Not absent — unchecked.",);
    }
    if absent == 1 {
        println!("  1 is not here. I will not pretend otherwise later.");
    } else if absent > 1 {
        println!("  {absent} are not here. I will not pretend otherwise later.");
    }
    if unreadable == 0 && absent == 0 {
        println!("  Everything I know how to check is here.");
    }

    if once {
        return Ok(());
    }

    println!();
    match &standing {
        Standing::TheMachine => {
            println!("  `disponibles` lists what can be installed, `instalar <id>`");
            println!("  installs one and shows what it asks for, `revertir` undoes it.");
            println!("  `modulos` lists what is installed, `correr <id>` runs one,");
            println!("  `discos` lists the disks I can see and `instalar-en <disco>`");
            println!("  puts this machine on one, so it stops needing this medium.");
            println!("  `permisos` shows what is granted, `recuerdos` says what I");
            println!("  will still know after a restart, `estado` re-reads the");
            println!("  machine, `nucleo` shows what the kernel has been saying,");
            println!("  `apagar` turns it off.");
        }
        Standing::AProgram { .. } => {
            println!("  `disponibles`, `instalar <id>`, `modulos`, `correr <id>`,");
            println!("  `permisos`, `revertir`, `recuerdos`, `estado`, `nucleo`,");
            println!("  `discos`, `instalar-en <disco>`.");
            println!("  `salir` to leave.");
        }
    }
    // Said wherever it is true, and nowhere it is not. Both standings hit the
    // same wall — the core refuses a confined run with no policy map — and a
    // machine with the LSM attached must not be told its enforcement is
    // missing, which is the kind of hardcoded sentence this file exists not to
    // have.
    if matches!(enforcement(), Outcome::Absent(_)) {
        println!("  Nothing here enforces a permission yet, so `correr` will say");
        println!("  so and stop. `correr <id> {UNCONFINED_WORD}` runs it anyway.");
    }
    println!();

    loop {
        print!("  > ");
        std::io::stdout().flush()?;

        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let line = line.trim();

        match line {
            "" => continue,
            "salir" | "exit" | "quit" => match &standing {
                Standing::TheMachine => {
                    println!();
                    println!("  There is nowhere to go. Turning the machine off is");
                    println!("  `apagar`; anything else keeps you here.");
                    println!();
                }
                Standing::AProgram { under } => {
                    println!("  Back to {under}.");
                    break;
                }
            },
            "estado" | "status" => {
                for reading in &gather(store) {
                    println!(
                        "{} {:<12} {}",
                        reading.mark(),
                        reading.subject,
                        reading.text()
                    );
                }
            }
            "apagar" | "poweroff" => {
                power_off(&standing);
            }
            "modulos" | "módulos" => {
                list_modules(store);
            }
            "disponibles" | "available" | "repo" => {
                list_available(store);
            }
            "permisos" | "permissions" => {
                show_permissions(store);
            }
            "revertir" | "rollback" => {
                revert(store, line);
            }
            "recuerdos" | "recordar" | "memory" | "recall" => {
                show_memory(store);
            }
            _ if line.starts_with("instalar ") || line.starts_with("install ") => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                install_module(store, rest, line);
            }
            "instalar" | "install" => {
                println!();
                println!("  Which one. `disponibles` lists what the repository holds.");
                println!();
            }
            "discos" | "disks" => {
                list_disks();
            }
            _ if line.starts_with("instalar-en ") || line.starts_with("install-onto ") => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                install_onto(rest);
            }
            "instalar-en" | "install-onto" => {
                println!();
                println!("  Which disk. `discos` lists them.");
                println!();
            }
            "nucleo" | "núcleo" | "kernel" | "dmesg" => {
                show_kernel(false);
            }
            "nucleo todo" | "núcleo todo" | "kernel all" => {
                show_kernel(true);
            }
            _ if line.starts_with("correr ") || line.starts_with("run ") => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                start_module(store, rest);
            }
            "correr" | "run" => {
                println!();
                println!("  Which one. `modulos` lists them.");
                println!();
            }
            _ => {
                println!();
                println!("  I have no model loaded, so I can only act on what the rules");
                println!("  already understand. `thalyx agent plan \"{line}\"` shows what");
                println!("  I would make of that, and says who understood it.");
                println!();
            }
        }
    }

    Ok(())
}

// ─────────────────────────────────────────── installing this machine onto a disk
//
// `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`: the criterion is a
// medium that, put into a PC with no operating system, leaves that machine running
// Thalyx. Everything up to here made that possible from a terminal on a development
// machine — and **there is no terminal on the PC**. There is no shell, so a verb
// that is not here does not exist for the person holding the machine.
//
// So `discos` and `instalar-en <disco>`. They are the last two words the exit
// criterion needed and they are the reason the criterion is reachable at all.

/// What Thalyx can see to install onto.
///
/// Whole disks only. Installing writes a partition table, so a partition is not a
/// thing that can be installed onto — offering one would produce a table written
/// inside a partition, which is legal, invisible, and boots nothing.
fn list_disks() {
    let disks = thalyx_install::partitions::every();
    let whole: Vec<&std::path::PathBuf> = disks
        .iter()
        .filter(|device| {
            // A whole disk is one sysfs knows as a disk rather than as somebody's
            // partition, and `partitions::of` answers for the first and errors for
            // the second. Asked rather than derived from the name, for the same
            // reason the installer asks: `nvme0n1` and `nvme0n1p1` differ by a
            // convention of the tools that print them.
            thalyx_install::partitions::of(device).is_ok()
        })
        .collect();

    println!();
    if whole.is_empty() {
        println!("  I can see no disks at all.");
        println!();
        println!("  Either nothing is attached, or this kernel has no driver for the");
        println!("  controller it is attached to. `estado` says what else is missing.");
        println!();
        return;
    }

    println!("  {} disk(s):", whole.len());
    println!();
    for device in &whole {
        let size = std::fs::File::open(device)
            .and_then(|mut file| std::io::Seek::seek(&mut file, std::io::SeekFrom::End(0)))
            .map(|bytes| format!("{} GiB", bytes / (1024 * 1024 * 1024)))
            .unwrap_or_else(|_| "size unreadable".to_string());
        let parts = thalyx_install::partitions::of(device).unwrap_or_default();
        println!(
            "    {:<16} {size}, {} partition(s)",
            device.display(),
            parts.len()
        );
        for (number, path) in &parts {
            let what = match thalyx_btrfs::identify(path) {
                Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) if label == thalyx_btrfs::LABEL => {
                    "a Thalyx store".to_string()
                }
                Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) if label.is_empty() => {
                    "btrfs, no label".to_string()
                }
                Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) => format!("btrfs `{label}`"),
                // Everything that is not Btrfs reads the same from here, and saying
                // "not btrfs" would read as "empty" about a disk somebody is deciding
                // whether to destroy.
                _ => "something I do not recognise".to_string(),
            };
            println!("      {number}  {what}");
        }
    }
    println!();
    println!("  `instalar-en <disco>` puts Thalyx on one. Everything on it is lost.");
    println!();
}

/// Put this machine onto a disk, so it stops needing the medium it booted from.
///
/// The kernel comes off the medium this machine started from, found by looking for a
/// FAT32 volume Thalyx labelled carrying the one file a firmware looks for — see
/// `thalyx_install::medium`, which explains why that is a name and not a guess, and
/// what happened the day it asked for the file alone. Nothing is mounted to do it:
/// the bytes are read the same way they were written, so this needs no vfat in the
/// kernel.
fn install_onto(disk: &str) {
    use std::io::{IsTerminal, Write};

    let disk = std::path::PathBuf::from(disk);

    println!();
    let sectors = match std::fs::File::open(&disk)
        .and_then(|mut file| std::io::Seek::seek(&mut file, std::io::SeekFrom::End(0)))
    {
        Ok(bytes) => bytes / thalyx_install::gpt::SECTOR,
        Err(error) => {
            println!("  I cannot open {}: {error}", disk.display());
            println!("  `discos` lists what I can see.");
            println!();
            return;
        }
    };

    let plan = match thalyx_install::Plan::of(&disk, sectors) {
        Ok(plan) => plan,
        Err(error) => {
            println!("  {error}");
            println!();
            return;
        }
    };

    // The kernel is found **before** anything is said about destroying the disk. A
    // machine that asked for confirmation, got it, wiped the disk and only then
    // discovered it had no kernel to write would have destroyed the disk for nothing
    // — and this is the one verb where that is unrecoverable.
    let found = match thalyx_install::medium::find(Some(&disk)) {
        Ok(found) => found,
        Err(error) => {
            println!("  I cannot find the medium I started from, so I have no kernel");
            println!(
                "  to install. Nothing has been written to {}.",
                disk.display()
            );
            println!();
            for line in error.to_string().lines() {
                println!("  {line}");
            }
            println!();
            return;
        }
    };

    let mib = |sectors: u64| sectors * thalyx_install::gpt::SECTOR / (1024 * 1024);
    println!("  About to install Thalyx onto {}.", disk.display());
    println!();
    println!(
        "  the kernel comes from {} — {} bytes",
        found.device.display(),
        found.kernel_bytes
    );
    println!();
    println!("  it will become:");
    println!(
        "    1  {:>8} MiB  the boot partition, holding that kernel",
        mib(plan.esp_sectors())
    );
    println!(
        "    2  {:>8} MiB  the store: system, modules, user",
        mib(plan.store_sectors())
    );
    println!();
    println!(
        "  Everything on {} will be gone. This cannot be undone.",
        disk.display()
    );
    println!();

    // The same confirmation `thalyx install` uses on the host, and for the same
    // reason: this is the most destructive thing Thalyx can be asked to do, the
    // argument is one word, and a `y` confirms a sentence the human stopped reading.
    if !std::io::stdin().is_terminal() {
        println!("  There is no terminal to confirm on, so I will not do this.");
        println!("  Silence is not consent.");
        println!();
        return;
    }
    print!("  Type the disk's path to confirm: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err()
        || answer.trim() != disk.display().to_string()
    {
        println!();
        println!("  That is not {}. Nothing was written.", disk.display());
        println!();
        return;
    }
    println!();

    // Onto the tmpfs, because that is the only writable place on this machine and
    // because the kernel must not touch the disk being installed onto before the
    // partition table replaces it.
    let staged = std::path::Path::new(INSTALL_WORKSPACE).join("bzImage");
    if let Some(parent) = staged.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        println!("  I could not make {}: {error}", parent.display());
        println!();
        return;
    }

    let mut volume = match thalyx_install::medium::Volume::open(&found.device) {
        Ok(Some(volume)) => volume,
        Ok(None) => {
            println!(
                "  {} stopped being readable between finding it and",
                found.device.display()
            );
            println!("  reading it. Nothing was written.");
            println!();
            return;
        }
        Err(error) => {
            println!("  I could not read {}: {error}", found.device.display());
            println!();
            return;
        }
    };
    if let Err(error) = volume.extract_boot_file(&staged) {
        println!("  I could not take the kernel off the medium: {error}");
        println!("  Nothing was written to {}.", disk.display());
        println!();
        return;
    }
    println!("  ok  kernel       taken off {}", found.device.display());

    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);

    match thalyx_install::install(
        &disk,
        &staged,
        std::path::Path::new(INSTALL_WORKSPACE),
        seconds,
    ) {
        Ok(installed) => {
            println!(
                "  ok  boot         {} — the kernel, at the one path a firmware looks for",
                installed.esp.display()
            );
            println!(
                "  ok  store        {} — labelled `{}`",
                installed.store.display(),
                installed.filesystem.label
            );
            for (name, why) in &installed.subvolumes.mounted {
                match why {
                    None => println!("  ok  subvolume    {name}"),
                    Some(reason) => println!("  NO  subvolume    {name}: {reason}"),
                }
            }
            println!();
            if installed.subvolumes.is_a_store() {
                println!("  That disk is a Thalyx machine now. `apagar`, take the medium");
                println!("  out, and start it again — it will find its store by the label");
                println!("  and will not need me.");
            } else {
                println!("  The boot half is written and the store is not finished.");
                println!(
                    "  Running `instalar-en {}` again finishes it.",
                    disk.display()
                );
            }
            println!();
        }
        Err(error) => {
            println!();
            println!("  The install did not finish: {error}");
            println!();
            println!(
                "  Whatever was on {} before is gone either way — the",
                disk.display()
            );
            println!("  partition table is written first. Running this again is safe");
            println!("  and is the way to finish it.");
            println!();
        }
    }
}

/// Where the install puts the kernel it takes off the medium, and its mount points.
///
/// `/run` and not `/tmp`, because the image has thirteen directories and `/tmp` is
/// not one of them. Same constant and same reason as `store_disk` and `install`.
const INSTALL_WORKSPACE: &str = "/run/thalyx/install";

#[cfg(test)]
mod tests {
    use super::*;

    /// The profile the prompt asks for has to be one that exists.
    ///
    /// Cheap, and it would have saved a kernel build and a boot: `"default"`
    /// sat here for as long as the prompt could run a module, and the only
    /// thing that could tell was a machine with enforcement live — which for
    /// most of that time was no machine at all.
    #[test]
    fn the_profile_the_prompt_runs_modules_under_is_one_that_resolves() {
        thalyx_sandbox::profile::resolve(SESSION_PROFILE).unwrap_or_else(|error| {
            panic!("the prompt would ask for a profile nothing can resolve: {error}")
        });
    }

    /// The reading looks at what the root *hands down*, not at what it has.
    ///
    /// The two files sit next to each other and read almost the same:
    /// `cgroup.controllers` is what a cgroup could hand down and
    /// `cgroup.subtree_control` is what it does. A machine that had every
    /// controller compiled in and delegated none of them would read as fine
    /// through the wrong one — which is exactly the machine this was written
    /// for, and exactly the machine the image was.
    #[test]
    fn a_root_that_could_hand_down_everything_and_hands_down_nothing_reads_as_absent() {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("cgroup.controllers"),
            "cpuset memory pids\n",
        )
        .unwrap();
        std::fs::write(root.path().join("cgroup.subtree_control"), "").unwrap();

        let reason = handed_down(root.path()).expect_err("nothing is handed down");
        assert!(reason.contains("memory"), "{reason}");
        assert!(reason.contains("pids"), "{reason}");
    }

    #[test]
    fn a_root_that_hands_down_what_the_profile_needs_reads_as_present() {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("cgroup.controllers"),
            "cpuset memory pids\n",
        )
        .unwrap();
        std::fs::write(root.path().join("cgroup.subtree_control"), "memory pids\n").unwrap();

        handed_down(root.path()).expect("both are handed down");
    }

    /// Half is not enough, and the message names which half.
    ///
    /// Without this, a check that looked at the first controller and stopped
    /// would pass both tests above.
    #[test]
    fn a_root_that_hands_down_half_of_it_says_which_half() {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("cgroup.controllers"),
            "cpuset memory pids\n",
        )
        .unwrap();
        std::fs::write(root.path().join("cgroup.subtree_control"), "memory\n").unwrap();

        let reason = handed_down(root.path()).expect_err("pids is missing");
        assert!(reason.contains("pids"), "{reason}");
        assert!(
            !reason.contains("memory"),
            "memory is handed down: {reason}"
        );
    }

    /// And it is a profile that actually isolates.
    ///
    /// `diagnostic` resolves too, and a prompt that quietly ran modules under
    /// it would pass the test above while confining nothing beyond the cgroup.
    /// The run would announce itself as degraded, which is the only reason
    /// this is a second claim rather than the same one.
    #[test]
    fn and_one_that_isolates_rather_than_merely_being_a_name() {
        let profile = thalyx_sandbox::profile::resolve(SESSION_PROFILE).expect("resolves");
        assert!(
            profile.isolates(),
            "the prompt runs modules under `{SESSION_PROFILE}`, which isolates nothing"
        );
    }
}
