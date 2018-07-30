//! Cross-platform polyfills.
//!
//! **[Crates.io](https://crates.io/crates/palaver) │ [Repo](https://github.com/alecmocatta/palaver)**
//!
//! This library attempts to provide reliable pollyfills for functionality that isn't implemented on all platforms, for example `gettid`, `memfd_create`, `fexecve`, as well as providing non-atomic versions of functions like `accept4`, `socket`+`SOCK_CLOEXEC`, `pipe2`, and other miscellanea like `seal` to make a file descriptor read-only thus suitable for `fexecve`.
//!
//! palaver = "Platform Abstraction Layer" / pa·lav·er *n.* – prolonged and tedious fuss.
//!
//! It's currently used on unix-family systems; most Windows functionality is TODO.

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
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![cfg_attr(feature = "cargo-clippy", warn(clippy_pedantic))]
#![cfg_attr(
	feature = "cargo-clippy",
	allow(
		inline_always,
		doc_markdown,
		if_not_else,
		indexing_slicing,
		cast_sign_loss,
		cast_possible_truncation,
		cast_possible_wrap
	)
)]

#[cfg(unix)]
extern crate nix;
extern crate proc_self;
extern crate valgrind_request;
extern crate void;
#[cfg(windows)]
extern crate winapi;
#[macro_use]
extern crate bitflags;

mod ext;
mod file;
mod socket;
mod thread;
mod valgrind;

#[cfg(unix)]
type Fd = std::os::unix::io::RawFd;
#[cfg(windows)]
type Fd = std::os::windows::io::RawHandle;

pub use file::*;
pub use socket::*;
pub use thread::*;
pub use valgrind::*;
