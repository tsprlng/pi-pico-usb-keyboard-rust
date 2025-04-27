//! This "library" build is here as a hack for loading unit tests to run on local arch, without depending on any hardware-related stuff.
//! (This is necessary as main.rs is inherently hardware-related code.)
//! See the `run-tests` script in the project root for more information.

#![cfg_attr(not(test), no_std)]

#[allow(dead_code, unused_imports)]
mod keymap;
#[allow(dead_code, unused_imports)]
mod rmk;
#[allow(dead_code, unused_imports)]
mod scan;
#[allow(dead_code, unused_imports)]
mod steno;
