//! Process-related functionality

#[cfg(unix)]
use crate::{file, Fd};
#[cfg(unix)]
use nix::{
	fcntl, libc, poll, sys::{signal, wait}, unistd::{self, Pid}
};
use std::process::Command;
#[cfg(unix)]
use std::{
	convert::{TryFrom, TryInto}, ptr, sync::atomic::{AtomicU8, Ordering}
};

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
// #[allow(missing_copy_implementations)]
#[derive(Debug)]
pub struct ChildHandle {
	/// Child Process ID
	pub pid: Pid,
	#[cfg(target_os = "freebsd")]
	/// Child Process Descriptor
	pub pd: Fd,
	owns: Option<Handle>,
}
#[cfg(unix)]
#[derive(Debug)]
struct Handle {
	state: AtomicU8, // 0, 1 = killed, 2 = reaped
	pipe_write: Fd,
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
	pub fn wait(&self) -> nix::Result<WaitStatus> {
		// EVFILT_PROCDESC on freebsd?
		// linux? https://lwn.net/Articles/773459/
		let ret = Self::wait_(self.pid);
		if let (Ok(_), Some(owns)) = (ret, &self.owns) {
			owns.state.store(2, Ordering::Relaxed);
		}
		ret
	}
	fn wait_(pid: Pid) -> nix::Result<WaitStatus> {
		// EVFILT_PROCDESC on freebsd?
		// linux? https://lwn.net/Articles/784831/
		loop {
			match wait::waitpid(pid, None) {
				Ok(wait::WaitStatus::Exited(pid_, code)) => {
					assert_eq!(pid_, pid);
					break Ok(WaitStatus::Exited(code));
				}
				Ok(wait::WaitStatus::Signaled(pid_, signal, dumped)) => {
					assert_eq!(pid_, pid);
					break Ok(WaitStatus::Signaled(signal, dumped));
				}
				Ok(_) | Err(nix::Error::Sys(nix::errno::Errno::EINTR)) => (),
				Err(err) => break Err(err),
			}
		}
	}
	/// Signal the child process
	pub fn signal<T: Into<Option<signal::Signal>>>(&self, signal: T) -> nix::Result<()> {
		let owns = self
			.owns
			.as_ref()
			.expect(".signal() can only be called on non-orphaned children");
		let signal = signal.into();
		if owns.state.load(Ordering::Relaxed) != 0 {
			return Err(nix::Error::Sys(nix::errno::Errno::ESRCH)); //optimisation, not necessary for correctness
		}
		{
			#[cfg(target_os = "freebsd")]
			{
				let res = unsafe {
					libc::pdkill(
						self.pd,
						match signal {
							Some(s) => s as libc::c_int,
							None => 0,
						},
					)
				};
				Errno::result(res).map(drop)
			}
			#[cfg(not(target_os = "freebsd"))]
			signal::kill(self.pid, signal)
		}?;
		if signal == Some(signal::SIGKILL) {
			let _ = owns.state.compare_and_swap(0, 1, Ordering::Relaxed);
		}
		Ok(())
	}
}

#[cfg(unix)]
impl Drop for ChildHandle {
	fn drop(&mut self) {
		if self.owns.is_some() {
			let state = *self.owns.as_mut().unwrap().state.get_mut();
			if state == 0 {
				self.signal(signal::SIGKILL).expect("a");
			}
			if state != 2 {
				let _ = self.wait().expect("b");
			}
			unistd::close(self.owns.as_mut().unwrap().pipe_write).unwrap();
			let group = Pid::from_raw(-self.pid.as_raw());
			// panic!();
			// eprintln!("{}", format!("kill2 {}", group));
			signal::kill(group, signal::SIGKILL).expect("c");
			// let status = ChildHandle::wait_(Pid::from_raw(-self.pid.as_raw())).expect("d");
			// assert!(matches!(status, WaitStatus::Signaled(signal::SIGKILL, _)), "{:?}", status);
		}
		// {
		// 	signal::kill(self.pid, signal::SIGKILL).unwrap();
		// 	let a = a.wait().unwrap();
		// 	assert!(matches!(a, WaitStatus::Signaled(signal::SIGKILL, _)), "{:?}", a);
		// }
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

struct StdErr;
impl std::io::Write for StdErr {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		use std::os::unix::io::{FromRawFd, IntoRawFd};
		let mut file = unsafe { std::fs::File::from_raw_fd(libc::STDERR_FILENO) };
		let ret = file.write(buf);
		let _ = file.into_raw_fd();
		ret
	}
	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

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
				owns: true,
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
				let child = if let ForkResult::Parent(child) = basic_fork()? {
					child
				} else {
					match basic_fork() {
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
					Ok(ForkResult::Parent(ChildHandle { pid, owns: None }))
				} else {
					Err(nix::Error::Sys(nix::errno::Errno::UnknownErrno))
				}
			})();
			let new2 = unsafe { signal::sigaction(signal::SIGCHLD, &old).unwrap() };
			assert_eq!(new.handler(), new2.handler());
			ret
		} else {
			let (pipe_read, pipe_write) = file::pipe(fcntl::OFlag::empty())?;
			Ok(match basic_fork()? {
				ForkResult::Child => {
					unistd::close(pipe_write).unwrap();
					let a = if let ForkResult::Parent(child) = basic_fork()? {
						child
					} else {
						loop {
							unistd::pause();
						}
					};
					let group = unistd::getpgrp();
					unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
					let b = if let ForkResult::Parent(child) = basic_fork()? {
						child
					} else {
						for fd in 0..1024 {
							let _ = unistd::close(fd);
						}
						loop {
							unistd::pause();
						}
					};
					signal::kill(a.pid, signal::SIGKILL).unwrap();
					let status = a.wait().unwrap();
					assert!(
						matches!(status, WaitStatus::Signaled(signal::SIGKILL, _)),
						"{:?}",
						status
					);
					unistd::setpgid(unistd::Pid::from_raw(0), group).unwrap();
					assert_eq!(unistd::getpgid(Some(b.pid)).unwrap(), unistd::getpid());
					// signal::kill(Pid::from_raw(-unistd::getpid().as_raw()), signal::SIGKILL).unwrap();
					// let b = b.wait().unwrap();
					// assert!(matches!(b, WaitStatus::Signaled(signal::SIGKILL, _)), "{:?}", a);
					// die if our owning process dies
					// PR_SET_PDEATHSIG would kill us if thread exits http://man7.org/linux/man-pages/man2/prctl.2.html
					extern "C" fn thread(arg: *mut libc::c_void) -> *mut libc::c_void {
						// TODO: abort on unwind
						let pipe_read: i32 = (arg as usize).try_into().unwrap();
						// std::thread::spawn(move||{
						let mut pollfds = [poll::PollFd::new(pipe_read, poll::PollFlags::POLLHUP)];
						let n = poll::poll(&mut pollfds, -1).unwrap();
						// assert_eq!(n, 1);
						// let err = unistd::read(pipe_read, &mut [0]);
						// let err2 = unistd::read(pipe_read, &mut [0]);
						// use std::io::Write;
						// StdErr.write_all(format!("aaaaaaa: {:?}\n", err).as_bytes());
						// assert_eq!(err, 0);
						// StdErr.write_all(b"aaaaa\n");
						// loop { }
						if n == 1
							&& pollfds[0]
								.revents()
								.unwrap()
								.contains(poll::PollFlags::POLLHUP)
						{
							// (err,err2) == (Ok(0),Ok(0)) {
							signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
						}
						// std::process::abort();
						// loop {}
						// });
						ptr::null_mut()
					}
					let mut native: libc::pthread_t = 0;
					let res = unsafe {
						libc::pthread_create(
							&mut native,
							ptr::null(),
							thread,
							// ptr::null_mut(),
							usize::try_from(pipe_read).unwrap() as _,
						)
					};
					assert_eq!(res, 0);
					// #[cfg(any(target_os = "android", target_os = "linux"))]
					// {
					// 	let err = unsafe {
					// 		nix::libc::prctl(nix::libc::PR_SET_PDEATHSIG, nix::libc::SIGKILL)
					// 	};
					// 	assert_eq!(err, 0);
					// }
					ForkResult::Child
				}
				ForkResult::Parent(mut child) => {
					unistd::close(pipe_read).unwrap();
					child.owns = Some(Handle {
						state: AtomicU8::new(0),
						pipe_write,
					});
					// std::thread::sleep_ms(500);
					// let group = Pid::from_raw(-child.pid.as_raw());
					// signal::kill(group, signal::SIGKILL).expect("e");
					// eprintln!("{}", format!("kill {}", group));
					// let status = ChildHandle::wait_(group).expect("f");
					// assert!(matches!(status, WaitStatus::Signaled(signal::SIGKILL, _)), "{:?}", status);
					ForkResult::Parent(child)
				}
			})
		}
	}
}

fn basic_fork() -> nix::Result<ForkResult> {
	Ok(match unistd::fork()? {
		unistd::ForkResult::Child => ForkResult::Child,
		unistd::ForkResult::Parent { child } => ForkResult::Parent(ChildHandle {
			pid: child,
			owns: None,
		}),
	})
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
