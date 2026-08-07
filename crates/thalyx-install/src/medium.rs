//! Finding the medium this machine was booted from, and reading the kernel off it.
//!
//! ## The problem this exists for
//!
//! `thalyx install` needs a kernel to put on the disk it is installing onto, and on
//! a development machine a person types the path to one. **Inside the machine there
//! is no path to type**: the image holds the Linux kernel and one program, there is
//! no shell, and the only kernel anywhere near is the one the firmware loaded — which
//! is on the medium, in a filesystem nothing has mounted.
//!
//! So the installer reads it. That means a FAT **reader**, which is the one thing
//! `fat` said it was not, and a way to decide which of the machine's disks is the
//! medium.
//!
//! ## How the medium is identified, and why it is not a guess
//!
//! By looking for a FAT32 volume **labelled `THALYX`** that carries
//! `\EFI\BOOT\BOOTX64.EFI`, and **refusing when more than one device has one**.
//!
//! The label is not decoration and it is not belt-and-braces. `\EFI\BOOT\BOOTX64.EFI`
//! is the removable-media fallback from the UEFI specification, which means it is the
//! path on *every* boot medium anybody has ever made: the EFI system partition of the
//! machine you are sitting at has it, a Windows installer stick has it, a Fedora
//! stick has it. Asked for on its own it is not a marker of Thalyx, it is a marker of
//! UEFI — and on 2026-08-07 that is exactly what happened. Stage 20 installed a
//! second disk with no `--kernel`, the search found the *host's* ESP, and Thalyx
//! copied somebody else's boot loader onto the disk and reported an install. The one
//! check that caught it was the byte comparison at the end.
//!
//! The label is what Thalyx itself writes, in [`fat::LABEL`], and it is the same
//! shape as finding the store by its own label. It obeys the same rule
//! `store_disk.rs` sets: what is forbidden is *"try /dev/vda, then /dev/sda, and take
//! the first that answers"*, because that heuristic finds the wrong disk exactly once
//! and the cost is a machine overwritten. Asking for a name Thalyx wrote is not that
//! — and two answers to a name is refused rather than resolved.
//!
//! **The disk being installed onto is excluded from the search.** Re-installing over
//! a machine that already has Thalyx would otherwise find two boot files — the
//! medium's and the target's own — and refuse, which would make the second install
//! of any machine impossible.
//!
//! ## What this deliberately does not do
//!
//! It does not read the GPT to find partitions of the EFI system type. That would be
//! a second reader for a second format, to narrow a search that is already narrow,
//! and it would answer a different question: a partition can carry that type and
//! hold nothing. What has to be true is that the file is there, so that is what is
//! asked.
//!
//! It also mounts nothing. `mount(2)` on a vfat volume needs the kernel to have
//! `CONFIG_VFAT_FS`, which this image does not ask for and does not need — the bytes
//! are read directly, the same way they were written.

use crate::fat;
use crate::fat::attr;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum MediumError {
    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "no block device on this machine holds a Thalyx boot medium: a FAT32 volume \
         labelled `{label}` with {path} on it.\n  \
         That is not the same as not looking — {looked} device(s) were read.{strangers}\n  \
         If this machine was booted some other way, name the kernel with --kernel."
    )]
    NotFound {
        path: String,
        label: &'static str,
        looked: usize,
        /// The volumes that carry the boot file under another label, named. Rule 10:
        /// "nothing was found" and "four things were found and none of them is
        /// Thalyx's" send a person to different halves of the problem.
        strangers: String,
    },

    #[error(
        "{count} devices hold a Thalyx boot medium, and choosing between them would \
         be guessing which one this machine started from:\n{names}\n  \
         Name the kernel with --kernel instead."
    )]
    Ambiguous { count: usize, names: String },

    #[error("{path} holds no {what}")]
    NoBootFile { path: PathBuf, what: String },

    #[error(
        "{path} looks like a FAT32 volume and its {what}. A boot medium Thalyx wrote \
         does not look like this."
    )]
    Malformed { path: PathBuf, what: String },
}

/// A FAT32 volume, opened for reading.
///
/// Reading only. Everything that writes one is in [`crate::fat`], and keeping the
/// two apart is what stops a repair path appearing here by accident: this runs
/// against a medium a human is holding, and the only correct thing to do to that
/// medium is nothing.
pub struct Volume {
    file: std::fs::File,
    path: PathBuf,
    geometry: fat::Geometry,
    /// The label as the boot sector states it, kept from the sector already read.
    ///
    /// The fallback, not the answer: the root directory's volume-label entry is the
    /// one `blkid` and `dosfslabel` treat as authoritative, and a volume relabelled
    /// after it was made has the new name there and the old name here.
    boot_label: [u8; 11],
}

impl Volume {
    /// Open `device` as FAT32, or answer `None` if it is not one.
    ///
    /// `None` and an error are different facts and are kept apart: most of the
    /// devices on a machine are not FAT32 and that is entirely normal, while a
    /// device that cannot be read at all is worth saying out loud.
    pub fn open(device: &Path) -> Result<Option<Self>, MediumError> {
        let io = |what: &'static str| {
            let path = device.to_path_buf();
            move |source| MediumError::Io { what, path, source }
        };

        let mut file = std::fs::File::open(device).map_err(io("opening"))?;
        let mut boot = [0u8; fat::SECTOR as usize];
        // A short read is not a FAT32 volume; it is a device smaller than one
        // sector, which happens for things like /dev/loop-control.
        if file.read_exact(&mut boot).is_err() {
            return Ok(None);
        }

        let word = |at: usize| u16::from_le_bytes(boot[at..at + 2].try_into().unwrap());
        let long = |at: usize| u32::from_le_bytes(boot[at..at + 4].try_into().unwrap());

        // Every one of these is a way of not being FAT32, and none of them is an
        // error. The order matters only in that the cheapest come first.
        if word(510) != 0xAA55 || word(11) != fat::SECTOR as u16 {
            return Ok(None);
        }
        let sectors_per_cluster = u64::from(boot[13]);
        let reserved_sectors = u64::from(word(14));
        let fats = u64::from(boot[16]);
        // FAT32 is the one where the 16-bit fields are zero and the 32-bit ones are
        // used. A volume with a root-entry count is FAT12 or FAT16, whatever the
        // string at offset 82 claims — the fields are the format and the string is
        // documentation.
        if word(17) != 0 || word(19) != 0 || word(22) != 0 {
            return Ok(None);
        }
        let sectors = u64::from(long(32));
        let fat_sectors = u64::from(long(36));
        let root_cluster = long(44);

        if sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || fats == 0
            || fats > 2
            || reserved_sectors == 0
            || fat_sectors == 0
            || sectors == 0
            || root_cluster < 2
        {
            return Ok(None);
        }

        // Fail closed, per rule 9. A volume whose data area does not fit inside it is
        // not a volume to start following cluster chains through: the offsets would
        // land past the end of the device, or worse, inside something else on it.
        let data_start = reserved_sectors + fats * fat_sectors;
        let Some(data) = sectors.checked_sub(data_start) else {
            return Err(MediumError::Malformed {
                path: device.to_path_buf(),
                what: format!("reserved region and FATs take {data_start} sectors of {sectors}"),
            });
        };
        let clusters = data / sectors_per_cluster;
        if (clusters + 2) * 4 > fat_sectors * fat::SECTOR {
            return Err(MediumError::Malformed {
                path: device.to_path_buf(),
                what: format!(
                    "FAT of {fat_sectors} sector(s) cannot describe its {clusters} clusters"
                ),
            });
        }

        let mut boot_label = [b' '; 11];
        boot_label.copy_from_slice(&boot[71..82]);

        Ok(Some(Self {
            file,
            path: device.to_path_buf(),
            boot_label,
            geometry: fat::Geometry {
                sectors,
                fat_sectors,
                clusters,
                reserved_sectors,
                sectors_per_cluster,
                fats,
                root_cluster,
            },
        }))
    }

    fn at(&mut self, offset: u64, into: &mut [u8]) -> Result<(), MediumError> {
        let io = |what: &'static str| {
            let path = self.path.clone();
            move |source| MediumError::Io { what, path, source }
        };
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(io("seeking in"))?;
        self.file.read_exact(into).map_err(io("reading"))
    }

    /// The next cluster in a chain, or `None` at the end of it.
    ///
    /// Anything from `0x0FFFFFF8` up is an end mark, not just the one value a writer
    /// happens to put there. A reader that compared against a single number would run
    /// off the end of volumes other tools wrote.
    fn next(&mut self, cluster: u32) -> Result<Option<u32>, MediumError> {
        let mut entry = [0u8; 4];
        self.at(self.geometry.fat_entry_at(cluster), &mut entry)?;
        let next = u32::from_le_bytes(entry) & 0x0FFF_FFFF;
        // Below 2 is a reserved entry and 0x0FFFFFF8 up is an end mark; past the
        // volume's own cluster count is a chain pointing outside the device, which
        // is damage and is treated as the end rather than followed.
        if !(2..0x0FFF_FFF8).contains(&next) || u64::from(next) >= self.geometry.clusters + 2 {
            return Ok(None);
        }
        Ok(Some(next))
    }

    /// Walk the records of the directory starting at `cluster` until `visit` answers.
    ///
    /// Deleted entries and long-filename slots never reach `visit`. Long names are
    /// skipped rather than parsed: everything looked for here has an 8.3 name by
    /// construction, and every long name also has a short one right after its slots.
    fn records<T>(
        &mut self,
        cluster: u32,
        mut visit: impl FnMut(&[u8]) -> Option<T>,
    ) -> Result<Option<T>, MediumError> {
        let mut at = Some(cluster);
        let mut seen = 0u64;
        while let Some(current) = at {
            // Bounded by the volume's own cluster count. A FAT with a loop in it —
            // damage, or a volume half-written by something that crashed — would
            // otherwise spin here forever inside PID 1.
            seen += 1;
            if seen > self.geometry.clusters + 2 {
                return Err(MediumError::Malformed {
                    path: self.path.clone(),
                    what: "directory chain is longer than the volume has clusters".into(),
                });
            }

            let mut content = vec![0u8; self.geometry.cluster_bytes() as usize];
            self.at(self.geometry.cluster_at(current), &mut content)?;
            for record in content.chunks_exact(32) {
                match record[0] {
                    // A zero first byte ends the directory: nothing after it has ever
                    // been used.
                    0x00 => return Ok(None),
                    // Deleted.
                    0xE5 => continue,
                    _ => {}
                }
                if record[11] & 0x0F == 0x0F {
                    continue;
                }
                if let Some(answer) = visit(record) {
                    return Ok(Some(answer));
                }
            }
            at = self.next(current)?;
        }
        Ok(None)
    }

    /// Find one 8.3 name in the directory starting at `cluster`.
    ///
    /// Returns the entry's first cluster and its recorded size.
    fn entry(&mut self, cluster: u32, name: &[u8; 11]) -> Result<Option<(u32, u32)>, MediumError> {
        self.records(cluster, |record| {
            // The volume label lives in the root directory as an entry whose name
            // field is the label, so a volume called `EFI` would otherwise be
            // followed as if it were the directory of that name — into cluster
            // zero, which is not a cluster.
            if record[11] & attr::VOLUME != 0 || &record[0..11] != name {
                return None;
            }
            let high = u16::from_le_bytes(record[20..22].try_into().unwrap());
            let low = u16::from_le_bytes(record[26..28].try_into().unwrap());
            let size = u32::from_le_bytes(record[28..32].try_into().unwrap());
            Some(((u32::from(high) << 16) | u32::from(low), size))
        })
    }

    /// What the volume calls itself.
    ///
    /// The root directory's volume-label entry, and the boot sector's field only if
    /// there is no such entry. That order is `blkid`'s and `mkfs.vfat` writes both,
    /// so the two agree on everything Thalyx makes — it matters for a volume somebody
    /// relabelled, where the boot sector keeps the name the volume was born with.
    pub fn label(&mut self) -> Result<String, MediumError> {
        let root = self.geometry.root_cluster;
        let found = self.records(root, |record| {
            (record[11] & attr::VOLUME != 0).then(|| {
                let mut bytes = [b' '; 11];
                bytes.copy_from_slice(&record[0..11]);
                bytes
            })
        })?;
        let bytes = found.unwrap_or(self.boot_label);
        // Trailing spaces are padding, not part of the name, and a label read with
        // them on would never compare equal to anything a person or this crate wrote.
        Ok(String::from_utf8_lossy(&bytes).trim_end().to_string())
    }

    /// Whether this volume holds the file at [`fat::BOOT_PATH`], and how big it is.
    pub fn boot_file(&mut self) -> Result<Option<u32>, MediumError> {
        let mut cluster = self.geometry.root_cluster;
        for (index, component) in fat::BOOT_PATH.iter().enumerate() {
            let name = short_name(component);
            let Some((next, size)) = self.entry(cluster, &name)? else {
                return Ok(None);
            };
            if index + 1 == fat::BOOT_PATH.len() {
                return Ok(Some(size));
            }
            cluster = next;
        }
        Ok(None)
    }

    /// Copy the boot file out to `destination`.
    pub fn extract_boot_file(&mut self, destination: &Path) -> Result<u64, MediumError> {
        use std::io::Write;

        let mut cluster = self.geometry.root_cluster;
        let mut size = 0u32;
        for (index, component) in fat::BOOT_PATH.iter().enumerate() {
            let name = short_name(component);
            let found = self.entry(cluster, &name)?.ok_or(MediumError::NoBootFile {
                path: self.path.clone(),
                what: fat::BOOT_PATH.join("\\"),
            })?;
            cluster = found.0;
            size = found.1;
            if index + 1 == fat::BOOT_PATH.len() {
                break;
            }
        }

        let io = |what: &'static str, path: &Path| {
            let path = path.to_path_buf();
            move |source| MediumError::Io { what, path, source }
        };
        let mut out = std::fs::File::create(destination).map_err(io("creating", destination))?;

        let cluster_bytes = self.geometry.cluster_bytes();
        let mut written = 0u64;
        let mut at = Some(cluster);
        while let Some(current) = at {
            if written >= u64::from(size) {
                break;
            }
            let mut content = vec![0u8; cluster_bytes as usize];
            self.at(self.geometry.cluster_at(current), &mut content)?;
            // Bounded by the recorded size and not by the chain: the last cluster of
            // a file is only partly the file, and copying all of it would produce a
            // kernel with up to a cluster of rubbish glued on the end.
            let want = std::cmp::min(cluster_bytes, u64::from(size) - written) as usize;
            out.write_all(&content[..want])
                .map_err(io("writing", destination))?;
            written += want as u64;
            at = self.next(current)?;
        }

        if written != u64::from(size) {
            return Err(MediumError::Malformed {
                path: self.path.clone(),
                what: format!("directory says {size} bytes and the cluster chain holds {written}"),
            });
        }
        out.sync_all().map_err(io("flushing", destination))?;
        Ok(written)
    }
}

/// An 8.3 name as eleven bytes. The three components of [`fat::BOOT_PATH`] are all
/// representable, which `fat`'s own tests assert.
fn short_name(name: &str) -> [u8; 11] {
    let (base, extension) = name.split_once('.').unwrap_or((name, ""));
    let mut field = [b' '; 11];
    let base = base.as_bytes();
    let extension = extension.as_bytes();
    field[..base.len().min(8)].copy_from_slice(&base[..base.len().min(8)]);
    field[8..8 + extension.len().min(3)].copy_from_slice(&extension[..extension.len().min(3)]);
    field
}

/// A device that turned out to hold a Thalyx boot medium.
pub struct Found {
    pub device: PathBuf,
    pub kernel_bytes: u32,
}

/// Look for the medium this machine was started from.
///
/// `except` names a disk to leave out — the one being installed onto, whose own boot
/// partition would otherwise be a second answer and make the search refuse.
pub fn find(except: Option<&Path>) -> Result<Found, MediumError> {
    let excluded = match except {
        Some(disk) => crate::partitions::of(disk).unwrap_or_default(),
        None => Vec::new(),
    };

    let mut found: Vec<Found> = Vec::new();
    // Volumes that carry the boot file under some other name. Kept so that the
    // failure can say "your EFI partition is not a Thalyx medium" instead of
    // "nothing was found", which are different problems with different answers.
    let mut strangers: Vec<(PathBuf, String)> = Vec::new();
    let mut looked = 0usize;
    for device in crate::partitions::every() {
        if Some(device.as_path()) == except || excluded.iter().any(|(_, path)| *path == device) {
            continue;
        }
        looked += 1;
        // A device that cannot be opened is skipped rather than fatal. On a real
        // machine this list holds things like an empty card reader, and one of those
        // must not stop an install.
        let Ok(Some(mut volume)) = Volume::open(&device) else {
            continue;
        };
        let Ok(Some(kernel_bytes)) = volume.boot_file() else {
            continue;
        };
        // Fail closed, rule 9: a label that cannot be read is not this machine's
        // medium. The cautious answer is to refuse and let a person name the kernel,
        // never to install whatever was on the unreadable volume.
        let label = volume.label().unwrap_or_default();
        if label != fat::LABEL {
            strangers.push((device, label));
            continue;
        }
        found.push(Found {
            device,
            kernel_bytes,
        });
    }

    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(MediumError::NotFound {
            path: fat::BOOT_PATH.join("\\"),
            label: fat::LABEL,
            looked,
            strangers: if strangers.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = strangers
                    .iter()
                    .map(|(device, label)| {
                        let called = if label.is_empty() {
                            "no label".to_string()
                        } else {
                            format!("labelled `{label}`")
                        };
                        format!("    {}  {called}", device.display())
                    })
                    .collect();
                format!(
                    "\n  {} of them carry that file and belong to something else — an EFI \
                     system\n  partition looks exactly like a boot medium, which is why the \
                     label is\n  asked for:\n{}",
                    strangers.len(),
                    names.join("\n")
                )
            },
        }),
        count => Err(MediumError::Ambiguous {
            count,
            names: found
                .iter()
                .map(|one| format!("    {}", one.device.display()))
                .collect::<Vec<_>>()
                .join("\n"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A volume written by this crate's writer, to be read by this crate's reader.
    fn written(kernel: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("bzImage");
        std::fs::write(&source, kernel).unwrap();

        let image = dir.path().join("esp.img");
        let sectors = 512 * 1024 * 1024 / fat::SECTOR;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&image)
            .unwrap();
        file.set_len(sectors * fat::SECTOR).unwrap();
        fat::write(&image, &mut file, sectors, &source, 1_786_106_096).unwrap();
        (dir, image)
    }

    #[test]
    fn the_reader_gets_back_exactly_what_the_writer_put_in() {
        // Round-tripping proves the pair agrees with itself and **not** that either
        // agrees with FAT. That is worth saying out loud rather than leaving to be
        // assumed: what establishes the format is Linux mounting what the writer
        // produced, in stage 20, and this reader running against the medium a
        // firmware actually booted from.
        //
        // What it does catch, and it is the mistake most likely to be made here, is a
        // reader that follows the cluster chain wrongly — because the kernel below is
        // several clusters long and each byte says which one it came from.
        let kernel: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let (dir, image) = written(&kernel);

        let mut volume = Volume::open(&image).unwrap().expect("not read as FAT32");
        assert_eq!(volume.boot_file().unwrap(), Some(kernel.len() as u32));

        let out = dir.path().join("recovered");
        let bytes = volume.extract_boot_file(&out).unwrap();
        assert_eq!(bytes, kernel.len() as u64);
        assert_eq!(std::fs::read(&out).unwrap(), kernel);
    }

    #[test]
    fn a_file_that_ends_mid_cluster_comes_back_at_its_recorded_length() {
        // The off-by-a-cluster this reader would otherwise make. A kernel with up to
        // four kilobytes of whatever followed it glued on the end is still a file a
        // firmware will load and jump into, so nothing would report the mistake until
        // a machine failed to boot.
        let kernel: Vec<u8> = (0..(4096 * 3 + 7u32)).map(|i| (i % 251) as u8).collect();
        let (dir, image) = written(&kernel);
        let mut volume = Volume::open(&image).unwrap().unwrap();
        let out = dir.path().join("recovered");
        volume.extract_boot_file(&out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap().len(), kernel.len());
        assert_eq!(std::fs::read(&out).unwrap(), kernel);
    }

    #[test]
    fn a_kernel_of_the_size_a_kernel_actually_is_comes_back_whole() {
        // The tests above run on three megabytes, and `image/build/bzImage` is four
        // times that. Written because on 2026-08-07 a real install copied the wrong
        // file and there were two candidate explanations — the search picking
        // somebody else's ESP, or this reader losing its way on a file bigger than
        // anything it had been asked for. This rules out the second one, so the
        // first cannot be assumed to have been fixed by fixing it.
        let kernel: Vec<u8> = (0..12_582_912u32).map(|i| (i % 251) as u8).collect();
        let (dir, image) = written(&kernel);

        let mut volume = Volume::open(&image).unwrap().unwrap();
        let out = dir.path().join("recovered");
        assert_eq!(volume.extract_boot_file(&out).unwrap(), kernel.len() as u64);
        assert_eq!(std::fs::read(&out).unwrap(), kernel);
    }

    #[test]
    fn a_volume_that_carries_the_boot_file_is_not_a_thalyx_medium_unless_it_says_so() {
        // The defect this whole label check exists for, modelled: a FAT32 volume with
        // \EFI\BOOT\BOOTX64.EFI on it and another operating system's name on the
        // outside. Every UEFI machine on earth has one of these attached, so a search
        // that asked only for the file would find it — and did, on 2026-08-07, and
        // installed a stranger's boot loader onto a disk while reporting success.
        let kernel: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let (_dir, image) = written(&kernel);

        let mut volume = Volume::open(&image).unwrap().unwrap();
        assert_eq!(volume.label().unwrap(), fat::LABEL);
        assert!(volume.boot_file().unwrap().is_some());
        drop(volume);

        // Relabelled the way `dosfslabel` does it: the root directory's volume entry,
        // which is the one that counts. The boot sector keeps saying THALYX, so this
        // also establishes which of the two the reader believes.
        let geometry = fat::Geometry::of(512 * 1024 * 1024 / fat::SECTOR).unwrap();
        let mut bytes = std::fs::read(&image).unwrap();
        let root = geometry.cluster_at(2) as usize;
        assert_eq!(bytes[root + 11] & fat::attr::VOLUME, fat::attr::VOLUME);
        bytes[root..root + 11].copy_from_slice(b"NO NAME    ");
        std::fs::write(&image, &bytes).unwrap();

        let mut volume = Volume::open(&image).unwrap().unwrap();
        assert_eq!(volume.label().unwrap(), "NO NAME");
        // And it still has the file. That is the point: the file is not the marker.
        assert!(volume.boot_file().unwrap().is_some());
    }

    #[test]
    fn a_volume_with_no_label_entry_falls_back_to_the_one_in_the_boot_sector() {
        // Rule 10 applied to a name: a volume whose root directory has no label entry
        // has not been found to be nameless — it has been found to keep its name
        // somewhere else. Reporting it as unlabelled would refuse a medium that says
        // THALYX on it in the one place every reader looks first.
        let (_dir, image) = written(b"a kernel this is not");

        let geometry = fat::Geometry::of(512 * 1024 * 1024 / fat::SECTOR).unwrap();
        let mut bytes = std::fs::read(&image).unwrap();
        let root = geometry.cluster_at(2) as usize;
        // Delete it the way FAT deletes anything, so the entry after it — the EFI
        // directory — is still found.
        bytes[root] = 0xE5;
        std::fs::write(&image, &bytes).unwrap();

        let mut volume = Volume::open(&image).unwrap().unwrap();
        assert_eq!(volume.label().unwrap(), fat::LABEL);
        assert!(volume.boot_file().unwrap().is_some());
    }

    #[test]
    fn a_device_that_is_not_fat32_is_not_an_error() {
        // Most of a machine's block devices are not FAT32 and that is completely
        // ordinary. Returning an error for each would make the search below fail on
        // every machine that has a disk.
        let dir = tempfile::tempdir().unwrap();

        let empty = dir.path().join("zeroes.img");
        std::fs::File::create(&empty)
            .unwrap()
            .set_len(64 * 1024 * 1024)
            .unwrap();
        assert!(Volume::open(&empty).unwrap().is_none());

        // Too short to hold a boot sector at all.
        let tiny = dir.path().join("tiny");
        std::fs::write(&tiny, b"no").unwrap();
        assert!(Volume::open(&tiny).unwrap().is_none());

        // And a Btrfs filesystem, which is what the *other* partition of every
        // Thalyx disk is — so this is the case the search meets on every real run.
        let store = dir.path().join("store.img");
        std::fs::File::create(&store)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        thalyx_btrfs::write(
            &store,
            thalyx_btrfs::LABEL,
            &thalyx_btrfs::Uuids::random(),
            0,
        )
        .unwrap();
        assert!(Volume::open(&store).unwrap().is_none());
    }

    #[test]
    fn a_fat32_volume_with_no_boot_file_is_told_apart_from_one_that_has_it() {
        // The control for the search. A reader that answered "yes" for any FAT32
        // volume would pick the first one on the machine, and on a laptop that is
        // somebody else's EFI partition.
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("something");
        std::fs::write(&source, b"not a kernel").unwrap();

        let image = dir.path().join("esp.img");
        let sectors = 512 * 1024 * 1024 / fat::SECTOR;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&image)
            .unwrap();
        file.set_len(sectors * fat::SECTOR).unwrap();
        fat::write(&image, &mut file, sectors, &source, 0).unwrap();

        // It does have the file — the writer always puts it at that path. So the
        // negative case is built by blanking the directory entry's name, which is
        // what a volume somebody else formatted looks like from here.
        let mut volume = Volume::open(&image).unwrap().unwrap();
        assert!(volume.boot_file().unwrap().is_some());
        drop(volume);

        let geometry = fat::Geometry::of(sectors).unwrap();
        let mut bytes = std::fs::read(&image).unwrap();
        // Cluster 4 is the BOOT directory; ending it at its first entry removes the
        // file without touching anything else.
        let at = geometry.cluster_at(4) as usize;
        bytes[at..at + 32 * 3].fill(0);
        std::fs::write(&image, &bytes).unwrap();

        let mut volume = Volume::open(&image).unwrap().unwrap();
        assert_eq!(volume.boot_file().unwrap(), None);
    }

    #[test]
    fn the_three_names_on_the_boot_path_survive_the_short_name_encoding() {
        assert_eq!(short_name("BOOTX64.EFI"), *b"BOOTX64 EFI");
        assert_eq!(short_name("EFI"), *b"EFI        ");
        assert_eq!(short_name("BOOT"), *b"BOOT       ");
    }
}
