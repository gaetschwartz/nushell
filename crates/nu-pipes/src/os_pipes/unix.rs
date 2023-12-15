use crate::{
    errors::PipeResult,
    libc_call, trace_pipe,
    unidirectional::{PipeFdType, PipeRead, PipeWrite},
    AsNativeFd, AsPipeFd, PipeFd,
};

use super::{IntoPipeFd, OsPipe, PipeError, PipeImplBase};

pub type OSError = std::io::Error;
pub type NativeFd = libc::c_int;

pub(crate) struct PipeImpl {}

impl PipeImplBase for PipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
        let mut fds = [0i32; 2];
        cfg_if::cfg_if! {
            if #[cfg(any(
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "hurd",
                target_os = "linux",
                target_os = "netbsd",
                target_os = "openbsd",
                target_os = "redox"
            ))] {
                // libc_call!(libc::pipe2(fds.as_mut_ptr(), 0));
                libc_call!(libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC));
            } else {
                libc_call!(libc::pipe(fds.as_mut_ptr()));
                libc_call!(libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC));
                libc_call!(libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC));
            }
        }

        Ok(OsPipe {
            read_fd: fds[0].into_pipe_fd(),
            write_fd: fds[1].into_pipe_fd(),
        })
    }

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> Result<(), PipeError> {
        trace_pipe!("!!! closing {:?}", fd.as_pipe_fd());
        libc_call!(libc::close(fd.as_pipe_fd().as_native_fd()));

        Ok(())
    }

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("reading {:?} bytes from {:?}", buf.len(), fd.as_pipe_fd());
        let bytes_read = libc_call!(libc::read(
            fd.as_pipe_fd().as_native_fd(),
            buf.as_mut_ptr() as *mut _,
            buf.len(),
        ));

        trace_pipe!("read {} bytes", bytes_read);

        Ok(bytes_read as usize)
    }

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("writing {:?} bytes to {:?}", buf.len(), fd.as_pipe_fd());

        let result = libc_call!(libc::write(
            fd.as_pipe_fd().as_native_fd(),
            buf.as_ptr() as *const _,
            buf.len(),
        ));

        trace_pipe!("wrote {} bytes", result);

        Ok(result as usize)
    }

    fn dup<T: PipeFdType>(fd: impl AsPipeFd<T>) -> Result<PipeFd<T>, PipeError> {
        let result = libc_call!(libc::dup(fd.as_pipe_fd().as_native_fd()));

        let dup_fd = result.into_pipe_fd();
        trace_pipe!("duplicated {:?} to {:?}", fd.as_pipe_fd(), dup_fd);
        Ok(dup_fd)
    }

    const INVALID_FD_VALUE: NativeFd = -1;
}
