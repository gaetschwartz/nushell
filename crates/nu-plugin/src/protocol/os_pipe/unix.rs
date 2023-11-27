use log::trace;
use nu_protocol::{Span, StreamDataType};
use serde::{Deserialize, Serialize};

use super::{Handle, PipeError};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    pub(crate) read_fd: libc::c_int,
    pub(crate) write_fd: libc::c_int,
}

impl OsPipe {
    pub fn create(span: Span) -> Result<Self, PipeError> {
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
            read_fd: fds[0],
            write_fd: fds[1],
            datatype: StreamDataType::Binary,
        })
    }

    pub fn close(&self, handle: Handle) -> Result<(), PipeError> {
        use libc::close;

        let fd = match handle {
            Handle::Read => self.read_fd,
            Handle::Write => self.write_fd,
        };

        let res = unsafe { close(fd) };

        if res < 0 {
            return Err(PipeError::FailedToClose(
                handle,
                std::io::Error::last_os_error().into(),
            ));
        }

        Ok(())
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // close write end of pipe
        // let _ = unsafe { libc::close(self.write_fd) };
        // if result < 0 {
        //     return Err(std::io::Error::last_os_error());
        // }
        trace!("OsPipe::reading for {:?}", self.read_fd);

        let result = unsafe { libc::read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }

        trace!("OsPipe::read {} bytes", result);

        Ok(result as usize)
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use libc::write;

        // https://stackoverflow.com/a/24099738
        // fifo is blocking
        // let _ = unsafe { libc::close(self.read_fd) };
        // if result < 0 {
        //     return Err(std::io::Error::last_os_error());
        // }

        let result = unsafe { write(self.write_fd, buf.as_ptr() as *const _, buf.len()) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(result as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
