//! Object in, enforcement attached.
//!
//! The order matters and is not interchangeable:
//!
//! 1. **Maps first.** A program refers to a map by file descriptor, and a
//!    descriptor does not exist until the map does.
//! 2. **CO-RE against the running kernel.** Every field offset the object
//!    carries is from some other kernel until this step replaces it.
//! 3. **Then the map descriptors**, written into the instructions.
//! 4. **Then load**, which is where the verifier either accepts it or explains
//!    at length why not.
//! 5. **Then link.** A loaded program that is not linked is in the kernel and
//!    in nobody's decision path. It lists identically to one that is.
//! 6. **Then pin**, because `thalyx-permd` is a different process and finds the
//!    policy map by its path in bpffs. Unpinned, enforcement would be attached
//!    and nothing could write a permission into it.
//!
//! ## Nothing half-attached
//!
//! If any program fails, the ones already linked are dropped and the whole
//! thing reports failure. Enforcement with one of its two hooks live is worse
//! than none: `thalyx session` would report it attached, and files would be
//! checked while connections were not.

use crate::btf::{Btf, kind as btf_kind};
use crate::elf::Elf;
use crate::{core, maps, program};
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("reading the object: {0}")]
    Elf(#[from] crate::elf::ElfError),

    #[error("reading the object's types: {0}")]
    Btf(#[from] crate::btf::BtfError),

    #[error("reading the object's maps: {0}")]
    Maps(#[from] maps::MapError),

    #[error("reading the object's programs: {0}")]
    Programs(#[from] program::ProgramError),

    #[error("relocating against this kernel: {0}")]
    Core(#[from] core::CoreError),

    #[error("this object has no .BTF section, so nothing in it can be understood")]
    NoBtf,

    #[error(
        "this kernel does not expose `{0}`. The hook the program attaches to \
         does not exist here, so enforcement cannot be attached"
    )]
    NoSuchHook(String),

    #[error("creating map `{name}`: {source}")]
    MapCreate {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("the kernel refused `{name}`: {rejection}")]
    Verifier {
        name: String,
        rejection: thalyx_syscall::VerifierRejection,
    },

    #[error(
        "attaching `{name}`: {source}\n      the program loaded and is in nobody's path{}",
        attach_hint(source)
    )]
    Attach {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("pinning {path}: {source}")]
    Pin {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("reading this kernel's BTF at {path}: {source}")]
    KernelBtf {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

type Result<T> = std::result::Result<T, LoadError>;

/// What EBUSY means when a trampoline is being installed on an LSM hook.
///
/// It cost a boot on 2026-08-04, and the errno is why: "Resource busy" reads as
/// something else already holding the hook, and nothing was. BPF attaches to an
/// LSM hook with a trampoline; when the function is not ftrace-managed the
/// kernel patches the text itself and expects the five-byte NOP that
/// `CONFIG_FUNCTION_TRACER` puts at the top of every function. The bytes were
/// something else, so `memcmp` failed, and that path returns `-EBUSY`.
///
/// Both causes are named because this genuinely cannot tell them apart from
/// here — `register_ftrace_direct` returns the same errno for a hook that
/// already carries a direct call. Naming one would be an explanation of a cause
/// nothing observed, which is the mistake `Estrategia-de-Pruebas.md` records
/// under messages that name a cause.
fn attach_hint(error: &std::io::Error) -> &'static str {
    // 16 on every architecture Linux supports; not a constant worth importing.
    const EBUSY: i32 = 16;
    if error.raw_os_error() != Some(EBUSY) {
        return "";
    }
    "\n      Busy here has two causes and this cannot tell them apart: the \
     kernel\n      has no trampoline support — CONFIG_FUNCTION_TRACER, and \
     the\n      CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS three dependencies \
     under it —\n      or something already holds a direct call on that hook."
}

/// Where the kernel publishes its own type information.
pub const KERNEL_BTF: &str = "/sys/kernel/btf/vmlinux";

/// What came out of a successful load.
///
/// Holding these descriptors is what keeps the maps and links alive when they
/// are not pinned. Dropping this detaches everything, which is what makes a
/// partial failure clean up after itself.
pub struct Loaded {
    pub maps: Vec<(String, OwnedFd)>,
    /// One link per program. These are the thing that makes it live.
    pub links: Vec<(String, OwnedFd)>,
}

/// The running kernel's types, read from bpffs.
pub fn kernel_btf() -> Result<Btf> {
    let bytes = std::fs::read(KERNEL_BTF).map_err(|source| LoadError::KernelBtf {
        path: KERNEL_BTF.to_string(),
        source,
    })?;
    Ok(Btf::parse(&bytes)?)
}

/// The BTF id of a kernel function, which is how an LSM program says where it
/// attaches.
///
/// Searched by kind as well as by name: a struct and a function can share a
/// name, and handing the kernel a struct's id produces a failure about the
/// attach type rather than about the hook.
fn hook_id(kernel: &Btf, name: &str) -> Result<u32> {
    kernel
        .ids()
        .find(|id| {
            kernel
                .type_of(*id)
                .is_ok_and(|t| t.kind == btf_kind::FUNC && t.name == name)
        })
        .ok_or_else(|| LoadError::NoSuchHook(name.to_string()))
}

/// Read, relocate, load, and attach.
pub fn load(object: &[u8], kernel: &Btf) -> Result<Loaded> {
    let elf = Elf::parse(object)?;
    let local = Btf::parse(elf.section(".BTF").ok_or(LoadError::NoBtf)?.bytes)?;

    // The licence the object declares, which the kernel checks before it will
    // let the program use a GPL-only helper. Read rather than assumed: an
    // object whose licence string was lost would otherwise fail deep inside
    // the verifier with a message about a helper.
    let license = elf
        .section("license")
        .map(|s| {
            let end = s
                .bytes
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(s.bytes.len());
            String::from_utf8_lossy(&s.bytes[..end]).into_owned()
        })
        .unwrap_or_default();

    // 1. Maps.
    let mut map_fds: Vec<(String, OwnedFd)> = Vec::new();
    for spec in maps::declared(&local)? {
        let descriptor = thalyx_syscall::bpf_map_create(
            &spec.name,
            spec.map_type,
            spec.key_size,
            spec.value_size,
            spec.max_entries,
            spec.flags,
        )
        .map_err(|source| LoadError::MapCreate {
            name: spec.name.clone(),
            source,
        })?;
        map_fds.push((spec.name, descriptor));
    }

    let by_name = |wanted: &str| -> Option<i32> {
        use std::os::fd::AsRawFd;
        map_fds
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, fd)| fd.as_raw_fd())
    };

    let relocations = elf
        .section(".BTF.ext")
        .map(|s| core::relocations(s.bytes, &local))
        .transpose()?
        .unwrap_or_default();

    // 2-5, per program. Links are collected here and dropped together on any
    // failure, so a half-attached enforcement never survives this function.
    let mut links = Vec::new();
    for mut spec in program::programs(&elf)? {
        if let Some((_, entries)) = relocations.iter().find(|(name, _)| *name == spec.section) {
            core::apply(
                &mut spec.instructions,
                &spec.section,
                entries,
                &local,
                kernel,
            )?;
        }

        spec.relocate_maps(&by_name)?;

        let attach_btf_id = hook_id(kernel, &spec.attach_to)?;
        let program = thalyx_syscall::bpf_prog_load(
            &spec.name,
            thalyx_syscall::BPF_PROG_TYPE_LSM,
            thalyx_syscall::BPF_LSM_MAC,
            attach_btf_id,
            &spec.instructions,
            &license,
        )
        .map_err(|rejection| LoadError::Verifier {
            name: spec.name.clone(),
            rejection,
        })?;

        let link = thalyx_syscall::bpf_attach_lsm(program.as_fd()).map_err(|source| {
            LoadError::Attach {
                name: spec.name.clone(),
                source,
            }
        })?;
        // The program descriptor is dropped here on purpose: the link holds a
        // reference to the program, so closing this one frees nothing. Keeping
        // it would only make the process's descriptor table say something the
        // kernel does not.
        drop(program);
        links.push((spec.name, link));
    }

    Ok(Loaded {
        maps: map_fds,
        links,
    })
}

impl Loaded {
    /// Put every map and link into bpffs, so they outlive this process.
    ///
    /// Maps go under `<root>/maps/<name>` because that is where `thalyx-permd`
    /// looks, and links under `<root>/links/<name>` so that `unload` can find
    /// them. The two directories are separate because removing a link detaches
    /// enforcement while removing a map destroys the policy, and those should
    /// not be one `rm` apart.
    pub fn pin(&self, root: &Path) -> Result<()> {
        let maps_dir = root.join("maps");
        let links_dir = root.join("links");
        for directory in [&maps_dir, &links_dir] {
            std::fs::create_dir_all(directory).map_err(|source| LoadError::Pin {
                path: directory.display().to_string(),
                source,
            })?;
        }

        // Maps first. A link pinned before its map would leave a window in
        // which enforcement is live and permd cannot reach the policy — short,
        // and exactly the window in which a module could start.
        for (name, descriptor) in &self.maps {
            let path = maps_dir.join(name);
            thalyx_syscall::bpf_obj_pin(descriptor.as_fd(), &path).map_err(|source| {
                LoadError::Pin {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        for (name, descriptor) in &self.links {
            let path = links_dir.join(name);
            thalyx_syscall::bpf_obj_pin(descriptor.as_fd(), &path).map_err(|source| {
                LoadError::Pin {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    #[test]
    fn the_licence_the_kernel_will_check_is_read_out_of_the_object() {
        // An LSM program uses GPL-only helpers. A lost licence string fails
        // deep in the verifier with a message about a helper, which sends the
        // reader to the wrong file entirely.
        let elf = Elf::parse(CAPTURED).unwrap();
        let license = elf.section("license").unwrap();
        let end = license.bytes.iter().position(|b| *b == 0).unwrap();
        assert_eq!(&license.bytes[..end], b"GPL");
    }

    #[test]
    fn a_hook_this_kernel_does_not_have_is_named_rather_than_guessed_at() {
        // The object's own BTF stands in for a kernel: it has no
        // `bpf_lsm_file_open`, because that function is the kernel's and not
        // the program's. The message has to say which hook — "cannot attach"
        // with no name is the failure this project keeps writing rules about.
        let elf = Elf::parse(CAPTURED).unwrap();
        let local = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        let error = hook_id(&local, "bpf_lsm_file_open")
            .expect_err("the object's own BTF has no kernel hooks in it");
        assert!(error.to_string().contains("bpf_lsm_file_open"), "{error}");
    }

    #[test]
    fn a_kernel_with_no_btf_is_reported_as_that_and_not_as_a_broken_object() {
        // Two failures that a single message would conflate: an object this
        // cannot read, and a kernel that publishes nothing to read it against.
        // The second means CONFIG_DEBUG_INFO_BTF is off, and the fix is a
        // kernel rebuild rather than anything in this repository.
        if std::path::Path::new(KERNEL_BTF).exists() {
            return; // The claim is about the absent case.
        }
        let Err(error) = kernel_btf() else {
            panic!("this machine publishes no BTF and kernel_btf() succeeded");
        };
        assert!(error.to_string().contains(KERNEL_BTF), "{error}");
    }

    #[test]
    fn a_busy_attach_says_what_busy_means_here_and_that_it_cannot_choose() {
        // "Resource busy" reads as somebody else holding the hook, and on
        // 2026-08-04 nobody was: the kernel had no trampoline support. The
        // machine that hit it has no shell to investigate with, so the errno
        // has to arrive with what it can mean.
        let error = LoadError::Attach {
            name: "thalyx_socket_connect".to_string(),
            source: std::io::Error::from_raw_os_error(16),
        };
        let text = error.to_string();
        assert!(text.contains("CONFIG_FUNCTION_TRACER"), "{text}");
        assert!(
            text.contains("already holds a direct call"),
            "only one of the two causes is named: {text}"
        );
    }

    #[test]
    fn another_errno_gets_no_paragraph_about_trampolines() {
        // The control. Without it the paragraph could be unconditional, and
        // every attach failure would send the reader to the kernel
        // configuration — including the ones that have nothing to do with it.
        let error = LoadError::Attach {
            name: "thalyx_file_open".to_string(),
            source: std::io::Error::from_raw_os_error(1),
        };
        let text = error.to_string();
        assert!(!text.contains("CONFIG_FUNCTION_TRACER"), "{text}");
        assert!(text.contains("in nobody's path"), "{text}");
    }
}
