// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable folder listing for the shell's folder picker (ADR-0071).
//!
//! `org.arinn.Folders1` on `/org/arinn/Daemon1` lists the **folders** under
//! an absolute path, a page at a time: no files, no hidden entries and no
//! file contents. It lets a `path` setting with `directory` be chosen from
//! the TV without the shell reading the file system (R-05). Paths are never
//! logged (spec §13).
use crate::{settings, validation::ErrorCode};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use zbus::zvariant::Type;

pub const INTERFACE: &str = "org.arinn.Folders1";

/// Kinds of [`Place`], in the order `Places` returns them.
pub const PLACE_HOME: &str = "home";
pub const PLACE_MOUNT: &str = "mount";
pub const PLACE_ROOT: &str = "root";

/// Folders whose subfolders are offered as mounts.
pub const MOUNT_PARENTS: &[&str] = &["/media", "/run/media", "/mnt"];

/// Most names in one `List` page.
pub const MAX_PAGE: u32 = 256;
/// Longest folder name, in bytes (the usual file system limit).
pub const MAX_NAME_BYTES: usize = 255;

/// A start point for the picker: the home folder, each mount, then `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Place {
    pub kind: String,
    pub path: String,
}

impl Place {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let kind_ok = [PLACE_HOME, PLACE_MOUNT, PLACE_ROOT].contains(&self.kind.as_str());
        let root_ok = (self.kind == PLACE_ROOT) == (self.path == "/");
        (kind_ok && root_ok && settings::path(&self.path))
            .then_some(())
            .ok_or(ErrorCode::InvalidArgument)
    }
}

/// A listable folder name: not hidden, not `.` or `..`, no `/`, no control
/// characters, 1 to [`MAX_NAME_BYTES`] bytes. Other entries are skipped, so
/// every listed name can be joined to its parent and passed back to `List`.
pub fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAME_BYTES
        && !value.starts_with('.')
        && !value.contains('/')
        && !value.chars().any(char::is_control)
}

/// Picker order: case-insensitive, then by exact text so that the order is
/// total and the `after` cursor is unambiguous.
pub fn order(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// One `List` page from the names of the subfolders: listable names in
/// picker order, those after `after` (empty for the first page), at most
/// `limit`. Returns the page and whether more names follow.
pub fn page(
    names: impl IntoIterator<Item = String>,
    after: &str,
    limit: u32,
) -> Result<(Vec<String>, bool), ErrorCode> {
    if limit == 0 || limit > MAX_PAGE || !(after.is_empty() || name(after)) {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut listed: Vec<String> = names
        .into_iter()
        .filter(|n| name(n) && (after.is_empty() || order(n, after) == Ordering::Greater))
        .collect();
    listed.sort_by(|a, b| order(a, b));
    listed.dedup();
    let more = listed.len() > limit as usize;
    listed.truncate(limit as usize);
    Ok((listed, more))
}

/// Shell-facing folder listing.
///
/// - `Places` returns the start points: [`PLACE_HOME`], each mounted folder
///   under [`MOUNT_PARENTS`] as [`PLACE_MOUNT`] in picker order, then
///   [`PLACE_ROOT`] (`/`).
/// - `List` returns the subfolder names of `path` after `after`, at most
///   `limit`, and whether more follow. An entry is a folder when it resolves
///   to a directory, so symbolic links to folders are listed.
///
/// Errors: `InvalidArgument` for a path that is not absolute and normal
/// (see [`settings::path`]), a bad cursor or a `limit` outside
/// 1..=[`MAX_PAGE`]; `NotFound` when the path does not exist, is not a
/// folder or its mount has gone; `PermissionDenied` when it cannot be read.
#[zbus::proxy(
    interface = "org.arinn.Folders1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait FoldersApi {
    fn places(&self) -> zbus::Result<Vec<Place>>;
    fn list(&self, path: &str, after: &str, limit: u32) -> zbus::Result<(Vec<String>, bool)>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wire_signatures() {
        assert_eq!(Place::SIGNATURE.to_string(), "(ss)");
        let place = Place {
            kind: PLACE_ROOT.into(),
            path: "/".into(),
        };
        let bytes = to_bytes(Context::new_dbus(LE, 0), &place).unwrap();
        let (back, _): (Place, _) = bytes.deserialize().unwrap();
        assert_eq!(back, place);
    }

    #[test]
    fn places_validate_kind_and_path() {
        let place = |kind: &str, path: &str| Place {
            kind: kind.into(),
            path: path.into(),
        };
        assert_eq!(place(PLACE_HOME, "/home/fixture").validate(), Ok(()));
        assert_eq!(
            place(PLACE_MOUNT, "/run/media/fixture/Disk").validate(),
            Ok(())
        );
        assert_eq!(place(PLACE_ROOT, "/").validate(), Ok(()));
        for bad in [
            place(PLACE_ROOT, "/home"),
            place(PLACE_HOME, "/"),
            place("network", "/srv"),
            place(PLACE_MOUNT, "relative"),
            place(PLACE_MOUNT, "/mnt/../etc"),
        ] {
            assert_eq!(bad.validate(), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
    }

    #[test]
    fn names_skip_hidden_and_unusable_entries() {
        assert!(name("Films"));
        assert!(name("Фильмы 2024"));
        for bad in ["", ".", "..", ".cache", "a/b", "line\nbreak"] {
            assert!(!name(bad), "{bad:?}");
        }
        assert!(name(&"x".repeat(MAX_NAME_BYTES)));
        assert!(!name(&"x".repeat(MAX_NAME_BYTES + 1)));
    }

    #[test]
    fn order_is_case_insensitive_and_total() {
        let mut list = names(&["beta", "Alpha", "alpha", "Gamma", "ALPHA"]);
        list.sort_by(|a, b| order(a, b));
        assert_eq!(list, names(&["ALPHA", "Alpha", "alpha", "beta", "Gamma"]));
    }

    #[test]
    fn pages_follow_the_cursor() {
        let all = names(&["c", ".hidden", "A", "b", "D", "e", "bad\tname"]);
        let (first, more) = page(all.clone(), "", 2).unwrap();
        assert_eq!(first, names(&["A", "b"]));
        assert!(more);
        let (second, more) = page(all.clone(), "b", 2).unwrap();
        assert_eq!(second, names(&["c", "D"]));
        assert!(more);
        let (last, more) = page(all.clone(), "D", 2).unwrap();
        assert_eq!(last, names(&["e"]));
        assert!(!more);
        // A cursor that no longer exists still continues in order.
        assert_eq!(page(all, "bb", 10).unwrap().0, names(&["c", "D", "e"]));
    }

    #[test]
    fn page_rejects_bad_arguments() {
        for (after, limit) in [("", 0), ("", MAX_PAGE + 1), (".hidden", 1), ("a/b", 1)] {
            assert_eq!(
                page(Vec::new(), after, limit),
                Err(ErrorCode::InvalidArgument),
                "{after:?} {limit}"
            );
        }
        assert_eq!(page(Vec::new(), "", MAX_PAGE), Ok((Vec::new(), false)));
    }
}
