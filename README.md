# palaver

[![Crates.io](https://img.shields.io/crates/v/palaver.svg?maxAge=86400)](https://crates.io/crates/palaver)
[![MIT / Apache 2.0 licensed](https://img.shields.io/crates/l/palaver.svg?maxAge=2592000)](#License)
[![Build Status](https://ci.appveyor.com/api/projects/status/github/alecmocatta/palaver?branch=master&svg=true)](https://ci.appveyor.com/project/alecmocatta/palaver)
[![Build Status](https://circleci.com/gh/alecmocatta/palaver/tree/master.svg?style=shield)](https://circleci.com/gh/alecmocatta/palaver)
[![Build Status](https://travis-ci.com/alecmocatta/palaver.svg?branch=master)](https://travis-ci.com/alecmocatta/palaver)

[Docs](https://docs.rs/palaver/0.2.0)

Cross-platform polyfills.

This library attempts to provide reliable polyfills for functionality that isn't implemented on all platforms.

`palaver` = "Platform Abstraction Layer" + pa·lav·er *n.* prolonged and tedious fuss.

## Functionality

<table><!-- https://github.com/alecmocatta/palaver/new/master to preview changes -->
<tr><th>Threading</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>gettid()</code></td><td>Get thread ID</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>count()</code></td><td>Number of threads in current process</td><td>✓</td><td>✓</td><td> </td><td> </td><td> </td><td>✓</td><td>✓</td></tr>
<tr><th>Files</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>seal_fd()</code></td><td>Make a file descriptor read-only</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>dup_fd()</code></td><td>Duplicate a file descriptor</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>copy_fd()</code></td><td>Copy a file descriptor to a specific offset</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>move_fd()</code></td><td>Move a file descriptor to a specific offset</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>move_fds()</code></td><td>Move file descriptors to specific offsets</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>fd_dir()</code></td><td>Get a path to the file descriptor directory</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>fd_path()</code></td><td>Get a path to a file descriptor</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>FdIter</code></td><td>Iterate all open file descriptors</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>memfd_create()</code></td><td>Create an anonymous file</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>fexecve()</code></td><td>Execute program specified via file descriptor</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>copy()</code></td><td>Copy by looping <code>io::copy</code></td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>copy_sendfile()</code></td><td>Copy using <code>sendfile</code></td><td>✓</td><td>✓</td><td> </td><td>✓</td><td> </td><td>✓</td><td>✓</td></tr>
<tr><td><code>copy_splice()</code></td><td>Copy using <code>splice</code></td><td>✓</td><td> </td><td> </td><td> </td><td> </td><td> </td><td>✓</td></tr>
<tr><td><code>pipe()</code></td><td>Create a pipe</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Socket</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>socket()</code></td><td>Create a socket</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>accept()</code></td><td>Accept a connection on a socket</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>is_connected()</code></td><td>Get whether a pending connection is connected</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>unreceived()</code></td><td>Get number of bytes readable</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>unsent()</code></td><td>Get number of bytes that have yet to be acknowledged</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Env</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>exe()</code></td><td>Opens the current running executable</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>exe_path()</code></td><td>Get a path to the current running executable</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>args()</code></td><td>Get command line arguments</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>vars()</code></td><td>Get environment variables</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Fork</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>fork()</code></td><td>Fork a process, using process descriptors where available</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Valgrind</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><code>is()</code></td><td>Check if running under Valgrind</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><code>start_fd()</code></td><td>Get Valgrind's file descriptor range</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
</table>

## License
Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE.txt](LICENSE-APACHE.txt) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT.txt](LICENSE-MIT.txt) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
