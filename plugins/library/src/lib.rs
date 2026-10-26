// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Local folder library provider (ADR-0024, ADR-0054, ADR-0056, ADR-0071).
//! Paths, titles and file URIs are not logged.

mod parse;
mod paths;
mod progress;
mod settings;
mod store;
mod uri;
mod watch;

pub use paths::{provider_digest, state_database};
pub use settings::schema;
pub use store::{Settings, Store, scan_shared, written_roots};
pub use uri::to_file_uri;
pub use watch::serve;
