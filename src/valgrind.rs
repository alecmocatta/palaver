//! Valgrind-related functionality

use super::*;
use nix::{errno, libc};
use std::{convert::TryInto, mem};

#[cfg(all(target_os = "linux", not(target_env = "musl")))]
fn getrlimit(resource: libc::__rlimit_resource_t) -> nix::Result<libc::rlimit64> {
	let mut rlim: libc::rlimit64 = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit64(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}
#[cfg(any(target_os = "android", target_env = "musl"))]
fn getrlimit(resource: libc::c_int) -> nix::Result<libc::rlimit64> {
	let mut rlim: libc::rlimit64 = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit64(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}
#[cfg(all(unix, not(any(target_os = "android", target_os = "linux"))))]
fn getrlimit(resource: libc::c_int) -> nix::Result<libc::rlimit> {
	let mut rlim: libc::rlimit = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}

/// Check if we're running under valgrind
pub fn is() -> Result<bool, ()> {
	#[cfg(feature = "nightly")]
	return Ok(valgrind_request::running_on_valgrind() > 0);
	#[cfg(not(feature = "nightly"))]
	Err(())
}
/// Valgrind sets up various file descriptors for its purposes; they're all > any user fds, and this function gets the lowest of them
pub fn start_fd() -> Fd {
	let rlim = getrlimit(libc::RLIMIT_NOFILE).unwrap();
	let valgrind_start_fd = rlim.rlim_max;
	assert!(
		valgrind_start_fd < Fd::max_value().try_into().unwrap(),
		"{:?}",
		valgrind_start_fd
	);
	valgrind_start_fd.try_into().unwrap()
}
