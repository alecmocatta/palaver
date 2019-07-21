#![warn(
	missing_copy_implementations,
	missing_debug_implementations,
	missing_docs,
	trivial_numeric_casts,
	unused_extern_crates,
	unused_import_braces,
	unused_qualifications,
	unused_results,
	clippy::pedantic
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![allow(
	clippy::type_complexity,
	clippy::non_ascii_literal,
	clippy::needless_pass_by_value
)]

use std::{collections::HashMap, env, ffi, path, process};

#[test]
fn echo() {
	let echo = escargot::CargoBuild::new()
		.example("echo")
		.current_release()
		.current_target()
		.no_default_features() // https://github.com/crate-ci/escargot/issues/23
		.run()
		.unwrap();
	let echo = echo.path();
	let echo_no_main = escargot::CargoBuild::new()
		.example("echo_no_main")
		.current_release()
		.current_target()
		.no_default_features() // https://github.com/crate-ci/escargot/issues/23
		.run()
		.unwrap();
	let echo_no_main = echo_no_main.path();
	run_echo(&echo, vec![], vec![]);
	run_echo(
		&echo,
		vec!["abc".into()],
		vec![("GET_ENV_LKJHGFDSA".into(), "get_env_asdfghjkl".into())],
	);
	run_echo(
		&echo,
		vec!["abc".into(), "ZA̡͊͠͝LGΌ".into()],
		vec![
			("GET_ENV_LKJHGFDSA".into(), "get_env_asdfghjkl".into()),
			("GET_ENV_ZA̡͊͠͝LGΌ".into(), "get_env_ZA̡͊͠͝LGΌ".into()),
		],
	);
	run_echo(&echo_no_main, vec![], vec![]);
	run_echo(
		&echo_no_main,
		vec!["abc".into()],
		vec![("GET_ENV_LKJHGFDSA".into(), "get_env_asdfghjkl".into())],
	);
	run_echo(
		&echo_no_main,
		vec!["abc".into(), "ZA̡͊͠͝LGΌ".into()],
		vec![
			("GET_ENV_LKJHGFDSA".into(), "get_env_asdfghjkl".into()),
			("GET_ENV_ZA̡͊͠͝LGΌ".into(), "get_env_ZA̡͊͠͝LGΌ".into()),
		],
	);
}

#[test]
fn same_as_rust() {
	assert!(palaver::env::args_os()
		.unwrap()
		.into_iter()
		.eq(env::args_os()));
	assert_eq!(
		hash_env(palaver::env::vars_os().unwrap()),
		hash_env(env::vars_os())
	);
}

fn run_echo(
	echo: &path::Path, args: Vec<ffi::OsString>, vars: Vec<(ffi::OsString, ffi::OsString)>,
) {
	let output = process::Command::new(echo)
		.args(&args)
		.envs(vars.iter().cloned())
		.output()
		.unwrap()
		.stdout;
	let (arg, env): (
		Option<Vec<ffi::OsString>>,
		Option<Vec<(ffi::OsString, ffi::OsString)>>,
	) = serde_json::from_slice(&output).unwrap();
	let (arg, env) = (arg.unwrap(), env.unwrap());
	assert!(arg.iter().skip(1).eq(&args));
	assert_eq!(
		hash_env(env),
		hash_env(env::vars_os().chain(vars.into_iter()))
	);
}

fn hash_env<I: IntoIterator<Item = (ffi::OsString, ffi::OsString)>>(
	env: I,
) -> HashMap<ffi::OsString, ffi::OsString> {
	let mut vars = HashMap::new();
	for (key, val) in env {
		let x = vars.insert(key, val);
		assert!(x.is_none()); // TODO handle collision
	}
	vars
}
