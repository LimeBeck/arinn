// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether a root lies on a share that is not mounted now (ADR-0115). A
//! share listed in `/etc/fstab` with `nofail` leaves its mount point behind
//! as an empty folder when the share is away, and that folder reads as an
//! available, empty root. A root at or under a mount point that fstab lists
//! (other than `/`), while `/proc/self/mountinfo` has nothing mounted there,
//! is unavailable instead. Nothing is mounted, and a folder that no fstab
//! entry covers is never affected. Paths are not logged.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

const FSTAB: &str = "/etc/fstab";
const MOUNTINFO: &str = "/proc/self/mountinfo";
/// The tables are small; a larger file is not a mount table.
const MAX_TABLE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct MountTables {
    /// Mount points that `/etc/fstab` lists, except `/`.
    listed: Vec<PathBuf>,
    /// Mount points mounted now; `None` when the table could not be read,
    /// and then no root is treated as unmounted.
    mounted: Option<HashSet<PathBuf>>,
}

impl MountTables {
    /// The system's tables, read at each scan.
    pub fn system() -> Self {
        let fstab = read_table(FSTAB).unwrap_or_default();
        let mountinfo = read_table(MOUNTINFO);
        Self {
            listed: listed_mount_points(&fstab),
            mounted: mountinfo.map(|text| mounted_points(&text)),
        }
    }

    /// Tables from text, for tests: `mountinfo` is `None` when unreadable.
    pub fn from_text(fstab: &str, mountinfo: Option<&str>) -> Self {
        Self {
            listed: listed_mount_points(fstab),
            mounted: mountinfo.map(mounted_points),
        }
    }

    /// True when the nearest fstab mount point at or above `root` is not
    /// mounted now.
    pub fn unmounted(&self, root: &Path) -> bool {
        let Some(mounted) = &self.mounted else {
            return false;
        };
        self.listed
            .iter()
            .filter(|point| root.starts_with(point))
            .max_by_key(|point| point.components().count())
            .is_some_and(|point| !mounted.contains(point))
    }
}

fn read_table(path: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(MAX_TABLE_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The second field of each fstab entry: absolute mount points, without `/`
/// and swap.
fn listed_mount_points(fstab: &str) -> Vec<PathBuf> {
    let mut points = Vec::new();
    for line in fstab.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(point) = line.split_whitespace().nth(1).map(unescape) else {
            continue;
        };
        let path = PathBuf::from(&point);
        if path.is_absolute() && path != Path::new("/") && !points.contains(&path) {
            points.push(path);
        }
    }
    points
}

/// The fifth field of each mountinfo line: the mount point.
fn mounted_points(mountinfo: &str) -> HashSet<PathBuf> {
    mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(4))
        .map(|point| PathBuf::from(unescape(point)))
        .collect()
}

/// fstab and mountinfo write space, tab, newline and backslash as `\ooo`.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let value = (u32::from(bytes[i + 1] - b'0') << 6)
                | (u32::from(bytes[i + 2] - b'0') << 3)
                | u32::from(bytes[i + 3] - b'0');
            if let Ok(byte) = u8::try_from(value) {
                out.push(byte);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FSTAB: &str = "\
# <file system> <dir> <type> <options> <dump> <pass>
UUID=1111 / ext4 defaults 0 1
UUID=2222 /home ext4 defaults 0 2
//nas/films /mnt/nas/films cifs nofail,credentials=/etc/nas 0 0
nas:/export/series /mnt/My\\040Series nfs nofail 0 0
/swapfile none swap sw 0 0
";

    fn mountinfo(points: &[&str]) -> String {
        points
            .iter()
            .enumerate()
            .map(|(i, point)| {
                format!("{i} 1 0:{i} / {point} rw,relatime shared:1 - ext4 /dev/x rw\n")
            })
            .collect()
    }

    #[test]
    fn a_listed_share_that_is_not_mounted_is_unmounted() {
        let away = MountTables::from_text(FSTAB, Some(&mountinfo(&["/", "/home"])));
        assert!(away.unmounted(Path::new("/mnt/nas/films")));
        assert!(away.unmounted(Path::new("/mnt/nas/films/new")));
        assert!(away.unmounted(Path::new("/mnt/My Series")));
        let here = MountTables::from_text(
            FSTAB,
            Some(&mountinfo(&[
                "/",
                "/home",
                "/mnt/nas/films",
                "/mnt/My\\040Series",
            ])),
        );
        assert!(!here.unmounted(Path::new("/mnt/nas/films")));
        assert!(!here.unmounted(Path::new("/mnt/My Series")));
    }

    #[test]
    fn a_folder_no_fstab_entry_covers_is_never_unmounted() {
        let tables = MountTables::from_text(FSTAB, Some(&mountinfo(&["/"])));
        // `/` is not a share, and swap has no mount point.
        assert!(!tables.unmounted(Path::new("/srv/films")));
        assert!(!tables.unmounted(Path::new("/mnt/nas")));
        assert!(!tables.unmounted(Path::new("/mnt/nas/filmsextra")));
        // No fstab, or an unreadable mountinfo: nothing is unmounted.
        let tables = MountTables::from_text("", Some(&mountinfo(&["/"])));
        assert!(!tables.unmounted(Path::new("/mnt/nas/films")));
        let tables = MountTables::from_text(FSTAB, None);
        assert!(!tables.unmounted(Path::new("/mnt/nas/films")));
    }

    #[test]
    fn the_nearest_listed_mount_point_decides() {
        let fstab = "a /mnt/nas nfs nofail 0 0\nb /mnt/nas/films nfs nofail 0 0\n";
        let tables = MountTables::from_text(fstab, Some(&mountinfo(&["/", "/mnt/nas/films"])));
        assert!(!tables.unmounted(Path::new("/mnt/nas/films/new")));
        assert!(tables.unmounted(Path::new("/mnt/nas/series")));
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(unescape("/mnt/a\\040b\\011c\\134d"), "/mnt/a b\tc\\d");
        assert_eq!(unescape("/mnt/x\\9"), "/mnt/x\\9");
        assert_eq!(unescape("/mnt/end\\04"), "/mnt/end\\04");
    }
}
