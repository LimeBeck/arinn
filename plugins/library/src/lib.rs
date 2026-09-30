// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Local folder library provider (ADR-0024, ADR-0054, ADR-0056).
//! Paths, titles and file URIs are not logged.

mod parse;
mod paths;
mod progress;
mod store;
mod uri;
mod watch;

pub use paths::{config_file, provider_digest, state_database};
pub use store::{Settings, Store};
pub use uri::to_file_uri;
pub use watch::serve;
