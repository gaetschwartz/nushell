use nu_protocol::{Span, StreamDataType};

use crate::OsPipe;

use super::{Handle, PipeError};

macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        name.strip_suffix("::f")
            .unwrap()
            .split("::")
            .last()
            .unwrap()
    }};
}

macro_rules! trace_pipe {
    // use eprintln to print "exec_name | function_name:line_number: a log event"
    ($($arg:tt)+) => (
        if option_env!("NU_TRACE_PIPE").is_some() {
            eprintln!("{} | {}: {}", std::path::PathBuf::from(std::env::args().next().unwrap_or_default())
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(), function!(), format!($($arg)+))
        }
    );
}

pub fn create_pipe(span: Span) -> Result<OsPipe, PipeError> {
    let mut fds: [libc::c_int; 2] = [0; 2];
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if result < 0 {
        return Err(PipeError::UnexpectedInvalidPipeHandle);
    }

    Ok(OsPipe {
        span,
        read_handle: Handle::Read(fds[0]),
        write_handle: Handle::Write(fds[1]),
        datatype: StreamDataType::Binary,
        handle_policy: super::HandlePolicy::Exclusive,
        encoding: super::StreamEncoding::Raw,
    })
}

pub fn close_handle(handle: Handle) -> Result<(), PipeError> {
    trace_pipe!("{:?}", handle);
    let res = unsafe { libc::close(handle.native()) };

    if res < 0 {
        return Err(PipeError::FailedToCloseHandle(
            handle,
            std::io::Error::last_os_error().into(),
        ));
    }

    Ok(())
}

pub fn read_handle(handle: Handle, buf: &mut [u8]) -> std::io::Result<usize> {
    trace_pipe!("{:?}", handle);
    let result = unsafe { libc::read(handle.native(), buf.as_mut_ptr() as *mut _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }
    trace_pipe!("read {} bytes", result);

    Ok(result as usize)
}

pub fn write_handle(handle: Handle, buf: &[u8]) -> std::io::Result<usize> {
    trace_pipe!("{:?}", handle);

    let result = unsafe { libc::write(handle.native(), buf.as_ptr() as *const _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }

    trace_pipe!("wrote {} bytes", result);

    Ok(result as usize)
}
