//! The content identity of a tree, the same on every backend.
//!
//! ## Why a second identity, beside `thalyx_snapshot::Witness`
//!
//! The witness answers *has anything at all touched this tree*, and it answers
//! it strictly: modification time, change time and inode go into it, because an
//! identity that two writes in one filesystem tick can share would authorise
//! destroying somebody's work. That is the right question for authorising a
//! restore and the wrong one for naming a version — two trees with the same
//! bytes copied at different moments have different witnesses, so a witness can
//! never be what a validation and a publication agree on, and it can never be
//! compared across two machines.
//!
//! This is the other question: *what does this tree hold*. Paths, what kind of
//! thing each is, the bytes of every file, whether it is executable, where every
//! link points. Nothing the filesystem makes up — no times, no inodes, no owners
//! — so the same tree has the same identity on `linux-current`, on the managed
//! model and inside Thalyx-Kernel, which is what lets EXP-13 say two backends
//! produced the same thing rather than that they produced something.
//!
//! ## What it refuses
//!
//! A path that is not UTF-8 or that could not be read makes the manifest
//! incomplete, and an incomplete manifest has **no** identity. Rule 9 and rule
//! 10: a tree nobody could read everywhere is not a tree anybody may name.

use crate::state::Changes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// The prefix every content identity carries, so an identity made under other
/// rules is refused rather than compared.
pub const CONTENT_VERSION: &str = "c1";

/// The first line of an encoded tree.
pub const TREE_FORMAT: &str = "thalyx-tree-v1";

/// Skipped wherever it appears, for the reason `thalyx_snapshot` skips it: a
/// subvolume's snapshots live beside it, and a nested one would otherwise be
/// counted as part of the tree it is a copy of.
pub const SNAPSHOT_DIR: &str = ".thalyx-snapshots";

/// The digest of one object, labelled with what kind of object it is.
///
/// The label and the length are both inside the hash, so a file whose bytes
/// happen to be a valid encoded tree does not share an identity with that tree,
/// and no two kinds share a digest space. The length goes last so a file can be
/// hashed as it is read.
pub fn object_digest(kind: &str, bytes: &[u8]) -> String {
    let mut hasher = labelled(kind);
    hasher.update(bytes);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hex::encode(hasher.finalize())
}

fn labelled(kind: &str) -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(b"thalyx-object-v1\0");
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher
}

/// The `bytes` digest of a file, read once, and how long it was.
pub fn file_digest(path: &Path) -> std::io::Result<(String, u64)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = labelled("bytes");
    let mut buffer = vec![0u8; 64 * 1024];
    let mut length = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length += read as u64;
    }
    hasher.update(length.to_le_bytes());
    Ok((hex::encode(hasher.finalize()), length))
}

/// One path of a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    File {
        digest: String,
        len: u64,
        executable: bool,
    },
    Symlink {
        target: String,
    },
    Directory,
    /// A fifo, a socket, a device: a kind and nothing else, because opening one
    /// can block on a writer that never comes. A managed version refuses to hold
    /// one.
    Other,
}

impl Node {
    fn is_directory(&self) -> bool {
        matches!(self, Node::Directory)
    }
}

/// Everything a tree held when it was walked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub nodes: BTreeMap<String, Node>,
    pub unreadable: Vec<String>,
    /// Bytes of file content that were read. Reported, never compared.
    pub bytes: u64,
}

impl Manifest {
    /// Walk a tree. Links are recorded, never followed.
    pub fn of(root: &Path) -> Manifest {
        let mut manifest = Manifest::default();
        walk(root, root, &mut manifest);
        manifest
    }

    pub fn is_complete(&self) -> bool {
        self.unreadable.is_empty()
    }

    /// The paths that are neither files, links nor directories.
    pub fn others(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| matches!(node, Node::Other))
            .map(|(path, _)| path.as_str())
            .collect()
    }

    /// The canonical bytes of this tree.
    ///
    /// One line per path, in byte order of the path, with the path hex-encoded
    /// so that no name — one with a newline or a space in it — can be spelled
    /// as two.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = format!("{TREE_FORMAT}\n");
        for (path, node) in &self.nodes {
            let name = hex::encode(path.as_bytes());
            let line = match node {
                Node::File {
                    digest,
                    len,
                    executable,
                } => format!(
                    "F {digest} {len} {} {name}\n",
                    if *executable { "x" } else { "-" }
                ),
                Node::Symlink { target } => {
                    format!("L {} {name}\n", hex::encode(target.as_bytes()))
                }
                Node::Directory => format!("D {name}\n"),
                Node::Other => format!("O {name}\n"),
            };
            out.push_str(&line);
        }
        out.into_bytes()
    }

    /// Read an encoded tree, refusing anything that is not exactly one.
    ///
    /// A service that accepts a tree from a client is accepting a claim, so the
    /// claim is checked: the header, every line, byte order with no repeats,
    /// every component a real name, and every path's parent present as a
    /// directory. Thalyx-Kernel's persistence contract lists the same refusals.
    pub fn decode(bytes: &[u8]) -> Result<Manifest, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "a tree is not UTF-8".to_string())?;
        let mut lines = text.split_terminator('\n');
        if lines.next() != Some(TREE_FORMAT) {
            return Err(format!("a tree does not begin with `{TREE_FORMAT}`"));
        }
        let mut manifest = Manifest::default();
        let mut previous: Option<String> = None;
        for (number, line) in lines.enumerate() {
            let fields: Vec<&str> = line.split(' ').collect();
            let bad = |why: &str| format!("line {} of a tree: {why}", number + 2);
            let (node, name) = match fields.as_slice() {
                ["F", digest, len, mode, name] => {
                    if !is_digest(digest) {
                        return Err(bad("not a digest"));
                    }
                    let len = len.parse::<u64>().map_err(|_| bad("not a length"))?;
                    let executable = match *mode {
                        "x" => true,
                        "-" => false,
                        _ => return Err(bad("not a mode")),
                    };
                    (
                        Node::File {
                            digest: digest.to_string(),
                            len,
                            executable,
                        },
                        *name,
                    )
                }
                ["L", target, name] => {
                    let target = hex::decode(target).map_err(|_| bad("not a link target"))?;
                    let target =
                        String::from_utf8(target).map_err(|_| bad("a link target is not UTF-8"))?;
                    (Node::Symlink { target }, *name)
                }
                ["D", name] => (Node::Directory, *name),
                ["O", name] => (Node::Other, *name),
                _ => return Err(bad("not a line of a tree")),
            };
            let path = hex::decode(name).map_err(|_| bad("a path is not hex"))?;
            let path = String::from_utf8(path).map_err(|_| bad("a path is not UTF-8"))?;
            if !is_relative_name(&path) {
                return Err(bad("a path that is not a plain relative name"));
            }
            if let Some(before) = &previous
                && before.as_str() >= path.as_str()
            {
                return Err(bad("paths out of order or repeated"));
            }
            if let Some((parent, _)) = path.rsplit_once('/')
                && !manifest.nodes.get(parent).is_some_and(Node::is_directory)
            {
                return Err(bad("a path whose parent is not a directory of the tree"));
            }
            previous = Some(path.clone());
            manifest.nodes.insert(path, node);
        }
        Ok(manifest)
    }

    /// `c1-<digest>`, or nothing when the walk had holes in it.
    pub fn id(&self) -> Option<String> {
        self.is_complete().then(|| {
            format!(
                "{CONTENT_VERSION}-{}",
                object_digest("tree", &self.encode())
            )
        })
    }

    /// The file digests this tree needs to exist somewhere to be whole.
    pub fn file_digests(&self) -> Vec<String> {
        let mut digests: Vec<String> = self
            .nodes
            .values()
            .filter_map(|node| match node {
                Node::File { digest, .. } => Some(digest.clone()),
                _ => None,
            })
            .collect();
        digests.sort();
        digests.dedup();
        digests
    }

    /// What this tree gained, changed and lost against `base`.
    ///
    /// Directories are walked and never counted, exactly as
    /// `thalyx_snapshot::difference` does it, because `changed()` is what a
    /// program reads and it must say the same thing on every backend. What
    /// differs is how "modified" is decided: here by what the file holds, there
    /// by size and time — a rewrite with the same bytes is a change there and
    /// not here, and a same-size rewrite inside one clock tick is the reverse.
    pub fn changes_from(&self, base: &Manifest) -> Changes {
        let mut changes = Changes::default();
        for (path, node) in &self.nodes {
            if node.is_directory() {
                continue;
            }
            match base.nodes.get(path) {
                None | Some(Node::Directory) => changes.add(path),
                Some(before) if before != node => changes.modify(path),
                Some(_) => {}
            }
        }
        for (path, node) in &base.nodes {
            if node.is_directory() {
                continue;
            }
            if !self.nodes.get(path).is_some_and(|now| !now.is_directory()) {
                changes.remove(path);
            }
        }
        changes.unreadable = self.unreadable.clone();
        changes
    }
}

/// The hex digest inside an identity, when it is one this build makes.
pub fn digest_of_id(id: &str) -> Option<&str> {
    id.strip_prefix(CONTENT_VERSION)
        .and_then(|rest| rest.strip_prefix('-'))
        .filter(|digest| is_digest(digest))
}

pub fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_relative_name(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn walk(root: &Path, directory: &Path, manifest: &mut Manifest) {
    let relative = |path: &Path| -> Option<String> {
        let inside = path.strip_prefix(root).ok()?;
        let mut parts = Vec::new();
        for part in inside.components() {
            parts.push(part.as_os_str().to_str()?.to_string());
        }
        Some(parts.join("/"))
    };
    let lossy = |path: &Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    };

    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            manifest.unreadable.push(lossy(directory));
            return;
        }
    };
    for entry in entries {
        let Ok(entry) = entry else {
            manifest.unreadable.push(lossy(directory));
            continue;
        };
        if entry.file_name() == SNAPSHOT_DIR {
            continue;
        }
        let path = entry.path();
        // A name that is not UTF-8 cannot be spelled the same way twice by two
        // readers, so it is a hole in the tree rather than a guess at it.
        let Some(name) = relative(&path) else {
            manifest.unreadable.push(lossy(&path));
            continue;
        };
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                manifest.unreadable.push(name);
                continue;
            }
        };
        let kind = metadata.file_type();
        if kind.is_dir() {
            manifest.nodes.insert(name, Node::Directory);
            walk(root, &path, manifest);
        } else if kind.is_symlink() {
            match std::fs::read_link(&path)
                .ok()
                .and_then(|target| target.to_str().map(str::to_string))
            {
                Some(target) => {
                    manifest.nodes.insert(name, Node::Symlink { target });
                }
                None => manifest.unreadable.push(name),
            }
        } else if kind.is_file() {
            use std::os::unix::fs::PermissionsExt;
            match file_digest(&path) {
                Ok((digest, len)) => {
                    manifest.bytes = manifest.bytes.saturating_add(len);
                    manifest.nodes.insert(
                        name,
                        Node::File {
                            digest,
                            len,
                            executable: metadata.permissions().mode() & 0o111 != 0,
                        },
                    );
                }
                Err(_) => manifest.unreadable.push(name),
            }
        } else {
            manifest.nodes.insert(name, Node::Other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a directory");
        for (path, text) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().expect("a parent")).expect("parents");
            std::fs::write(full, text).expect("a file");
        }
        dir
    }

    #[test]
    fn the_same_bytes_at_different_moments_are_the_same_tree() {
        // The property the witness deliberately does not have, and the reason
        // this exists: a copy made later is a different witness and the same
        // version.
        let one = tree(&[
            ("src/lib.rs", "pub fn a() {}\n"),
            ("Cargo.toml", "[package]\n"),
        ]);
        std::thread::sleep(std::time::Duration::from_millis(15));
        let two = tree(&[
            ("Cargo.toml", "[package]\n"),
            ("src/lib.rs", "pub fn a() {}\n"),
        ]);
        let (a, b) = (Manifest::of(one.path()), Manifest::of(two.path()));
        assert!(a.id().is_some());
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn one_byte_one_bit_or_one_empty_directory_is_a_different_tree() {
        use std::os::unix::fs::PermissionsExt;
        let base = tree(&[("a.sh", "echo\n")]);
        let id = Manifest::of(base.path()).id();

        std::fs::write(base.path().join("a.sh"), "echo!\n").expect("rewrite");
        let rewritten = Manifest::of(base.path()).id();
        assert_ne!(id, rewritten);

        let mut mode = std::fs::metadata(base.path().join("a.sh"))
            .expect("stat")
            .permissions();
        mode.set_mode(0o755);
        std::fs::set_permissions(base.path().join("a.sh"), mode).expect("chmod");
        let executable = Manifest::of(base.path()).id();
        assert_ne!(
            rewritten, executable,
            "the executable bit is part of a version"
        );

        std::fs::create_dir(base.path().join("empty")).expect("mkdir");
        assert_ne!(executable, Manifest::of(base.path()).id());
    }

    #[test]
    fn a_tree_survives_its_own_encoding_and_a_forged_one_does_not() {
        let dir = tree(&[("a/b/c.txt", "c"), ("a/d.txt", "d")]);
        std::os::unix::fs::symlink("b/c.txt", dir.path().join("a/link")).expect("a link");
        let manifest = Manifest::of(dir.path());
        let back = Manifest::decode(&manifest.encode()).expect("its own encoding");
        assert_eq!(back.nodes, manifest.nodes);

        let escaping = format!(
            "{TREE_FORMAT}\nF {} 1 - {}\n",
            "0".repeat(64),
            hex::encode("../outside")
        );
        assert!(Manifest::decode(escaping.as_bytes()).is_err());

        let orphan = format!(
            "{TREE_FORMAT}\nF {} 1 - {}\n",
            "0".repeat(64),
            hex::encode("nowhere/file")
        );
        assert!(Manifest::decode(orphan.as_bytes()).is_err());
    }

    #[test]
    fn changes_count_files_and_never_directories() {
        let dir = tree(&[("keep.rs", "k"), ("edit.rs", "e"), ("gone.rs", "g")]);
        let base = Manifest::of(dir.path());
        std::fs::write(dir.path().join("edit.rs"), "E").expect("edit");
        std::fs::remove_file(dir.path().join("gone.rs")).expect("remove");
        std::fs::create_dir_all(dir.path().join("new/dir")).expect("dirs");
        std::fs::write(dir.path().join("new/dir/added.rs"), "a").expect("add");

        let changes = Manifest::of(dir.path()).changes_from(&base);
        assert_eq!(changes.added, vec!["new/dir/added.rs"]);
        assert_eq!(changes.modified, vec!["edit.rs"]);
        assert_eq!(changes.removed, vec!["gone.rs"]);
        assert_eq!(changes.count(), 3);
    }

    #[test]
    fn a_tree_that_could_not_be_read_everywhere_has_no_identity() {
        let manifest = Manifest {
            unreadable: vec!["secret".to_string()],
            ..Manifest::default()
        };
        assert_eq!(manifest.id(), None);
    }
}
