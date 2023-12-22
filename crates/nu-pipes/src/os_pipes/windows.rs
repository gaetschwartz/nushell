use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, RawHandle},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    Foundation::{
        CloseHandle, DuplicateHandle, BOOL, DUPLICATE_SAME_ACCESS, ERROR_BROKEN_PIPE, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{ReadFile, WriteFile},
    System::{Pipes::CreatePipe, Threading::GetCurrentProcess},
};

use crate::{
    trace_pipe,
    unidirectional::{PipeFdType, PipeRead, PipeWrite},
    AsNativeFd, AsPipeFd, AsRawPipeFd, FromNativeFd, FromRawPipeFd, OsPipe, PipeFd, PipeResult,
    RawPipeFd,
};

use super::{PipeError, PipeImplBase};

pub type NativeFd = windows::Win32::Foundation::HANDLE;

pub(crate) type PipeImpl = Win32PipeImpl;

pub(crate) struct Win32PipeImpl();

const DEFAULT_SECURITY_ATTRIBUTES: SECURITY_ATTRIBUTES = SECURITY_ATTRIBUTES {
    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
    lpSecurityDescriptor: std::ptr::null_mut(),
    bInheritHandle: BOOL(0),
};

impl PipeImplBase for Win32PipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
        trace_pipe!("Creating pipe");

        let mut read_fd = INVALID_HANDLE_VALUE;
        let mut write_fd = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(
                &mut read_fd,
                &mut write_fd,
                Some(&DEFAULT_SECURITY_ATTRIBUTES),
                0,
            )
        }?;

        Ok(OsPipe {
            read_fd: unsafe { PipeFd::from_native_fd(read_fd) },
            write_fd: unsafe { PipeFd::from_native_fd(write_fd) },
        })
    }

    fn close_pipe<T: PipeFdType>(handle: impl AsPipeFd<T>) -> PipeResult<()> {
        // CLOSE
        trace_pipe!("Closing {:?}", handle.as_pipe_fd());
        unsafe { CloseHandle(handle.as_pipe_fd().native_fd()) }?;
        Ok(())
    }

    fn read(handle: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("Reading {} from {:?}", buf.len(), handle.as_pipe_fd());

        let mut bytes_read = 0;
        let res = unsafe {
            ReadFile(
                handle.as_pipe_fd().native_fd(),
                Some(buf),
                Some(&mut bytes_read),
                None,
            )
        };

        match res {
            Ok(_) => {
                trace_pipe!("Read {} bytes", bytes_read);
                Ok(bytes_read as usize)
            }
            Err(e) if e.code() == ERROR_BROKEN_PIPE.to_hresult() => {
                trace_pipe!("Broken pipe, meaning EOF");
                Ok(0)
            }
            Err(e) => {
                trace_pipe!("Read error: {:?}", e);
                Err(e.into())
            }
        }
    }

    fn write(handle: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("Writing {} bytes to {:?}", buf.len(), handle.as_pipe_fd());

        let mut bytes_written = 0;
        unsafe {
            WriteFile(
                handle.as_pipe_fd().native_fd(),
                Some(buf),
                Some(&mut bytes_written),
                None,
            )
        }?;

        trace_pipe!("Wrote {} bytes", bytes_written);

        Ok(bytes_written as usize)
    }

    fn dup<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<crate::PipeFd<T>> {
        let mut new_fd = INVALID_HANDLE_VALUE;
        unsafe {
            let current_process = GetCurrentProcess();
            DuplicateHandle(
                current_process,
                fd.as_pipe_fd().native_fd(),
                current_process,
                &mut new_fd,
                0,
                BOOL::from(true),
                DUPLICATE_SAME_ACCESS,
            )
        }?;
        let dup_fd = unsafe { PipeFd::from_native_fd(new_fd) };
        trace_pipe!("Duplicated {:?} to {:?}", fd.as_pipe_fd(), dup_fd);

        Ok(dup_fd)
    }

    const INVALID_FD_VALUE: NativeFd = INVALID_HANDLE_VALUE;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(remote = "windows::Win32::Foundation::HANDLE")]
pub(crate) struct FdSerializable(pub isize);

impl<T: PipeFdType> AsRawHandle for PipeFd<T> {
    fn as_raw_handle(&self) -> RawHandle {
        self.0 .0 as _
    }
}
impl<T: PipeFdType> FromRawHandle for PipeFd<T> {
    unsafe fn from_raw_handle(handle: RawHandle) -> Self {
        Self(HANDLE(handle as _), std::marker::PhantomData)
    }
}

impl<T: FromRawHandle> FromRawPipeFd for T {
    #[inline]
    unsafe fn from_raw_pipe_fd(fd: RawPipeFd) -> Self {
        Self::from_raw_handle(fd as _)
    }
}

impl AsRawPipeFd for NativeFd {
    #[inline]
    unsafe fn as_raw_pipe_fd(&self) -> RawPipeFd {
        self.0 as _
    }
}

impl<T: PipeFdType> FromNativeFd for PipeFd<T> {
    unsafe fn from_native_fd(fd: NativeFd) -> PipeFd<T> {
        PipeFd::from_raw_pipe_fd(fd.as_raw_pipe_fd())
    }
}

impl<T: PipeFdType> From<PipeFd<T>> for Stdio {
    fn from(val: PipeFd<T>) -> Self {
        unsafe { Stdio::from_raw_handle(val.as_raw_handle()) }
    }
}

impl<T: PipeFdType> From<&PipeFd<T>> for Stdio {
    fn from(val: &PipeFd<T>) -> Self {
        unsafe { Stdio::from_raw_handle(val.as_raw_handle()) }
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::BOOL;

    #[test]
    fn default_security_attributes() {
        let sa = super::DEFAULT_SECURITY_ATTRIBUTES;
        assert_eq!(sa.bInheritHandle, BOOL::from(false));
    }
}
