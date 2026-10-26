// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether a root lies on a share that is away now (ADR-0115). A share
//! listed in `/etc/fstab` with `nofail` leaves its mount point behind as an
//! empty folder when the share is away, and that folder reads as an
//! available, empty root. [`MountTables::away`] answers whether the nearest
//! mount point that fstab lists (other than `/`) at or above a root has
//! nothing mounted on it or between it and the root, by
//! `/proc/self/mountinfo`; the store also requires the root to be empty.
//! Nothing is mounted, and a folder that no fstab entry covers is never
//! affected. Paths are not logged.

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

    /// True when the nearest fstab mount point at or above `root` has
    /// nothing mounted on it, nor on any folder between it and `root`.
    ///
    /// The kernel records mount points with symbolic links resolved (an
    /// ostree desktop's `/mnt` is `/var/mnt`), so the root and the listed
    /// points that cover it are compared in canonical form, falling back to
    /// the text when a path cannot be resolved. A mount that fstab does not
    /// list, such as a share mounted by hand under an away fstab entry, is
    /// still a mount. Only the listed points that cover the root are
    /// resolved, so an unrelated hung share is never touched.
    pub fn away(&self, root: &Path) -> bool {
        let Some(mounted) = &self.mounted else {
            return false;
        };
        let real_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let Some(point) = self
            .listed
            .iter()
            .filter(|point| root.starts_with(point) || real_root.starts_with(point))
            .map(|point| std::fs::canonicalize(point).unwrap_or_else(|_| point.clone()))
            .filter(|point| real_root.starts_with(point))
            .max_by_key(|point| point.components().count())
        else {
            return false;
        };
        !mounted
            .iter()
            .any(|mount| real_root.starts_with(mount) && mount.starts_with(&point))
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

    // A prefix that does not exist, so no path resolves on the host and the
    // tests do not depend on its folders.
    const P: &str = "/arinn-test-absent";

    fn fstab() -> String {
        format!(
            "\
# <file system> <dir> <type> <options> <dump> <pass>
UUID=1111 / ext4 defaults 0 1
UUID=2222 {P}/home ext4 defaults 0 2
//nas/films {P}/mnt/nas/films cifs nofail,credentials=/etc/nas 0 0
nas:/export/series {P}/mnt/My\\040Series nfs nofail 0 0
/swapfile none swap sw 0 0
"
        )
    }

    fn mountinfo(points: &[&str]) -> String {
        points
            .iter()
            .enumerate()
            .map(|(i, point)| {
                format!("{i} 1 0:{i} / {point} rw,relatime shared:1 - ext4 /dev/x rw\n")
            })
            .collect()
    }

    fn at(path: &str) -> PathBuf {
        PathBuf::from(format!("{P}{path}"))
    }

    #[test]
    fn a_listed_share_with_nothing_mounted_is_away() {
        let home = format!("{P}/home");
        let away = MountTables::from_text(&fstab(), Some(&mountinfo(&["/", &home])));
        assert!(away.away(&at("/mnt/nas/films")));
        assert!(away.away(&at("/mnt/nas/films/new")));
        assert!(away.away(&at("/mnt/My Series")));
        let films = format!("{P}/mnt/nas/films");
        let series = format!("{P}/mnt/My\\040Series");
        let here =
            MountTables::from_text(&fstab(), Some(&mountinfo(&["/", &home, &films, &series])));
        assert!(!here.away(&at("/mnt/nas/films")));
        assert!(!here.away(&at("/mnt/My Series")));
    }

    #[test]
    fn a_folder_no_fstab_entry_covers_is_never_away() {
        let tables = MountTables::from_text(&fstab(), Some(&mountinfo(&["/"])));
        // `/` is not a share, and swap has no mount point.
        assert!(!tables.away(&at("/srv/films")));
        assert!(!tables.away(&at("/mnt/nas")));
        assert!(!tables.away(&at("/mnt/nas/filmsextra")));
        // No fstab, or an unreadable mountinfo: nothing is away.
        let tables = MountTables::from_text("", Some(&mountinfo(&["/"])));
        assert!(!tables.away(&at("/mnt/nas/films")));
        let tables = MountTables::from_text(&fstab(), None);
        assert!(!tables.away(&at("/mnt/nas/films")));
    }

    #[test]
    fn the_nearest_listed_mount_point_decides() {
        let fstab = format!("a {P}/mnt/nas nfs nofail 0 0\nb {P}/mnt/nas/films nfs nofail 0 0\n");
        let films = format!("{P}/mnt/nas/films");
        let tables = MountTables::from_text(&fstab, Some(&mountinfo(&["/", &films])));
        assert!(!tables.away(&at("/mnt/nas/films/new")));
        assert!(tables.away(&at("/mnt/nas/series")));
    }

    // Review of #557: a share that fstab does not list, mounted under an
    // away fstab entry (a systemd mount unit or `mount` by hand), is up.
    #[test]
    fn a_mount_between_the_listed_point_and_the_root_is_up() {
        let fstab = format!("//nas/media {P}/mnt/nas cifs nofail 0 0\n");
        let films = format!("{P}/mnt/nas/films");
        let tables = MountTables::from_text(&fstab, Some(&mountinfo(&["/", &films])));
        assert!(!tables.away(&at("/mnt/nas/films")));
        assert!(!tables.away(&at("/mnt/nas/films/new")));
        assert!(tables.away(&at("/mnt/nas/series")));
    }

    // Review of #557: mountinfo holds resolved paths. On an ostree desktop
    // `/mnt` is a symbolic link to `/var/mnt`.
    #[test]
    fn a_mount_point_reached_through_a_symbolic_link_matches() {
        let scratch = std::env::temp_dir().join(format!(
            "arinn-library-mounts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let real = scratch.join("var/mnt");
        std::fs::create_dir_all(real.join("nas/films")).unwrap();
        std::os::unix::fs::symlink(&real, scratch.join("mnt")).unwrap();
        let real = std::fs::canonicalize(&real).unwrap();
        let link = scratch.join("mnt");
        let fstab = format!(
            "//nas/films {} cifs nofail 0 0\n",
            link.join("nas").display()
        );
        let mounted = real.join("nas").display().to_string();
        let up = MountTables::from_text(&fstab, Some(&mountinfo(&["/", &mounted])));
        assert!(!up.away(&link.join("nas/films")));
        assert!(!up.away(&real.join("nas/films")));
        let down = MountTables::from_text(&fstab, Some(&mountinfo(&["/"])));
        assert!(down.away(&link.join("nas/films")));
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(unescape("/mnt/a\\040b\\011c\\134d"), "/mnt/a b\tc\\d");
        assert_eq!(unescape("/mnt/x\\9"), "/mnt/x\\9");
        assert_eq!(unescape("/mnt/end\\04"), "/mnt/end\\04");
    }
}
