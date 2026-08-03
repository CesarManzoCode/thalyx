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
            Some(point) => Outcome::Found(format!("mounted at {point}")),
            None => Outcome::Absent("no cgroup2 filesystem".to_string()),
        },
        Err(error) => Outcome::Unreadable(format!("/proc/mounts: {error}")),
    }
}

fn enforcement() -> Outcome {
    use thalyx_permd::PolicyStore;
    let store = thalyx_permd::BpftoolStore::default_map();
    if store.is_available() {
        Outcome::Found("the kernel holds the policy map".to_string())
    } else {
        Outcome::Absent(
            "the policy map is not loaded, so no permission would be enforced".to_string(),
        )
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
        println!("  {unreadable} of those I could not check at all. Not absent — unchecked.",);
    }
    if absent > 0 {
        println!("  {absent} are not here. I will not pretend otherwise later.");
    }
    if unreadable == 0 && absent == 0 {
        println!("  Everything I know how to check is here.");
    }

    if once {
        return Ok(());
    }

    println!();
    println!("  Say what you want. `salir` to leave.");
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
