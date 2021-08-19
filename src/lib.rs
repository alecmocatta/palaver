//! Cross-platform polyfills.
//!
//! **[Crates.io](https://crates.io/crates/palaver) │ [Repo](https://github.com/alecmocatta/palaver)**
//!
//! This library attempts to provide reliable polyfills for functionality that isn't implemented on all platforms.
//!
//! `palaver` = "Platform Abstraction Layer" + pa·lav·er *n.* prolonged and tedious fuss.

#![doc(html_root_url = "https://docs.rs/palaver/0.3.0-alpha.3")]
#![warn(
	missing_copy_implementations,
	missing_debug_implementations,
	missing_docs,
	trivial_casts,
	trivial_numeric_casts,
	unused_import_braces,
	unused_qualifications,
	unused_results,
	clippy::pedantic
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![allow(
	clippy::doc_markdown,
	clippy::empty_loop,
	clippy::if_not_else,
	clippy::missing_errors_doc,
	clippy::module_name_repetitions,
	clippy::must_use_candidate,
	clippy::same_functions_in_if_condition,
	clippy::shadow_unrelated,
	clippy::similar_names,
	clippy::wildcard_imports,
	clippy::match_single_binding
)]

pub mod env;
mod ext;
pub mod file;
pub mod process;
pub mod socket;
pub mod thread;
#[cfg(unix)]
pub mod valgrind;

#[doc(hidden)]
#[deprecated]
pub use process as fork;

#[cfg(unix)]
type Fd = std::os::unix::io::RawFd;
#[cfg(windows)]
type Fd = std::os::windows::io::RawHandle;
