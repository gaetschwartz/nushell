use nu_protocol::{PipelineData, RawStream};

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
    matches!(option_env!("NU_TRACE_PIPES"), Some("1" | "true"))
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
            eprintln!("{} | {}: {}", std::path::PathBuf::from(std::env::args().next().unwrap_or_default())
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(), $crate::function!(), format!($($arg)+))
        }
    );
}

pub fn named_thread<T: 'static + Send, F: 'static + Send + FnOnce() -> T, S: Into<String>>(
    name: S,
    f: F,
) -> Result<std::thread::JoinHandle<T>, std::io::Error> {
    std::thread::Builder::new().name(name.into()).spawn(f)
}

pub trait MaybeRawStream {
    fn take_stream(&mut self) -> Option<RawStream>;
}

impl MaybeRawStream for PipelineData {
    fn take_stream(&mut self) -> Option<RawStream> {
        match self {
            PipelineData::Value { .. } => None,
            PipelineData::ListStream { .. } => None,
            PipelineData::ExternalStream { stdout, .. } => stdout.take(),
            PipelineData::Empty => None,
        }
    }
}

impl MaybeRawStream for Option<PipelineData> {
    fn take_stream(&mut self) -> Option<RawStream> {
        match self {
            Some(PipelineData::ExternalStream { stdout, .. }) => stdout.take(),
            _ => None,
        }
    }
}
