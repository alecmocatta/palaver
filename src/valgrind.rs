//! Valgrind-related functionality

#[cfg(unix)]
use super::*;
#[cfg(unix)]
use nix::{errno, libc};
#[cfg(unix)]
use std::convert::TryInto;
#[cfg(unix)]
use std::mem;

#[cfg(all(target_os = "linux", not(target_env = "musl")))]
fn getrlimit(resource: libc::__rlimit_resource_t) -> Result<libc::rlimit64, nix::Error> {
	let mut rlim: libc::rlimit64 = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit64(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}
#[cfg(any(target_os = "android", target_env = "musl"))]
fn getrlimit(resource: libc::c_int) -> Result<libc::rlimit64, nix::Error> {
	let mut rlim: libc::rlimit64 = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit64(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}
#[cfg(all(unix, not(any(target_os = "android", target_os = "linux"))))]
fn getrlimit(resource: libc::c_int) -> Result<libc::rlimit, nix::Error> {
	let mut rlim: libc::rlimit = unsafe { mem::uninitialized() };
	let err = unsafe { libc::getrlimit(resource, &mut rlim) };
	errno::Errno::result(err).map(|_| rlim)
}

/// Check if we're running under valgrind
#[cfg(feature = "nightly")]
pub fn is() -> bool {
	valgrind_request::running_on_valgrind() > 0
}
/// Valgrind sets up various file descriptors for its purposes; they're all > any user fds, and this function gets the lowest of them
#[cfg(unix)]
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
