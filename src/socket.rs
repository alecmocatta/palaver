use super::*;
#[cfg(unix)]
use nix::{libc, poll, sys::socket};

bitflags!{
	/// Akin to nix::sys::socket::SockFlag but avail cross-platform
	pub struct SockFlag: libc::c_int {
		#[allow(missing_docs)]
		const SOCK_NONBLOCK = 0b0000_0001;
		#[allow(missing_docs)]
		const SOCK_CLOEXEC  = 0b0000_0010;
	}
}
/// Falls back to non-atomic if SOCK_NONBLOCK/SOCK_CLOEXEC unavailable
pub fn socket<T: Into<Option<socket::SockProtocol>>>(
	domain: socket::AddressFamily, ty: socket::SockType, flags: SockFlag, protocol: T,
) -> Result<Fd, nix::Error> {
	let mut flags_ = socket::SockFlag::empty();
	flags_ = flags_;
	#[cfg(
		any(
			target_os = "android",
			target_os = "dragonfly",
			target_os = "freebsd",
			target_os = "linux",
			target_os = "netbsd",
			target_os = "openbsd"
		)
	)]
	{
		flags_.set(
			socket::SockFlag::SOCK_NONBLOCK,
			flags.contains(SockFlag::SOCK_NONBLOCK),
		);
		flags_.set(
			socket::SockFlag::SOCK_CLOEXEC,
			flags.contains(SockFlag::SOCK_CLOEXEC),
		);
	}
	socket::socket(domain, ty, flags_, protocol).map(|fd| {
		#[cfg(
			not(
				any(
					target_os = "android",
					target_os = "dragonfly",
					target_os = "freebsd",
					target_os = "linux",
					target_os = "netbsd",
					target_os = "openbsd"
				)
			)
		)]
		{
			use nix::fcntl;
			let mut flags_ = fcntl::OFlag::from_bits(
				fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap(),
			).unwrap();
			flags_.set(
				fcntl::OFlag::O_NONBLOCK,
				flags.contains(SockFlag::SOCK_NONBLOCK),
			);
			let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFL(flags_)).unwrap();
			let mut flags_ = fcntl::FdFlag::from_bits(
				fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap(),
			).unwrap();
			flags_.set(
				fcntl::FdFlag::FD_CLOEXEC,
				flags.contains(SockFlag::SOCK_CLOEXEC),
			);
			let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
		}
		fd
	})
}
/// Like accept4, falls back to non-atomic accept
pub fn accept(sockfd: Fd, flags: SockFlag) -> Result<Fd, nix::Error> {
	#[cfg(
		any(
			target_os = "android",
			target_os = "freebsd",
			target_os = "linux",
			target_os = "openbsd"
		)
	)]
	{
		let mut flags_ = socket::SockFlag::empty();
		flags_.set(
			socket::SockFlag::SOCK_NONBLOCK,
			flags.contains(SockFlag::SOCK_NONBLOCK),
		);
		flags_.set(
			socket::SockFlag::SOCK_CLOEXEC,
			flags.contains(SockFlag::SOCK_CLOEXEC),
		);
		socket::accept4(sockfd, flags_)
	}
	#[cfg(
		not(
			any(
				target_os = "android",
				target_os = "freebsd",
				target_os = "linux",
				target_os = "openbsd"
			)
		)
	)]
	{
		use nix::fcntl;
		socket::accept(sockfd).map(|fd| {
			let fff = fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap();
			let mut flags_ = fcntl::OFlag::from_bits_truncate(fff); //.unwrap_or_else(||panic!("{:?} {:?}", fff, fff & !fcntl::OFlag::from_bits_truncate(fff).bits()));
			flags_.set(
				fcntl::OFlag::O_NONBLOCK,
				flags.contains(SockFlag::SOCK_NONBLOCK),
			);
			let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFL(flags_)).unwrap();
			let mut flags_ = fcntl::FdFlag::from_bits(
				fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap(),
			).unwrap();
			flags_.set(
				fcntl::FdFlag::FD_CLOEXEC,
				flags.contains(SockFlag::SOCK_CLOEXEC),
			);
			let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
			fd
		})
	}
}

/// Intended to check for completion after `connect(2)` has returned `EINPROGRESS`.
///
/// Note: Must be called before any data has been written to this `fd`.
pub fn is_connected(fd: Fd) -> bool {
	let mut events = [poll::PollFd::new(fd, poll::EventFlags::POLLOUT)];
	let n = poll::poll(&mut events, 0).unwrap();
	assert!(n == 0 || n == 1);
	n == 1 && events[0].revents().unwrap() == poll::EventFlags::POLLOUT
}

/// Count of bytes that have yet to be read from a socket
pub fn unreceived(fd: Fd) -> usize {
	let mut available: libc::c_int = 0;
	let err = unsafe { libc::ioctl(fd, libc::FIONREAD, &mut available) };
	assert!(err == 0 && available >= 0);
	available as usize
}
/// Count of bytes that have been written to a socket, but have yet to be acked by the remote end
pub fn unsent(fd: Fd) -> usize {
	let mut unsent: libc::c_int = 0;
	#[cfg(any(target_os = "android", target_os = "linux"))]
	let err = unsafe { libc::ioctl(fd, libc::TIOCOUTQ, &mut unsent) };
	#[cfg(any(target_os = "macos", target_os = "ios"))]
	let err = unsafe {
		libc::getsockopt(
			fd,
			libc::SOL_SOCKET,
			libc::SO_NWRITE,
			&mut unsent as *mut libc::c_int as *mut libc::c_void,
			&mut (std::mem::size_of_val(&unsent) as libc::socklen_t),
		)
	};
	#[cfg(
		not(
			any(
				target_os = "android",
				target_os = "linux",
				target_os = "macos",
				target_os = "ios"
			)
		)
	)]
	compile_error!("x");
	assert!(err == 0 && unsent >= 0, "{} {}", err, unsent);
	unsent as usize
}
