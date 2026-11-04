// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Owner-authorized CloudMus-compatible OAuth registration. These public embedded
//! application values identify a third-party client, not an Arinn-owned registration.
//! No user credential belongs here. Never log this module's values.
//! Source (registration data only; no SDK implementation is vendored):
//! https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/device_auth.py
//! Upstream SDK: Copyright 2019-2026 Ilya (Marshal), LGPLv3.

pub(crate) const CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
pub(crate) const CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";
