//! Cross-platform polyfills.
//!
//! **[Crates.io](https://crates.io/crates/palaver) │ [Repo](https://github.com/alecmocatta/palaver)**
//!
//! This library attempts to provide reliable polyfills for functionality that isn't implemented on all platforms.
//!
//! `palaver` = "Platform Abstraction Layer" + pa·lav·er *n.* prolonged and tedious fuss.

#![feature(try_from, uniform_paths)]
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
	clippy::doc_markdown,
	clippy::if_not_else,
	clippy::shadow_unrelated,
	clippy::similar_names,
	clippy::module_name_repetitions
)]

pub mod env;
mod ext;
pub mod file;
#[cfg(unix)]
pub mod fork;
pub mod socket;
pub mod thread;
pub mod valgrind;

#[cfg(unix)]
type Fd = std::os::unix::io::RawFd;
#[cfg(windows)]
type Fd = std::os::windows::io::RawHandle;
