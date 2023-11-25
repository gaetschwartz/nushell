

use nu_protocol::{Span, StreamDataType};
use serde::{Deserialize, Serialize};

use super::PipeError;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    read_fd: libc::c_int,
    write_fd: libc::c_int,
}

impl OsPipe {
    pub fn create(span: Span) -> Result<Self, PipeError> {
        use libc::pipe;

        let mut fds: [libc::c_int; 2] = [0; 2];
        let result = unsafe { pipe(fds.as_mut_ptr()) };
        if result == 0 {
            Ok(OsPipe {
                span,
                read_fd: fds[0],
                write_fd: fds[1],
                datatype: StreamDataType::Binary,
            })
        } else {
            Err(PipeError::UnexpectedInvalidPipeHandle)
        }
    }

    pub fn close(&mut self) -> Result<(), PipeError> {
        use libc::close;

        let (read_res, write_res) = unsafe { (close(self.read_fd), close(self.write_fd)) };

        if read_res < 0 || write_res < 0 {
            return Err(PipeError::FailedToClose(None));
        }

        Ok(())
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use libc::read;

        let result = unsafe { read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(result as usize)
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use libc::write;

        // https://stackoverflow.com/a/24099738
        // fifo is blocking

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
