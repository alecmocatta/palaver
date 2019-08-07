//! Process-related functionality

#[cfg(unix)]
use nix::libc;
use std::process::Command;

#[cfg(target_os = "freebsd")]
use super::Fd;

/// Count the number of processes visible to this process. Counts the lines of `ps aux` minus one (the header).
pub fn count() -> usize {
	let out = Command::new("ps")
		.arg("aux")
		.output()
		.expect("failed to execute process");
	out.stdout
		.split(|&x| x == b'\n')
		.skip(1)
		.filter(|x| !x.is_empty())
		.count()
}

/// Count the number of threads visible to this process. Counts the lines of `ps -eL` and equivalent minus one (the header).
pub fn count_threads() -> usize {
	let out = if cfg!(any(target_os = "linux", target_os = "android")) {
		Command::new("ps")
			.arg("-eL")
			.output()
			.expect("failed to execute process")
	} else if cfg!(any(target_os = "macos", target_os = "ios")) {
		Command::new("ps")
			.arg("-eM")
			.output()
			.expect("failed to execute process")
	} else {
		unimplemented!()
	};
	out.stdout
		.split(|&x| x == b'\n')
		.skip(1)
		.filter(|x| !x.is_empty())
		.count()
}

/// Child process handle
#[cfg(unix)]
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub struct ChildHandle {
	/// Child Process ID
	pub child_pid: libc::pid_t,
	#[cfg(target_os = "freebsd")]
	/// Child Process Descriptor
	pub child_pd: Fd,
}

#[cfg(unix)]
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
#[cfg(unix)]
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub enum ForkResult {
	/// Parent process
	Parent(ChildHandle),
	/// Child process
	Child,
}

/// A Rust fork wrapper that uses process descriptors (pdfork) on FreeBSD and normal fork elsewhere.
///
/// Process descriptors are like file descriptors but for processes:
/// - they are immune to PID race conditions (they track the exact process in the kernel);
/// - they work in the [Capsicum](https://wiki.freebsd.org/Capsicum) capability mode sandbox.
///
/// ```no_run
/// extern crate libc;
/// extern crate palaver;
/// use palaver::process::*;
///
/// match fork().unwrap() {
///     ForkResult::Parent(child_proc) => {
///         // do stuff
///         // you can access child_proc.child_pid on any platform
///         // you can also access child_proc.child_pd on FreeBSD
///         if !child_proc.signal(libc::SIGTERM) {
///             panic!("sigterm");
///         }
///     },
///     ForkResult::Child => {
///         // do stuff
///     }
/// }
/// ```

// See also https://github.com/qt/qtbase/blob/v5.12.0/src/3rdparty/forkfd/forkfd.c
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

#[cfg(test)]
mod tests {
	#[test]
	fn count() {
		let count = super::count();
		assert_ne!(count, 0);
		if !cfg!(windows) {
			let count_threads = super::count_threads();
			assert_ne!(count_threads, 0);
			assert!(
				count_threads >= count,
				"{} threads < {} processes",
				count_threads,
				count
			); // TODO: retry to avoid bad luck flakiness?
		}
	}
}
