//! Utility functions for the crate

/// Returns the name of the current function
#[macro_export(local_inner_macros)]
macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        type_name_of(f)
            .strip_suffix("::f")
            .unwrap()
            .split("::")
            .last()
            .unwrap()
    }};
}

/// Returns true if the NU_TRACE_PIPES env var is set to 1 or true
pub fn trace_pipes_enabled() -> bool {
    match std::option_env!("NU_TRACE_PIPES") {
        Some(s) => konst::const_eq!(s, "1") || konst::const_eq!(s, "true"),
        _ => false,
    }
}

/// Returns the name of the current executable
#[macro_export(local_inner_macros)]
macro_rules! exec_name {
    () => {{
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or("???".to_string())
    }};
}

/// Prints a log event to stderr if NU_TRACE_PIPES is set to 1 or true
#[macro_export]
macro_rules! trace_pipe {
    // use eprintln to print "exec_name | function_name:line_number: a log event"
    ($($arg:tt)+) => (
        if $crate::utils::trace_pipes_enabled() {
            use std::io::Write;

            const RESET_ANSI: &str = "\x1b[0m";
            const DIMMED_ANSI: &str = "\x1b[2m";
            const BLUE_ANSI: &str = "\x1b[38;5;39m";
            const LIGHT_BLUE_ANSI: &str = "\x1b[38;5;153m";
            const ORANGE_ANSI: &str = "\x1b[38;5;208m";

            let mut lock = std::io::stderr().lock();
            let s = format!("[{BLUE_ANSI}{}{RESET_ANSI}@{LIGHT_BLUE_ANSI}{}{RESET_ANSI}] {DIMMED_ANSI}({}:{}){RESET_ANSI} {ORANGE_ANSI}{}{RESET_ANSI}: {}",$crate::exec_name!(),std::thread::current().name().unwrap_or("<unnamed>"), file!(), line!(), $crate::function!(), format_args!($($arg)+), RESET_ANSI = RESET_ANSI, DIMMED_ANSI = DIMMED_ANSI);
            lock.write_all(s.as_bytes()).unwrap();
            lock.write_all(b"\n").unwrap();
        }
    );
}

pub(crate) trait NamedScopedThreadSpawn<'scope, 'env: 'scope> {
    fn spawn_named<'a, T: 'static + Send, F: 'static + Send + FnOnce() -> T>(
        &'scope self,
        name: &'a str,
        f: F,
    ) -> Result<std::thread::ScopedJoinHandle<'scope, T>, std::io::Error>;
}

impl<'scope, 'env: 'scope> NamedScopedThreadSpawn<'scope, 'env>
    for std::thread::Scope<'scope, 'env>
{
    fn spawn_named<'a, T: 'static + Send, F: 'static + Send + FnOnce() -> T>(
        &'scope self,
        name: &'a str,
        f: F,
    ) -> Result<std::thread::ScopedJoinHandle<'scope, T>, std::io::Error> {
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn_scoped(self, f)
    }
}

#[allow(dead_code)]
pub(crate) const LIBC_CALL_ERROR: &str = "Failed to call ";

/// Generates the error message for a failed libc call
#[macro_export(local_inner_macros)]
macro_rules! libc_call_error {
    ($call:expr) => {
        konst::string::str_concat!(&[
            $crate::utils::LIBC_CALL_ERROR,
            "`",
            std::stringify!($call),
            "`"
        ])
    };
}

/// Calls a libc function and returns an error if the return value is negative
#[macro_export]
macro_rules! libc_call {
    ($call:expr) => {{
        let res = unsafe { $call };
        if res < 0 {
            Err($crate::errors::PipeError::last_os_error(
                $crate::libc_call_error!($call),
            ))
        } else {
            Ok(res)
        }
    }};
    // takes a placeholder var that will be sent as a mutable pointer to the call
    // returns the value of the placeholder var
    (
        $call:expr,
        $var:ident => $var_value:expr
    ) => {{
        let mut $var = $var_value;
        let res = unsafe { $call };
        if res < 0 {
            Err($crate::errors::PipeError::last_os_error(
                $crate::libc_call_error!($call),
            ))
        } else {
            Ok($var)
        }
    }};
}

#[cfg(test)]
#[allow(unused_unsafe, unused_mut)]
mod tests {
    #[test]
    fn libc_call_returns_value() {
        let res = libc_call!(42);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 42);
    }

    fn write_to(data: &mut isize, value: isize) -> isize {
        *data = value * 2;
        value - 1
    }

    #[test]
    fn libc_call_returns_value_with_placeholder() {
        let res = libc_call!(write_to(&mut data, 42), data => 0);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 84);
    }

    #[test]
    fn libc_call_returns_error() {
        let res = libc_call!(-1);
        assert!(res.is_err());
    }
    #[test]
    fn libc_call_returns_error_containing_function_name() {
        fn my_erroring_func() -> isize {
            -1
        }
        let res = libc_call!(my_erroring_func());
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .to_string()
            .contains(stringify!(my_erroring_func)));

        let res = libc_call!(my_erroring_func(), data => 0);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .to_string()
            .contains(stringify!(my_erroring_func)));

        // fallback to the full call if the function name can't be extracted
        let res = libc_call!(5 - 7);
        assert!(res.is_err(), "Expected error, got {:?}", res);
        assert!(res.unwrap_err().to_string().contains("5 - 7"));

        let res = libc_call!(  5 - 7, data => 0);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("5 - 7"));
    }

    #[test]
    fn libc_call_returns_error_with_placeholder() {
        let res = libc_call!(write_to(&mut data, 0), data => 0);
        assert!(res.is_err());
        println!("{:?}", res);
    }
}
