//! The name of the app in the files, the threads and the variables of the module.
//!
//! The app sets its name one time at start, so two apps never share a cache key or a temp file
//! name, and the module holds no app name of its own.

use std::sync::OnceLock;

/// The name that the app set, or none before the set.
static APP_NAME: OnceLock<&'static str> = OnceLock::new();

/// The name before an app sets one: the tests of the module and an app that forgot the call.
const DEFAULT_APP_NAME: &str = "tuilibr";

/// Sets the name of the app for the cache key, the thread names and the temp files. Call it as
/// the first line of `main`, with `env!("CARGO_PKG_NAME")`. A second call changes nothing.
pub fn set_app_name(name: &'static str) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a second call keeps the first name on purpose"
    )]
    let _ = APP_NAME.set(name);
}

/// The name of the app, `tuilibr` before the set.
pub fn app_name() -> &'static str {
    APP_NAME.get().copied().unwrap_or(DEFAULT_APP_NAME)
}

/// The environment variable of the real-file test `name`: `<APP>_TEST_JPEG` for `JPEG`, with the
/// app name in upper case.
#[cfg(any(test, feature = "test-support"))]
pub fn test_variable(name: &str) -> String {
    format!("{}_TEST_{name}", app_name().to_ascii_uppercase())
}

/// The file of the real-file test `name`. Without the variable the test skips with a note.
#[cfg(any(test, feature = "test-support"))]
pub fn test_file(name: &str) -> Option<std::path::PathBuf> {
    crate::pictures::decode::real_test_file(&test_variable(name))
}

#[cfg(test)]
mod tests {
    use super::{app_name, test_file, test_variable};

    #[test]
    fn real_file_variables_carry_the_app_name() {
        let prefix = app_name().to_ascii_uppercase();
        assert_eq!(test_variable("JPEG"), format!("{prefix}_TEST_JPEG"));
        assert_eq!(test_file("UNSET_IN_ANY_RUN"), None);
    }
}
