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

use std::cmp::Ordering;
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

/// A stretch of boot where the kernel said nothing.
struct Gap {
    seconds: f64,
    before: String,
    after: String,
    at: f64,
}

/// The longest silences between consecutive kernel messages, longest first.
///
/// Split out from [`show_slowest`] so it can be exercised without a kernel, and
/// pure so that what it claims — that these are the biggest gaps and that they are
/// in order — is something a test can hold it to.
///
/// **A gap is where the time went, not what took it.** The message *after* a
/// silence is the one that finished; the one before is where the waiting started.
/// Both are printed because either alone sends a person to the wrong half.
fn slowest_gaps(messages: &[thalyx_syscall::KernelMessage], how_many: usize) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = messages
        .windows(2)
        .map(|pair| Gap {
            seconds: pair[1].seconds - pair[0].seconds,
            before: pair[0].text.clone(),
            after: pair[1].text.clone(),
            at: pair[0].seconds,
        })
        .collect();
    // Descending, and by a total order because f64 has none. A NaN cannot come out
    // of subtracting two timestamps the kernel printed, and ordering it last rather
    // than panicking keeps a malformed record from taking the whole verb down.
    gaps.sort_by(|a, b| b.seconds.partial_cmp(&a.seconds).unwrap_or(Ordering::Equal));
    gaps.truncate(how_many);
    gaps
}

/// Where the boot spent its time.
///
/// Exists because `nucleo` could answer two questions and not this one: four lines
/// of trouble, or seven hundred lines of everything, and a person watching a
/// machine take forty seconds to reach its prompt can read neither. Cesar asked on
/// 2026-08-07 whether that was normal — on hardware where the delay was the same
/// from two different USB sticks, which is what a fixed timeout looks like and not
/// what slow reading looks like.
///
/// The kernel already timestamps every line. Nobody had subtracted them.
fn show_slowest() {
    println!();
    let messages = match thalyx_syscall::kernel_messages() {
        Ok(messages) => messages,
        Err(error) => {
            println!("  I cannot read what the kernel said: {error}");
            println!("  That is not the same as it having said nothing.");
            println!();
            return;
        }
    };
    if messages.len() < 2 {
        println!("  Fewer than two messages, so there is no gap to measure.");
        println!();
        return;
    }

    let gaps = slowest_gaps(&messages, 8);
    let total = messages.last().map(|m| m.seconds).unwrap_or_default();
    let waited: f64 = gaps.iter().map(|gap| gap.seconds).sum();

    println!("  The kernel talked for {total:.1}s. The longest silences in it:");
    println!();
    for gap in &gaps {
        println!("    {:>6.2}s  at {:>8.2}s", gap.seconds, gap.at);
        println!("            after   {}", gap.before);
        println!("            then    {}", gap.after);
        println!();
    }
    println!(
        "  Those {} account for {waited:.1}s of {total:.1}s.",
        gaps.len()
    );
    println!();
    // Said rather than left to be inferred: a long gap is where the clock went, and
    // the thing that finished it is usually not the thing that was slow. This verb
    // narrows down where to look; it does not name a culprit, and pretending it did
    // would be the machine guessing on somebody's behalf.
    println!("  A gap says where the time went, not what took it. The line after a");
    println!("  silence is the one that finished waiting.");
    println!();
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
/// The machine, re-read, in one object.
///
/// `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **A3**. It is the
/// first cost — discovery — answered in one call: what an agent would otherwise
/// find out by running six commands and reading six paragraphs.
///
/// **The three states are the point and they never collapse into two.** Rule 10
/// of `Estrategia-de-Pruebas.md` on the wire: `found`, `absent` and `unreadable`
/// are three different facts about the machine, and every version of anything in
/// this project that merged the last two has been a defect. A caller that reads
/// `absent` goes and fixes something; one that reads `unreadable` knows the
/// machine did not answer, which is a different job.
fn state_object(readings: &[Reading]) -> String {
    let carried: Vec<serde_json::Value> = readings
        .iter()
        .map(|reading| {
            let (state, detail) = match &reading.outcome {
                Outcome::Found(detail) => ("found", detail),
                Outcome::Absent(detail) => ("absent", detail),
                Outcome::Unreadable(detail) => ("unreadable", detail),
            };
            serde_json::json!({
                "subject": reading.subject,
                "state": state,
                "detail": detail,
            })
        })
        .collect();

    thalyx_files::machine::answer(
        "state",
        vec![
            ("count", serde_json::json!(carried.len())),
            ("readings", serde_json::json!(carried)),
        ],
    )
}

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

/// What arrived since the human last saw the screen, and whether any of it was
/// lost before anyone looked.
struct Fresh {
    count: usize,
    /// Records fell out of the ring buffer between the last look and this one,
    /// so `count` is a floor and not a total.
    lost: bool,
    cursor: Option<u64>,
}

/// Trouble in `messages` that the human has not been told about.
///
/// Split out from [`KernelWatch`] so it can be exercised without a `/dev/kmsg`,
/// which the development container does have but a test must not depend on the
/// contents of.
fn trouble_since(messages: &[thalyx_syscall::KernelMessage], seen: Option<u64>) -> Fresh {
    let cursor = messages.iter().map(|m| m.sequence).max().or(seen);
    let count = messages
        .iter()
        .filter(|m| seen.is_none_or(|seen| m.sequence > seen) && m.is_trouble())
        .count();

    // The oldest record still in the buffer is newer than the last one accounted
    // for, so everything between them was overwritten unread. Saying "3" when it
    // was 3 of some larger number is the kind of quiet undercount this system is
    // not allowed to make.
    let lost = match (seen, messages.first()) {
        (Some(seen), Some(oldest)) => oldest.sequence > seen + 1,
        _ => false,
    };

    Fresh {
        count,
        lost,
        cursor,
    }
}

/// The prompt's half of turning the kernel's console volume down.
///
/// `init.rs` leaves only emergencies on the console, because the first real
/// machine to boot Thalyx had a USB device that would not enumerate and the
/// kernel's retries made the prompt unusable. Turning the volume down without
/// this would be hiding, which is the one thing this system is not allowed to
/// do — so the prompt says, in its own words, that there is something to look
/// at, and `nucleo` is where it is.
struct KernelWatch {
    seen: Option<u64>,
    /// Said once and then not again. A watcher that cannot read the kernel and
    /// announces it before every prompt has reinvented the problem it exists to
    /// solve.
    complained: bool,
}

impl KernelWatch {
    /// Starts from what the kernel has already said, because all of that was on
    /// the screen during the boot and announcing it again would be noise.
    fn from_now() -> Self {
        Self {
            seen: thalyx_syscall::kernel_messages()
                .ok()
                .and_then(|messages| messages.iter().map(|m| m.sequence).max()),
            complained: false,
        }
    }

    /// One line, or nothing at all when the kernel has been quiet.
    fn since_last_prompt(&mut self) -> Option<String> {
        let messages = match thalyx_syscall::kernel_messages() {
            Ok(messages) => messages,
            // Rule 10: a failure to read is not a failure to exist. A prompt
            // that silently stopped watching looks exactly like a kernel with
            // nothing to say.
            Err(error) => {
                if self.complained {
                    return None;
                }
                self.complained = true;
                return Some(format!(
                    "  !  cannot read what the kernel is saying, so this prompt \
                     is no longer watching: {error}"
                ));
            }
        };

        let fresh = trouble_since(&messages, self.seen);
        self.seen = fresh.cursor;
        if fresh.count == 0 {
            return None;
        }
        let at_least = if fresh.lost { "at least " } else { "" };
        let plural = if fresh.count == 1 { "" } else { "s" };
        Some(format!(
            "  !  {at_least}{} new kernel problem{plural}; `nucleo` shows {}",
            fresh.count,
            if fresh.count == 1 { "it" } else { "them" }
        ))
    }
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
            // Files first, and not out of politeness. There is no shell behind
            // this session, so a verb that is not on this list does not exist
            // for the person holding the machine — and the first thing anybody
            // does on a computer is look at what is on it.
            //
            // The standard names are the ones taught here, by Cesar's decision
            // of 2026-08-09: a system whose first screen offers a vocabulary
            // nobody has seen reads as a toy, and adoption is the whole reason
            // that matters. The Spanish verbs all still work.
            println!("  `ls` shows what is here, `cat <archivo>` shows what is in");
            println!("  one, `cd <carpeta>` moves, `pwd` says where you are and");
            println!("  `clear` wipes the screen. `cd` alone goes home, `ls -a`");
            println!("  includes hidden names and `ls -l` shows sizes.");
            println!("  `mkdir`, `touch`, `cp <de> <a>`, `mv <de> <a>` and");
            println!("  `rm <cosa>` change what is there. `*` and `?` work.");
            println!("  `ensayo <verbo> …` says what one of those would do");
            println!("  without doing any of it.");
            println!("  `structured on` makes every one of those answer in JSON");
            println!("  instead, for a program reading them; `structured off`");
            println!("  brings the sentences back, and `describe` lists every");
            println!("  verb this machine has, for a person or for a program.");
            println!("  `indexar` reads a tree and records what refers to what;");
            println!("  then `depende <archivo>` says what it refers to and");
            println!("  `usan <archivo>` says what refers to it — which no");
            println!("  amount of looking through folders can answer.");
            println!("  `disponibles` lists what can be installed, `instalar <id>`");
            println!("  installs one and shows what it asks for, `revertir` undoes it.");
            println!("  `modulos` lists what is installed, `correr <id>` runs one,");
            println!("  `discos` lists the disks I can see and `instalar-en <disco>`");
            println!("  puts this machine on one, so it stops needing this medium.");
            println!("  `permisos` shows what is granted, `recuerdos` says what I");
            println!("  will still know after a restart, `estado` re-reads the");
            println!("  machine, `nucleo` shows what the kernel has been saying");
            println!("  and `nucleo lento` where the boot spent its time,");
            println!("  `apagar` turns it off.");
        }
        Standing::AProgram { .. } => {
            println!("  `ls`, `cat <archivo>`, `cd <carpeta>`, `pwd`, `clear`,");
            println!("  `mkdir`, `touch`, `cp`, `mv`, `rm`, `structured on|off`,");
            println!("  `ensayo <verbo> …`, `describe`,");
            println!("  `indexar`, `depende <archivo>`, `usan <archivo>`,");
            println!("  `disponibles`, `instalar <id>`, `modulos`, `correr <id>`,");
            println!("  `permisos`, `revertir`, `recuerdos`, `estado`, `nucleo`,");
            println!("  `discos`, `instalar-en <disco>`.");
            println!("  `salir` to leave. `apagar` exists and refuses here,");
            println!("  because this machine is not mine to turn off.");
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

    let mut watch = KernelWatch::from_now();

    // Where the person is. Carried across lines because that is what makes a
    // relative name mean anything — without it every verb would need the whole
    // path typed out, which is the difference between a system somebody can work
    // in and one they can only inspect.
    let mut here = crate::files::Where::start();

    // Which face the file verbs answer in. Human until something asks otherwise,
    // because a person who has not asked for JSON must never be handed it —
    // `vault/01-Filosofia/Filosofia-Fundacional.md` requires the human keep
    // everything, and being unable to read the answers is not keeping it.
    let mut face = crate::files::Face::Human;

    // Opened once and held: raw mode is a change to the terminal that outlives
    // this program, and every transition in and out is a chance to leave it
    // broken for whoever comes next.
    let mut terminal = crate::term::Terminal::open();

    loop {
        // Before the prompt and not after it, so the notice never lands on a
        // line the human is in the middle of typing — which is the whole defect
        // this exists to answer.
        if let Some(notice) = watch.since_last_prompt() {
            println!("{notice}");
        }
        // The location is in the prompt rather than only in `donde`, because a
        // relative name means nothing without it: `leer notas.txt` reads a
        // different file depending on where the person is standing, and a prompt
        // that hid that would make the same words do different things with no
        // warning on screen.
        // Nothing at all in the structured face. A prompt is for somebody who is
        // waiting; a program reading the stream has been promised one object per
        // line, and `  /home > {"op":…}` is not one object per line — the answer
        // would have to be found inside the line before it could be parsed.
        let prompt = match face {
            crate::files::Face::Machine => String::new(),
            crate::files::Face::Human => format!("  {} > ", here.briefly()),
        };
        let at = here.at().to_path_buf();
        let line = match terminal.read_line(&prompt, |before| completions(&at, before))? {
            crate::term::Ended::Line(line) => line,
            // Ctrl-C throws the line away and gives a fresh prompt. It is not a
            // way out of the session, because on the image there is nowhere out.
            crate::term::Ended::Abandoned => continue,
            crate::term::Ended::Closed => break,
        };
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
                let readings = gather(store);
                if face == crate::files::Face::Machine {
                    println!("{}", state_object(&readings));
                } else {
                    for reading in &readings {
                        println!(
                            "{} {:<12} {}",
                            reading.mark(),
                            reading.subject,
                            reading.text()
                        );
                    }
                }
            }
            "apagar" | "poweroff" => {
                power_off(&standing);
            }
            "modules" | "modulos" | "módulos" => {
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
                if face == crate::files::Face::Machine {
                    let _ = crate::agent::recall_object(store, SESSION_TASK);
                } else {
                    show_memory(store);
                }
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
            // ─────────────────────────────────────────── files, layer 1 of the decree
            //
            // `Principio-Doble-Ruta.md`, non-negotiable, first layer: plain file
            // work without the agent. These four are the smallest set that makes
            // the rest of it possible — a person who cannot see what is there
            // cannot copy, move or delete it either.
            "clear" | "limpiar" | "cls" => {
                crate::files::clear();
            }
            // The verb the objective decree was waiting on: everything below
            // already returns facts, and this is what lets something ask for
            // them instead of for sentences about them.
            _ if starts_any(line, &["structured ", "estructurado "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::structured(&mut face, rest);
            }
            "structured" | "estructurado" => {
                crate::files::structured(&mut face, "");
            }
            // A1: the machine reading itself out loud, so that something that
            // arrived knowing nothing about Thalyx can ask instead of guessing.
            _ if starts_any(line, &["describe ", "describir "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::catalogue::describe(face, rest);
            }
            "describe" | "describir" => {
                crate::catalogue::describe(face, "");
            }
            // C1: the semantic index, reachable by something that is not
            // Thalyx's own CLI for the first time.
            _ if starts_any(line, &["indexar ", "index "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::index::build(store.root(), &here, rest, face)?;
            }
            "indexar" | "index" => {
                crate::index::build(store.root(), &here, "", face)?;
            }
            _ if starts_any(line, &["depende ", "depends "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::index::edges(store.root(), &here, rest, false, face)?;
            }
            "depende" | "depends" => {
                crate::index::edges(store.root(), &here, "", false, face)?;
            }
            _ if starts_any(line, &["usan ", "dependents "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::index::edges(store.root(), &here, rest, true, face)?;
            }
            "usan" | "dependents" => {
                crate::index::edges(store.root(), &here, "", true, face)?;
            }
            // D1: what a verb would do, without doing any of it.
            _ if starts_any(line, &["ensayo ", "rehearse "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::rehearse(&here, rest, face)?;
            }
            "ensayo" | "rehearse" => {
                crate::files::rehearse(&here, "", face)?;
            }
            "pwd" | "donde" | "dónde" | "where" => {
                crate::files::where_am_i(&here, face);
            }
            _ if starts_any(line, &["cd ", "ir ", "go "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::go(&mut here, rest, face);
            }
            // Unlike `instalar` and `correr`, the bare verb is not a question:
            // `ir` with nothing after it means home, which is somewhere a person
            // always wants to be able to get back to in one word.
            "cd" | "ir" | "go" => {
                crate::files::go(&mut here, "", face);
            }
            _ if starts_any(line, &["ls ", "ver ", "look "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::look(&here, rest, face);
            }
            "ls" | "ver" | "look" => {
                crate::files::look(&here, "", face);
            }
            _ if starts_any(line, &["cat ", "leer ", "read "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::read(&here, rest, face);
            }
            _ if starts_any(line, &["mkdir ", "crear-carpeta ", "nueva-carpeta "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::make(&here, rest, true, face)?;
            }
            _ if starts_any(line, &["touch ", "crear "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::make(&here, rest, false, face)?;
            }
            _ if starts_any(line, &["cp ", "copiar "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::transfer(&here, rest, false, face)?;
            }
            _ if starts_any(line, &["mv ", "mover ", "renombrar "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::transfer(&here, rest, true, face)?;
            }
            _ if starts_any(line, &["rm ", "borrar ", "eliminar "]) => {
                let rest = line.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                crate::files::erase(&here, rest, face)?;
            }
            "cat" | "leer" | "read" => {
                crate::files::read(&here, "", face);
            }
            // The bare forms, and they are here because a test that drove the
            // prompt found they were missing. Every arm above matches on a
            // **trailing space**, so `rm` typed alone was not a verb at all and
            // fell through to "I have no model loaded" — the same defect Cesar
            // hit with `clear` on the first real session, still alive in five
            // verbs. A common command answering with a speech about something
            // else is exactly how a system reads as unfinished.
            //
            // Each one lands on its own "which one" rather than on a shared
            // message, because `cp` needs two names and `rm` needs one, and a
            // hint that does not say which is a hint nobody can act on.
            "mkdir" | "crear-carpeta" | "nueva-carpeta" => {
                crate::files::make(&here, "", true, face)?;
            }
            "touch" | "crear" => {
                crate::files::make(&here, "", false, face)?;
            }
            "cp" | "copiar" => {
                crate::files::transfer(&here, "", false, face)?;
            }
            "mv" | "mover" | "renombrar" => {
                crate::files::transfer(&here, "", true, face)?;
            }
            "rm" | "borrar" | "eliminar" => {
                crate::files::erase(&here, "", face)?;
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
            "nucleo lento" | "núcleo lento" | "kernel slow" => {
                show_slowest();
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

/// Every verb the session answers to, in the order they are offered.
///
/// One list, used by Tab and nothing else — the dispatch still matches its own
/// arms, so this cannot make a verb *work*. That is on purpose and it is the
/// honest arrangement: a name here that the dispatch does not have completes and
/// then fails, which is visible immediately, while the reverse — a working verb
/// missing from here — costs nobody anything.
/// The verbs offered when tab is pressed at the start of a line.
///
/// Generated from [`crate::catalogue`] rather than listed again here. It used to
/// be a second copy of the same fact, and a verb added to one and not the other
/// is a verb tab could never find.
fn verbs() -> Vec<String> {
    crate::catalogue::every_name()
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// What could follow what has been typed so far.
///
/// The first word is a verb and everything after it is a path, which is the
/// whole rule. Offering file names where a verb belongs would put `Documentos`
/// at the start of a line, where nothing can run it.
fn completions(here: &std::path::Path, before: &str) -> Vec<String> {
    if !before.contains(' ') {
        return verbs();
    }

    let fragment = before.rsplit(' ').next().unwrap_or("");
    // The directory being completed *in* is whatever the fragment names up to
    // its last slash, so `cat Documentos/no` looks inside `Documentos`.
    let (folder, partial) = match fragment.rsplit_once('/') {
        Some((folder, partial)) => (thalyx_files::resolve(here, folder), partial.to_string()),
        None => (here.to_path_buf(), fragment.to_string()),
    };
    let prefix = &fragment[..fragment.len() - partial.len()];

    let Ok(listing) = thalyx_files::list(&folder) else {
        return Vec::new();
    };
    listing
        .entries
        .iter()
        // Hidden names appear only once the person has typed the dot that says
        // they want them, which is the same rule `ls` follows and for the same
        // reason: thirty-five of them would bury every real answer.
        .filter(|entry| partial.starts_with('.') || !thalyx_files::is_hidden(&entry.name))
        .map(|entry| {
            let name = entry.name.to_string_lossy();
            // The slash matters: it is what lets a second Tab descend instead of
            // stopping at the folder.
            let tail = if entry.kind == thalyx_files::Kind::Directory {
                "/"
            } else {
                ""
            };
            format!("{prefix}{name}{tail}")
        })
        .collect()
}

/// Whether a line opens with any of these verbs, so a verb can have more than
/// one name without the dispatch growing a clause per spelling.
///
/// Cesar decided on 2026-08-09 that the standard names lead and the Spanish ones
/// keep working: somebody who arrives from Linux types `ls` and it answers, and
/// somebody who learned Thalyx's own words is not made to unlearn them. A name
/// is not a foreign program — every one of these is the same Rust inside
/// `thalyx`, and `make -C image count` still says one.
fn starts_any(line: &str, verbs: &[&str]) -> bool {
    verbs.iter().any(|verb| line.starts_with(verb))
}

/// What is on a partition right now, read off the disk rather than remembered.
///
/// Its own function because two places need the same sentence and they need it for
/// different reasons: `discos` so a person can tell their disks apart, and
/// `instalar-en` so the thing about to be destroyed is named **before** the
/// question is asked. Those two drifting apart is how a disk gets lost — the list
/// says `btrfs "fedora"` and the confirmation says nothing at all.
fn whats_on(path: &std::path::Path) -> String {
    match thalyx_btrfs::identify(path) {
        Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) if label == thalyx_btrfs::LABEL => {
            "a Thalyx store".to_string()
        }
        Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) if label.is_empty() => {
            "btrfs, no label".to_string()
        }
        Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) => format!("btrfs `{label}`"),
        // Not Btrfs, so ask the FAT reader before giving up. Thalyx writes exactly
        // one FAT volume — the boot partition of everything it installs — and on
        // 2026-08-07 it described its own, on the medium it was running from, as
        // "something I do not recognise". A machine that cannot name its own work
        // has no business naming anybody else's.
        _ => match thalyx_install::medium::Volume::open(path) {
            Ok(Some(mut volume)) => match volume.label() {
                Ok(label) if label == thalyx_install::fat::LABEL => {
                    "a Thalyx boot partition".to_string()
                }
                Ok(label) if label.trim().is_empty() => "FAT, no label".to_string(),
                Ok(label) => format!("FAT `{}`", label.trim()),
                // Rule 10: the volume is there and something about reading it
                // failed, which is not the same as there being nothing.
                Err(_) => "FAT I could not read the label of".to_string(),
            },
            // Everything that is neither Btrfs nor FAT reads the same from here, and
            // saying "not btrfs" would read as "empty" about a disk somebody is
            // deciding whether to destroy.
            _ => "something I do not recognise".to_string(),
        },
    }
}

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
            //
            // That property was asserted here and did not exist until 2026-08-07,
            // when Cesar's own machine printed seven disks of which four were
            // partitions — including 444 GiB of his Fedora, offered under a line
            // saying everything on it is lost. `of` looked up
            // /sys/dev/block/<major>:<minor>, which a partition has too, found no
            // children carrying a `partition` file, and returned `Ok([])`. **A
            // comment claiming a property is not the property**, and the one thing
            // that could tell was a machine with partitions on it.
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
            println!("      {number}  {}", whats_on(path));
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

    // What is there now, named before the question rather than after it. `thalyx
    // install` on the host has always done this and the session's verb did not,
    // which mattered the day `discos` ran on a machine with another operating
    // system on it: the list said `btrfs "fedora"` and the confirmation said only
    // "everything will be gone", leaving the human to carry the mapping from a
    // device name to a system in their head. Read off the disk, so it describes
    // the disk being destroyed and not the one that was listed a minute ago.
    match thalyx_install::partitions::of(&disk) {
        Ok(existing) if !existing.is_empty() => {
            println!("  it has {} partition(s) on it now:", existing.len());
            for (number, path) in &existing {
                println!("    {number}  {}", whats_on(path));
            }
            println!();
        }
        Ok(_) => {
            println!("  it has no partitions on it now.");
            println!();
        }
        // Rule 10 again, and it matters most here: not being able to look is not
        // the same as there being nothing to lose, and the difference decides
        // whether somebody should go and check before answering.
        Err(error) => {
            println!("  I could not read what is on it: {error}");
            println!("  That is not the same as it being empty.");
            println!();
        }
    }

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
    let answer = crate::term::read_answer()
        .ok()
        .flatten()
        .unwrap_or_default();
    if answer.trim() != disk.display().to_string() {
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

    /// One record, shaped like the kernel's own.
    fn said(sequence: u64, priority: u8) -> thalyx_syscall::KernelMessage {
        thalyx_syscall::KernelMessage {
            priority,
            sequence,
            seconds: sequence as f64,
            text: format!("record {sequence}"),
        }
    }

    /// One record at a chosen second, so a gap is a thing a test can state.
    fn at(sequence: u64, seconds: f64, text: &str) -> thalyx_syscall::KernelMessage {
        thalyx_syscall::KernelMessage {
            priority: 6,
            sequence,
            seconds,
            text: text.to_string(),
        }
    }

    #[test]
    fn the_longest_silence_of_the_boot_comes_first() {
        // The question this answers is "a machine took forty seconds to reach its
        // prompt, where did they go". Anything but longest-first buries it.
        let gaps = slowest_gaps(
            &[
                at(1, 0.0, "start"),
                at(2, 0.5, "quick"),
                at(3, 30.5, "the slow one finished"),
                at(4, 31.0, "quick again"),
            ],
            8,
        );
        assert_eq!(gaps.len(), 3);
        assert!(
            (gaps[0].seconds - 30.0).abs() < 0.001,
            "{}",
            gaps[0].seconds
        );
        assert_eq!(gaps[0].before, "quick");
        assert_eq!(gaps[0].after, "the slow one finished");
    }

    #[test]
    fn both_sides_of_a_silence_are_kept_because_either_alone_misleads() {
        // The line *after* a gap is the one that finished waiting, and it is the
        // one a person will blame. The line before is where the waiting started.
        // Reporting one without the other sends somebody to the wrong half, which
        // is the whole failure mode of reading a boot log by eye.
        let gaps = slowest_gaps(&[at(1, 1.0, "asked"), at(2, 21.0, "answered")], 8);
        assert_eq!(gaps[0].before, "asked");
        assert_eq!(gaps[0].after, "answered");
        assert!(
            (gaps[0].at - 1.0).abs() < 0.001,
            "the gap starts where it starts"
        );
    }

    #[test]
    fn a_boot_with_nothing_to_report_produces_no_gaps_rather_than_a_panic() {
        // The control, and the edge that would take down the verb: `windows(2)` on
        // fewer than two records yields nothing, and a machine whose ring buffer
        // was wiped must print "no gaps" rather than crash the session.
        assert!(slowest_gaps(&[], 8).is_empty());
        assert!(slowest_gaps(&[at(1, 0.0, "only one")], 8).is_empty());
    }

    #[test]
    fn a_problem_that_arrived_while_the_console_was_quiet_is_announced() {
        // The whole point. With the console at emergencies only, an error like
        // the USB timeout that made the first real machine's prompt unusable
        // never reaches the screen, so the prompt has to say it is there.
        let fresh = trouble_since(&[said(1, 6), said(2, 3)], Some(1));
        assert_eq!(fresh.count, 1, "the error after the cursor was not counted");
        assert_eq!(fresh.cursor, Some(2));
        assert!(!fresh.lost);
    }

    #[test]
    fn what_the_human_already_read_during_the_boot_is_not_announced_again() {
        // Without this the first prompt of every boot would report every error
        // the machine printed on its way up, which the human just watched go by.
        let messages = [said(1, 3), said(2, 3)];
        let fresh = trouble_since(&messages, Some(2));
        assert_eq!(fresh.count, 0, "already-seen trouble was announced again");
    }

    #[test]
    fn something_that_merely_happened_does_not_interrupt_the_prompt() {
        // The control. A watcher that counted every record would fire on every
        // prompt on a healthy machine, and a notice that is always there is one
        // nobody reads — which is the defect being fixed, rebuilt one level up.
        let fresh = trouble_since(&[said(9, 5), said(10, 6), said(11, 7)], Some(8));
        assert_eq!(fresh.count, 0, "notices and info were treated as trouble");
        assert_eq!(fresh.cursor, Some(11), "the cursor has to move anyway");
    }

    #[test]
    fn records_lost_to_the_ring_buffer_make_the_count_a_floor_and_say_so() {
        // The kernel overwrites its oldest records. A machine that was loud
        // enough to wrap the buffer is exactly the one where "3 problems" would
        // be an undercount presented as a total.
        let fresh = trouble_since(&[said(50, 3), said(51, 3)], Some(2));
        assert_eq!(fresh.count, 2);
        assert!(
            fresh.lost,
            "records between the cursor and the oldest survivor went unread, \
             and the count was reported as if it were all of them"
        );
    }

    #[test]
    fn a_first_look_with_no_cursor_counts_what_is_there_without_claiming_loss() {
        // Only reachable when reading /dev/kmsg failed at session start. There
        // is nothing to have lost yet, and reporting loss would send somebody
        // looking for messages that never existed.
        let fresh = trouble_since(&[said(1, 3), said(2, 6)], None);
        assert_eq!(fresh.count, 1);
        assert!(!fresh.lost);
    }

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
