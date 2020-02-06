//! Process-related functionality

#[cfg(unix)]
use nix::{
	errno::Errno, fcntl, libc, sys::{signal, wait}, unistd::{self, Pid}, Error
};
use std::process::Command;
#[cfg(unix)]
use std::{
	os::unix::net::UnixDatagram, sync::atomic::{AtomicU8, Ordering}
};

#[cfg(unix)]
use crate::{file, Fd};

#[doc(inline)]
#[cfg(unix)]
pub use signal::Signal;

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
	guard_write: Fd,
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
	Signaled(Signal, bool),
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
	pub fn signal<T: Into<Option<Signal>>>(&self, signal: T) -> nix::Result<()> {
		let signal = signal.into();
		#[cfg(target_os = "freebsd")]
		{
			assert_ne!(self.pd, i32::max_value(), "todo");
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
				let _ = self.signal(signal::SIGKILL);
			}
			if state != 2 {
				let _ = self.wait().unwrap();
			}
			let group = Pid::from_raw(-self.pid.as_raw());
			let _ = signal::kill(group, signal::SIGKILL);
			#[cfg(not(target_os = "freebsd"))]
			unistd::close(self.owns.as_mut().unwrap().guard_write).unwrap();
		}
		#[cfg(target_os = "freebsd")]
		{
			if self.pd != i32::max_value() {
				unistd::close(self.pd).unwrap();
			}
		}
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

/// A Rust fork wrapper that provides more coherent, FreeBSD-inspired semantics.
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
/// match fork(false).unwrap() {
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
		// TODO: make this not racy, could add a third fork?
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
		let (ready_read, ready_write) = UnixDatagram::pair().unwrap();
		Ok(match basic_fork(false)? {
			ForkResult::Child => {
				drop(ready_read);
				let new = signal::SigAction::new(
					signal::SigHandler::SigDfl,
					signal::SaFlags::empty(),
					signal::SigSet::empty(),
				);
				let old = unsafe { signal::sigaction(signal::SIGCHLD, &new).unwrap() };
				let pid = unistd::getpid();
				let group = unistd::getpgrp();
				let our_group_retainer = if group != pid {
					let (temp_read, temp_write) = file::pipe(fcntl::OFlag::O_CLOEXEC).unwrap();
					let child = if let ForkResult::Parent(child) = basic_fork(false)? {
						child
					} else {
						drop(ready_write);
						unistd::close(temp_write).unwrap();
						let err = unistd::read(temp_read, &mut [0]).unwrap();
						assert_eq!(err, 0);
						signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
						loop {}
					};
					unistd::close(temp_read).unwrap();
					unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
					Some((child, temp_write))
				} else {
					None
				};
				let (guard_read, guard_write) = file::pipe(fcntl::OFlag::O_CLOEXEC).unwrap();
				let mut prev = signal::SigSet::empty();
				signal::sigprocmask(
					signal::SigmaskHow::SIG_BLOCK,
					Some(&signal::SigSet::all()),
					Some(&mut prev),
				)
				.unwrap();
				let our_pid_retainer = if let ForkResult::Parent(child) = basic_fork(false)? {
					child
				} else {
					ignore_signals();
					signal::sigprocmask(signal::SigmaskHow::SIG_SETMASK, Some(&prev), None)
						.unwrap();
					drop(ready_write);
					if let Some((_retainer, temp_write)) = &our_group_retainer {
						unistd::close(*temp_write).unwrap();
					}
					unistd::close(guard_write).unwrap();
					for fd in 0..1024 {
						// TODO // && fd > 2 {
						if fd != guard_read {
							let _ = unistd::close(fd);
						}
					}
					let err = unistd::read(guard_read, &mut [0]).unwrap();
					assert_eq!(err, 0);
					assert_eq!(unistd::getpgrp(), pid);
					let _ = signal::kill(pid, signal::SIGKILL);
					signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
					loop {}
				};
				signal::sigprocmask(signal::SigmaskHow::SIG_SETMASK, Some(&prev), None).unwrap();
				unistd::close(guard_read).unwrap();
				send_fd::send_fd(guard_write, &ready_write).unwrap_or_else(|_| {
					if let Some((retainer, temp_write)) = &our_group_retainer {
						unistd::close(*temp_write).unwrap();
						let _ = signal::kill(retainer.pid, signal::SIGKILL);
					}
					let _ = signal::kill(our_pid_retainer.pid, signal::SIGKILL);
					signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
					loop {}
				});
				drop(ready_write);
				unistd::close(guard_write).unwrap();
				if let Some((retainer, temp_write)) = our_group_retainer {
					unistd::getpgid(Some(retainer.pid))
						.and_then(|group| unistd::setpgid(unistd::Pid::from_raw(0), group))
						.and_then(|_| signal::kill(retainer.pid, None))
						.unwrap_or_else(|_| {
							unistd::close(temp_write).unwrap();
							let _ = signal::kill(retainer.pid, signal::SIGKILL);
							let _ = signal::kill(our_pid_retainer.pid, signal::SIGKILL);
							signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
							loop {}
						});
					let _ = signal::kill(retainer.pid, signal::SIGKILL);
					let _ = retainer.wait().unwrap();
				}
				if new.handler() != old.handler() {
					let new2 = unsafe { signal::sigaction(signal::SIGCHLD, &old).unwrap() };
					assert_eq!(new.handler(), new2.handler());
				}
				ForkResult::Child
			}
			ForkResult::Parent(mut child) => {
				drop(ready_write);
				let guard_write = send_fd::receive_fd(&ready_read).unwrap_or_else(|_| {
					signal::kill(unistd::getpid(), signal::SIGKILL).unwrap();
					loop {}
				});
				drop(ready_read);
				child.owns = Some(Handle {
					state: AtomicU8::new(0),
					#[cfg(not(target_os = "freebsd"))]
					guard_write,
				});
				let _ = guard_write;
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
		let res = unsafe {
			libc::pdfork(
				&mut pd,
				libc::PD_CLOEXEC | if may_outlive { libc::PD_DAEMON } else { 0 },
			)
		};
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

#[cfg(unix)]
fn ignore_signals() {
	let new = signal::SigAction::new(
		signal::SigHandler::SigIgn,
		signal::SaFlags::empty(),
		signal::SigSet::empty(),
	);
	for signal in signal::Signal::iterator() {
		if signal == signal::Signal::SIGKILL || signal == signal::Signal::SIGSTOP {
			continue;
		}
		let _ = unsafe { signal::sigaction(signal, &new) };
	}
}

#[cfg(unix)]
mod send_fd {
	#![allow(trivial_casts)]

	use nix::errno::Errno;
	use std::{
		convert::TryInto, mem, os::unix::{
			io::{AsRawFd, RawFd}, net::UnixDatagram
		}, ptr::{read_unaligned, write_unaligned}
	};

	const BUF_SIZE: usize = 32; // mac is 16, linux 20, should be big enough everywhere?

	// https://github.com/Aaron1011/spawn-pidfd/blob/44c40733905b0d793ac8393079ecb393774cedda/src/lib.rs#L63-L123
	pub fn send_fd(fd: RawFd, sock: &UnixDatagram) -> nix::Result<()> {
		let mut msg: libc::msghdr = unsafe { mem::zeroed() };
		let fds: [libc::c_int; 1] = [fd];
		let buf_size = unsafe {
			libc::CMSG_SPACE(std::mem::size_of::<[libc::c_int; 1]>().try_into().unwrap()) as usize
		};
		assert!(BUF_SIZE >= buf_size, "{} < {}", BUF_SIZE, buf_size);
		let mut buf: [libc::c_char; BUF_SIZE] = unsafe { mem::zeroed() };

		msg.msg_control = buf.as_mut_ptr() as *mut libc::c_void;
		msg.msg_controllen = mem::size_of_val(&buf).try_into().unwrap();

		let mut iov: [libc::iovec; 1] = unsafe { mem::zeroed() };
		iov[0].iov_base = &mut () as *mut _ as *mut libc::c_void;
		iov[0].iov_len = 0;

		msg.msg_iov = iov.as_mut_ptr();
		msg.msg_iovlen = 1;

		let cmsg: *mut libc::cmsghdr;
		unsafe {
			cmsg = libc::CMSG_FIRSTHDR(&msg);
			(*cmsg).cmsg_level = libc::SOL_SOCKET;
			(*cmsg).cmsg_type = libc::SCM_RIGHTS;
			(*cmsg).cmsg_len =
				(libc::CMSG_LEN((mem::size_of::<[libc::c_int; 1]>()).try_into().unwrap())
					as libc::size_t)
					.try_into()
					.unwrap();
			#[allow(clippy::cast_ptr_alignment)]
			write_unaligned(libc::CMSG_DATA(cmsg) as *mut libc::c_int, fds[0]);
			msg.msg_controllen = (*cmsg).cmsg_len;

			let ret = libc::sendmsg(sock.as_raw_fd(), &msg, 0);
			Errno::result(ret).map(drop)
		}
	}
	pub fn receive_fd(sock: &UnixDatagram) -> nix::Result<RawFd> {
		let mut msg: libc::msghdr = unsafe { mem::zeroed() };
		let buf_size = unsafe {
			libc::CMSG_SPACE(std::mem::size_of::<[libc::c_int; 1]>().try_into().unwrap()) as usize
		};
		assert!(BUF_SIZE >= buf_size, "{} < {}", BUF_SIZE, buf_size);
		let mut buf: [libc::c_char; BUF_SIZE] = unsafe { mem::zeroed() };

		let mut dummy: libc::c_char = 0;
		let mut iov: [libc::iovec; 1] = unsafe { mem::zeroed() };
		iov[0].iov_base = &mut dummy as *mut _ as *mut libc::c_void;
		iov[0].iov_len = 1;

		msg.msg_iov = iov.as_mut_ptr();
		msg.msg_iovlen = 1;
		msg.msg_control = buf.as_mut_ptr() as *mut libc::c_void;
		msg.msg_controllen = mem::size_of_val(&buf).try_into().unwrap();
		let ret = unsafe { libc::recvmsg(sock.as_raw_fd(), &mut msg as *mut libc::msghdr, 0) };
		Errno::result(ret).map(|_r| {
			let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
			#[allow(clippy::cast_ptr_alignment)]
			let fd = unsafe { read_unaligned(libc::CMSG_DATA(cmsg) as *mut libc::c_int) };
			fd
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
