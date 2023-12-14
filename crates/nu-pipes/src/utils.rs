#[macro_export]
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

#[allow(dead_code)]
pub(crate) const fn trace_pipes_enabled() -> bool {
    match option_env!("NU_TRACE_PIPES") {
        Some(s) => konst::const_eq!(s, "1") || konst::const_eq!(s, "true"),
        _ => false,
    }
}

#[allow(dead_code)]
pub(crate) fn exec_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or("???".to_string())
}

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
            let s = format!("[{BLUE_ANSI}{}{RESET_ANSI}@{LIGHT_BLUE_ANSI}{}{RESET_ANSI}] {DIMMED_ANSI}({}:{}){RESET_ANSI} {ORANGE_ANSI}{}{RESET_ANSI}: {}",$crate::utils::exec_name(),std::thread::current().name().unwrap_or("<unnamed>"), file!(), line!(), $crate::function!(), format_args!($($arg)+), RESET_ANSI = RESET_ANSI, DIMMED_ANSI = DIMMED_ANSI);
            lock.write_all(s.as_bytes()).unwrap();
            lock.write_all(b"\n").unwrap();
        }
    );
}

pub fn named_thread<T: 'static + Send, F: 'static + Send + FnOnce() -> T, S: Into<String>>(
    name: S,
    f: F,
) -> Result<std::thread::JoinHandle<T>, std::io::Error> {
    std::thread::Builder::new().name(name.into()).spawn(f)
}

pub fn catch_result<T, E: std::error::Error, F: FnOnce() -> Result<T, E>>(f: F) -> Result<T, E> {
    f()
}

#[macro_export]
macro_rules! libc_call {
    ($call:expr) => {{
        let res = unsafe { $call };
        if res < 0 {
            return Err($crate::errors::PipeError::last_os_error(format!(
                "Failed to call {}",
                stringify!($call)
            )));
        }
        res
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
            return Err($crate::errors::PipeError::last_os_error(format!(
                "Failed to call {}",
                stringify!($call)
            )));
        }
        $var
    }};
}

#[macro_export]
macro_rules! libc_call_res {
    // take an optional string to add to the error
    ($call:expr, $ctx:expr) => {{
        let res = unsafe { $call };
        if res < 0 {
            Err($crate::errors::PipeError::last_os_error(format!(
                "Failed to call {} while {}",
                stringify!($call)
            )))
        } else {
            Ok(res)
        }
    }};
    ($call:expr) => {{
        let res = unsafe { $call };
        if res < 0 {
            Err($crate::errors::PipeError::last_os_error(format!(
                "Failed to call {}",
                stringify!($call)
            )))
        } else {
            Ok(res)
        }
    }};
}
