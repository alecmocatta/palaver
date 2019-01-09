//! A Rust fork wrapper that uses process descriptors (pdfork) on FreeBSD and normal fork elsewhere.
//!
//! Process descriptors are like file descriptors but for processes:
//! - they are immune to PID race conditions (they track the exact process in the kernel);
//! - they work in the [Capsicum](https://wiki.freebsd.org/Capsicum) capability mode sandbox.
//!
//! ```no_run
//! extern crate libc;
//! extern crate palaver;
//! use palaver::fork::*;
//!
//! match fork().unwrap() {
//!     ForkResult::Parent(child_proc) => {
//!         // do stuff
//!         // you can access child_proc.child_pid on any platform
//!         // you can also access child_proc.child_pd on FreeBSD
//!         if !child_proc.signal(libc::SIGTERM) {
//!             panic!("sigterm");
//!         }
//!     },
//!     ForkResult::Child => {
//!         // do stuff
//!     }
//! }
//! ```

#[cfg(target_os = "freebsd")]
use super::Fd;
use nix::libc;

/// Child process handle
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub struct ChildHandle {
	/// Child Process ID
	pub child_pid: libc::pid_t,
	#[cfg(target_os = "freebsd")]
	/// Child Process Descriptor
	pub child_pd: Fd,
}

impl ChildHandle {
	/// Signal the child process
	#[cfg(unix)]
	pub fn signal(&self, sig: libc::c_int) -> bool {
		#[cfg(target_os = "freebsd")]
		unsafe {
			libc::pdkill(self.child_pd, sig) == 0
		}
		#[cfg(not(target_os = "freebsd"))]
		unsafe {
			libc::kill(self.child_pid, sig) == 0
		}
	}
}

#[cfg(target_os = "freebsd")]
impl Drop for ChildHandle {
	fn drop(&mut self) {
		let err = unsafe { libc::close(self.child_pd) };
		assert_eq!(err, 0);
	}
}

/// Fork result
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub enum ForkResult {
	/// Parent process
	Parent(ChildHandle),
	/// Child process
	Child,
}

/// Fork
#[cfg(unix)]
pub fn fork() -> Result<ForkResult, ()> {
	#[cfg(target_os = "freebsd")]
	{
		let mut child_pd = -1;
		let child_pid = unsafe { libc::pdfork(&mut child_pd, 0) };
		if child_pid < 0 {
			Err(())
		} else if child_pid > 0 {
			Ok(ForkResult::Parent(ChildHandle {
				child_pid,
				child_pd,
			}))
		} else {
			Ok(ForkResult::Child)
		}
	}
	#[cfg(not(target_os = "freebsd"))]
	{
		let child_pid = unsafe { libc::fork() };
		if child_pid < 0 {
			Err(())
		} else if child_pid > 0 {
			Ok(ForkResult::Parent(ChildHandle { child_pid }))
		} else {
			Ok(ForkResult::Child)
		}
	}
}
