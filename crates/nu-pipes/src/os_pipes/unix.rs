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
                libc_call!(libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC))?;
            } else {
                libc_call!(libc::pipe(fds.as_mut_ptr()))?;
                libc_call!(libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
                libc_call!(libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
            }
        }

        Ok(OsPipe {
            read_fd: unsafe { fds[0].into_pipe_fd() },
            write_fd: unsafe { fds[1].into_pipe_fd() },
        })
    }

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> Result<(), PipeError> {
        trace_pipe!("!!! closing {:?}", fd.as_pipe_fd());
        libc_call!(libc::close(fd.as_pipe_fd().native_fd()))?;

        Ok(())
    }

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("reading {:?} bytes from {:?}", buf.len(), fd.as_pipe_fd());
        let bytes_read = libc_call!(libc::read(
            fd.as_pipe_fd().native_fd(),
            buf.as_mut_ptr() as *mut _,
            buf.len(),
        ))?;

        trace_pipe!("read {} bytes", bytes_read);

        Ok(bytes_read as usize)
    }

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("writing {:?} bytes to {:?}", buf.len(), fd.as_pipe_fd());

        let written = libc_call!(libc::write(
            fd.as_pipe_fd().native_fd(),
            buf.as_ptr() as *const _,
            buf.len(),
        ))?;

        trace_pipe!("wrote {} bytes", written);

        Ok(written as usize)
    }

    fn dup<T: PipeFdType>(fd: impl AsPipeFd<T>) -> Result<PipeFd<T>, PipeError> {
        let duped = libc_call!(libc::dup(fd.as_pipe_fd().native_fd()))?;

        let dup_fd = unsafe { PipeFd::from_raw_fd(duped) };
        trace_pipe!("duplicated {:?} to {:?}", fd.as_pipe_fd(), dup_fd);
        Ok(dup_fd)
    }

    const INVALID_FD_VALUE: NativeFd = -1;
}

impl<T: PipeFdType> IntoPipeFd<T> for NativeFd {
    unsafe fn into_pipe_fd(self) -> PipeFd<T> {
        PipeFd::from_raw_fd(self)
    }
}

#[cfg(test)]
mod test {
    use crate::{unidirectional::pipe, AsNativeFd, AsPipeFd};

    trait HasFlagSet {
        fn is(&self, flag: libc::c_int) -> bool;
        fn isnt(&self, flag: libc::c_int) -> bool {
            !self.is(flag)
        }
    }

    impl HasFlagSet for libc::c_int {
        fn is(&self, flag: libc::c_int) -> bool {
            self & flag == flag
        }
    }

    #[test]
    fn created_pipes_are_o_cloexec() {
        let (read, write) = pipe().unwrap();

        let read_flags = unsafe { libc::fcntl(read.as_pipe_fd().native_fd(), libc::F_GETFD) };
        let write_flags = unsafe { libc::fcntl(write.as_pipe_fd().native_fd(), libc::F_GETFD) };

        assert!(read_flags.is(libc::FD_CLOEXEC));
        assert!(write_flags.is(libc::FD_CLOEXEC));
    }

    #[test]
    fn duplicating_pipe_fd_doesnt_preserve_cloexec() {
        let (read, write) = pipe().unwrap();

        let dup_read = read.try_clone().unwrap();
        let dup_write = write.try_clone().unwrap();

        let dup_read_flags =
            unsafe { libc::fcntl(dup_read.as_pipe_fd().native_fd(), libc::F_GETFD) };
        let dup_write_flags =
            unsafe { libc::fcntl(dup_write.as_pipe_fd().native_fd(), libc::F_GETFD) };

        assert!(dup_read_flags.isnt(libc::FD_CLOEXEC));
        assert!(dup_write_flags.isnt(libc::FD_CLOEXEC));
    }
    #[test]
    fn duplicating_pipe_fd_creates_new_fd() {
        let (read, write) = pipe().unwrap();

        let dup_read = read.try_clone().unwrap();
        let dup_write = write.try_clone().unwrap();

        assert_ne!(read, dup_read);
        assert_ne!(write, dup_write);
    }
}
