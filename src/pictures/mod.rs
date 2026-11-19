//! The picture module: the decode, the resize, the shape, the jobs and the Kitty output of the
//! pictures.
//!
//! The app sets its name with `names::set_app_name` at start, turns on the decoders that it needs
//! through the features `turbo`, `heic`, `jxl` and `sips`, and reaches its test helpers through the
//! feature `test-support`. The rules of the Kitty layer are in `docs/pictures.md`.
//!
//! The module imports no app type. It refers to itself through `super::`, `self::` or
//! `crate::pictures::`, and to no other module of the crate.
pub mod decode;
pub mod jobs;
pub mod kitty;
pub mod motion;
pub mod names;
pub mod resize;
pub mod shape;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
