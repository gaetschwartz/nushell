use crate::{errors::PipeResult, trace_pipe, unidirectional::PipeMode};

use super::{Handle, OsPipe, PipeError, PipeImplBase};

pub type InnerHandleType = libc::c_int;

pub type OSError = std::io::Error;

pub(crate) type PipeImpl = UnixPipeImpl;

pub(crate) struct UnixPipeImpl {}

impl PipeImplBase for UnixPipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
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

    fn close_handle(handle: &Handle) -> Result<(), PipeError> {
        eprintln!("!!! closing {:?}", handle);
        let res = unsafe { libc::close(handle.native()) };

        if res < 0 {
            return Err(PipeError::FailedToCloseHandle(
                *handle,
                std::io::Error::last_os_error(),
            ));
        }

        Ok(())
    }

    fn read_handle(handle: &Handle, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("{:?}", handle);
        let result = unsafe { libc::read(handle.native(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if result < 0 {
            return Err(PipeError::FailedToRead(
                *handle,
                std::io::Error::last_os_error(),
            ));
        }
        trace_pipe!("read {} bytes", result);

        Ok(result as usize)
    }

    fn write_handle(handle: &Handle, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("{:?}", handle);

        let result = unsafe { libc::write(handle.native(), buf.as_ptr() as *const _, buf.len()) };
        if result < 0 {
            return Err(PipeError::FailedToWrite(
                *handle,
                std::io::Error::last_os_error(),
            ));
        }

        trace_pipe!("wrote {} bytes", result);

        Ok(result as usize)
    }

    fn should_close_other_for_mode(mode: PipeMode) -> bool {
        match mode {
            PipeMode::CrossProcess => true,
            PipeMode::InProcess => false,
        }
    }
}
