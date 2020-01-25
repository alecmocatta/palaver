//! Inspect the process's environment.
//!
//! This library goes further than the stdlib to get arguments and environment
//! variables, including reading from `/proc/self/cmdline` and similar.
//!
//! This is helpful for library crates that don't want to require them to be
//! passed down to the library by the user; particularly if called from a
//! non-Rust application where the Rust stdlib hasn't had a chance to capture
//! them from the `int main (int argc, char *argv[])` invocation thus breaking
//! `std::env::args()`.
//!
//! # Examples
//!
//! ```
//! use std::io::Read;
//! use palaver::env::exe;
//!
//! let mut current_binary = vec![];
//! exe().unwrap().read_to_end(&mut current_binary).unwrap();
//! println!("Current binary is {} bytes long!", current_binary.len());
//! ```
//!
//! ```
//! use palaver::env;
//!
//! pub fn my_library_func() {
//!     let args = env::args();
//!     let vars = env::vars();
//! }
//! ```

#![allow(
	clippy::type_complexity,
	clippy::option_option,
)]

#[cfg(unix)]
use libc::c_char;
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::io::Read;
#[cfg(unix)]
use std::{ffi::CStr, os::unix::ffi::OsStringExt};
use std::{ffi::OsString, fs, io, path, sync};

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
		std::env::current_exe()
	}
}

lazy_static::lazy_static! {
	static ref ARGV: sync::RwLock<Option<Option<Vec<OsString>>>> = sync::RwLock::new(None);
	static ref ENVP: sync::RwLock<Option<Option<Vec<(OsString, OsString)>>>> =
		sync::RwLock::new(None);
}

/// Returns the arguments which this program was started with (normally passed
/// via the command line).
///
/// The first element is traditionally the path of the executable, but it can be
/// set to arbitrary text, and it may not even exist, so this property should
/// not be relied upon for security purposes.
///
/// # Errors
///
/// This will return `None` if `get_env` can't acquire them. Please file an issue.
///
/// # Panics
///
/// Will panic if any argument to the process is not valid unicode. If this is
/// not desired, consider using the [`args_os`] function.
///
/// [`args_os`]: fn.args_os.html
///
/// # Examples
///
/// ```
/// use palaver::env;
///
/// // Prints each argument on a separate line
/// for argument in env::args().expect("Couldn't retrieve arguments") {
///     println!("{}", argument);
/// }
/// ```
pub fn args() -> Option<Vec<String>> {
	args_os().map(|x| x.into_iter().map(|a| a.into_string().unwrap()).collect())
}

/// Returns a vector of (variable, value) pairs of strings, for all the
/// environment variables of the current process.
///
/// # Errors
///
/// This will return `None` if `get_env` can't acquire them. Please file an issue.
///
/// # Panics
///
/// Will panic if any key or value in the environment is not valid unicode. If
/// this is not desired, consider using the [`vars_os`] function.
///
/// [`vars_os`]: fn.vars_os.html
///
/// # Examples
///
/// ```
/// use palaver::env;
///
/// // Prints the environment variables
/// for (key, value) in env::vars().expect("Couldn't retrieve env vars") {
///     println!("{}: {}", key, value);
/// }
/// ```
pub fn vars() -> Option<Vec<(String, String)>> {
	vars_os().map(|x| {
		x.into_iter()
			.map(|(a, b)| (a.into_string().unwrap(), b.into_string().unwrap()))
			.collect()
	})
}

/// Returns the arguments which this program was started with (normally passed
/// via the command line).
///
/// The first element is traditionally the path of the executable, but it can be
/// set to arbitrary text, and it may not even exist, so this property should
/// not be relied upon for security purposes.
///
/// # Errors
///
/// This will return `None` if `get_env` can't acquire them. Please file an issue.
///
/// # Examples
///
/// ```
/// use palaver::env;
///
/// // Prints each argument on a separate line
/// for argument in env::args_os().expect("Couldn't retrieve arguments") {
///     println!("{:?}", argument);
/// }
/// ```
pub fn args_os() -> Option<Vec<OsString>> {
	if ARGV.read().unwrap().is_none() {
		let mut write = ARGV.write().unwrap();
		if write.is_none() {
			*write = Some(argv_from_global().ok().or_else(|| argv_from_proc().ok()));
		}
	}
	ARGV.read().unwrap().as_ref().unwrap().clone()
}

/// Returns a vector of (variable, value) pairs of OS strings, for all the
/// environment variables of the current process.
///
/// # Errors
///
/// This will return `None` if `get_env` can't acquire them. Please file an issue.
///
/// # Examples
///
/// ```
/// use palaver::env;
///
/// // Prints the environment variables
/// for (key, value) in env::vars_os().expect("Couldn't retrieve env vars") {
///     println!("{:?}: {:?}", key, value);
/// }
/// ```
pub fn vars_os() -> Option<Vec<(OsString, OsString)>> {
	if ENVP.read().unwrap().is_none() {
		let mut write = ENVP.write().unwrap();
		if write.is_none() {
			*write = Some(envp_from_global().ok().or_else(|| envp_from_proc().ok()));
		}
	}
	ENVP.read().unwrap().as_ref().unwrap().clone()
}

fn argv_from_global() -> Result<Vec<OsString>, ()> {
	#[cfg(any(windows, target_os = "macos", target_os = "ios"))]
	{
		// std uses windows GetCommandLineW and mac _NSGetArgv
		Ok(std::env::args_os().collect())
	}
	#[cfg(not(any(windows, target_os = "macos", target_os = "ios")))]
	{
		Err(())
	}
}

fn argv_from_proc() -> Result<Vec<OsString>, io::Error> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		let mut cmdline = Vec::new();
		let _ = fs::File::open("/proc/self/cmdline")?
			.read_to_end(&mut cmdline)
			.unwrap(); // limited to 4096 bytes?
		if let Some(b'\0') = cmdline.last() {
			let null = cmdline.pop().unwrap();
			assert_eq!(null, b'\0');
		}
		Ok(cmdline
			.split(|&x| x == b'\0')
			.map(|x| OsStringExt::from_vec(x.to_vec()))
			.collect::<Vec<_>>())
	}
	#[cfg(not(any(target_os = "android", target_os = "linux")))]
	{
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no /proc/self/cmdline equivalent",
		))
	}
}

fn envp_from_global() -> Result<Vec<(OsString, OsString)>, ()> {
	#[cfg(unix)]
	{
		unsafe fn environ() -> *mut *const *const c_char {
			#[cfg(target_os = "macos")]
			{
				extern "C" {
					fn _NSGetEnviron() -> *mut *const *const c_char;
				}
				_NSGetEnviron()
			}
			#[cfg(not(target_os = "macos"))]
			{
				extern "C" {
					// #[cfg_attr(target_os = "linux", linkage = "extern_weak")]
					static mut environ: *const *const c_char;
				}
				&mut environ
			}
		}
		unsafe {
			let mut environ = *environ();
			if environ.is_null() {
				return Err(());
			}
			let mut result = Vec::new();
			while !(*environ).is_null() {
				if let Some(key_value) = parse_env(CStr::from_ptr(*environ).to_bytes()) {
					result.push(key_value);
				}
				environ = environ.offset(1);
			}
			Ok(result)
		}
	}
	#[cfg(windows)]
	{
		// std uses GetEnvironmentStringsW
		Ok(std::env::vars_os().collect())
	}
}
fn envp_from_proc() -> Result<Vec<(OsString, OsString)>, io::Error> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		let mut envp = Vec::new();
		let _ = fs::File::open("/proc/self/environ")?
			.read_to_end(&mut envp)
			.unwrap(); // limited to 4096 bytes?
		if let Some(b'\0') = envp.last() {
			let null = envp.pop().unwrap();
			assert_eq!(null, b'\0');
		}
		Ok(envp
			.split(|&x| x == b'\0')
			.flat_map(|x| parse_env(x))
			.collect::<Vec<_>>())
	}
	#[cfg(not(any(target_os = "android", target_os = "linux")))]
	{
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no /proc/self/environ equivalent",
		))
	}
}

#[cfg(unix)]
fn parse_env(input: &[u8]) -> Option<(OsString, OsString)> {
	// https://github.com/rust-lang/rust/blob/a1e6bcb2085cba3d5e549ba3b175f99487c21639/src/libstd/sys/unix/os.rs#L431
	if input.is_empty() {
		return None;
	}
	let pos = input[1..].iter().position(|&x| x == b'=').map(|p| p + 1);
	pos.map(|p| {
		(
			OsStringExt::from_vec(input[..p].to_vec()),
			OsStringExt::from_vec(input[p + 1..].to_vec()),
		)
	})
}

#[doc(hidden)]
// https://github.com/golang/go/issues/13492
#[cfg(any(all(target_os = "linux", target_env = "gnu"), target_os = "macos"))]
#[cfg_attr(
	all(target_os = "linux", target_env = "gnu"),
	link_section = ".init_array"
)]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
// #[cfg_attr(target_os = "windows", link_section = ".CRT$XCU")] XIU
#[used]
pub static GRAB_ARGV_ENVP: extern "C" fn(
	argc: libc::c_int,
	argv: *const *const c_char,
	envp: *const *const c_char,
) = {
	// Or should it be an array? https://github.com/rust-lang/rust/pull/39987#issue-107077124 https://doc.rust-lang.org/unstable-book/language-features/used.html
	#[cfg_attr(target_os = "linux", link_section = ".text.startup")]
	extern "C" fn grab_argv_envp(
		_argc: libc::c_int, argv: *const *const c_char, envp: *const *const c_char,
	) {
		unsafe {
			let mut collect_args: Vec<OsString> = Vec::new();
			let mut argv = argv;
			while !(*argv).is_null() {
				collect_args.push(OsStringExt::from_vec(
					CStr::from_ptr(*argv).to_bytes().to_vec(),
				));
				argv = argv.offset(1);
			}
			let mut collect_vars: Vec<(OsString, OsString)> = Vec::new();
			let mut envp = envp;
			while !(*envp).is_null() {
				let x = CStr::from_ptr(*envp).to_bytes();
				if let Some(x) = parse_env(x) {
					collect_vars.push(x);
				}
				envp = envp.offset(1);
			}
			*ARGV.write().unwrap() = Some(Some(collect_args));
			*ENVP.write().unwrap() = Some(Some(collect_vars));
		}
	}
	grab_argv_envp
};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn same() {
		let args = vec![
			ARGV.read().unwrap().clone().unwrap_or(None),
			argv_from_global().ok(),
			argv_from_proc().ok(),
			Some(std::env::args_os().collect::<Vec<_>>()),
		];
		let mut args2 = args.clone().into_iter().flatten().collect::<Vec<_>>();
		args2.dedup();
		assert!(args2.len() == 1, "{:?}", args);

		let args = vec![
			ENVP.read().unwrap().clone().unwrap_or(None),
			envp_from_global().ok(),
			envp_from_proc().ok(),
			Some(std::env::vars_os().collect::<Vec<_>>()),
		];
		let mut args2 = args.clone().into_iter().flatten().collect::<Vec<_>>();
		args2.dedup();
		assert!(args2.len() == 1, "{:?}", args);
	}
}
