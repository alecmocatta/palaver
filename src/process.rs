//! Process-related functionality

#[cfg(unix)]
use nix::{
	cmsg_space, errno::Errno, fcntl, libc, sys::socket, sys::uio, sys::{signal, wait}, unistd::{self, Pid}, Error
};
use std::process::Command;
#[cfg(unix)]
use std::{
	os::unix::io::AsRawFd, os::unix::net::UnixDatagram, sync::atomic::{AtomicU8, Ordering}
};

#[cfg(unix)]
use crate::{file, Fd};

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
#[derive(Debug)]
pub struct ChildHandle {
	/// Child Process ID
	pub pid: Pid,
	/// Child Process Descriptor
	#[cfg(target_os = "freebsd")]
	pub pd: Fd,
	owns: Option<Handle>,
}

#[cfg(unix)]
#[derive(Debug)]
struct Handle {
	state: AtomicU8, // 0, 1 = killed, 2 = reaped
	#[cfg(not(target_os = "freebsd"))]
	eternal_write: Fd,
}

/// Possible return values from [`ChildHandle::wait`].
#[cfg(unix)]
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
	// TODO: catch multiple waiters
	pub fn wait(&self) -> nix::Result<WaitStatus> {
		let ret = Self::wait_(self.pid);
		if let (Ok(_), Some(owns)) = (ret, &self.owns) {
			owns.state.store(2, Ordering::Relaxed);
		}
		ret
	}
	fn wait_(pid: Pid) -> nix::Result<WaitStatus> {
		// EVFILT_PROCDESC on freebsd?
		// pidfd linux? https://lwn.net/Articles/784831/ https://lwn.net/Articles/794707/ https://github.com/pop-os/pidfd
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
				Ok(_) | Err(Error::Sys(Errno::EINTR)) => (),
				Err(err) => break Err(err),
			}
		}
	}
	/// Signal the child process
	#[allow(unreachable_code)]
	pub fn signal<T: Into<Option<signal::Signal>>>(&self, signal: T) -> nix::Result<()> {
		let signal = signal.into();
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
			return Errno::result(res).map(drop);
		}
		let owns = self
			.owns
			.as_ref()
			.expect(".signal() can only be called on non-orphaned children");
		if owns.state.load(Ordering::Relaxed) != 0 {
			return Err(Error::Sys(Errno::ESRCH));
		}
		signal::kill(self.pid, signal)?;
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
			let group = Pid::from_raw(-self.pid.as_raw());
			signal::kill(group, signal::SIGKILL).expect("c");
			#[cfg(not(target_os = "freebsd"))]
			unistd::close(self.owns.as_mut().unwrap().eternal_write).unwrap();
		}
		#[cfg(target_os = "freebsd")]
		unistd::close(self.pd).unwrap();
	}
}

/// Fork result
#[cfg(unix)]
#[derive(Debug)]
pub enum ForkResult {
	/// Parent process
	Parent(ChildHandle),
	/// Child process
	Child,
}

/// A Rust fork wrapper that provides more coherent, FreeBSD-inspired semantics:
///
/// - immune to PID race conditions (see [here](https://lwn.net/Articles/773459/) for a description of the race);
/// - thus it's possible to `waitpid()` on one thread and `kill()` on another without a race;
/// - option to orphan a process, i.e. hand it off to init;
/// - child processes are killed on parent termination;
/// - and it works in the [Capsicum](https://wiki.freebsd.org/Capsicum) capability mode sandbox.
///
/// It's implemented using process descriptors (pdfork) on FreeBSD and normal fork + an extra process elsewhere.
///
/// # Example
/// ```no_run
/// use palaver::process::*;
///
/// match fork(false, true).unwrap() {
///     ForkResult::Parent(child_proc) => {
///         // do stuff
///         // you can access child_proc.pid on any platform
///         // you can also access child_proc.pd on FreeBSD
///         if let Err(err) = child_proc.signal(nix::sys::signal::SIGTERM) {
///             panic!("sigterm: {:?}", err);
///         }
///     },
///     ForkResult::Child => {
///         // do stuff
///     }
/// }
/// ```
// See also https://github.com/qt/qtbase/blob/v5.12.0/src/3rdparty/forkfd/forkfd.c
#[cfg(unix)]
#[allow(clippy::too_many_lines)]
pub fn fork(orphan: bool) -> nix::Result<ForkResult> {
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
			let child = if let ForkResult::Parent(child) = basic_fork(false)? {
				child
			} else {
				match basic_fork(true) {
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
				#[cfg(target_os = "freebsd")]
				let pd = i32::max_value(); // TODO!
				Ok(ForkResult::Parent(ChildHandle {
					pid,
					#[cfg(target_os = "freebsd")]
					pd,
					owns: None,
				}))
			} else {
				Err(Error::Sys(Errno::UnknownErrno))
			}
		})();
		if new.handler() != old.handler() {
			let new2 = unsafe { signal::sigaction(signal::SIGCHLD, &old).unwrap() };
			assert_eq!(new.handler(), new2.handler());
		}
		ret
	} else {
		if cfg!(target_os = "freebsd") {
			return basic_fork(false);
		}
		let (ready_write, ready_read) = UnixDatagram::pair().unwrap();
		Ok(match basic_fork(false)? {
			ForkResult::Child => {
				drop(ready_read);
				let new = signal::SigAction::new(
					signal::SigHandler::SigDfl,
					signal::SaFlags::empty(),
					signal::SigSet::empty(),
				);
				let _ = unsafe { signal::sigaction(signal::SIGCHLD, &new).unwrap() };
				let pid = unistd::getpid();
				let group = unistd::getpgrp();
				let (eternal_read, eternal_write) = file::pipe(fcntl::OFlag::empty()).unwrap();
				let our_group_retainer = if group != pid {
					let child = if let ForkResult::Parent(child) = basic_fork(false)? {
						child
					} else {
						drop(ready_write);
						unistd::close(eternal_write).unwrap();
						let err = unistd::read(eternal_read, &mut [0]).unwrap();
						assert_eq!(err, 0);
						signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
						loop {}
					};
					unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
					Some(child)
				} else {
					None
				};
				let _our_pid_retainer = if let ForkResult::Parent(child) = basic_fork(false)? {
					child
				} else {
					drop(ready_write);
					unistd::close(eternal_write).unwrap();
					for fd in 0..1024 {
						// TODO // && fd > 2 {
						if fd != eternal_read {
							let _ = unistd::close(fd);
						}
					}
					let err = unistd::read(eternal_read, &mut [0]).unwrap();
					assert_eq!(err, 0);
					assert_eq!(unistd::getpgrp(), pid);
					let _ = signal::kill(pid, signal::SIGKILL);
					signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
					loop {}
				};
				unistd::close(eternal_read).unwrap();
				let iov = [uio::IoVec::from_slice(&[])];
				let fds = [eternal_write];
				let cmsg = [socket::ControlMessage::ScmRights(&fds)];
				let _ = socket::sendmsg(
					ready_write.as_raw_fd(),
					&iov,
					&cmsg,
					socket::MsgFlags::empty(),
					None,
				)
				.map(|x| {
					assert_eq!(x, 0);
				});
				drop(ready_write);
				unistd::close(eternal_write).unwrap();
				if let Some(retainer) = our_group_retainer {
					let group = unistd::getpgid(Some(retainer.pid)).unwrap_or(group); // slightly more immune to races than getpgrp?
					unistd::setpgid(unistd::Pid::from_raw(0), group).unwrap();
					signal::kill(retainer.pid, signal::SIGKILL).unwrap();
					let _ = retainer.wait().unwrap();
				}
				ForkResult::Child
			}
			ForkResult::Parent(mut child) => {
				drop(ready_write);
				let mut buf = [0; 8];
				let iovec = [uio::IoVec::from_mut_slice(&mut buf)];
				let mut space = cmsg_space!([Fd; 2]);
				let eternal_write = socket::recvmsg(
					ready_read.as_raw_fd(),
					&iovec,
					Some(&mut space),
					socket::MsgFlags::empty(),
				)
				.map(|msg| {
					let mut iter = msg.cmsgs();
					match (iter.next(), iter.next()) {
						(Some(socket::ControlMessageOwned::ScmRights(fds)), None) => {
							assert_eq!(msg.bytes, 0);
							assert_eq!(fds.len(), 1);
							fds[0]
						}
						_ => panic!(),
					}
				})
				.unwrap();
				drop(ready_read);
				child.owns = Some(Handle {
					state: AtomicU8::new(0),
					#[cfg(not(target_os = "freebsd"))]
					eternal_write,
				});
				let _ = eternal_write;
				ForkResult::Parent(child)
			}
		})
	}
}

#[cfg(unix)]
fn basic_fork(may_outlive: bool) -> nix::Result<ForkResult> {
	#[cfg(target_os = "freebsd")]
	{
		let mut pd = -1;
		let res = unsafe { libc::pdfork(&mut pd, if may_outlive { libc::PD_DAEMON } else { 0 }) };
		Errno::result(res).map(|res| match res {
			0 => ForkResult::Child,
			pid => ForkResult::Parent(ChildHandle {
				pid: Pid::from_raw(pid),
				pd,
				owns: None,
			}),
		})
	}
	#[cfg(not(target_os = "freebsd"))]
	{
		let _ = may_outlive;
		Ok(match unistd::fork()? {
			unistd::ForkResult::Child => ForkResult::Child,
			unistd::ForkResult::Parent { child } => ForkResult::Parent(ChildHandle {
				pid: child,
				owns: None,
			}),
		})
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
