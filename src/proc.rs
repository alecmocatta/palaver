//! `/proc/self` functionality
//!
//! Including getting the current executable, open file descriptors, and paths for open file descriptors that can be passed to e.g. `exec` (for those systems without `fexecve`).
//!
//! # Examples
//!
//! ```
//! extern crate nix;
//! extern crate palaver;
//!
//! use std::io::Read;
//! use palaver::proc::*;
//!
//! # fn main() {
//! // Close all file descriptors except std{in,out,err}.
//! for fd in FdIter::new().unwrap() {
//! 	if fd > 2 {
//! 		nix::unistd::close(fd).unwrap();
//! 	}
//! }
//!
//! let mut current_binary = vec![];
//! exe().unwrap().read_to_end(&mut current_binary).unwrap();
//! println!("Current binary is {} bytes long!", current_binary.len());
//! # }
//! ```

use super::*;
#[cfg(target_family = "unix")]
use nix::libc;
use std::ffi::OsString;
#[cfg(target_family = "unix")]
use std::{
	ffi::{CStr, CString}, os::unix::ffi::OsStringExt
};

#[allow(unused_imports)]
use std::{env, ffi, fmt, fs, io, path};

/// Returns a [File](std::fs::File) of the currently running executable. Akin to `fd::File::open("/proc/self/exe")` on Linux.
pub fn exe() -> io::Result<fs::File> {
	exe_path().and_then(fs::File::open)
}

/// Returns the path of the currently running executable. On Linux this is `/proc/self/exe`.
// https://stackoverflow.com/questions/1023306/finding-current-executables-path-without-proc-self-exe
pub fn exe_path() -> io::Result<path::PathBuf> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		Ok(path::PathBuf::from("/proc/self/exe"))
	}
	#[cfg(any(target_os = "dragonfly"))]
	{
		Ok(path::PathBuf::from("/proc/curproc/file"))
	}
	#[cfg(any(target_os = "netbsd"))]
	{
		Ok(path::PathBuf::from("/proc/curproc/exe"))
	}
	#[cfg(any(target_os = "solaris"))]
	{
		Ok(path::PathBuf::from(format!(
			"/proc/{}/path/a.out",
			nix::unistd::getpid()
		))) // or /proc/{}/object/a.out ?
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "dragonfly",
		target_os = "linux",
		target_os = "netbsd",
		target_os = "solaris"
	)))]
	{
		env::current_exe()
	}
}

/// Returns the path of the directory that contains entries for each open file descriptor. On Linux this is `/proc/self/fd`. Doesn't work on Windows.
pub fn fd_dir() -> io::Result<path::PathBuf> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		Ok(path::PathBuf::from("/proc/self/fd"))
	}
	#[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios"))]
	{
		Ok(path::PathBuf::from("/dev/fd"))
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "freebsd",
		target_os = "macos",
		target_os = "ios"
	)))]
	{
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no known /proc/self/fd equivalent for OS",
		))
	}
}
/// Returns the path of the entry for a particular open file descriptor. On Linux this is `/proc/self/fd/{fd}`. Doesn't work on Windows.
pub fn fd_path(fd: Fd) -> io::Result<path::PathBuf> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		Ok(path::PathBuf::from(format!("/proc/self/fd/{}", fd)))
	}
	#[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios"))]
	{
		Ok(path::PathBuf::from(format!("/dev/fd/{}", fd)))
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "freebsd",
		target_os = "macos",
		target_os = "ios"
	)))]
	{
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no known /proc/self/fd equivalent for OS",
		))
	}
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Iterator for all open file descriptors. Doesn't work on Windows.
pub struct FdIter(#[cfg(target_family = "unix")] *mut libc::DIR);
impl FdIter {
	/// Create a new FdIter. Returns Err on OSs that don't support this.
	pub fn new() -> Result<Self, io::Error> {
		let dir = fd_dir()?;
		#[cfg(target_family = "unix")]
		{
			let dir =
				CString::new(<path::PathBuf as Into<OsString>>::into(dir).into_vec()).unwrap();
			let dirp: *mut libc::DIR = unsafe { libc::opendir(dir.as_ptr()) };
			assert!(!dirp.is_null());
			Ok(Self(dirp))
		}
		#[cfg(target_family = "windows")]
		{
			Err(io::Error::new(
				io::ErrorKind::NotFound,
				"can't iterate dir?",
			))
		}
	}
}
impl Iterator for FdIter {
	// https://stackoverflow.com/questions/899038/getting-the-highest-allocated-file-descriptor/918469#918469
	type Item = Fd;

	fn next(&mut self) -> Option<Self::Item> {
		#[cfg(target_family = "unix")]
		{
			let mut dent;
			while {
				dent = unsafe { libc::readdir(self.0) };
				!dent.is_null()
			} {
				// https://github.com/rust-lang/rust/issues/34668
				let name = unsafe { CStr::from_ptr((*dent).d_name.as_ptr()) };
				if name == CStr::from_bytes_with_nul(b".\0").unwrap()
					|| name == CStr::from_bytes_with_nul(b"..\0").unwrap()
				{
					continue;
				}
				let fd = name
					.to_str()
					.map_err(|_| ())
					.and_then(|fd| fd.parse::<Fd>().map_err(|_| ()));
				if fd.is_err() || fd.unwrap() == unsafe { libc::dirfd(self.0) } {
					continue;
				}
				return Some(fd.unwrap());
			}
			None
		}
		#[cfg(target_family = "windows")]
		{
			unreachable!()
		}
	}
}
impl fmt::Debug for FdIter {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("FdIter").finish()
	}
}
impl Drop for FdIter {
	fn drop(&mut self) {
		#[cfg(target_family = "unix")]
		{
			let ret = unsafe { libc::closedir(self.0) };
			assert_eq!(ret, 0);
		}
	}
}
