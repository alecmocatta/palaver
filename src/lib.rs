//! Cross-platform polyfills.
//!
//! **[Crates.io](https://crates.io/crates/palaver) │ [Repo](https://github.com/alecmocatta/palaver)**
//!
//! This library attempts to provide reliable polyfills for functionality that isn't implemented on all platforms, for example `gettid`, `memfd_create`, `fexecve`, `/proc/self`, as well as providing non-atomic versions of functions like `accept4`, `socket`+`SOCK_CLOEXEC`, `pipe2`, and other miscellanea like `seal` to make a file descriptor read-only thus suitable for `fexecve`.
//!
//! palaver = "Platform Abstraction Layer" / pa·lav·er *n.* – prolonged and tedious fuss.
//!
//! It's currently used on unix-family systems; most Windows functionality is TODO.

#![feature(try_from)]
#![doc(html_root_url = "https://docs.rs/palaver/0.1.0")]
#![warn(
	missing_copy_implementations,
	missing_debug_implementations,
	missing_docs,
	trivial_numeric_casts,
	unused_extern_crates,
	unused_import_braces,
	unused_qualifications,
	unused_results,
	clippy::pedantic
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![allow(
	clippy::inline_always,
	clippy::doc_markdown,
	clippy::if_not_else,
	clippy::indexing_slicing,
	clippy::shadow_unrelated
)]

#[cfg(unix)]
extern crate nix;
extern crate valgrind_request;
extern crate void;
#[cfg(windows)]
extern crate winapi;
#[macro_use]
extern crate bitflags;
#[cfg(any(target_os = "macos", target_os = "ios"))]
extern crate mach;

mod ext;
pub mod file;
pub mod proc;
pub mod socket;
pub mod thread;
pub mod valgrind;

#[cfg(unix)]
type Fd = std::os::unix::io::RawFd;
#[cfg(windows)]
type Fd = std::os::windows::io::RawHandle;
