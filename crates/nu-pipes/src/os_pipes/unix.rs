use crate::unidirectional::PipeMode;

use super::{Handle, OsPipe, PipeError};

pub type InnerHandleType = libc::c_int;

pub type OSError = std::io::Error;

pub(crate) fn create_pipe() -> Result<OsPipe, PipeError> {
    let mut fds: [libc::c_int; 2] = [0; 2];
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if result < 0 {
        return Err(PipeError::UnexpectedInvalidPipeHandle);
    }

    Ok(OsPipe {
        read_handle: Handle::Read(fds[0]),
        write_handle: Handle::Write(fds[1]),
    })
}

pub fn close_handle(handle: &Handle) -> Result<(), PipeError> {
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

pub fn read_handle(handle: &Handle, buf: &mut [u8]) -> std::io::Result<usize> {
    trace_pipe!("{:?}", handle);
    let result = unsafe { libc::read(handle.native(), buf.as_mut_ptr() as *mut _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }
    trace_pipe!("read {} bytes", result);

    Ok(result as usize)
}

pub fn write_handle(handle: &Handle, buf: &[u8]) -> std::io::Result<usize> {
    trace_pipe!("{:?}", handle);

    let result = unsafe { libc::write(handle.native(), buf.as_ptr() as *const _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }

    trace_pipe!("wrote {} bytes", result);

    Ok(result as usize)
}

pub(crate) fn should_close_other_for_mode(mode: PipeMode) -> bool {
    match mode {
        PipeMode::CrossProcess => true,
        PipeMode::InProcess => false,
    }
}
