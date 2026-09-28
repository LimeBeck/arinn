// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Provider-side snapshot primitives. Registration and transport are not yet public.
pub mod platform;
pub mod snapshots;
pub use arinn_protocol::{application, validation};
pub mod runtime;
