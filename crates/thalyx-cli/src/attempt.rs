//! `intento` — begin something, and be able to take all of it back.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D2**, and the
//! sentence [[Filosofia-Fundacional]] uses for the advantage no other operating
//! system has: *«intenta esto y si sale mal deshazlo»*.
//!
//! The reasoning lives in [`thalyx_core::attempt`], where it can be exercised on
//! a filesystem that is not Btrfs. What is here is the two faces and the one
//! decision that cannot be in the core: **who is asked before a tree is
//! replaced.** `Camino-Confiable` puts the confirmation in the layer that can
//! talk to a terminal, because a core that prompted could be made to prompt by
//! something that is not a human.
//!
//! ## Why abandoning asks and keeping does not
//!
//! Keeping touches nothing on the live tree; it only lets a snapshot go.
//! Abandoning replaces the tree, and the tree is **shared** — the person may
//! have written in it while the attempt was open, and their work is not the
//! agent's to discard because the agent changed its mind. So both faces are
//! shown exactly what would be lost before anything moves, and neither is
//! allowed to proceed on the first word.
//!
//! For a program that is one exchange and not a prompt: the first `abandonar`
//! answers with the difference and `done: false`, and the caller repeats it with
//! `si` to go ahead. It has seen the cost in the object it is answering, which
//! is what makes it a confirmation rather than a rubber stamp.
//!
//! ## Why a program may now say the whole thing at once
//!
//! Because on the machine face that second call was never a confirmation.
//! `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md` records three real runs of
//! the reversible benchmark, and in **all three** `abandonar` is followed
//! immediately by `abandonar si` with no call in between: the agent looked at
//! nothing, and the repeat carried no authority the first call had not already
//! given it. That is the same caller echoing itself, which is exactly the rubber
//! stamp the paragraph above says this is not.
//!
//! So there is a second way to say yes, and it is stronger rather than weaker —
//! name the attempt by the snapshot `empezar` answered with, and state **which
//! state of the workspace** is being authorised for destruction:
//!
//! ```text
//! intento abandonar snapshot=<snapshot> state=<witness>
//! ```
//!
//! It proceeds only if the attempt named is the one on record **and** the
//! workspace is, at the instant of the destruction, exactly the state named. A
//! caller that opened the attempt has both without asking: `empezar` answers
//! with the snapshot and with the witness, and so does every answer that
//! withholds consent. So the ordinary case is one call.
//!
//! ## Why it is a witness and not two counts, since 2026-08-29
//!
//! It was two counts — `delete=<N> revert=<M>` — for one day. The argument was
//! that a person writing in the shared tree would move one of them, so a stale
//! claim would stop matching and nothing would be destroyed.
//!
//! **The argument was wrong, and wrong in the direction that loses work.** A
//! person who edits a file the agent had *already* edited moves neither number:
//! one modified file before, one modified file after. The claim still matched,
//! the abandon proceeded, and their edit went back to the snapshot. A count is
//! a summary and a summary is not an identity, which is now written down in
//! `vault/03-Primitivas/Identidad-de-Estado.md` and asserted in
//! `thalyx-snapshot/tests/state_identity.rs` as the counterexample it came from.
//!
//! What replaces it is `thalyx_snapshot::Witness`: a digest over every path in
//! the tree with its size, its modification time, its change time and its inode
//! number. Any write to any file moves it. And it is checked **inside the
//! lock**, immediately before the tree is replaced, because a check made
//! outside one is a comparison with a moment after it.
//!
//! The two-step is untouched, so a caller that guessed wrong pays exactly what
//! it always paid, and never more.
//!
//! ## Why the native backend and not the `btrfs` command
//!
//! Because this verb runs inside Thalyx, where there is no `btrfs` to run. On
//! 2026-08-28 `make -C image agent` created the workspace as a real subvolume
//! and `thalyx_attempt` still answered `not_a_subvolume`: the spawn failed, and
//! `thalyx_snapshot::Btrfs` has no way to say *I could not ask* — so a missing
//! binary was reported as a fact about the filesystem, on the one verb the
//! design leans on. `thalyx_snapshot::Native` asks the kernel instead, and every
//! `Snapshots` built here uses it.
//!
//! ## What this container cannot check
//!
//! Btrfs. The policy — which attempt is open, what a second one does, what an
//! abandon aims at, what happens when the snapshot is gone — is covered by
//! `thalyx_core::attempt` against the directory fake. What only Cesar's machine
//! can exercise is that the snapshot is atomic, that it costs nothing, and that
//! the swap is a real `RENAME_EXCHANGE`. Here there is no Btrfs at all, so what
//! runs is the refusal — and that the refusal is a refusal, and not a copy
//! pretending to be a snapshot, is itself worth a test.

use crate::files::{Face, Where};
use serde_json::json;
use std::path::{Path, PathBuf};
use thalyx_core::Store;
use thalyx_core::attempt::{self, Open};
use thalyx_snapshot::{Native, Snapshots, Volumes};

type Fallible = Result<(), Box<dyn std::error::Error>>;

const OP: &str = "attempt";

/// Why a place cannot be attempted on.
pub enum NotHere {
    /// Where the session stands is not itself a subvolume.
    NotASubvolume,
    /// It is a subvolume, and it is the root of the running system.
    TheWholeSystem,
}

/// The subvolume an attempt would be about: **exactly** where the session
/// stands, and never an ancestor of it.
///
/// ## The defect this replaced, which is the worst one this project has had
///
/// This walked upwards. The argument was that an attempt is about the tree
/// somebody is working in and Thalyx has more than one subvolume, so finding
/// the nearest one above seemed helpful. On 2026-08-10 it ran on Cesar's Fedora
/// machine from a directory under `/tmp`, walked past every level of it, and
/// stopped at the first subvolume it found — **`/`**. It took a read-only
/// snapshot of his entire root filesystem and reported that abandoning would
/// delete 1,343,582 files, including `/boot`.
///
/// Nothing was destroyed, because that test never abandoned. What it destroyed
/// was the argument: walking upwards leaves the scope the caller had in mind
/// **silently**, and on every ordinary Btrfs install the walk terminates at the
/// most dangerous possible answer. A verb that can replace a whole subvolume
/// must never choose which one by searching.
///
/// So: where you stand, or nothing. It costs a `cd` and it cannot surprise
/// anybody.
///
/// ## And `/` is refused even when you stand in it
///
/// Not because the snapshot would fail — it succeeds, which is the problem —
/// but because abandoning it means swapping the root of the running system out
/// from under every process on the machine, this one included. That is not a
/// thing Thalyx should be able to be asked for by a session verb, whoever asks.
fn subvolume_to_attempt<V: Volumes>(volumes: &V, here: &Path) -> Result<PathBuf, NotHere> {
    if !volumes.is_subvolume(here).unwrap_or(false) {
        return Err(NotHere::NotASubvolume);
    }
    // Compared after canonicalising, so `/.` and `/home/..` are the same refusal
    // as `/`. A check on the string alone is a check somebody gets around by
    // accident.
    let real = here.canonicalize().unwrap_or_else(|_| here.to_path_buf());
    if real == Path::new("/") {
        return Err(NotHere::TheWholeSystem);
    }
    Ok(real)
}

impl NotHere {
    /// The stable word the refusal carries.
    pub fn word(&self) -> &'static str {
        match self {
            NotHere::NotASubvolume => "not_a_subvolume",
            NotHere::TheWholeSystem => "the_whole_system",
        }
    }

    /// What it says, in the words this verb has always said it in.
    pub fn message(&self, here: &Path) -> String {
        match self {
            NotHere::NotASubvolume => format!(
                "{} is not itself a Btrfs subvolume, so there is nothing to come back \
                 to and nothing was started. A boundary is about the subvolume you are \
                 standing in — it never looks upwards for one",
                here.display()
            ),
            NotHere::TheWholeSystem => "a boundary on / would mean swapping the root of the \
                 running system out from under every process on it, this one included. \
                 Nothing was started"
                .to_string(),
        }
    }
}

/// The subvolume a boundary opened here would be about, for a verb that is not
/// this one.
///
/// `hacer` opens the same boundary `intento` does and must choose the same tree
/// by the same rule — where the session stands, exactly, never an ancestor.
/// Exported rather than reimplemented because the rule is the one this file's
/// longest comment is about, and a second copy of it is a second chance to
/// re-learn 2026-08-10 by taking a snapshot of somebody's root filesystem.
pub fn subvolume_for(here: &Path) -> Result<PathBuf, NotHere> {
    subvolume_to_attempt(&Native, here)
}

fn declined(face: Face, word: &str, why: &str) {
    if face == Face::Machine {
        face.say(thalyx_files::machine::declined(OP, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

/// An open attempt, as both faces describe it.
fn open_fields(open: &Open) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("attempt", json!(open.label)),
        ("snapshot", json!(open.snapshot)),
        ("subvolume", json!(open.subvolume.display().to_string())),
        ("since", json!(open.started_at)),
    ]
}

/// The workspace's exact identity, as every answer that mentions one spells it.
///
/// One helper rather than the field written out at each site, because a caller
/// that has to authorise a destruction with this string must find it spelled
/// the same way everywhere it appears — and because the incomplete case has to
/// be *visible*: a tree that could not be read everywhere has no exact identity,
/// and an answer that carried a digest without saying so would invite a caller
/// to authorise a destruction on the strength of a walk that had holes in it.
fn state_fields(state: &thalyx_snapshot::Witness) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "state",
            if state.is_complete() {
                json!(state.id)
            } else {
                json!(null)
            },
        ),
        ("state_files", json!(state.files)),
        ("state_unreadable", json!(state.unreadable)),
        // What the identity cost to take, said rather than left to be measured.
        // Since 2026-08-29 a state check reads what every file holds, because a
        // witness made of timestamps cannot separate two writes inside one
        // filesystem tick — and an identity that is cheap and wrong authorises
        // destroying somebody's work. A caller that finds this number large
        // knows why its rollback is not instant.
        ("state_bytes", json!(state.bytes)),
    ]
}

/// What a difference costs, in the words both faces use.
fn cost_fields(difference: &thalyx_snapshot::Difference) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        // The number that decides the answer. A file made since the attempt
        // began has no older version to go back to: abandoning does not revert
        // it, it deletes it.
        ("would_delete", json!(difference.added_total)),
        ("would_revert", json!(difference.modified_total)),
        ("would_bring_back", json!(difference.removed_total)),
        // Counted as differences, because what could not be compared must not
        // be reported as identical.
        ("uncomparable", json!(difference.unreadable.len())),
        ("would_delete_named", json!(difference.added)),
        ("would_revert_named", json!(difference.modified)),
        // The third list, added on 2026-08-28 when the external agent bridge
        // made this the answer to "what have I changed since I began". Two of
        // the three kinds were named and the third was only counted, so an agent
        // asking what it had done was told about the files it made and the files
        // it edited and only *how many* it had deleted — which is the one of the
        // three a reviewer most needs to see by name.
        ("would_bring_back_named", json!(difference.removed)),
    ]
}

/// What a caller said when it asked to abandon.
///
/// Read off the words and not off a joined line, because two of these are
/// numbers: a claim about what a destruction costs that was parsed loosely is
/// worse than no claim at all.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    /// The attempt the caller believes it is settling, named by the snapshot it
    /// goes back to — which is the field `empezar` answered with, spelled the
    /// same way, so that stating it is a copy and never a translation.
    snapshot: Option<String>,
    /// The exact state of the workspace it is authorising the destruction of,
    /// spelled the way `thalyx_snapshot::Witness` spells one.
    state: Option<String>,
    /// The bare `si` of the two-step protocol.
    said_yes: bool,
}

/// Read `snapshot=`, `delete=` and `revert=` off the words, or say which one is
/// not a count.
///
/// A malformed number is refused rather than dropped. Dropping it would leave a
/// caller believing it had stated the cost when it had not, and the answer it
/// got — the cost object again — would look like the tree had changed under it.
fn asked_of_abandon(given: &[crate::words::Word]) -> Result<Asked, String> {
    let mut asked = Asked::default();
    for word in given.iter().map(crate::words::Word::as_str) {
        let Some((key, value)) = word.split_once('=') else {
            asked.said_yes |= matches!(word, "si" | "sí" | "yes" | "y");
            continue;
        };
        match key {
            "snapshot" | "instantanea" if !value.is_empty() => {
                asked.snapshot = Some(value.to_string());
            }
            "state" | "estado" if !value.is_empty() => asked.state = Some(value.to_string()),
            // Refused rather than ignored, and this is the retired protection
            // saying so out loud. A caller still spelling the counts is a caller
            // running against the rules of 2026-08-28, under which a third
            // party's edit to a file the agent had already edited authorised
            // itself. Dropping the word would leave it believing it had stated
            // the cost; answering with the cost object would look like the tree
            // had moved. So it is told which claim the machine now takes.
            "delete" | "borrar" | "revert" | "revertir" => {
                return Err(format!(
                    "`{key}=` no longer authorises anything: how many files change is a \
                     summary, and a summary cannot say *which* state is being destroyed. \
                     Name the state instead — `state=` — which every answer of this verb \
                     hands back ready to copy"
                ));
            }
            _ => {}
        }
    }
    Ok(asked)
}

/// Whether an abandon may go ahead, having asked nobody else.
#[derive(Debug, PartialEq, Eq)]
enum Consent {
    Given,
    /// Change nothing and answer with what it would have cost.
    ShowTheCost,
    /// The caller named an attempt that is not the one on record.
    NotThisAttempt,
}

/// The whole of the decision, and nothing else in this file makes it.
///
/// A function of two values and no filesystem, which is what lets every rule
/// below be exercised in a container that has no Btrfs. `Estrategia-de-Pruebas`:
/// policy that can only be run on Btrfs is policy that is never run.
///
/// What the caller has to claim is **the state**, not what the state costs.
/// Bringing a file back destroys nothing and how many files change is a
/// summary, and neither is an identity — see the header.
///
/// ## Why the state is not compared here, since 2026-08-29
///
/// It was, for one day, and that is the defect `dev/verify.sh` stage 55 found
/// on Fedora the first time it ran. This function held the witness the *plan*
/// was made from and returned [`Consent::ShowTheCost`] when the caller's claim
/// did not match it — so a rollback authorised against a tree somebody else had
/// written in never reached [`thalyx_core::attempt::abandon`] at all, and the
/// one answer the whole mechanism exists to give, `workspace_moved`, was
/// unreachable from the face that gives answers.
///
/// Nothing was destroyed by it. What came back instead was the cost object with
/// `done: false` and a **fresh** `confirm_with` line, which is worse than it
/// looks: an agent in a loop copies that line and destroys the person's work on
/// the next call, having been told nothing about why the first one did not
/// take. A refusal that reads as "try again with this" is not a refusal.
///
/// It was invisible in the container for the plainest reason: the core test
/// that proves the refusal calls `abandon` directly, and the only thing that
/// was ever wrong was the path between this function and it. Rule 5 — the
/// instrument includes the harness — and its narrower form, that a test of two
/// layers has to cross the join.
///
/// So a caller that names the attempt **and** names a state has made an
/// authorisation, and this says so. Whether the claim is true about the tree is
/// a question with exactly one right place to be asked, and it is not here: it
/// is under the lock, in the instant before the swap, where nothing can run
/// between the answer and the act.
fn consent(asked: &Asked, open: &Open) -> Consent {
    // First, and before `si` is looked at. A caller that named an attempt and
    // named the wrong one believes it is settling something else, and letting a
    // yes carry through would be honouring a word said about a different tree.
    if asked
        .snapshot
        .as_deref()
        .is_some_and(|named| named != open.snapshot)
    {
        return Consent::NotThisAttempt;
    }

    // The attempt named, and a state named: that is a program saying "this
    // exact tree and no other". It goes to the tree to be judged.
    if asked.snapshot.is_some() && asked.state.is_some() {
        return Consent::Given;
    }

    if asked.said_yes {
        return Consent::Given;
    }

    Consent::ShowTheCost
}

/// What the tree will be held to when it is replaced.
///
/// The plan's own witness is never what authorises a rollback. It was taken
/// outside the lock and is therefore a statement about a moment that has
/// already passed; what the caller *stated* is what it is held to, and a
/// human's yes is held to nothing but the tree in front of them.
///
/// Its own function so that the join can be asserted without a filesystem. The
/// defect stage 55 found was never in [`consent`] alone and never in
/// [`thalyx_core::attempt::abandon`] alone — it was that the first never
/// reached the second, and a test of each half separately is what let it ship.
fn how_it_is_authorised(asked: &Asked) -> attempt::Authorised<'_> {
    match asked.state.as_deref() {
        Some(claimed) => attempt::Authorised::ByState(claimed),
        None => attempt::Authorised::ByAHuman,
    }
}

/// The line that abandons this attempt in one call, built from what is true now.
///
/// Handed back by the answer that withholds consent, so a caller never has to
/// assemble it. That is not a weakening: the line **names the state**, so if
/// anybody writes in the tree in between — including in a file the caller had
/// already written in — the line it was given stops matching and the abandon
/// stops. A `si` copied from an answer has no such property.
///
/// `None` when the tree could not be read everywhere: there is no exact
/// identity to hand out, and handing out an inexact one would be inviting a
/// caller to authorise a destruction on a walk that had holes in it.
fn one_call_to_abandon(open: &Open, state: &thalyx_snapshot::Witness) -> Option<String> {
    state.is_complete().then(|| {
        format!(
            "intento abandonar snapshot={} state={}",
            open.snapshot, state.id
        )
    })
}

/// `intento [empezar <etiqueta> | confirmar | abandonar [si]]`.
pub fn run(store: &Store, here: &Where, rest: &str, face: Face, request_id: &str) -> Fallible {
    // Words and not a split on the first space, so that `intento empezar 'dos
    // palabras'` is one label. It also means the bridge can quote its arguments
    // uniformly instead of knowing which verbs read a raw line.
    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let word = given.first().map(|w| w.as_str()).unwrap_or("");
    let tail = crate::words::phrase(given.get(1..).unwrap_or(&[]));
    let tail = tail.as_str();

    match word {
        "" => status(store, face),
        "empezar" | "start" | "iniciar" => begin(store, here, tail, face, request_id),
        "confirmar" | "keep" | "guardar" => keep(store, face, request_id),
        "abandonar" | "abandon" | "deshacer" => {
            abandon(store, given.get(1..).unwrap_or(&[]), face, request_id)
        }
        other => {
            declined(
                face,
                "unknown_argument",
                &format!("`{other}` is not one of `empezar`, `confirmar`, `abandonar`"),
            );
            Ok(())
        }
    }
}

/// Whatever attempt is open, and what abandoning it would cost right now.
fn status(store: &Store, face: Face) -> Fallible {
    let open = match attempt::open(store) {
        Ok(open) => open,
        // Rule 10, and the sharpest case of it here: a record that could not be
        // read reported as "nothing open" is what would let a caller start a
        // second attempt over a live snapshot.
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let Some(open) = open else {
        if face == Face::Machine {
            face.say(thalyx_files::machine::answer(
                OP,
                vec![("open", json!(false))],
            ));
        } else {
            println!();
            println!("  No attempt is open. `intento empezar <etiqueta>` starts one,");
            println!("  and everything after it can be taken back in one word.");
            println!();
        }
        return Ok(());
    };

    let snapshots = Snapshots::of(Native, &open.subvolume);
    let cost = attempt::what_abandoning_costs(store, &snapshots);

    if face == Face::Machine {
        let mut carried = vec![("open", json!(true))];
        carried.extend(open_fields(&open));
        match &cost {
            Ok((_, plan)) => {
                carried.push(("can_be_abandoned", json!(true)));
                carried.push((
                    "abandon_in_one_call",
                    json!(one_call_to_abandon(&open, &plan.state)),
                ));
                carried.extend(state_fields(&plan.state));
                carried.extend(cost_fields(&plan.difference));
            }
            // An attempt that can no longer be abandoned is still an open
            // attempt, and saying only the first half would leave a caller
            // believing it has a way back that it does not.
            Err(error) => {
                carried.push(("can_be_abandoned", json!(false)));
                carried.push(("why_not", json!(error.to_string())));
            }
        }
        face.say(thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    println!("  `{}` is open, since {}.", open.label, open.started_at);
    println!("  on {}", open.subvolume.display());
    match &cost {
        Ok((_, plan)) => {
            println!();
            println!("  {}", plan.what_it_costs());
            println!();
            println!("  `intento confirmar` keeps all of it.");
            println!("  `intento abandonar` puts it back as it was.");
        }
        Err(error) => {
            println!();
            println!("  {error}");
            println!("  This attempt can no longer be undone. `intento confirmar`");
            println!("  closes it and keeps the work that is there.");
        }
    }
    println!();
    Ok(())
}

fn begin(store: &Store, here: &Where, label: &str, face: Face, request_id: &str) -> Fallible {
    let label = if label.is_empty() { "attempt" } else { label };

    let volumes = Native;
    let subvolume = match subvolume_to_attempt(&volumes, here.at()) {
        Ok(subvolume) => subvolume,
        // Named rather than approximated, and never widened. A copy of a
        // directory is not a snapshot, and a caller told "started" would make
        // thirty changes believing it could take them back.
        Err(NotHere::NotASubvolume) => {
            declined(
                face,
                "not_a_subvolume",
                &format!(
                    "{} is not itself a Btrfs subvolume, so there is nothing to come back \
                     to and no attempt was started. An attempt is about the subvolume you \
                     are standing in — it never looks upwards for one",
                    here.at().display()
                ),
            );
            return Ok(());
        }
        Err(NotHere::TheWholeSystem) => {
            declined(
                face,
                "the_whole_system",
                "an attempt on / would mean swapping the root of the running system out \
                 from under every process on it, this one included. No attempt was started",
            );
            return Ok(());
        }
    };

    let snapshots = Snapshots::of(volumes, &subvolume);
    match attempt::begin(store, &snapshots, label, request_id) {
        Ok(open) => {
            if face == Face::Machine {
                let mut carried = vec![("open", json!(true)), ("began", json!(true))];
                carried.extend(open_fields(&open));
                // What the workspace is, at the instant the boundary closed
                // around it. Not there to be stored and reused later — by the
                // time a caller has changed anything it is stale, which is the
                // whole point — but so that a caller which changes *nothing*
                // can still undo in one call, and so that the shape of the
                // field is the same in every answer this verb gives.
                carried.extend(state_fields(&thalyx_snapshot::witness(&open.subvolume)));
                // The two ways out, in the answer that opened it. A caller that
                // has to look up how to settle what it just started is one that
                // will leave attempts open.
                carried.push(("keep", json!("intento confirmar")));
                carried.push(("abandon", json!("intento abandonar")));
                face.say(thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!(
                    "  `{}` is open on {}.",
                    open.label,
                    open.subvolume.display()
                );
                println!("  Everything from here can be taken back with");
                println!("  `intento abandonar`, or kept with `intento confirmar`.");
                println!();
            }
            Ok(())
        }
        Err(error) => {
            let word = match &error {
                thalyx_core::CoreError::Attempt(
                    thalyx_core::attempt::AttemptError::AlreadyOpen(_),
                ) => "already_open",
                _ => "unreadable",
            };
            declined(face, word, &error.to_string());
            Ok(())
        }
    }
}

fn keep(store: &Store, face: Face, request_id: &str) -> Fallible {
    let open = match attempt::open(store) {
        Ok(Some(open)) => open,
        Ok(None) => {
            declined(face, "none_open", "no attempt is open");
            return Ok(());
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let snapshots = Snapshots::of(Native, &open.subvolume);
    match attempt::keep(store, &snapshots, request_id) {
        Ok(open) => {
            if face == Face::Machine {
                let mut carried = vec![("open", json!(false)), ("kept", json!(true))];
                carried.extend(open_fields(&open));
                // Said plainly, because it is what the caller just gave up.
                carried.push(("reversible", json!(false)));
                face.say(thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!("  `{}` is closed and the work is kept.", open.label);
                println!("  There is no longer a way back to where it started.");
                println!();
            }
            Ok(())
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            Ok(())
        }
    }
}

fn abandon(store: &Store, given: &[crate::words::Word], face: Face, request_id: &str) -> Fallible {
    // Before the record is read, because a call whose own words do not parse
    // asks nothing of the machine and costs the caller one corrected call.
    let asked = match asked_of_abandon(given) {
        Ok(asked) => asked,
        Err(why) => {
            declined(face, "bad_argument", &why);
            return Ok(());
        }
    };

    let open = match attempt::open(store) {
        Ok(Some(open)) => open,
        Ok(None) => {
            declined(face, "none_open", "no attempt is open");
            return Ok(());
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let snapshots = Snapshots::of(Native, &open.subvolume);
    let (open, plan) = match attempt::what_abandoning_costs(store, &snapshots) {
        Ok(both) => both,
        Err(error) => {
            declined(face, "snapshot_gone", &error.to_string());
            return Ok(());
        }
    };

    match consent(&asked, &open) {
        Consent::Given => {}
        // Named rather than turned into the cost object, because it is not a
        // decision about a cost: it is a caller that believes it is settling
        // some other attempt, and the answer it needs is which one is open.
        Consent::NotThisAttempt => {
            declined(
                face,
                "not_this_attempt",
                &format!(
                    "the attempt on record is `{}`, whose snapshot is `{}`, and this call \
                     named `{}`. Nothing was undone",
                    open.label,
                    open.snapshot,
                    asked.snapshot.as_deref().unwrap_or(""),
                ),
            );
            return Ok(());
        }
        Consent::ShowTheCost => {
            // The confirmation, in whichever face is asking. Both are shown exactly
            // what would be lost first, because the tree is shared: a person may
            // have written in it while the attempt was open, and their work is not
            // the agent's to discard because the agent changed its mind.
            if face == Face::Machine {
                let mut carried = vec![
                    ("open", json!(true)),
                    ("done", json!(false)),
                    ("needs", json!("confirmation")),
                    // The exact line, built from what the tree holds right now, so
                    // that saying yes costs one call and never an assembled guess.
                    (
                        "confirm_with",
                        json!(one_call_to_abandon(&open, &plan.state)),
                    ),
                ];
                carried.extend(open_fields(&open));
                carried.extend(state_fields(&plan.state));
                carried.extend(cost_fields(&plan.difference));
                face.say(thalyx_files::machine::answer(OP, carried));
                return Ok(());
            }

            println!();
            println!(
                "  Abandoning `{}` would return {}",
                open.label,
                open.subvolume.display()
            );
            println!("  to how it was at {}.", open.started_at);
            println!();
            println!("  {}", plan.what_it_costs());
            for name in &plan.difference.added {
                println!("    {name}  — made since, would be deleted");
            }
            println!();

            // Silence is not consent, and this is the one verb in the session that
            // can destroy a person's work.
            match crate::ask::confirm("  Undo all of it? [y/N] ", &crate::ask::Accepts::Yes) {
                crate::ask::Answered::Yes => {}
                crate::ask::Answered::No => {
                    println!();
                    println!("  Nothing was undone. `{}` is still open.", open.label);
                    println!();
                    return Ok(());
                }
                crate::ask::Answered::NoOneToAsk => {
                    println!("  There is no terminal to confirm on, so nothing was undone.");
                    println!("  `intento abandonar si` is the way to say yes without one.");
                    println!();
                    return Ok(());
                }
                crate::ask::Answered::Unreadable => {
                    println!("  The answer could not be read, so nothing was undone.");
                    println!("  `intento abandonar si` is the way to say yes without a terminal.");
                    println!();
                    return Ok(());
                }
            }
        }
    }

    let authorised = how_it_is_authorised(&asked);

    match attempt::abandon(store, &snapshots, &open, &plan, authorised, request_id) {
        Ok(restored) => {
            if face == Face::Machine {
                let mut carried = vec![
                    ("open", json!(false)),
                    ("done", json!(true)),
                    ("abandoned", json!(true)),
                    // Two different guarantees, and an audit that cannot tell
                    // them apart cannot say whether an interruption was
                    // survivable.
                    ("atomic", json!(restored.atomic)),
                    ("replaced_kept_as", json!(restored.replaced_kept_as)),
                ];
                carried.extend(open_fields(&open));
                carried.extend(cost_fields(&plan.difference));
                face.say(thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!("  {} is back as it was.", open.subvolume.display());
                println!("  What was there is kept as {}.", restored.replaced_kept_as);
                println!();
            }
            Ok(())
        }
        Err(error) => {
            // The word matters to a caller that is a program. `superseded` is
            // not a broken record and not a missing one: it is somebody else
            // having settled this attempt between the plan and the yes, and an
            // agent told `unreadable` would go looking for a corrupt file.
            let word = match &error {
                thalyx_core::CoreError::Attempt(
                    thalyx_core::attempt::AttemptError::Superseded(_),
                ) => "superseded",
                thalyx_core::CoreError::Attempt(thalyx_core::attempt::AttemptError::NoneOpen) => {
                    "none_open"
                }
                // Its own word, and the most important one this verb has. A
                // caller told `unreadable` would go looking for a corrupt file;
                // what actually happened is that somebody wrote in the shared
                // tree and **nothing was destroyed**, which is a fact the caller
                // has to be able to act on rather than guess at.
                thalyx_core::CoreError::Attempt(
                    thalyx_core::attempt::AttemptError::WorkspaceMoved { .. },
                ) => "workspace_moved",
                thalyx_core::CoreError::Attempt(
                    thalyx_core::attempt::AttemptError::WorkspaceUnreadable { .. },
                ) => "workspace_unreadable",
                _ => "unreadable",
            };
            declined(face, word, &error.to_string());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_snapshot::Result as SnapshotResult;

    /// A filesystem where **everything** is a subvolume.
    ///
    /// Not a Btrfs emulator and not trying to be: what is under test is the one
    /// decision this file makes before touching anything — which path an attempt
    /// is allowed to be about — and the only input that decision takes from the
    /// filesystem is "is this a subvolume". Rule 8: the fake models the property
    /// under test, which here is the refusal and not the snapshot.
    struct EverythingIsOne;

    impl Volumes for EverythingIsOne {
        fn is_subvolume(&self, _: &Path) -> SnapshotResult<bool> {
            Ok(true)
        }
        fn snapshot(&self, _: &Path, _: &Path) -> SnapshotResult<()> {
            unreachable!("nothing here gets as far as taking one")
        }
        fn restore_from(&self, _: &Path, _: &Path) -> SnapshotResult<()> {
            unreachable!("nothing here gets as far as restoring")
        }
        fn delete(&self, _: &Path) -> SnapshotResult<()> {
            unreachable!("nothing here gets as far as deleting")
        }
    }

    struct NothingIsOne;

    impl Volumes for NothingIsOne {
        fn is_subvolume(&self, _: &Path) -> SnapshotResult<bool> {
            Ok(false)
        }
        fn snapshot(&self, _: &Path, _: &Path) -> SnapshotResult<()> {
            unreachable!()
        }
        fn restore_from(&self, _: &Path, _: &Path) -> SnapshotResult<()> {
            unreachable!()
        }
        fn delete(&self, _: &Path) -> SnapshotResult<()> {
            unreachable!()
        }
    }

    #[test]
    fn the_root_of_the_running_system_is_refused_even_though_it_is_a_subvolume() {
        // On every ordinary Fedora Btrfs install `/` *is* a subvolume, so the
        // snapshot succeeds — which is the problem and not the safeguard.
        // Abandoning it means swapping the root of the running system out from
        // under every process on it, this one included.
        assert!(matches!(
            subvolume_to_attempt(&EverythingIsOne, Path::new("/")),
            Err(NotHere::TheWholeSystem)
        ));
    }

    #[test]
    fn a_path_that_only_spells_its_way_to_the_root_is_refused_too() {
        // A check on the string alone is a check somebody gets around by
        // accident, on the verb that can replace a whole subvolume.
        for spelling in ["/.", "/home/..", "//"] {
            assert!(
                matches!(
                    subvolume_to_attempt(&EverythingIsOne, Path::new(spelling)),
                    Err(NotHere::TheWholeSystem)
                ),
                "{spelling} was not recognised as the root"
            );
        }
    }

    #[test]
    fn a_subvolume_that_is_not_the_root_is_allowed() {
        // The control. Without it, a guard that refused everything would pass
        // both tests above and `intento` would never work anywhere.
        let scratch = tempfile::tempdir().expect("somewhere real to point at");
        assert!(subvolume_to_attempt(&EverythingIsOne, scratch.path()).is_ok());
    }

    /// An attempt on record, with a snapshot name shaped like a real one.
    fn on_record() -> Open {
        Open {
            label: "rename".to_string(),
            snapshot: "2026-08-29T11-04-02Z-rename".to_string(),
            subvolume: PathBuf::from("/work"),
            started_at: "2026-08-29T11:04:02Z".to_string(),
            request_id: "mcp-2".to_string(),
        }
    }

    /// The workspace exactly as it stands, in the shape a caller has to name it.
    ///
    /// A literal and not a walk of a real tree, because what is under test here
    /// is the *decision* and not the digest — the digest has its own tests, in
    /// `thalyx-snapshot/tests/state_identity.rs`, over trees somebody really
    /// wrote in. `Estrategia-de-Pruebas`: policy that can only be exercised
    /// against a filesystem is policy that is never exercised.
    fn as_it_stands() -> thalyx_snapshot::Witness {
        thalyx_snapshot::Witness {
            id: "w2-1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            files: 12,
            unreadable: 0,
            bytes: 4096,
        }
    }

    fn asked(line: &str) -> Asked {
        asked_of_abandon(&crate::words::words(line).expect("a line that closes"))
            .expect("a line this verb can read")
    }

    #[test]
    fn an_agent_that_names_the_attempt_and_the_state_abandons_in_one_call() {
        // The whole point, and the round trip it removes. Three real benchmark
        // runs went `abandonar` then `abandonar si` back to back, with no call
        // in between — so the second was the same agent echoing itself, and this
        // is the shape that says the same thing once and says more.
        let open = on_record();
        let state = as_it_stands();
        assert_eq!(
            consent(
                &asked(&format!(
                    "snapshot=2026-08-29T11-04-02Z-rename state={}",
                    state.id
                )),
                &open,
            ),
            Consent::Given
        );
    }

    #[test]
    fn the_line_the_answer_hands_back_is_the_line_that_works() {
        // The two halves of the protocol, joined. A `confirm_with` that did not
        // parse back into consent would be an answer teaching a call that fails,
        // and nothing else in this file would notice: the field is a string and
        // the parser is somewhere else.
        let open = on_record();
        let state = as_it_stands();
        let handed_back = one_call_to_abandon(&open, &state).expect("a complete tree has a line");
        assert_eq!(
            consent(&asked(&handed_back), &open),
            Consent::Given,
            "the answer hands back `{handed_back}`, which does not abandon"
        );
    }

    #[test]
    fn a_state_claim_is_carried_to_the_tree_and_never_settled_here() {
        // **The defect stage 55 found on Fedora, as an assertion.** This
        // function used to compare the caller's claim against the witness the
        // *plan* was made from, and answer `ShowTheCost` when they differed.
        //
        // Nothing was destroyed by that. What it destroyed was the answer: a
        // rollback authorised against a tree somebody else had written in never
        // reached `thalyx_core::attempt::abandon`, so `workspace_moved` — the
        // one word the whole mechanism exists to say — could not be said. The
        // caller got the cost object and a fresh line to copy instead, which an
        // agent in a loop copies, and then the person's work is gone.
        //
        // So a claim is carried, whatever it says. Where it is judged is under
        // the lock, in the instant before the swap, and the two tests that hold
        // that end are `thalyx_core::attempt`'s
        // `a_write_to_a_file_the_agent_had_already_written_stops_the_rollback`
        // and `a_rollback_that_names_the_state_the_tree_is_in_goes_ahead_in_one_call`.
        let open = on_record();
        let stale = one_call_to_abandon(&open, &as_it_stands()).expect("a line");

        assert_eq!(consent(&asked(&stale), &open), Consent::Given);
        // And it is carried as a claim about a state, not as a bare yes. A
        // `ByAHuman` here would mean the tree was replaced as it stands, which
        // is the destruction the claim was supposed to prevent.
        assert!(
            matches!(
                how_it_is_authorised(&asked(&stale)),
                attempt::Authorised::ByState(claimed) if claimed == as_it_stands().id
            ),
            "the state the caller named is not what the destruction is held to"
        );
    }

    #[test]
    fn a_bare_yes_is_held_to_nothing_but_the_tree_in_front_of_the_person() {
        // The other side of it, and why the two forms are not one. A person at
        // a terminal was shown what would be lost and said yes about the tree
        // they were looking at. They cannot state a digest and must not be
        // asked to, so their yes authorises the tree as it stands.
        assert!(matches!(
            how_it_is_authorised(&asked("si")),
            attempt::Authorised::ByAHuman
        ));
    }

    #[test]
    fn naming_an_attempt_that_is_not_the_open_one_is_refused_even_with_a_yes() {
        // Fail closed, and before `si` is looked at. A caller that named an
        // attempt believes it is settling that one; honouring a yes it said
        // about a different tree is how an agent abandons somebody else's work
        // while believing it abandoned its own.
        let open = on_record();
        assert_eq!(
            consent(
                &asked("snapshot=2026-08-29T09-00-00Z-something-else state=w2-abc si"),
                &open,
            ),
            Consent::NotThisAttempt
        );
    }

    #[test]
    fn a_claim_that_states_only_half_of_it_gets_the_cost_instead() {
        // Each of these is a caller that has not said what it accepts. None of
        // them destroys anything, and each costs exactly what the old protocol
        // cost: one more call.
        let open = on_record();
        let state = as_it_stands();
        for half in [
            "snapshot=2026-08-29T11-04-02Z-rename".to_string(),
            format!("state={}", state.id),
            String::new(),
        ] {
            assert_eq!(
                consent(&asked(&half), &open),
                Consent::ShowTheCost,
                "`intento abandonar {half}` went ahead"
            );
        }
    }

    #[test]
    fn a_tree_that_could_not_be_read_everywhere_is_never_handed_a_one_call_line() {
        // Rule 9 and rule 10, on the one path where nobody is asked twice. A
        // directory that could not be opened — or a file whose bytes could not
        // be read — is not one that is empty, so a tree with a hole in it has no
        // exact identity, and there is no line to hand out.
        //
        // A caller that assembles one anyway is refused, and this is not where:
        // `Witness::matches` is false for an incomplete witness whatever it is
        // compared against, and the comparison that matters happens under the
        // lock. `thalyx_core::attempt`'s
        // `a_tree_that_could_not_be_read_everywhere_authorises_nothing` is the
        // assertion that end holds.
        let open = on_record();
        let holed = thalyx_snapshot::Witness {
            unreadable: 1,
            ..as_it_stands()
        };
        assert!(
            one_call_to_abandon(&open, &holed).is_none(),
            "a tree nobody finished reading must not be handed a one-call line"
        );
        assert!(
            !holed.matches(&holed.id),
            "an incomplete witness matched itself, which is an authorisation"
        );
    }

    #[test]
    fn the_two_step_protocol_is_exactly_as_it_was() {
        // The control. Everything above is worth nothing if the old way stopped
        // working: the human face's escape hatch is this word, and so is every
        // caller written before today.
        let open = on_record();
        assert_eq!(consent(&asked(""), &open), Consent::ShowTheCost);
        for yes in ["si", "sí", "yes", "y"] {
            assert_eq!(
                consent(&asked(yes), &open),
                Consent::Given,
                "`intento abandonar {yes}` stopped working"
            );
        }
    }

    #[test]
    fn the_retired_count_claim_is_refused_rather_than_ignored() {
        // A caller still spelling `delete=`/`revert=` is running against the
        // rules of 2026-08-28, under which a third party's edit to a file the
        // agent had already edited authorised itself. Ignoring the words would
        // leave it believing it had stated the cost when it had not, and the
        // answer it got — the cost object again — would read as the tree having
        // changed under it. So it is told, in the words of the verb.
        for retired in [
            "delete=0 revert=3",
            "snapshot=2026-08-29T11-04-02Z-rename delete=0 revert=3",
        ] {
            let refusal = asked_of_abandon(&crate::words::words(retired).unwrap())
                .expect_err("the retired claim must not be readable as a claim");
            assert!(
                refusal.contains("state="),
                "the refusal has to name what replaced it: {refusal}"
            );
        }
    }

    #[test]
    fn a_place_that_is_not_a_subvolume_is_refused_without_looking_upwards() {
        // The defect of 2026-08-10, as a test. This used to walk up until it
        // found a subvolume, and on Cesar's machine that walk started under
        // `/tmp` and ended at `/` — a read-only snapshot of his whole root
        // filesystem, and an answer saying that abandoning would delete
        // 1,343,582 files. Nothing was destroyed; the argument for walking was.
        let scratch = tempfile::tempdir().expect("somewhere real to point at");
        assert!(matches!(
            subvolume_to_attempt(&NothingIsOne, scratch.path()),
            Err(NotHere::NotASubvolume)
        ));
    }
}
