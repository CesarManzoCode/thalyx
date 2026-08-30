//! `ejecutar <ruta>` — the human's route to running a program nobody signed.
//!
//! `vault/02-Arquitectura/Programas-Ajenos.md` is the decree; everything of
//! substance is in `thalyx_core::foreign`. This file is the CLI's half: it
//! reads the line, asks the human on the trusted path, and reports what
//! happened plainly enough that "a module ran" and "a guest ran" cannot be
//! confused for each other.
//!
//! ## Why the words are `leyendo` and `escribiendo`, and why they go first
//!
//! `Palabras.md` decreed that flags come first and the subject is the rest of
//! the line. Here that is not a style choice: everything after the program is
//! **the program's own arguments**, and a grant that could appear among them
//! would be a grant a program could ask for by being invoked with the right
//! argument. The boundary has to be somewhere a person can see, and the
//! program's path is the only place both a person and a parser agree on.

use crate::files::Face;
use serde_json::json;
use std::ffi::OsString;
use std::path::PathBuf;
use thalyx_core::Store;
use thalyx_manifest::{Permission, PermissionKind};

type Fallible = Result<(), Box<dyn std::error::Error>>;

const OP: &str = "execute";

/// The profile a foreign program runs under.
///
/// The same one a module gets. Not a looser one written for guests: a profile
/// per kind of caller is two isolation stories that have to stay in agreement,
/// and the one that would rot is the one nobody's module uses.
const FOREIGN_PROFILE: &str = thalyx_sandbox::profile::MODULE_STANDARD;

/// What the line asked for, once it parses.
#[derive(Debug)]
pub struct Asked {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub grants: Vec<Permission>,
}

/// Why a line did not parse, in the words the answer carries.
#[derive(Debug)]
struct Refusal {
    word: &'static str,
    remedy: &'static str,
    message: String,
}

/// Read `[leyendo <ruta>]… [escribiendo <ruta>]… <programa> [argumentos…]`.
///
/// The grant words are consumed only while they are still at the front. The
/// first word that is not one of them is the program, and every word after it
/// belongs to the program — including a word spelled `leyendo`, which is the
/// case that makes this rule worth writing down rather than inferring.
fn parse(rest: &str) -> Result<Asked, Refusal> {
    let words = crate::words::words(rest).map_err(|unclosed| Refusal {
        word: unclosed.word(),
        remedy: unclosed.remedy(),
        message: "the line has a quote that never closes".to_string(),
    })?;

    let mut grants = Vec::new();
    let mut index = 0;

    while index < words.len() {
        let action = match words[index].as_str() {
            "leyendo" | "reading" => "read",
            "escribiendo" | "writing" => "write",
            _ => break,
        };

        let Some(path) = words.get(index + 1) else {
            return Err(Refusal {
                word: "grant_without_path",
                remedy: "name_the_path",
                message: format!(
                    "`{}` has to be followed by the path it grants",
                    words[index].as_str()
                ),
            });
        };

        grants.push(Permission {
            // Absolute, because the confinement is built from this string and
            // the program does not share this session's idea of where "here"
            // is. A relative grant would name one directory to the human and
            // another to the kernel.
            resource: std::path::absolute(path.as_str())
                .unwrap_or_else(|_| PathBuf::from(path.as_str()))
                .display()
                .to_string(),
            action: action.to_string(),
            // Lives as long as the run, which is what this was always meant
            // to say. `Persistent` would be a permission nobody could later
            // find to withdraw — it is attached to a path, not to anything the
            // store knows the name of — and `Jit` was worse in the other
            // direction: it carries a deadline the kernel enforces, thirty
            // seconds by default, and a policy is a single entry with a single
            // deadline. So a guest that named a path lost *everything* half a
            // minute in, the read floor included, and died on its next open.
            //
            // Cesar decided on 2026-08-25 that a guest's grant lasts the run.
            // `release()` withdraws it when the process exits and takes the
            // cgroup with it. What that gives up is named rather than hidden:
            // the thirty seconds were also the kernel's backstop against a
            // Thalyx that hung and never reached `release()`. See
            // `Programas-Ajenos.md`.
            kind: PermissionKind::Session,
        });
        index += 2;
    }

    let Some(program) = words.get(index) else {
        return Err(Refusal {
            word: "nothing_asked",
            remedy: "name_a_program",
            message: "which program — `ejecutar <ruta>`".to_string(),
        });
    };

    Ok(Asked {
        program: PathBuf::from(program.as_str()),
        args: words[index + 1..]
            .iter()
            .map(|word| OsString::from(word.as_str()))
            .collect(),
        grants,
    })
}

/// `ejecutar …`
pub fn execute(store: &Store, rest: &str, face: Face) -> Fallible {
    let asked = match parse(rest) {
        Ok(asked) => asked,
        Err(refusal) => {
            say_refusal(face, &refusal);
            return Ok(());
        }
    };

    // The machine face cannot confirm, and this is the one verb where that is
    // the whole point rather than a limitation. `Camino-Confiable.md` is not
    // weakened by the structured face; it is reported by it, in the same shape
    // as any other refusal, so a caller reading one stream still sees it.
    if face.is_machine() {
        face.say(thalyx_files::machine::refused_with(
            OP,
            "needs_a_human",
            "confirm_at_a_terminal",
            "nobody signed this program, so a human has to say yes at a terminal. \
             Silence is not consent.",
            vec![
                ("program", json!(asked.program.display().to_string())),
                ("ran", json!(false)),
            ],
        ));
        return Ok(());
    }

    // The terminal is no longer checked for here. `crate::ask` checks it, and
    // it checks it **after** the context below has been printed — which is the
    // whole difference between this verb working on the display and refusing
    // there. Under the screen the context is what the confirmation is drawn
    // from, so a refusal issued before it exists refuses with nothing to show.

    // Drawn by Thalyx, from the resolved path rather than from what was typed:
    // the human is being asked about the file that will run, and `bin/tool` and
    // the symlink it follows are two different answers to that question.
    let resolved = asked
        .program
        .canonicalize()
        .unwrap_or(asked.program.clone());

    println!();
    println!("  This program is not a module. Nobody signed it and nothing");
    println!("  vouches for it — Thalyx will confine it and nothing more.");
    println!();
    println!("    {}", resolved.display());
    if !asked.args.is_empty() {
        println!(
            "    arguments: {}",
            asked
                .args
                .iter()
                .map(|arg| thalyx_core::trusted_path::sanitise(&arg.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    println!();
    println!("  It will be able to reach:");
    println!("    its own directory, read-only, and the system paths");
    for grant in &asked.grants {
        // Through the same sanitiser the module capability prompt uses. A path
        // is text somebody else chose, and the confirmation frame is only a
        // frame if what it contains cannot draw one.
        for line in thalyx_core::trusted_path::sanitise_permission(grant) {
            println!("    {line}");
        }
    }
    if asked.grants.is_empty() {
        println!("    and nothing else on this machine");
    }
    println!();
    // A read that failed is not a yes, and it is not an empty answer either.
    // The four outcomes are four sentences, because *nobody answered* and *the
    // answer could not be read* send a person to look in different places.
    match crate::ask::confirm("  Run it? [y/N] ", &crate::ask::Accepts::Yes) {
        crate::ask::Answered::Yes => {}
        crate::ask::Answered::No => {
            println!();
            println!("  Not run.");
            println!();
            return Ok(());
        }
        crate::ask::Answered::NoOneToAsk => {
            println!();
            println!("  There is no terminal to confirm on, so I will not run this.");
            println!("  Silence is not consent.");
            println!();
            return Ok(());
        }
        crate::ask::Answered::Unreadable => {
            println!();
            println!("  Could not read the answer; refusing.");
            println!();
            return Ok(());
        }
    }

    let policies = thalyx_permd::KernelStore::default_map();
    let helper = std::env::current_exe()?;

    let outcome = thalyx_core::run_foreign(
        store,
        &policies,
        thalyx_core::ForeignRequest {
            program: &asked.program,
            args: asked.args,
            grants: asked.grants,
            helper,
            request_id: crate::new_request_id(),
            profile: FOREIGN_PROFILE,
            // Nothing added. What a person typed at a prompt runs with what
            // Thalyx runs with, and a verb that quietly enriched the
            // environment of a guest would be handing it something nobody
            // confirmed.
            environment: Vec::new(),
        },
    );

    match outcome {
        Ok(outcome) => report(&outcome),
        Err(error) => {
            println!();
            println!("  {error}");
            println!();
        }
    }

    Ok(())
}

fn report(outcome: &thalyx_core::ForeignOutcome) {
    println!();
    println!("  ran: {}", outcome.program.display());
    println!(
        "  confined to cgroup {}, allowed=0x{:x}",
        outcome.cgroup_id, outcome.policy.allowed
    );
    println!("  {}", outcome.isolation);
    if let Some(uid) = outcome.uid {
        println!("  ran as user {uid}, which is this program's and no other's");
    }
    for grant in &outcome.grants {
        println!("    {}", grant.describe());
    }
    if outcome.grants.is_empty() {
        println!("    (nothing granted; it saw the system paths and its own directory)");
    }

    // Everything the program wrote goes through the sanitiser, for the reason
    // written in `run.rs`: text routed through Thalyx accomplishes nothing if
    // that text can then repaint the screen. A guest is the case that reason
    // was written for.
    if !outcome.wrote.stdout.is_empty() {
        println!();
        println!("  it wrote:");
        for line in thalyx_core::trusted_path::sanitise_output(&outcome.wrote.stdout) {
            println!("    {line}");
        }
    }
    if !outcome.wrote.stderr.is_empty() {
        println!();
        println!("  on its error stream:");
        for line in thalyx_core::trusted_path::sanitise_output(&outcome.wrote.stderr) {
            println!("    {line}");
        }
    }
    if outcome.wrote.truncated {
        println!();
        println!("  (it wrote more than Thalyx keeps for one run; the rest is not here)");
    }

    println!();
    match outcome.exit_code {
        Some(0) => println!("  it finished cleanly"),
        Some(code) => println!("  it exited with status {code}"),
        // The third answer, and the one under this profile that most often
        // means the filter stopped it. Never printed as "no code".
        None => {
            println!("  it was killed by a signal, which under this profile is usually the filter")
        }
    }
    println!();
}

/// `ensayo ejecutar …` — everything that can be worked out without running it.
///
/// D1 of `Superficie-para-el-LLM.md`. It matters more here than for a file
/// verb: the input that causes the damage is a path to somebody else's code,
/// and the difference between the right one and the wrong one is a few
/// characters that the confirmation prompt will happily draw either way.
pub fn rehearse(rest: &str, face: Face) -> Fallible {
    let asked = match parse(rest) {
        Ok(asked) => asked,
        Err(refusal) => {
            say_refusal(face, &refusal);
            return Ok(());
        }
    };

    // Resolved the same way the real verb resolves it, and reported as what it
    // is. A rehearsal that resolved the path differently would be describing a
    // different run — which is the one failure a rehearsal cannot survive.
    let resolved = asked.program.canonicalize();
    let runnable = resolved.as_ref().is_ok_and(|path| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|data| data.is_file() && data.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    });

    let grants: Vec<String> = asked.grants.iter().map(|g| g.describe()).collect();

    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            "rehearse",
            vec![
                ("verb", json!(OP)),
                ("named", json!(asked.program.display().to_string())),
                (
                    "resolves_to",
                    json!(
                        resolved
                            .as_ref()
                            .ok()
                            .map(|path| path.display().to_string())
                    ),
                ),
                ("runnable", json!(runnable)),
                ("grants", json!(grants)),
                ("would_confirm", json!(true)),
                ("would_run", json!(false)),
                (
                    "arguments",
                    json!(
                        asked
                            .args
                            .iter()
                            .map(|arg| arg.to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                    ),
                ),
            ],
        ));
        return Ok(());
    }

    println!();
    match &resolved {
        Ok(path) if runnable => println!("  would run: {}", path.display()),
        Ok(path) => println!(
            "  {} is there and is not something I can run",
            path.display()
        ),
        Err(error) => println!("  {} cannot be resolved: {error}", asked.program.display()),
    }
    println!("  it would be able to reach its own directory and the system paths");
    for grant in &grants {
        println!("    {grant}");
    }
    if grants.is_empty() {
        println!("    and nothing else on this machine");
    }
    println!();
    println!("  and it would ask you first. Nothing ran.");
    println!();
    Ok(())
}

fn say_refusal(face: Face, refusal: &Refusal) {
    if face.is_machine() {
        face.say(thalyx_files::machine::refused(
            OP,
            refusal.word,
            refusal.remedy,
            &refusal.message,
        ));
    } else {
        println!();
        println!("  {}", refusal.message);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grant_words_stop_at_the_program() {
        // The rule this is here for: a word that spells a grant is a grant only
        // while it is still in front of the program. After it, it is one of the
        // program's arguments — otherwise a program could be handed a grant by
        // whoever chose its arguments.
        let asked = parse("leyendo /tmp/a tool leyendo /etc/shadow").unwrap();

        assert_eq!(asked.program, PathBuf::from("tool"));
        assert_eq!(asked.grants.len(), 1);
        assert_eq!(asked.grants[0].resource, "/tmp/a");
        assert_eq!(
            asked.args,
            vec![OsString::from("leyendo"), OsString::from("/etc/shadow")]
        );
    }

    #[test]
    fn a_grant_is_made_absolute_before_anything_is_built_from_it() {
        // The confinement is built from this string, and the program has no
        // idea where this session thinks "here" is. A relative grant would
        // name one directory to the human and another to the kernel.
        let asked = parse("leyendo notas tool").unwrap();
        assert!(
            asked.grants[0].resource.starts_with('/'),
            "{}",
            asked.grants[0].resource
        );
    }

    #[test]
    fn reading_and_writing_are_different_grants() {
        let asked = parse("leyendo /a escribiendo /b tool").unwrap();
        assert_eq!(asked.grants[0].action, "read");
        assert_eq!(asked.grants[1].action, "write");
    }

    #[test]
    fn a_grant_lasts_the_run_and_neither_longer_nor_shorter() {
        // Two failures on either side of one line. `Persistent` would be a
        // permission nobody could later find to withdraw. `Jit` — which this
        // was until 2026-08-25, under a comment claiming it meant "as long as
        // the run" — carries a deadline the kernel enforces, so the run was
        // capped at thirty seconds instead.
        let asked = parse("leyendo /a tool").unwrap();
        assert_ne!(asked.grants[0].kind, PermissionKind::Persistent);

        // The claim, asked of the thing that decides it rather than of the
        // label. A kind whose name sounds right and still produced a deadline
        // is exactly what was here before.
        let policy =
            thalyx_permd::policy_for(&asked.grants, 1_000, thalyx_permd::DEFAULT_JIT_LIFETIME_NS)
                .expect("a grant on a path is expressible");
        assert_eq!(
            policy.expires_ns, 0,
            "the kernel would take this grant away while the program was still running"
        );
    }

    #[test]
    fn a_grant_word_with_nothing_after_it_is_refused_rather_than_dropped() {
        // Silently dropping it would run the program with one permission fewer
        // than the human read on the screen, which is the direction that looks
        // safe and is not: they would believe the run had reached something it
        // never could, and stop looking for why the work did not happen.
        let refusal = parse("leyendo").unwrap_err();
        assert_eq!(refusal.word, "grant_without_path");
    }

    #[test]
    fn an_empty_line_asks_which_program_rather_than_running_anything() {
        assert_eq!(parse("").unwrap_err().word, "nothing_asked");
    }
}
