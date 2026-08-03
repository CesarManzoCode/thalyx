//! Is enforcement in the kernel's decision path, right now?
//!
//! Every earlier answer to that question in this project was a proxy for it,
//! and each proxy was wrong in the same direction: it said yes to things that
//! do not enforce anything.
//!
//! - `thalyx session` asked whether the **policy map** was pinned. A map is a
//!   place to put permissions. It says a loader ran, not that anything reads it.
//! - `make status` and `demo-enforcement.sh` asked whether a **directory**
//!   existed in bpffs — and specifically the directory `bpftool` happens to
//!   create. Thalyx's own loader pins in a different shape, so the demo refused
//!   to run against enforcement that was live, saying it was not attached.
//! - `dev/verify.sh` counted **every LSM link on the machine**, which includes
//!   the ten the file watcher owns. Two programs attached and it printed three.
//!
//! The thing that actually enforces is a **link**. A program can be loaded,
//! pinned, listed, and in nobody's path, and it lists identically to one that is
//! live. That is precisely how a security tool reads as armed while disarmed,
//! and this module is the answer that cannot be fooled that way: it asks the
//! kernel to enumerate its links, follows each to the program it runs, and
//! compares the names against the object Thalyx would have loaded.
//!
//! ## Where the expected names come from
//!
//! The object itself, never a list beside it. Two lists that must agree, kept
//! in two places, disagree eventually — and here the disagreement would be a
//! machine reporting enforcement it does not have.

use crate::elf::Elf;
use crate::program;

#[derive(Debug, thiserror::Error)]
pub enum AttachedError {
    #[error("reading the object: {0}")]
    Elf(#[from] crate::elf::ElfError),

    #[error("reading the object's programs: {0}")]
    Programs(#[from] program::ProgramError),

    #[error(
        "the kernel's links could not be listed: {0}\n      \
         this needs CAP_SYS_ADMIN, and without it nothing here can tell \
         enforcement that is absent from enforcement it may not look at"
    )]
    Kernel(#[source] std::io::Error),
}

type Result<T> = std::result::Result<T, AttachedError>;

/// What of an object is live in the kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The object's programs, under the names the kernel would hold them by.
    pub expected: Vec<String>,
    /// Those of them that a live link actually runs.
    pub live: Vec<String>,
}

impl Attachment {
    /// Every hook the object declares is in the decision path.
    pub fn is_complete(&self) -> bool {
        !self.expected.is_empty() && self.live.len() == self.expected.len()
    }

    /// Nothing of this object is live.
    pub fn is_absent(&self) -> bool {
        self.live.is_empty()
    }

    /// The hooks that should be live and are not.
    pub fn missing(&self) -> Vec<&str> {
        self.expected
            .iter()
            .filter(|name| !self.live.contains(name))
            .map(String::as_str)
            .collect()
    }

    /// One line, in the shape the session prints.
    ///
    /// The partial case gets its own wording on purpose. Enforcement with one
    /// of its two hooks live is worse than none: files would be checked while
    /// connections were not, and a single count would let that read as working.
    pub fn describe(&self) -> String {
        if self.is_absent() {
            return "nothing of it is in the kernel's decision path".to_string();
        }
        if self.is_complete() {
            return format!(
                "{} of {} hook(s) live: {}",
                self.live.len(),
                self.expected.len(),
                self.live.join(", ")
            );
        }
        format!(
            "only {} of {} hook(s) live — {} enforce(s) nothing: {}",
            self.live.len(),
            self.expected.len(),
            self.missing().len(),
            self.missing().join(", ")
        )
    }
}

/// What of `object` the kernel is currently running.
pub fn attachment(object: &[u8]) -> Result<Attachment> {
    let elf = Elf::parse(object)?;
    let expected: Vec<String> = program::programs(&elf)?
        .iter()
        .map(|spec| thalyx_syscall::kernel_visible_name(&spec.name))
        .collect();

    let links = thalyx_syscall::live_links().map_err(AttachedError::Kernel)?;
    Ok(against(&expected, &links))
}

/// The comparison, separated from the call so it can be exercised without a
/// kernel — which is every machine this is written on.
fn against(expected: &[String], links: &[thalyx_syscall::LiveLink]) -> Attachment {
    let live = expected
        .iter()
        .filter(|name| {
            links.iter().any(|link| {
                // The program type is checked as well as the name. A name is
                // fifteen characters of anyone's choosing, and a link of some
                // other type running a program that happens to be called
                // `thalyx_file_open` is not this enforcement.
                link.program_type == thalyx_syscall::BPF_PROG_TYPE_LSM && &&link.program == name
            })
        })
        .cloned()
        .collect();

    Attachment {
        expected: expected.to_vec(),
        live,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_syscall::LiveLink;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    fn link(name: &str) -> LiveLink {
        LiveLink {
            link_id: 1,
            program_id: 1,
            program_type: thalyx_syscall::BPF_PROG_TYPE_LSM,
            program: name.to_string(),
        }
    }

    fn expected() -> Vec<String> {
        let elf = Elf::parse(CAPTURED).unwrap();
        program::programs(&elf)
            .unwrap()
            .iter()
            .map(|spec| thalyx_syscall::kernel_visible_name(&spec.name))
            .collect()
    }

    #[test]
    fn the_names_compared_are_the_truncated_ones_the_kernel_actually_holds() {
        // `thalyx_socket_connect` is twenty-one characters and the kernel keeps
        // fifteen. Comparing the full name against what the kernel reports
        // matches nothing, and the answer would be "enforcement is not
        // attached" on a machine where it is — the failure this whole module
        // exists to stop, arriving through the fix for it.
        let names = expected();
        assert!(
            names.contains(&"thalyx_socket_c".to_string()),
            "expected the truncated name, got {names:?}"
        );
        for name in &names {
            assert!(
                name.len() <= 15,
                "`{name}` is longer than the kernel allows"
            );
        }
    }

    #[test]
    fn a_pinned_but_unlinked_object_reads_as_attaching_nothing() {
        // The whole point. No links at all is the state of a machine where
        // every map and program is pinned and enforcement is off.
        let attachment = against(&expected(), &[]);
        assert!(attachment.is_absent());
        assert!(!attachment.is_complete());
        assert!(attachment.describe().contains("nothing"));
    }

    #[test]
    fn one_hook_of_two_is_reported_as_worse_than_none_rather_than_as_attached() {
        // A count alone would say "1 link live" and read as working. Files
        // checked while connections are not is the failure with no symptom.
        let names = expected();
        let attachment = against(&names, &[link(&names[0])]);
        assert!(!attachment.is_complete());
        assert!(!attachment.is_absent());
        assert!(attachment.describe().contains("only 1"), "{attachment:?}");
        assert_eq!(attachment.missing(), vec![names[1].as_str()]);
    }

    #[test]
    fn every_hook_live_is_the_only_thing_that_counts_as_attached() {
        let names = expected();
        let links: Vec<LiveLink> = names.iter().map(|n| link(n)).collect();
        assert!(against(&names, &links).is_complete());
    }

    #[test]
    fn another_projects_links_do_not_count_as_this_objects() {
        // `dev/verify.sh` counted every LSM link on the machine, which includes
        // the ten the file watcher owns — two programs attached and it printed
        // three. A count that can be satisfied by somebody else's program is
        // not a measurement of this one.
        let names = expected();
        let watcher: Vec<LiveLink> = ["thalyx_inode_cre", "thalyx_file_perm"]
            .iter()
            .map(|n| link(n))
            .collect();
        assert!(against(&names, &watcher).is_absent());
    }

    #[test]
    fn a_link_of_another_type_running_the_same_name_is_not_enforcement() {
        // A program name is fifteen characters of anyone's choosing. Matching
        // on the name alone would let any process on the machine make Thalyx
        // report enforcement by naming a tracepoint after it.
        let names = expected();
        let impostor = LiveLink {
            program_type: thalyx_syscall::BPF_PROG_TYPE_LSM + 1,
            ..link(&names[0])
        };
        assert!(against(&names, &[impostor]).is_absent());
    }
}
