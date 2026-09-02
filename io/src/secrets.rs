//! OS-keyring-backed storage for OpenDrop's stream-integration secrets.
//!
//! Every secret is stored under a fixed service name (`opendrop-native`) in
//! the platform credential store (Secret Service on Linux, Keychain on
//! macOS, Credential Manager on Windows); the entry's username slot holds
//! the caller-supplied key name.
//!
//! All `keyring::Error`s are converted to `String`. Nothing here panics or
//! calls `.unwrap()`/`.expect()` on a `keyring::Error`: a Secret Service
//! (or equivalent) daemon may not be running, e.g. on a minimal Hyprland
//! session with no keyring agent started.

const SERVICE_NAME: &str = "opendrop-native";

/// Twitch OAuth token, used to authenticate chat/API access.
/// Mirrors the key used by `electron/secrets-store.cjs` via
/// `secretsStore.getSecret('twitch-oauth-token')` in `electron/main.cjs`.
pub const TWITCH_OAUTH_TOKEN: &str = "twitch-oauth-token";

/// OBS WebSocket connection password.
/// Mirrors `secretsStore.getSecret('obs-password')` in `electron/main.cjs`.
pub const OBS_PASSWORD: &str = "obs-password";

/// Kick bearer token.
/// Mirrors `secretsStore.getSecret('kick-bearer-token')` in `electron/main.cjs`.
pub const KICK_BEARER_TOKEN: &str = "kick-bearer-token";

/// Kick XSRF token.
/// Mirrors `secretsStore.getSecret('kick-xsrf-token')` in `electron/main.cjs`.
pub const KICK_XSRF_TOKEN: &str = "kick-xsrf-token";

/// Kick session cookies.
/// Mirrors `secretsStore.getSecret('kick-cookies')` in `electron/main.cjs`.
pub const KICK_COOKIES: &str = "kick-cookies";

/// CloudPresets anonymous device identity token, sent as the `X-Cloud-
/// Token` header on every request to the CloudPresets backend Worker.
/// Mirrors the *key name* convention of `OpenDrop-VJ/src/lib/engine/
/// cloud-presets.ts`'s `TOKEN_KEY = 'od-cloud-token'` (a `localStorage` key
/// there, this OS keyring here): there is no Electron-side counterpart to
/// mirror, this is a native-only feature.
pub const CLOUD_PRESETS_TOKEN: &str = "cloud-presets-token";

/// Retrieve a secret from the OS keyring.
///
/// Returns `Ok(None)` when no secret is stored under `key` (a missing
/// secret is not a failure), and `Err` for any other keyring failure
/// (e.g. no credential store available on this platform/session).
pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Store `value` under `key` in the OS keyring, overwriting any existing
/// value.
pub fn set_secret(key: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| e.to_string())?;
    entry.set_password(value).map_err(|e| e.to_string())
}

/// Remove the secret stored under `key` from the OS keyring.
pub fn clear_secret(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first 5 key strings must exactly mirror the ones used by
    /// `electron/secrets-store.cjs`'s callers in `electron/main.cjs`
    /// (verified there, not invented); `CLOUD_PRESETS_TOKEN` has no
    /// Electron-side counterpart (native-only feature, see its own doc
    /// comment) and is just pinned against regression here. This
    /// assertion needs no keyring backend, so it always runs.
    #[test]
    fn secret_key_constants_match_electron_reference() {
        assert_eq!(TWITCH_OAUTH_TOKEN, "twitch-oauth-token");
        assert_eq!(OBS_PASSWORD, "obs-password");
        assert_eq!(KICK_BEARER_TOKEN, "kick-bearer-token");
        assert_eq!(KICK_XSRF_TOKEN, "kick-xsrf-token");
        assert_eq!(KICK_COOKIES, "kick-cookies");
        assert_eq!(CLOUD_PRESETS_TOKEN, "cloud-presets-token");
    }

    /// Exercises the real OS keyring end-to-end. Ignored by default: this
    /// is an OS-integration boundary, and CI / sandboxed environments (e.g.
    /// this repo's minimal Hyprland dev session) may have no Secret Service
    /// daemon registered at all, which would make a non-ignored test either
    /// fail for reasons unrelated to this code or, on a machine where the
    /// backend exists but is locked, hang waiting for an interactive
    /// unlock prompt. Run explicitly with `cargo test -- --ignored` on a
    /// machine with a working keyring backend.
    ///
    /// Even then, `set_secret` is allowed to return `Err` (e.g. backend
    /// present but unavailable) without failing the test: this only
    /// asserts the round trip when a backend actually accepted the write.
    #[test]
    #[ignore = "requires a real OS keyring/Secret Service backend"]
    fn round_trip_secret_when_keyring_available() {
        let test_key = "opendrop-native-secrets-rs-round-trip-test";
        let test_value = "opendrop-native-secrets-rs-round-trip-value";

        match set_secret(test_key, test_value) {
            Ok(()) => {
                assert_eq!(
                    get_secret(test_key).unwrap(),
                    Some(test_value.to_string())
                );
                clear_secret(test_key).unwrap();
                assert_eq!(get_secret(test_key).unwrap(), None);
            }
            Err(e) => {
                eprintln!("note: skipping keyring round-trip, no backend available ({e})");
            }
        }
    }
}
