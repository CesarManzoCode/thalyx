//! `thalyx-net` — the network interfaces this machine has, and nothing it can
//! do with them.
//!
//! The decree is `vault/02-Arquitectura/Red.md`, point 8 of the usable terminal
//! and the last of the nine. Cesar decided it on 2026-08-23: **Thalyx learns to
//! see the network and not to use it.**
//!
//! Before this, none of the 110 options in `image/thalyx.config` was a network
//! card. `CONFIG_NET` was there so that the LSM could deny a module access to a
//! stack, and that is all it was there for.
//!
//! ## Why seeing and using are not two sizes of the same job
//!
//! The image carries the kernel and one program. In Linux, getting an address
//! and reaching the internet are separate programs — a DHCP client, a resolver,
//! a TLS library. Here they cannot be separate programs, so all of it would have
//! to live inside `thalyx`, written from nothing. And the thing it would buy —
//! a store that can fetch modules from somewhere — depends on a Phase 2 question
//! nobody has answered: from where.
//!
//! So this reads. It never writes, and it never sends a packet.
//!
//! ## The three things measured rather than quoted
//!
//! Every one of these came from reading `/sys/class/net` on a live machine, and
//! each one is a way the obvious implementation would have lied.
//!
//! 1. **`type` is a number.** `1` is Ethernet, `772` is loopback — the kernel's
//!    `ARPHRD_*` constants, which no userspace header hands over.
//!
//! 2. **An interface that is down does not say it has no cable. It says
//!    nothing.** Reading `carrier` on an interface the kernel has taken down
//!    fails with `EINVAL`; it does not return `0`. *No cable* and *cannot tell*
//!    are two different facts, and rule 10 of `Estrategia-de-Pruebas.md` exists
//!    because reporting the second as the first is how somebody spends an hour
//!    on a cable that was never the problem.
//!
//! 3. **`speed` has three states.** A number; `-1` when the link is up and the
//!    speed is not known, which is what a virtual device answers; and unreadable
//!    when the interface is down. Printing `-1 Mb/s` would be inventing a
//!    measurement.
//!
//! A wireless card is recognised by its `phy80211` link and **not** by `type`:
//! in ordinary managed mode a wireless interface also answers `1`, because it
//! presents itself to the system as Ethernet. Going by the type would file
//! somebody's laptop card under cable.

use std::path::{Path, PathBuf};

/// Where the kernel keeps them. A parameter everywhere below, so the tests can
/// read a tree this repository wrote instead of the machine they run on.
pub const SYS_CLASS_NET: &str = "/sys/class/net";

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error(
        "cannot read {path}: {source}.\n  \
         Without it there is no way to know what interfaces exist. On the image \
         this directory is always there; under another system it may not be \
         mounted."
    )]
    NoSysfs {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What the kernel says a device is, from its `type` file.
///
/// Kept as the number for anything unrecognised rather than folded into an
/// "other": a machine that reports `type 512` is telling you something, and
/// swallowing it would leave a person with an interface and no name for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ethernet,
    Loopback,
    /// Presents itself as Ethernet, but the kernel gave it a `phy80211`.
    Wireless,
    Other(u32),
    /// The `type` file could not be read. Rule 10: this is not a kind.
    Unreadable,
}

impl Kind {
    /// The word a program matches on. Stable, lowercase, never translated.
    pub fn word(self) -> String {
        match self {
            Kind::Ethernet => "ethernet".to_string(),
            Kind::Loopback => "loopback".to_string(),
            Kind::Wireless => "wireless".to_string(),
            Kind::Other(number) => format!("arphrd_{number}"),
            Kind::Unreadable => "unreadable".to_string(),
        }
    }
}

/// Whether something is plugged in — and the third answer, which is the point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    Up,
    Down,
    /// The kernel refused the question, which is what it does while the
    /// interface is down. **Not** the same as `Down`, and never printed as it.
    Unknown,
}

impl Carrier {
    pub fn word(self) -> &'static str {
        match self {
            Carrier::Up => "up",
            Carrier::Down => "down",
            Carrier::Unknown => "unknown",
        }
    }
}

/// Negotiated link speed, in megabits per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Mbps(u64),
    /// The link is up and the driver does not know — `-1` in sysfs. A virtual
    /// device answers this, and so does a card whose driver never implemented
    /// the callback.
    NotKnown,
    /// The file could not be read at all, which is the usual answer while the
    /// interface is down.
    Unreadable,
}

/// One interface, as far as reading can establish it.
///
/// Every field that can fail to be read says so in its own type rather than
/// falling back to a plausible value. A `mac` of `None` means the file did not
/// answer, not that the device has no address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub kind: Kind,
    /// The hardware address, lowercase hex as the kernel writes it.
    pub mac: Option<String>,
    /// `operstate`, verbatim: `up`, `down`, `unknown`, `dormant`, and others.
    /// Not turned into a boolean, because `unknown` is what loopback answers and
    /// flattening it would make `lo` look broken.
    pub state: Option<String>,
    pub carrier: Carrier,
    pub speed: Speed,
    pub mtu: Option<u64>,
    /// The module driving it, from the `device/driver` link. Virtual devices
    /// have no `device` at all, and that is `None` rather than an error.
    pub driver: Option<String>,
    /// Whether anything on a bus is behind it.
    ///
    /// Found by running this: a container showed `ifb0` and `ifb1` — the
    /// kernel's intermediate functional block devices — reporting `type 1`,
    /// with an address, and the first version counted them as two network
    /// cards on a machine that has one. Every purely software interface is
    /// like that, and what tells them apart is that a real card hangs off a
    /// bus and has a `device` link into it.
    pub on_a_bus: bool,
}

impl Interface {
    /// Whether this is something a person would call a network card.
    ///
    /// Loopback is not: it is always there, always up, and reporting "1
    /// interface" on a machine with no card would be true and useless. Neither
    /// is a software device that presents an Ethernet type without anything
    /// physical behind it.
    ///
    /// The interface is still listed either way — only the count and this
    /// answer change, so nothing is hidden from somebody looking for a card
    /// whose driver registers no parent device.
    pub fn is_a_card(&self) -> bool {
        matches!(self.kind, Kind::Ethernet | Kind::Wireless) && self.on_a_bus
    }
}

/// Read one file and trim it, or say it could not be read.
///
/// Everything in sysfs is a small text file with a trailing newline. The
/// distinction this preserves is the whole point: `Err` here is folded into an
/// explicit variant by each caller, never into a default.
fn text(path: &Path) -> Option<String> {
    let read = std::fs::read_to_string(path).ok()?;
    let trimmed = read.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn number(path: &Path) -> Option<i64> {
    text(path)?.parse().ok()
}

/// What `type` means, with the two values that matter measured on a live
/// machine rather than taken from a header.
fn kind_of(directory: &Path) -> Kind {
    let Some(number) = number(&directory.join("type")) else {
        return Kind::Unreadable;
    };
    // Asked before the type, because a wireless card in managed mode reports
    // Ethernet. `phy80211` is the link the kernel makes for a device that is
    // really behind an 802.11 phy.
    if directory.join("phy80211").exists() {
        return Kind::Wireless;
    }
    match number {
        1 => Kind::Ethernet,
        772 => Kind::Loopback,
        other if other >= 0 => Kind::Other(other as u32),
        // A negative type is not a type. Rule 9: the cautious answer, never the
        // one that looks tidy.
        _ => Kind::Unreadable,
    }
}

fn carrier_of(directory: &Path) -> Carrier {
    match number(&directory.join("carrier")) {
        Some(1) => Carrier::Up,
        Some(0) => Carrier::Down,
        // Both the unreadable case (EINVAL while the interface is down) and a
        // value written by a kernel that does not exist yet. Fail closed.
        _ => Carrier::Unknown,
    }
}

fn speed_of(directory: &Path) -> Speed {
    match number(&directory.join("speed")) {
        Some(mbps) if mbps > 0 => Speed::Mbps(mbps as u64),
        // `-1`, and `0` with it: neither is a speed anything moves at.
        Some(_) => Speed::NotKnown,
        None => Speed::Unreadable,
    }
}

/// The module behind the device, from the `device/driver` symlink.
///
/// Read as a link and not followed with `canonicalize`, which would fail on a
/// broken one and lose the name that is right there in the target.
fn driver_of(directory: &Path) -> Option<String> {
    let target = std::fs::read_link(directory.join("device").join("driver")).ok()?;
    Some(target.file_name()?.to_string_lossy().to_string())
}

fn one(directory: &Path, name: &str) -> Interface {
    Interface {
        name: name.to_string(),
        kind: kind_of(directory),
        // `00:00:00:00:00:00` is what loopback carries and it is a real answer
        // from the file, so it is kept: inventing a `None` for it here would be
        // this module deciding what the kernel meant.
        mac: text(&directory.join("address")),
        state: text(&directory.join("operstate")),
        carrier: carrier_of(directory),
        speed: speed_of(directory),
        mtu: number(&directory.join("mtu")).and_then(|value| u64::try_from(value).ok()),
        driver: driver_of(directory),
        on_a_bus: directory.join("device").exists(),
    }
}

/// Every interface the kernel is showing, sorted by name.
///
/// Sorted so that two runs of `red` on an unchanged machine read identically —
/// a listing whose order comes from `readdir` moves for reasons that have
/// nothing to do with the network, and a person comparing two of them would be
/// reading noise.
pub fn every_in(root: &Path) -> Result<Vec<Interface>, NetError> {
    let entries = std::fs::read_dir(root).map_err(|source| NetError::NoSysfs {
        path: root.to_path_buf(),
        source,
    })?;

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // A name that is not one, rather than a name this refuses to show. An
        // interface can be called almost anything; what it cannot be is a path
        // that walks somewhere else.
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            continue;
        }
        found.push(one(&entry.path(), &name));
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

/// The same, of this machine.
pub fn every() -> Result<Vec<Interface>, NetError> {
    every_in(Path::new(SYS_CLASS_NET))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory shaped the way sysfs shapes one, with only the files named.
    fn interface(root: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let directory = root.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        for (file, value) in files {
            std::fs::write(directory.join(file), value).unwrap();
        }
        directory
    }

    /// Captured verbatim from `/sys/class/net/eth0` on a live machine on
    /// 2026-08-23. Rule 6: a hand-written fixture proves the reader matches the
    /// author's model of the format, and this format is somebody else's.
    ///
    /// The `speed` of `-1` is the part no invented fixture would have carried.
    const A_REAL_ETHERNET: &[(&str, &str)] = &[
        ("address", "02:fc:00:00:00:01\n"),
        ("operstate", "up\n"),
        ("carrier", "1\n"),
        ("type", "1\n"),
        ("speed", "-1\n"),
        ("mtu", "1400\n"),
    ];

    /// The same machine's `lo`, verbatim. `operstate` is `unknown` and the
    /// carrier is up, which is why the state is not a boolean.
    const A_REAL_LOOPBACK: &[(&str, &str)] = &[
        ("address", "00:00:00:00:00:00\n"),
        ("operstate", "unknown\n"),
        ("carrier", "1\n"),
        ("type", "772\n"),
        ("mtu", "65536\n"),
    ];

    /// A device with something on a bus behind it, which is what makes it a
    /// card rather than a piece of software wearing an Ethernet type.
    fn on_a_bus(directory: &Path) {
        std::fs::create_dir_all(directory.join("device")).unwrap();
    }

    #[test]
    fn a_real_ethernet_interface_is_read_field_by_field() {
        let root = tempfile::tempdir().unwrap();
        on_a_bus(&interface(root.path(), "eth0", A_REAL_ETHERNET));

        let every = every_in(root.path()).unwrap();
        assert_eq!(every.len(), 1);
        let eth0 = &every[0];
        assert_eq!(eth0.name, "eth0");
        assert_eq!(eth0.kind, Kind::Ethernet);
        assert_eq!(eth0.mac.as_deref(), Some("02:fc:00:00:00:01"));
        assert_eq!(eth0.state.as_deref(), Some("up"));
        assert_eq!(eth0.carrier, Carrier::Up);
        assert_eq!(eth0.mtu, Some(1400));
        assert!(eth0.is_a_card());
    }

    #[test]
    fn a_speed_of_minus_one_is_not_a_speed() {
        // The value a virtual device really answers. Anything that turned this
        // into a number would print `-1 Mb/s`, or `18446744073709551615`.
        let root = tempfile::tempdir().unwrap();
        interface(root.path(), "eth0", A_REAL_ETHERNET);
        assert_eq!(every_in(root.path()).unwrap()[0].speed, Speed::NotKnown);
    }

    #[test]
    fn an_interface_that_is_down_says_it_cannot_tell_rather_than_no_cable() {
        // The measured fact this whole module is shaped around: reading
        // `carrier` on a down interface fails with EINVAL. It does not answer
        // `0`. A missing file is the same shape of failure and stands in for it
        // here — what is under test is that a failed read is never reported as
        // `down`.
        let root = tempfile::tempdir().unwrap();
        interface(
            root.path(),
            "ifb0",
            &[
                ("address", "da:be:b8:08:cc:5f\n"),
                ("operstate", "down\n"),
                ("type", "1\n"),
            ],
        );

        let ifb0 = &every_in(root.path()).unwrap()[0];
        assert_eq!(ifb0.carrier, Carrier::Unknown);
        assert_ne!(ifb0.carrier, Carrier::Down);
        assert_eq!(ifb0.speed, Speed::Unreadable);
        // And the control: the state was readable, so this is a machine that
        // answered some questions and not a directory that answered none.
        assert_eq!(ifb0.state.as_deref(), Some("down"));
    }

    #[test]
    fn a_carrier_of_zero_is_a_missing_cable_and_says_so() {
        // Without this the test above proves nothing: a reader that answered
        // `Unknown` to everything would pass it.
        let root = tempfile::tempdir().unwrap();
        interface(
            root.path(),
            "eth1",
            &[("type", "1\n"), ("carrier", "0\n"), ("operstate", "down\n")],
        );
        assert_eq!(every_in(root.path()).unwrap()[0].carrier, Carrier::Down);
    }

    #[test]
    fn a_software_interface_wearing_an_ethernet_type_is_not_counted_as_a_card() {
        // Found by running it, not by reading it: a container showed `ifb0` and
        // `ifb1` — the kernel's intermediate functional block devices — with
        // `type 1` and a hardware address, and `red` reported three cards on a
        // machine with one. Every virtual interface has that shape, and what
        // separates them from a card is that a card hangs off a bus.
        let root = tempfile::tempdir().unwrap();
        interface(
            root.path(),
            "ifb0",
            &[
                ("type", "1\n"),
                ("address", "da:be:b8:08:cc:5f\n"),
                ("operstate", "down\n"),
            ],
        );
        on_a_bus(&interface(root.path(), "eth0", A_REAL_ETHERNET));

        let every = every_in(root.path()).unwrap();
        let ifb0 = every.iter().find(|one| one.name == "ifb0").unwrap();
        let eth0 = every.iter().find(|one| one.name == "eth0").unwrap();

        assert!(!ifb0.is_a_card());
        assert!(!ifb0.on_a_bus);
        // It is still listed, with everything that could be read of it. The
        // count changed; nothing was hidden.
        assert_eq!(ifb0.kind, Kind::Ethernet);
        assert_eq!(ifb0.mac.as_deref(), Some("da:be:b8:08:cc:5f"));
        // And the control, without which a rule that called nothing a card
        // would pass this.
        assert!(eth0.is_a_card());
    }

    #[test]
    fn loopback_is_not_counted_as_a_card_however_up_it_is() {
        let root = tempfile::tempdir().unwrap();
        interface(root.path(), "lo", A_REAL_LOOPBACK);

        let lo = &every_in(root.path()).unwrap()[0];
        assert_eq!(lo.kind, Kind::Loopback);
        assert_eq!(lo.carrier, Carrier::Up);
        assert_eq!(lo.state.as_deref(), Some("unknown"));
        // `lo` is always there and always up. Counting it would let a machine
        // with no card at all report an interface.
        assert!(!lo.is_a_card());
    }

    #[test]
    fn a_wireless_card_is_known_by_its_phy_and_not_by_a_type_that_says_ethernet() {
        // In managed mode a wireless interface reports type 1. Going by the
        // type would file somebody's laptop card under cable, and the sentence
        // that came out would be confidently wrong rather than incomplete.
        let root = tempfile::tempdir().unwrap();
        let directory = interface(
            root.path(),
            "wlan0",
            &[("type", "1\n"), ("carrier", "1\n"), ("operstate", "up\n")],
        );
        std::fs::create_dir(directory.join("phy80211")).unwrap();
        on_a_bus(&directory);

        let wlan0 = &every_in(root.path()).unwrap()[0];
        assert_eq!(wlan0.kind, Kind::Wireless);
        assert!(wlan0.is_a_card());
    }

    #[test]
    fn a_type_nobody_here_recognises_keeps_its_number_instead_of_being_hidden() {
        let root = tempfile::tempdir().unwrap();
        interface(root.path(), "sit0", &[("type", "776\n")]);

        let sit0 = &every_in(root.path()).unwrap()[0];
        assert_eq!(sit0.kind, Kind::Other(776));
        assert_eq!(sit0.kind.word(), "arphrd_776");
        assert!(!sit0.is_a_card());
    }

    #[test]
    fn an_unreadable_type_is_not_a_kind_and_is_never_a_card() {
        // Rule 9: a corrupt field produces the cautious answer. Calling it
        // Ethernet would put a device a person cannot use on a list of cards.
        let root = tempfile::tempdir().unwrap();
        interface(root.path(), "odd0", &[("type", "not a number\n")]);

        let odd0 = &every_in(root.path()).unwrap()[0];
        assert_eq!(odd0.kind, Kind::Unreadable);
        assert!(!odd0.is_a_card());
    }

    #[test]
    fn the_listing_is_sorted_so_two_runs_of_it_can_be_compared() {
        let root = tempfile::tempdir().unwrap();
        for name in ["wlan0", "eth0", "lo", "enp3s0"] {
            interface(root.path(), name, &[("type", "1\n")]);
        }
        let names: Vec<String> = every_in(root.path())
            .unwrap()
            .into_iter()
            .map(|interface| interface.name)
            .collect();
        assert_eq!(names, vec!["enp3s0", "eth0", "lo", "wlan0"]);
    }

    #[test]
    fn a_sysfs_that_is_not_there_is_an_error_and_not_an_empty_machine() {
        // Rule 10, at the top level. "No interfaces" and "I could not look" are
        // different answers, and a machine with a card would be told it has
        // none.
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("nothing-here");
        let error = every_in(&missing).unwrap_err();
        let NetError::NoSysfs { path, .. } = &error;
        assert_eq!(path, &missing);
    }

    /// The one claim above that a fixture tree cannot make: that this reader
    /// never turns a failed read into `Down`.
    ///
    /// Every other test in this file reads a directory this repository wrote,
    /// where a missing file stands in for a kernel refusing the question. That
    /// is rule 8 and it is not the property itself, so this one asks the machine
    /// it is running on, and checks the mapping against a second reading of the
    /// same files taken here rather than against a remembered answer.
    ///
    /// **What this deliberately does not claim.** The first version asserted
    /// that a down interface always refuses the question, because that is what
    /// the two interfaces on the machine it was written on did. It is not true:
    /// on Cesar's Fedora a Docker bridge that is down answers `0` honestly, and
    /// the assertion took down the whole suite. Whether the question is refused
    /// belongs to the driver, not to being down — and the module never needed
    /// that. What it needs is that a refusal is never reported as a missing
    /// cable, which is a property of this code and is true on every machine.
    ///
    /// Rule 3: what this machine cannot show, it says out loud, and
    /// `THALYX_REQUIRE_REAL_SYSFS_TESTS=1` turns that into a failure.
    #[test]
    fn on_this_machine_every_carrier_answer_matches_the_file_it_came_from() {
        let root = Path::new(SYS_CLASS_NET);
        let Ok(every) = every_in(root) else {
            let gap = "NOT PROVEN: /sys/class/net could not be read here, so no \
                       carrier answer was checked against its file";
            assert_ne!(
                std::env::var("THALYX_REQUIRE_REAL_SYSFS_TESTS").as_deref(),
                Ok("1"),
                "{gap}"
            );
            println!("{gap}");
            return;
        };

        assert!(
            !every.is_empty(),
            "this machine showed no interfaces at all, so nothing below was checked"
        );

        let mut refusals = 0;
        for interface in &every {
            // Read again, here, with no help from the module under test. This is
            // the whole instrument: a second reading of the same file.
            let raw = std::fs::read_to_string(root.join(&interface.name).join("carrier"));
            let expected = match raw.as_deref().map(str::trim) {
                Ok("1") => Carrier::Up,
                Ok("0") => Carrier::Down,
                // Anything the kernel refuses or writes that is not those two.
                // A physical card that has never been brought up refuses with
                // EINVAL; a software bridge with nothing attached answers 0.
                // Both are ordinary, and only the first one is this branch.
                _ => Carrier::Unknown,
            };
            if raw.is_err() {
                refusals += 1;
            }
            assert_eq!(
                interface.carrier,
                expected,
                "{} carries `{:?}` and its carrier file said {:?}",
                interface.name,
                interface.carrier,
                raw.as_deref().map(str::trim),
            );
        }

        // The half a machine may not be able to show. Without a refusal to
        // observe, everything above passed on a machine where every file could
        // be read, and the branch that matters was never taken.
        if refusals == 0 {
            let gap = "NOT PROVEN: every interface on this machine answered the \
                       carrier question, so Carrier::Unknown was never produced \
                       from a real refusal";
            assert_ne!(
                std::env::var("THALYX_REQUIRE_REFUSED_CARRIER_TESTS").as_deref(),
                Ok("1"),
                "{gap}"
            );
            println!("{gap}");
        }
    }

    #[test]
    fn a_machine_with_no_driver_link_says_none_rather_than_failing() {
        // Every virtual device is like this — there is no `device` at all — and
        // an implementation that treated the missing link as an error would
        // report a broken interface for the most ordinary case there is.
        let root = tempfile::tempdir().unwrap();
        interface(root.path(), "lo", A_REAL_LOOPBACK);
        assert_eq!(every_in(root.path()).unwrap()[0].driver, None);
    }

    #[test]
    fn the_driver_is_the_last_part_of_the_link_and_not_the_whole_path() {
        // Captured shape: on a live machine `eth0`'s link pointed at
        // /sys/bus/virtio/drivers/virtio_net.
        let root = tempfile::tempdir().unwrap();
        let directory = interface(root.path(), "eth0", A_REAL_ETHERNET);
        std::fs::create_dir_all(directory.join("device")).unwrap();
        std::os::unix::fs::symlink(
            "/sys/bus/virtio/drivers/virtio_net",
            directory.join("device").join("driver"),
        )
        .unwrap();

        assert_eq!(
            every_in(root.path()).unwrap()[0].driver.as_deref(),
            Some("virtio_net")
        );
    }
}
