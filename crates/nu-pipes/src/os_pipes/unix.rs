use crate::{
    errors::PipeResult,
    trace_pipe,
    unidirectional::{PipeFdType, PipeMode, PipeRead, PipeWrite},
    AsNativeFd, AsPipeFd,
};

use super::{IntoPipeFd, OsPipe, PipeError, PipeImplBase};

pub type OSError = std::io::Error;
pub type NativeFd = libc::c_int;

pub(crate) struct PipeImpl {}

impl PipeImplBase for PipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
        let mut fds = [0i32; 2];
        let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if result < 0 {
            return Err(PipeError::os_error("failed to create pipe"));
        }

        Ok(OsPipe {
            read_fd: fds[0].into_pipe_fd(),
            write_fd: fds[1].into_pipe_fd(),
        })
    }

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> Result<(), PipeError> {
        trace_pipe!("!!! closing {:?}", fd.as_pipe_fd());
        let res = unsafe { libc::close(fd.as_pipe_fd().as_native_fd()) };

        if res < 0 {
            return Err(PipeError::os_error(format!(
                "failed to close handle {:?}",
                fd.as_pipe_fd()
            )));
        }

        Ok(())
    }

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("reading {:?} bytes from {:?}", buf.len(), fd.as_pipe_fd());
        let result = unsafe {
            libc::read(
                fd.as_pipe_fd().as_native_fd(),
                buf.as_mut_ptr() as *mut _,
                buf.len(),
            )
        };
        if result < 0 {
            return Err(PipeError::os_error(format!(
                "failed to read from handle {:?}",
                fd.as_pipe_fd()
            )));
        }
        trace_pipe!("read {} bytes", result);

        Ok(result as usize)
    }

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("writing {:?} bytes to {:?}", buf.len(), fd.as_pipe_fd());

        let result = unsafe {
            libc::write(
                fd.as_pipe_fd().as_native_fd(),
                buf.as_ptr() as *const _,
                buf.len(),
            )
        };
        if result < 0 {
            return Err(PipeError::os_error(format!(
                "failed to write to handle {:?}",
                fd.as_pipe_fd()
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

    const INVALID_FD_VALUE: NativeFd = -1;
}
