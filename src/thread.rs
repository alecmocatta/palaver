#[cfg(unix)]
use nix::libc;
use std::{sync, thread};

/// Get an identifier for the thread;
/// - uses gettid on Linux;
/// - pthread_threadid_np on macOS;
/// - pthread_getthreadid_np on FreeBSD;
/// - GetCurrentThreadId on windows.
#[inline]
pub fn gettid() -> u64 {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		use nix::unistd;
		Into::<libc::pid_t>::into(unistd::gettid()) as u64
	}
	#[cfg(any(target_os = "macos", target_os = "ios"))]
	{
		use std::mem;
		// https://github.com/google/xi-editor/blob/346bfe2d96f412cca5c8aa858287730f5ed521c3/rust/trace/src/sys_tid.rs
		#[link(name = "pthread")]
		extern "C" {
			fn pthread_threadid_np(
				thread: libc::pthread_t, thread_id: *mut libc::uint64_t,
			) -> libc::c_int;
		}

		let mut tid: libc::uint64_t = unsafe { mem::uninitialized() };
		let err = unsafe { pthread_threadid_np(0, &mut tid) };
		assert_eq!(err, 0);
		tid
	}
	#[cfg(target_os = "freebsd")]
	{
		#[link(name = "pthread")]
		extern "C" {
			fn pthread_getthreadid_np() -> libc::c_int;
		}
		(unsafe { pthread_getthreadid_np() }) as u64
	}

	#[cfg(windows)]
	{
		extern "C" {
			fn GetCurrentThreadId() -> libc::c_ulong;
		}
		(unsafe { GetCurrentThreadId() }) as u64
	}
}

/// A wrapper around `std::thread::spawn()` that blocks until the new thread has left library code. Library code can do things like temporarily opening fds (leading to dup2 on other threads returning EBUSY on Linux), so blocking this thread until it's done just makes things more predictable.
pub fn spawn<F, T>(name: String, f: F) -> thread::JoinHandle<T>
where
	F: FnOnce() -> T,
	F: Send + 'static,
	T: Send + 'static,
{
	// #[cfg(any(target_os = "macos", target_os = "ios"))]
	// {
	let (sender, receiver) = sync::mpsc::channel::<()>();
	let ret = thread::Builder::new()
		.name(name)
		.spawn(move || {
			drop(sender);
			f()
		})
		.unwrap();
	if let Err(sync::mpsc::RecvError) = receiver.recv() {
	} else {
		unreachable!()
	}
	ret
	// }
	// #[cfg(not(any(target_os = "macos", target_os = "ios")))]
	// {
	// 	thread::Builder::new().name(name).spawn(f).unwrap()
	// }
}
