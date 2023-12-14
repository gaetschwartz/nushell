use std::ptr;

// use log::trace;

use windows::{
    core::HRESULT,
    Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile},
        System::Pipes::{CreatePipe, PeekNamedPipe},
    },
};

use crate::{
    errors::OSErrorKind,
    trace_pipe,
    unidirectional::{PipeFdType, PipeFdTypeEnum, PipeMode, PipeRead, PipeWrite},
    AsNativeFd, AsPipeFd, OsPipe, PipeResult,
};

use super::{IntoPipeFd, PipeError, PipeImplBase};

pub type NativeFd = windows::Win32::Foundation::HANDLE;
pub type OSError = windows::core::Error;

#[derive(Debug)]
#[repr(transparent)]
struct WindowsErrorCode(i32);

impl PartialEq<WindowsErrorCode> for HRESULT {
    fn eq(&self, other: &WindowsErrorCode) -> bool {
        self.0 as u16 == other.0 as u16
    }
}

const ERROR_BROKEN_PIPE: WindowsErrorCode = WindowsErrorCode(0x0000_006D);

pub(crate) type PipeImpl = Win32PipeImpl;

pub(crate) struct Win32PipeImpl {}

impl PipeImplBase for Win32PipeImpl {
    fn create_pipe() -> Result<OsPipe, PipeError> {
        trace_pipe!("Creating pipe");

        let mut read_fd = INVALID_HANDLE_VALUE;
        let mut write_fd = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(
                &mut read_fd,
                &mut write_fd,
                Some(&SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: ptr::null_mut(),
                    bInheritHandle: true.into(),
                }),
                0,
            )
        }?;
        Ok(OsPipe {
            read_fd: read_fd.into_pipe_fd(),
            write_fd: write_fd.into_pipe_fd(),
        })
    }

    fn close_pipe<T: PipeFdType>(handle: impl AsPipeFd<T>) -> PipeResult<()> {
        if T::TYPE == PipeFdTypeEnum::Write {
            // FLUSH FIRST
            trace_pipe!("Flushing {:?}", handle.as_pipe_fd());
            match unsafe { FlushFileBuffers(handle.as_pipe_fd().as_native_fd()) } {
                Ok(_) => {}
                Err(e) => {
                    trace_pipe!("Flush failed with {:?}", e);
                }
            }
        }

        // CLOSE
        trace_pipe!("Closing {:?}", handle.as_pipe_fd());
        unsafe { CloseHandle(handle.as_pipe_fd().as_native_fd()) }?;
        Ok(())
    }

    fn read(handle: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("Reading {} from {:?}", buf.len(), handle.as_pipe_fd());

        let mut bytes_read = 0;
        let mut bytes_available = 0;
        let mut bytes_available_this_message = 0;
        unsafe {
            PeekNamedPipe(
                handle.as_pipe_fd().as_native_fd(),
                Some(buf.as_mut_ptr() as _),
                buf.len() as u32,
                Some(&mut bytes_read),
                Some(&mut bytes_available),
                Some(&mut bytes_available_this_message),
            )
        }?;

        trace_pipe!(
            "Read: {:?}, bytes_available: {}, bytes_available_this_message: {}",
            bytes_read,
            bytes_available,
            bytes_available_this_message
        );

        if bytes_available == 0 {
            return Ok(0);
        }

        // read the bytes
        let mut bytes_read_2 = 0;
        let to_read = std::cmp::min(bytes_available as usize, buf.len());
        // null sink the rest
        let mut sink = vec![0u8; to_read];
        let res = unsafe {
            ReadFile(
                handle.as_pipe_fd().as_native_fd(),
                Some(&mut sink),
                Some(&mut bytes_read_2),
                None,
            )
        };
        assert_eq!(bytes_read, bytes_read_2);

        match res {
            Ok(_) if bytes_read == 0 => Err(PipeError {
                kind: OSErrorKind::BrokenPipe,
                message: "Pipe is closed".to_string(),
                code: Some(ERROR_BROKEN_PIPE.0),
            }),
            Ok(_) => Ok(bytes_read as usize),
            Err(e) => Err(e.into()),
        }
    }

    fn write(handle: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize> {
        trace_pipe!("Writing {} bytes to {:?}", buf.len(), handle.as_pipe_fd());

        let mut bytes_written = 0;
        unsafe {
            WriteFile(
                handle.as_pipe_fd().as_native_fd(),
                Some(buf),
                Some(&mut bytes_written),
                None,
            )
        }?;

        trace_pipe!("Wrote {} bytes", bytes_written);

        Ok(bytes_written as usize)
    }

    fn should_close_other_for_mode(_mode: PipeMode) -> bool {
        match _mode {
            PipeMode::CrossProcess => false,
            PipeMode::InProcess => false,
        }
    }

    const INVALID_FD_VALUE: NativeFd = INVALID_HANDLE_VALUE;
}

pub mod handle_serialization {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<windows::Win32::Foundation::HANDLE, D::Error>
    where
        D: Deserializer<'de>,
    {
        let handle = <isize>::deserialize(deserializer)?;
        Ok(windows::Win32::Foundation::HANDLE(handle))
    }

    pub fn serialize<S>(
        handle: &windows::Win32::Foundation::HANDLE,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        handle.0.serialize(serializer)
    }
}

impl AsNativeFd for i32 {
    fn as_native_fd(&self) -> NativeFd {
        windows::Win32::Foundation::HANDLE(*self as isize)
    }
}
