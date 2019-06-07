//! Thread-related functionality

#[cfg(unix)]
use nix::libc;
#[cfg(any(
	target_os = "android",
	target_os = "linux",
	target_os = "macos",
	target_os = "ios",
	target_os = "freebsd"
))]
use try_from::TryInto;

/// Get an identifier for the thread;
///
/// - uses `gettid` on Linux;
/// - `pthread_threadid_np` on macOS;
/// - `pthread_getthreadid_np` on FreeBSD;
/// - `_lwp_self` on NetBSD;
/// - `GetCurrentThreadId` on Windows.
#[inline]
pub fn gettid() -> u64 {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		use nix::unistd;
		Into::<libc::pid_t>::into(unistd::gettid())
			.try_into()
			.unwrap()
	}
	#[cfg(any(target_os = "macos", target_os = "ios"))]
	{
		use std::mem;
		// https://github.com/google/xi-editor/blob/346bfe2d96f412cca5c8aa858287730f5ed521c3/rust/trace/src/sys_tid.rs
		// or mach_thread_self ?
		#[link(name = "pthread")]
		extern "C" {
			fn pthread_threadid_np(thread: libc::pthread_t, thread_id: *mut u64) -> libc::c_int;
		}
		let mut tid: u64 = unsafe { mem::uninitialized() };
		let err = unsafe { pthread_threadid_np(0, &mut tid) };
		assert_eq!(err, 0);
		tid
	}
	#[cfg(target_os = "freebsd")]
	{
		// or thr_self ?
		#[link(name = "pthread")]
		extern "C" {
			fn pthread_getthreadid_np() -> libc::c_int;
		}
		(unsafe { pthread_getthreadid_np() }).try_into().unwrap()
	}
	#[cfg(target_os = "netbsd")]
	{
		extern "C" {
			fn _lwp_self() -> libc::c_uint;
		}
		(unsafe { _lwp_self() }).into()
	}
	#[cfg(windows)]
	{
		(unsafe { winapi::um::processthreadsapi::GetCurrentThreadId() }).into()
	}
}

/// Count the number of threads of the current process. Uses [`/proc/self/stat`](http://man7.org/linux/man-pages/man5/proc.5.html):`num_threads` on Linux, [`task_threads`](http://web.mit.edu/darwin/src/modules/xnu/osfmk/man/task_threads.html) on macOS.
pub fn count() -> usize {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		procinfo::pid::stat_self()
			.unwrap()
			.num_threads
			.try_into()
			.unwrap()
	}
	#[cfg(any(target_os = "macos", target_os = "ios"))]
	{
		use mach::{
			kern_return::{kern_return_t, KERN_SUCCESS}, mach_types::thread_act_array_t, message::mach_msg_type_number_t, task::task_threads, traps::mach_task_self, vm_types::{vm_address_t, vm_map_t, vm_size_t}
		};
		use std::{mem, ptr};
		extern "C" {
			pub fn vm_deallocate(
				target_task: vm_map_t, address: vm_address_t, size: vm_size_t,
			) -> kern_return_t;
		}

		let this_task = unsafe { mach_task_self() };

		let mut thread_list: thread_act_array_t = ptr::null_mut();
		let mut thread_count: mach_msg_type_number_t = 0;
		let kret = unsafe { task_threads(this_task, &mut thread_list, &mut thread_count) };
		assert_eq!(kret, KERN_SUCCESS);
		let thread_count: usize = thread_count.try_into().unwrap();

		for i in 0..thread_count {
			let kret = unsafe {
				mach::mach_port::mach_port_deallocate(
					this_task,
					*thread_list.offset(i.try_into().unwrap()),
				)
			};
			assert_eq!(kret, KERN_SUCCESS);
		}
		let kret = unsafe {
			vm_deallocate(
				this_task,
				thread_list as usize,
				mem::size_of_val(&*thread_list) * thread_count,
			)
		};
		assert_eq!(kret, KERN_SUCCESS);
		thread_count
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "macos",
		target_os = "ios"
	)))]
	unimplemented!()
}
