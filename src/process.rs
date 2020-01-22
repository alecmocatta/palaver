//! Process-related functionality

#[cfg(unix)]
use nix::{
	libc, sys::{signal, wait}, unistd::{self, Pid}
};
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
	pub pid: Pid,
	#[cfg(target_os = "freebsd")]
	/// Child Process Descriptor
	pub pd: Fd,
	orphan: bool,
}

/// Possible return values from [`ChildHandle::wait`].
#[derive(Clone, Copy, Debug)]
pub enum WaitStatus {
	/// The process exited normally (as with `exit()` or returning from
	/// `main`) with the given exit code. This case matches the C macro
	/// `WIFEXITED(status)`; the second field is `WEXITSTATUS(status)`.
	Exited(i32),
	/// The process was killed by the given signal. The third field
	/// indicates whether the signal generated a core dump. This case
	/// matches the C macro `WIFSIGNALED(status)`; the last two fields
	/// correspond to `WTERMSIG(status)` and `WCOREDUMP(status)`.
	Signaled(signal::Signal, bool),
}

#[cfg(unix)]
impl ChildHandle {
	/// Signal the child process
	pub fn wait(mut self) -> nix::Result<WaitStatus> {
		// EVFILT_PROCDESC on freebsd?
		// linux? https://lwn.net/Articles/773459/
		self.orphan = true;
		loop {
			match wait::waitpid(self.pid, None) {
				Ok(wait::WaitStatus::Exited(pid, code)) => {
					assert_eq!(pid, self.pid);
					break Ok(WaitStatus::Exited(code));
				}
				Ok(wait::WaitStatus::Signaled(pid, signal, dumped)) => {
					assert_eq!(pid, self.pid);
					break Ok(WaitStatus::Signaled(signal, dumped));
				}
				Ok(_) | Err(nix::Error::Sys(nix::errno::Errno::EINTR)) => (),
				Err(err) => break Err(err),
			}
		}
	}
	/// Signal the child process
	pub fn signal<T: Into<Option<signal::Signal>>>(&self, sig: T) -> nix::Result<()> {
		#[cfg(target_os = "freebsd")]
		{
			let res = unsafe {
				libc::pdkill(
					self.pd,
					match signal.into() {
						Some(s) => s as libc::c_int,
						None => 0,
					},
				)
			};
			Errno::result(res).map(drop)
		}
		#[cfg(not(target_os = "freebsd"))]
		signal::kill(self.pid, sig)
	}
}

#[cfg(unix)]
impl Drop for ChildHandle {
	fn drop(&mut self) {
		if !self.orphan {
			panic!("must call .wait() before dropping");
		}
		#[cfg(target_os = "freebsd")]
		{
			let err = unsafe { libc::close(self.pd) };
			assert_eq!(err, 0);
		}
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
///         // you can access child_proc.pid on any platform
///         // you can also access child_proc.pd on FreeBSD
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
pub fn fork(orphan: bool) -> nix::Result<ForkResult> {
	#[cfg(target_os = "freebsd")]
	{
		if orphan {
			unimplemented!();
		}
		let mut child_pd = -1;
		let child_pid = unsafe { libc::pdfork(&mut child_pd, 0) };
		if child_pid < 0 {
			Err(())
		} else if child_pid > 0 {
			Ok(ForkResult::Parent(ChildHandle {
				child_pid,
				child_pd,
				orphan,
			}))
		} else {
			Ok(ForkResult::Child)
		}
	}
	#[cfg(not(target_os = "freebsd"))]
	{
		if orphan {
			// inspired by fork2 http://www.faqs.org/faqs/unix-faq/programmer/faq/
			// TODO: how to make this not racy?
			let new = signal::SigAction::new(
				signal::SigHandler::SigDfl,
				signal::SaFlags::empty(),
				signal::SigSet::empty(),
			);
			let old = unsafe { signal::sigaction(signal::SIGCHLD, &new).unwrap() };
			let ret = (|| {
				let child = if let ForkResult::Parent(child) = fork(false)? {
					child
				} else {
					match fork(false) {
						Ok(ForkResult::Child) => {
							return Ok(ForkResult::Child);
						}
						Ok(ForkResult::Parent(_)) => unsafe { libc::_exit(0) },
						Err(_) => unsafe { libc::_exit(1) },
					}
				};
				let exit = child.wait().unwrap();
				if let WaitStatus::Exited(0) = exit {
					let pid = Pid::from_raw(i32::max_value()); // TODO!
					Ok(ForkResult::Parent(ChildHandle { pid, orphan }))
				} else {
					Err(nix::Error::Sys(nix::errno::Errno::UnknownErrno))
				}
			})();
			let new2 = unsafe { signal::sigaction(signal::SIGCHLD, &old).unwrap() };
			assert_eq!(new.handler(), new2.handler());
			ret
		} else {
			Ok(match unistd::fork()? {
				unistd::ForkResult::Child => ForkResult::Child,
				unistd::ForkResult::Parent { child } => {
					ForkResult::Parent(ChildHandle { pid: child, orphan })
				}
			})
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
