use crate::{errors::PipeResult, trace_pipe, unidirectional::PipeMode};

use super::{OsPipe, PipeError, PipeFd, PipeImplBase};

pub type OSError = std::io::Error;
pub type NativeFd = libc::c_int;

pub(crate) struct PipeImpl {}

impl PipeImplBase for PipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
        let mut fds: [libc::c_int; 2] = [0; 2];
        let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if result < 0 {
            return Err(PipeError::os_error("failed to create pipe"));
        }

        Ok(OsPipe {
            read_fd: PipeFd::Read(fds[0]),
            write_fd: PipeFd::Write(fds[1]),
        })
    }

    fn close_pipe(handle: &PipeFd) -> Result<(), PipeError> {
        trace_pipe!("!!! closing {:?}", handle);
        let res = unsafe { libc::close(handle.native()) };

        if res < 0 {
            return Err(PipeError::os_error(format!(
                "failed to close handle {:?}",
                handle
            )));
        }

        Ok(())
    }

    fn read(handle: &PipeFd, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("{:?}", handle);
        let result = unsafe { libc::read(handle.native(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if result < 0 {
            return Err(PipeError::os_error(format!(
                "failed to read from handle {:?}",
                handle
            )));
        }
        trace_pipe!("read {} bytes", result);

        Ok(result as usize)
    }

    fn write(handle: &PipeFd, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("{:?}", handle);

        let result = unsafe { libc::write(handle.native(), buf.as_ptr() as *const _, buf.len()) };
        if result < 0 {
            return Err(PipeError::os_error(format!(
                "failed to write to handle {:?}",
                handle
            )));
        }

        trace_pipe!("wrote {} bytes", result);

        Ok(result as usize)
    }

    fn should_close_other_for_mode(mode: PipeMode) -> bool {
        match mode {
            PipeMode::InProcess => false,
            PipeMode::CrossProcess => true,
        }
    }
}
