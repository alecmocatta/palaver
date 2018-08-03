# palaver

[![Crates.io](https://img.shields.io/crates/v/palaver.svg?style=flat-square&maxAge=86400)](https://crates.io/crates/palaver)
[![Apache-2.0 licensed](https://img.shields.io/crates/l/palaver.svg?style=flat-square&maxAge=2592000)](LICENSE.txt)
[![Build Status](https://circleci.com/gh/alecmocatta/palaver/tree/master.svg?style=shield)](https://circleci.com/gh/alecmocatta/palaver)
[![Build Status](https://travis-ci.com/alecmocatta/palaver.svg?branch=master)](https://travis-ci.com/alecmocatta/palaver)

[Docs](https://docs.rs/crate/palaver/0.1.0)

Cross-platform polyfills.

This library attempts to provide reliable pollyfills for functionality that isn't implemented on all platforms, for example `gettid`, `memfd_create`, `fexecve`, as well as providing non-atomic versions of functions like `accept4`, `socket`+`SOCK_CLOEXEC`, `pipe2`, and other miscellanea like `seal` to make a file descriptor read-only thus suitable for `fexecve`.

palaver = "Platform Abstraction Layer" / pa·lav·er *n.* – prolonged and tedious fuss.

It's currently used on unix-family systems; most Windows functionality is TODO.

## License
Licensed under Apache License, Version 2.0, ([LICENSE.txt](LICENSE.txt) or http://www.apache.org/licenses/LICENSE-2.0).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.
