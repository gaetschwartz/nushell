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
pub(crate) fn trace_pipes_enabled() -> bool {
    matches!(option_env!("NU_TRACE_PIPES"), Some("1") | Some("true"))
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
            const PURPLE_ANSI: &str = "\x1b[38;5;129m";
            const RESET_ANSI: &str = "\x1b[0m";
            const DIMMED_ANSI: &str = "\x1b[2m";
            const BLUE_ANSI: &str = "\x1b[38;5;39m";
            const LIGHT_BLUE_ANSI: &str = "\x1b[38;5;153m";
            const ORANGE_ANSI: &str = "\x1b[38;5;208m";
            eprintln!("{PURPLE_ANSI}[TRACE]{RESET_ANSI} [{BLUE_ANSI}{}{RESET_ANSI}@{LIGHT_BLUE_ANSI}{}{RESET_ANSI}] {DIMMED_ANSI}({}:{}){RESET_ANSI} {ORANGE_ANSI}{}{RESET_ANSI}: {}",$crate::utils::exec_name(),std::thread::current().name().unwrap_or("<unnamed>"), file!(), line!(), $crate::function!(), format_args!($($arg)+), PURPLE_ANSI = PURPLE_ANSI, RESET_ANSI = RESET_ANSI, DIMMED_ANSI = DIMMED_ANSI);
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
