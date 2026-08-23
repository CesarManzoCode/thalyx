//! The module verbs, both faces: what could be installed, what is, and what is
//! granted.
//!
//! ## Why these three moved here, and why they were the ones that had no face
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md` decrees that every thing is
//! born with two faces and that the second is not added afterwards. These three
//! were built before that decree existed, so they were born with one — and stayed
//! that way through every point of the catalogue, because the catalogue was about
//! *new* surface and nobody went back for the old.
//!
//! What that costs is not obvious until it is named. `disponibles`, `modulos` and
//! `permisos` are the three questions an agent has to answer before it can do the
//! one thing Thalyx exists to let it do — install a module. A program that cannot
//! read them cannot check whether the thing it is about to install is already
//! installed, cannot find out what the repository holds, and cannot see what it
//! granted last time. It is the whole loop, and it was prose.
//!
//! ## The three states, which is the reason this file is longer than it looks
//!
//! Rule 10 of `Estrategia-de-Pruebas.md`: a failure to read is not a failure to
//! exist, and these are the three places in the session where confusing them is
//! most expensive. An empty store and an unreadable one look identical to a
//! caller that gets `[]`, and the caller that reads the first goes and installs
//! something while the caller that reads the second has a broken machine. So the
//! error is its own answer with its own word, in all three.

use crate::files::Face;
use serde_json::{Value, json};
use thalyx_core::Store;
use thalyx_core::permissions::Registry;

type Fallible = std::io::Result<()>;

/// What is in the repository and could be installed.
///
/// Separate verb from `modulos` because they answer different questions, and
/// conflating them is how a person ends up believing something is installed
/// because they saw its name. `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`
/// step 2 is installing from a local repository, and inside the machine there
/// is no shell to hand a path to — so the repository has to be findable.
pub fn available(store: &Store, rest: &str, face: Face) -> Fallible {
    const OP: &str = "available";

    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let (_, window) = match crate::index::asked_of(&given) {
        Ok(asked) => asked,
        Err(why) => return refused(face, OP, "bad_cursor", "read_the_error", &why.to_string()),
    };

    let repo = store.repo_root();
    let scan = match thalyx_core::repo::scan(&repo) {
        Ok(scan) => scan,
        Err(error) => {
            // Not "the repository is empty". The one sentence a person reads as
            // an inventory is the one a program reads as an inventory too.
            let (word, remedy) = read_failure(&error);
            return refused(face, OP, word, remedy, &error.to_string());
        }
    };

    if face.is_machine() {
        // Already sorted by `scan`, which sorts so that two machines resolve the
        // same way; the cursor rides on that same order rather than a second one.
        let page = match thalyx_files::window::page(
            scan.candidates,
            |candidate| {
                let mut key = candidate.module_id.clone().into_bytes();
                key.push(0);
                key.extend_from_slice(candidate.version.to_string().as_bytes());
                key
            },
            &window,
        ) {
            Ok(page) => page,
            Err(why) => return refused(face, OP, "bad_window", "read_the_error", &why.to_string()),
        };

        let rows: Vec<Value> = page
            .rows
            .iter()
            .map(|candidate| {
                json!({
                    "module_id": candidate.module_id,
                    "version": candidate.version.to_string(),
                    "path": candidate.path.display().to_string(),
                })
            })
            .collect();

        // Never paged and never folded into the rows. A bundle whose signature
        // does not check out is the single most important thing this list can
        // say, and it is short by nature — the same argument `machine::listing`
        // makes for what it could not read.
        let refused_rows: Vec<Value> = scan
            .rejected
            .iter()
            .map(|one| {
                json!({
                    "path": one.path.display().to_string(),
                    "reason": one.reason,
                })
            })
            .collect();

        let mut carried = vec![
            ("repo", json!(repo.display().to_string())),
            ("candidates", json!(rows)),
            ("refused", json!(refused_rows)),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
        println!("{}", thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    if scan.candidates.is_empty() && scan.rejected.is_empty() {
        println!("  The repository is empty.");
        println!();
        println!("  It is {}, on the store.", repo.display());
        println!();
        return Ok(());
    }

    for candidate in &scan.candidates {
        println!("  {} {}", candidate.module_id, candidate.version);
    }
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
    println!();
    Ok(())
}

/// What is installed on this machine.
pub fn installed(store: &Store, rest: &str, face: Face) -> Fallible {
    const OP: &str = "modules";

    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let (_, window) = match crate::index::asked_of(&given) {
        Ok(asked) => asked,
        Err(why) => return refused(face, OP, "bad_cursor", "read_the_error", &why.to_string()),
    };

    let list = match store.installed() {
        Ok(list) => list,
        Err(error) => {
            // The place this rule was written for. `modulos` is the verb a
            // caller reads as an inventory, so an unreadable store answering
            // "nothing is installed" is the exact shape rule 10 forbids: the
            // caller goes and installs a second copy of what is already there.
            let (word, remedy) = read_failure(&error);
            return refused(face, OP, word, remedy, &error.to_string());
        }
    };

    if face.is_machine() {
        let mut rows = list.clone();
        // Sorted here rather than trusted from the store: the cursor names a
        // position in an ordering, and an ordering nobody promised is not one.
        rows.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let page = match thalyx_files::window::page(
            rows,
            |(id, version)| {
                let mut key = id.clone().into_bytes();
                key.push(0);
                key.extend_from_slice(version.to_string().as_bytes());
                key
            },
            &window,
        ) {
            Ok(page) => page,
            Err(why) => return refused(face, OP, "bad_window", "read_the_error", &why.to_string()),
        };

        let carried_rows: Vec<Value> = page
            .rows
            .iter()
            .map(|(id, version)| json!({ "module_id": id, "version": version.to_string() }))
            .collect();

        let mut carried = vec![("modules", json!(carried_rows))];
        carried.extend(thalyx_files::machine::window_fields(&page));
        println!("{}", thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    if list.is_empty() {
        println!("  Nothing is installed.");
        println!();
        println!("  If a store was expected here, the first lines of the boot say");
        println!("  whether one was mounted. An empty store and an absent one look");
        println!("  the same from this list, and only the boot told them apart.");
    } else {
        for (id, version) in &list {
            println!("  {id} {version}");
        }
        println!();
        println!("  `correr <id>` runs one.");
    }
    println!();
    Ok(())
}

/// What is granted right now, and to whom.
///
/// A grant is in force only while its module is the current version. Listing
/// the raw registry would show permissions for modules that are not installed,
/// which is precisely the orphan grant the design forbids — and worse, it would
/// show it as if it were real.
pub fn permissions(store: &Store, face: Face) -> Fallible {
    const OP: &str = "permissions";

    let registry = match Registry::load(store.permissions_path()) {
        Ok(registry) => registry,
        Err(error) => return refused(face, OP, "unreadable", "cannot", &error.to_string()),
    };

    let mut in_force: Vec<(&String, &Vec<thalyx_core::permissions::Grant>)> = Vec::new();
    let mut inert: Vec<(&String, usize)> = Vec::new();
    for (module_id, grants) in registry.all() {
        if store.is_installed(module_id) {
            in_force.push((module_id, grants));
        } else {
            inert.push((module_id, grants.len()));
        }
    }
    in_force.sort_by_key(|(id, _)| (*id).clone());
    inert.sort_by_key(|(id, _)| (*id).clone());

    if face.is_machine() {
        let granted: Vec<Value> = in_force
            .iter()
            .flat_map(|(module_id, grants)| {
                grants.iter().map(move |grant| {
                    json!({
                        "module_id": module_id,
                        "action": grant.action,
                        "resource": grant.resource,
                        "kind": grant.kind.to_string(),
                        "granted_at": grant.granted_at,
                    })
                })
            })
            .collect();

        // Their own field, and never mixed into `granted`. A record for a module
        // that is not installed grants nothing, and a caller that read one list
        // would believe something is in force that is not — which is the orphan
        // grant the design forbids, arriving through the parser instead.
        let inert_rows: Vec<Value> = inert
            .iter()
            .map(|(module_id, count)| json!({ "module_id": module_id, "recorded": count }))
            .collect();

        println!(
            "{}",
            thalyx_files::machine::answer(
                OP,
                vec![
                    ("granted", json!(granted)),
                    ("count", json!(granted.len())),
                    ("inert", json!(inert_rows)),
                ],
            )
        );
        return Ok(());
    }

    println!();
    if in_force.is_empty() {
        println!("  no permissions in force");
    }
    for (module_id, grants) in &in_force {
        println!("  {module_id}");
        for grant in grants.iter() {
            println!(
                "    {} {} ({}) granted {}",
                grant.action, grant.resource, grant.kind, grant.granted_at
            );
        }
    }
    if !inert.is_empty() {
        println!();
        println!("  inert records ({} module(s) not installed):", inert.len());
        for (module_id, count) in &inert {
            println!("    {module_id}  {count} recorded, none in force");
        }
        println!();
        println!("  These grant nothing: a permission holds only while its module");
        println!("  is current. `thalyx store clean` clears the records.");
    }
    println!();
    Ok(())
}

/// Which of the two failures rule 10 keeps apart this one is.
///
/// `CoreError` already draws the line in its own documentation — absent means
/// nothing was ever recorded, unreadable means something was and nobody knows
/// what — and it is drawn again here because the wire is where it gets lost.
/// Both arrive as `Io`, and folding them would hand a caller one word for a
/// store that was never made and a store that is corrupt. The first is fixed by
/// making one; there is nothing the second's caller can do from in here.
fn read_failure(error: &thalyx_core::CoreError) -> (&'static str, &'static str) {
    match error {
        thalyx_core::CoreError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            ("absent", "format_a_store")
        }
        _ => ("unreadable", "cannot"),
    }
}

/// One shape for every refusal here, so that `op`, the word and the remedy
/// cannot be forgotten by whichever branch is being written at the time.
fn refused(face: Face, op: &str, word: &str, remedy: &str, message: &str) -> Fallible {
    if face.is_machine() {
        println!(
            "{}",
            thalyx_files::machine::refused(op, word, remedy, message)
        );
    } else {
        println!();
        println!("  {message}");
        if word == "unreadable" {
            // The half of the answer rule 10 is about, said in the words a
            // person needs: this is not an empty list.
            println!("  That is not the same as it being empty, and I will not");
            println!("  report it as empty.");
        }
        println!();
    }
    Ok(())
}
