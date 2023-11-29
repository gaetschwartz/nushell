use log::trace;
use nu_protocol::{Span, StreamDataType};

use crate::OsPipe;

use super::{Handle, PipeError};

pub fn create_pipe(span: Span) -> Result<OsPipe, PipeError> {
    let mut fds: [libc::c_int; 2] = [0; 2];
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if result < 0 {
        return Err(PipeError::UnexpectedInvalidPipeHandle);
    }
    // let flags = libc::O_CLOEXEC;
    // let result = unsafe { libc::fcntl(fds[0], libc::F_SETFD, flags) };
    // if result < 0 {
    //     return Err(PipeError::UnexpectedInvalidPipeHandle);
    // }
    Ok(OsPipe {
        span,
        read_handle: Handle::Read(fds[0]),
        write_handle: Handle::Write(fds[1]),
        datatype: StreamDataType::Binary,
    })
}

pub fn close_handle(handle: Handle) -> Result<(), PipeError> {
    use libc::close;

    let res = unsafe { close(handle.into()) };

    if res < 0 {
        return Err(PipeError::FailedToClose(
            handle,
            std::io::Error::last_os_error().into(),
        ));
    }

    Ok(())
}

pub fn read_handle(handle: Handle, buf: &mut [u8]) -> std::io::Result<usize> {
    // close write end of pipe
    // let _ = unsafe { libc::close(self.write_fd) };
    // if result < 0 {
    //     return Err(std::io::Error::last_os_error());
    // }
    trace!("OsPipe::reading for {:?}", handle);

    let result = unsafe { libc::read(handle.into(), buf.as_mut_ptr() as *mut _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }

    trace!("OsPipe::read {} bytes", result);

    Ok(result as usize)
}

pub fn write_handle(handle: Handle, buf: &[u8]) -> std::io::Result<usize> {
    use libc::write;

    // https://stackoverflow.com/a/24099738
    // fifo is blocking
    // let _ = unsafe { libc::close(self.read_fd) };
    // if result < 0 {
    //     return Err(std::io::Error::last_os_error());
    // }

    let result = unsafe { write(handle.into(), buf.as_ptr() as *const _, buf.len()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(result as usize)
}
