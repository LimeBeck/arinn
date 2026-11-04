// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Remove OAuth response credentials before any public or retained browser snapshot.
use url::Url;

pub(crate) fn redact(value: &str) -> Option<String> {
    let Ok(mut url) = Url::parse(value) else {
        let lower = value.to_ascii_lowercase();
        return (lower.contains("access_token=")
            || lower.contains("code_verifier=")
            || (lower.contains("code=") && lower.contains("state=")))
        .then(String::new);
    };
    let keys: Vec<_> = url
        .query_pairs()
        .chain(url::form_urlencoded::parse(
            url.fragment().unwrap_or("").as_bytes(),
        ))
        .map(|(k, _)| k.to_ascii_lowercase())
        .collect();
    let callback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        && url.path() == "/oauth/callback";
    let sensitive = callback
        || keys.iter().any(|k| {
            matches!(
                k.as_str(),
                "access_token" | "refresh_token" | "id_token" | "code_verifier" | "oauth_token"
            )
        })
        || (keys.iter().any(|k| k == "code") && keys.iter().any(|k| k == "state"));
    if !sensitive {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_query_fragment_and_encoded_names_are_removed() {
        for value in [
            "http://127.0.0.1:17845/oauth/callback?code=synthetic&state=fixture",
            "https://fixture.invalid/callback?%63ode=synthetic&state=fixture",
            "https://fixture.invalid/callback#access_token=synthetic&state=fixture",
        ] {
            let safe = redact(value).unwrap();
            assert!(!safe.contains('?') && !safe.contains('#') && !safe.contains("synthetic"));
        }
        assert_eq!(redact("https://fixture.invalid/watch?v=1"), None);
        assert_eq!(
            redact("https://fixture.invalid/authorize?state=public&code_challenge=public"),
            None
        );
    }
}
