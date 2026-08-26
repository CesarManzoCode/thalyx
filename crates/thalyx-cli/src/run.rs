//! `thalyx module run` — the human's route to running a module confined.
//!
//! Everything of substance is in `thalyx_core::run`. This file is the CLI's
//! half: it collects arguments, hands them over, and reports what happened
//! plainly enough that "confined" and "unconfined" cannot be confused.

use crate::files::Face;
use serde_json::json;
use std::ffi::OsString;
use thalyx_core::Store;
use thalyx_journal::Origin;
use thalyx_permd::KernelStore;

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// One request to run a module, as one value.
///
/// Gathered into a struct when adding the face made it eight loose arguments.
/// Seven positional arguments of which three are strings and two are booleans is
/// a call nobody can read at the call site, and the way it breaks is a
/// transposition the compiler accepts: `profile` and `entrypoint` are both
/// `&str`, and swapping them produces a run that fails somewhere much later.
pub struct Asked<'a> {
    pub root: &'a std::path::Path,
    pub module_id: &'a str,
    pub profile: &'a str,
    pub entrypoint: &'a str,
    pub args: Vec<OsString>,
    pub unconfined: bool,
    pub request_id: String,
    pub face: Face,
}

pub fn run(asked: Asked<'_>) -> Fallible {
    let Asked {
        root,
        module_id,
        profile,
        entrypoint,
        args,
        unconfined,
        request_id,
        face,
    } = asked;

    let store = Store::open(root)?;
    let policies = KernelStore::default_map();

    // The helper is this binary. It re-executes itself into the module's
    // cgroup and only then becomes the module, so the module's first
    // instruction runs in a process that is already confined.
    let helper = std::env::current_exe()?;

    let outcome = thalyx_core::run(
        &store,
        &policies,
        thalyx_core::RunRequest {
            module_id,
            profile,
            entrypoint,
            args,
            helper,
            request_id,
            origin: Origin::UserUtterance,
            unconfined,
        },
    )?;

    if face.is_machine() {
        say_it(&outcome);
        return Ok(());
    }

    println!();
    println!("{} {}", outcome.module_id, outcome.version);
    println!("  ran: {}", outcome.program.display());

    match (outcome.cgroup_id, outcome.policy) {
        (Some(id), Some(policy)) => {
            println!("  confined to cgroup {id}, allowed=0x{:x}", policy.allowed);
            if let Some(isolation) = &outcome.isolation {
                println!("  {isolation}");
            }
            if let Some(uid) = outcome.uid {
                println!("  ran as user {uid}, which is this module's and no other's");
            }
            for permission in &outcome.permissions {
                println!("    {}", permission.describe());
            }
            if outcome.permissions.is_empty() {
                println!("    (no permissions; every guarded operation is denied)");
            }
            match &outcome.enforcement {
                Some(thalyx_permd::Enforcement::Enforcing) | None => {}
                Some(mode) => {
                    println!();
                    println!("  WARNING: the kernel side is {}.", mode.describe());
                    println!("  The policy above was written and nothing applied it. Make it");
                    println!("  binding with `negar`.");
                }
            }
            if !outcome.isolated {
                println!();
                println!("  WARNING: this profile isolates nothing beyond the cgroup.");
                println!("  The journal records this run as degraded.");
            }
        }
        _ => {
            println!("  RAN UNCONFINED — nothing enforced its permissions.");
            println!("  The journal records this run as degraded.");
        }
    }

    // What the module said over its channel.
    //
    // Printed by Thalyx and not by the module, which is the whole arrangement:
    // a module has no terminal, and everything it wants a human to see passes
    // through here. That is also why the marker says who is speaking — text
    // from a module must never be able to look like Thalyx talking.
    //
    // And why every line goes through `sanitise`. Routing the text through
    // Thalyx accomplishes nothing on its own if the text may then contain a
    // newline and repaint the marker, or an escape sequence and repaint the
    // screen. The marker is only a marker if the module cannot draw one.
    if !outcome.said.is_empty() {
        println!();
        println!(
            "  {} said:",
            thalyx_core::trusted_path::sanitise(&outcome.module_id)
        );
        for (level, text) in &outcome.said {
            let marker = match level {
                thalyx_abi::Level::Info => " ",
                thalyx_abi::Level::Warning => "!",
                thalyx_abi::Level::Error => "x",
            };
            for line in thalyx_core::trusted_path::sanitise_block(text) {
                println!("  {marker} {line}");
            }
        }
    }

    // What the module wrote at its own descriptors.
    //
    // Reported apart from the channel, and labelled as the different thing it
    // is: the channel is the surface Thalyx mediates, and this is bytes at a
    // descriptor. Saying so is the honest half — a module writing `granted=
    // reachable` has told nobody anything Thalyx checked.
    //
    // Every line still goes through `sanitise_block` and still carries a
    // marker, which is the property that matters and the only one the null
    // device was buying: a module must not be able to draw a line that looks
    // like Thalyx drew it. What it may not do is speak *unmarked*; what it may
    // do is speak.
    if !outcome.wrote.is_empty() {
        println!();
        println!(
            "  {} wrote, at descriptors Thalyx does not mediate:",
            thalyx_core::trusted_path::sanitise(&outcome.module_id)
        );
        // `>` is stdout and `!` is stderr. Kept apart because they arrived on
        // separate pipes: any interleaving would be one Thalyx invented.
        for (marker, text) in [(">", &outcome.wrote.stdout), ("!", &outcome.wrote.stderr)] {
            if text.is_empty() {
                continue;
            }
            for line in thalyx_core::trusted_path::sanitise_output(text) {
                println!("  {marker} {line}");
            }
        }
        if outcome.wrote.truncated {
            println!("  … and more, past what Thalyx keeps of one run's output.");
        }
    }

    // A module that said more than Thalyx will hold has to be reported as
    // such. A list that silently stopped growing looks exactly like a module
    // that stopped talking, and the two are different events.
    if outcome.dropped_notices > 0 {
        println!(
            "  … and {} more notice(s), past what Thalyx keeps for one run.",
            outcome.dropped_notices
        );
    }

    if let Some(error) = &outcome.channel_error {
        println!();
        println!("  the module's channel to Thalyx broke: {error}");
        println!("  anything it asked for after that point did not happen.");
    }

    println!();
    match outcome.exit_code {
        Some(0) => println!("  exited cleanly"),
        Some(code) => println!("  exited with status {code}"),
        None => println!("  terminated by a signal"),
    }

    Ok(())
}

/// One run, as one object.
///
/// ## Why nothing here goes through `sanitise`
///
/// The human face routes every line a module produced through
/// `trusted_path::sanitise_block`, and the reason is written there: routing a
/// module's text through Thalyx accomplishes nothing if that text can then
/// contain a newline and repaint the marker, or an escape sequence and repaint
/// the screen. **The marker is only a marker if the module cannot draw one.**
///
/// On this face the marker is not drawn, it is structural: the module's text is
/// the value of a named field of an object, and `serde_json` escapes every
/// control character on the way out. A module cannot end the object early, cannot
/// start a second one, and cannot move a byte from `wrote` into `said` — the
/// framing that `sanitise` is defending in the terminal is defended here by the
/// encoding, and it does not depend on anyone remembering to call a function.
///
/// So the bytes are carried through as they were written. That is the tie-break
/// rule of `Superficie-para-el-LLM.md` applied honestly: a caller asking what a
/// module wrote is asking what it wrote, and handing it a cleaned copy would be
/// answering a different question. The human keeps the sanitised route, which is
/// the one where the risk actually lives.
fn say_it(outcome: &thalyx_core::run::RunOutcome) {
    const OP: &str = "run";

    let said: Vec<serde_json::Value> = outcome
        .said
        .iter()
        .map(|(level, text)| {
            json!({
                "level": match level {
                    thalyx_abi::Level::Info => "info",
                    thalyx_abi::Level::Warning => "warning",
                    thalyx_abi::Level::Error => "error",
                },
                "text": text,
            })
        })
        .collect();

    println!(
        "{}",
        thalyx_files::machine::answer(
            OP,
            vec![
                ("module_id", json!(outcome.module_id)),
                ("version", json!(outcome.version)),
                ("program", json!(outcome.program.display().to_string())),
                // The one field that decides whether anything else here can be
                // believed as enforcement. `confined: false` and a policy that
                // denied nothing are the same run, and the journal calls it
                // degraded — so a caller must not have to derive it from the
                // absence of a cgroup id.
                ("confined", json!(outcome.cgroup_id.is_some())),
                // Beside `confined`, and for its reason. A confined run under
                // an observing kernel and a confined run under an enforcing
                // one are the same JSON without this, and they are not the
                // same run.
                (
                    "enforcing",
                    json!(matches!(
                        outcome.enforcement,
                        Some(thalyx_permd::Enforcement::Enforcing)
                    ))
                ),
                (
                    "enforcement",
                    json!(outcome.enforcement.as_ref().map(|mode| mode.describe()))
                ),
                ("cgroup_id", json!(outcome.cgroup_id)),
                ("isolated", json!(outcome.isolated)),
                ("isolation", json!(outcome.isolation)),
                ("uid", json!(outcome.uid)),
                (
                    "permissions",
                    json!(
                        outcome
                            .permissions
                            .iter()
                            .map(|permission| permission.describe())
                            .collect::<Vec<_>>()
                    )
                ),
                ("said", json!(said)),
                // `wrote` apart from `said`, and named for what it is. The
                // channel is the surface Thalyx mediates; this is bytes at a
                // descriptor, and a module writing `granted=reachable` there has
                // told nobody anything Thalyx checked.
                (
                    "wrote",
                    json!({
                        // Kept apart because they arrived on separate pipes: any
                        // interleaving would be one Thalyx invented.
                        "stdout": outcome.wrote.stdout,
                        "stderr": outcome.wrote.stderr,
                        "truncated": outcome.wrote.truncated,
                    })
                ),
                // Output that silently stopped growing and a module that stopped
                // talking look identical, so the count is said out loud.
                ("dropped_notices", json!(outcome.dropped_notices)),
                (
                    "channel_error",
                    json!(outcome.channel_error.as_ref().map(|e| e.to_string()))
                ),
                // `null` is *terminated by a signal*, and it is a third answer
                // rather than a missing one — which is why it is written out
                // instead of left off when there is no code.
                ("exit_code", json!(outcome.exit_code)),
            ],
        )
    );
}

/// `ensayo correr <id>` — D1's last hole, closed on 2026-08-26.
///
/// The reason it stayed open was written next to it and was true when it was
/// written: what a run would be allowed to do is a question for the kernel
/// side, and answering it from the manifest would describe a run the machine
/// may not be able to give. That stopped being true on 2026-08-25, when Thalyx
/// learned to read the mode.
///
/// Nothing here works anything out. `thalyx_core::foresee_run` is the code that
/// would do the run, stopped one line before the program exists — so a
/// rehearsal that disagreed with the verb would be the verb changing, not this.
pub fn foresee(asked: Asked<'_>) -> Fallible {
    const OP: &str = "rehearse";

    let Asked {
        root,
        module_id,
        profile,
        entrypoint,
        args,
        unconfined,
        request_id,
        face,
    } = asked;

    let store = Store::open(root)?;
    let policies = KernelStore::default_map();

    let foreseen = match thalyx_core::foresee_run(
        &store,
        &policies,
        &thalyx_core::RunRequest {
            module_id,
            profile,
            entrypoint,
            args,
            // Never used: this stops before anything is spawned. Named rather
            // than left out because `RunRequest` is the request the run takes,
            // and a rehearsal built on a different request would be answering
            // about a different run.
            helper: std::env::current_exe()?,
            request_id,
            origin: Origin::UserUtterance,
            unconfined,
        },
    ) {
        Ok(foreseen) => foreseen,
        Err(error) => {
            // A module that cannot be resolved is not a run that would be
            // refused — it is a question with no subject. Two different words,
            // because the caller's next move differs.
            if face.is_machine() {
                face.say(thalyx_files::machine::declined(
                    OP,
                    "cannot",
                    &error.to_string(),
                ));
            } else {
                println!();
                println!("  {error}");
                println!();
            }
            return Ok(());
        }
    };

    if face.is_machine() {
        let holds: Vec<serde_json::Value> = foreseen
            .permissions
            .iter()
            .map(|permission| {
                json!({
                    "resource": permission.resource,
                    "action": permission.action,
                    "kind": format!("{:?}", permission.kind).to_lowercase(),
                })
            })
            .collect();

        face.say(thalyx_files::machine::answer(
            OP,
            vec![
                ("verb", json!("run")),
                ("module_id", json!(foreseen.module_id)),
                ("version", json!(foreseen.version)),
                ("program", json!(foreseen.program.display().to_string())),
                ("would_run", json!(foreseen.would_run)),
                // Rule 10 reaches the wire: three states, not two. A kernel
                // whose mode could not be read is neither denying nor watching,
                // and a caller that saw `null` for both cases could not tell a
                // machine with nothing loaded from one that would not answer.
                (
                    "enforcement",
                    match &foreseen.enforcement {
                        None => json!(null),
                        Some(thalyx_permd::Enforcement::Enforcing) => json!("enforcing"),
                        Some(thalyx_permd::Enforcement::Observing) => json!("observing"),
                        Some(thalyx_permd::Enforcement::Unreadable(_)) => json!("unreadable"),
                    },
                ),
                ("degraded", json!(foreseen.degraded)),
                ("unconfined", json!(foreseen.unconfined)),
                ("isolation", json!(foreseen.isolation)),
                ("isolates", json!(foreseen.isolates)),
                ("own_user", json!(foreseen.own_user)),
                ("holds", json!(holds)),
                ("count", json!(foreseen.permissions.len())),
                ("refusal", json!(foreseen.refusal)),
            ],
        ));
        return Ok(());
    }

    println!();
    println!("{} {}", foreseen.module_id, foreseen.version);
    println!("  would run: {}", foreseen.program.display());
    println!("  {}", foreseen.isolation);
    if foreseen.own_user {
        println!("  as a user of its own");
    }
    if foreseen.permissions.is_empty() {
        println!("  holding nothing; every guarded operation would be denied");
    } else {
        println!("  holding:");
        for permission in &foreseen.permissions {
            println!("    {}", permission.describe());
        }
    }

    match &foreseen.refusal {
        Some(why) => {
            println!();
            println!("  It would not start: {why}");
        }
        None if foreseen.degraded => {
            println!();
            // The whole reason this rehearsal is worth having. "It would run"
            // and "it would run with nothing enforcing it" are the same
            // sentence to anyone who is not told, and this is the moment where
            // being told still costs nothing.
            println!("  WARNING: it would run degraded. The policy above would be");
            println!("  written and nothing would apply it. `negar` makes it binding.");
        }
        None => {}
    }
    println!();
    println!("  Nothing ran.");
    println!();
    Ok(())
}
