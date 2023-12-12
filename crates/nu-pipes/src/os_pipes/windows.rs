use std::ptr;

// use log::trace;

use windows::{
    core::HRESULT,
    Win32::{
        Foundation::{CloseHandle, BOOL, INVALID_HANDLE_VALUE},
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{ReadFile, WriteFile},
        System::Pipes::CreatePipe,
    },
};

use crate::{trace_pipe, unidirectional::PipeMode, AsNativeFd, OsPipe, PipeResult};

use super::{IntoPipeFd, PipeError, PipeImplBase};

pub type NativeFd = windows::Win32::Foundation::HANDLE;
pub type OSError = windows::core::Error;

const DEFAULT_SECURITY_ATTRIBUTES: SECURITY_ATTRIBUTES = SECURITY_ATTRIBUTES {
    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
    lpSecurityDescriptor: ptr::null_mut(),
    // bInheritHandle: BOOL::from(true),
    bInheritHandle: BOOL(1),
};

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
        let mut read_handle = INVALID_HANDLE_VALUE;
        let mut write_handle = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(
                &mut read_handle,
                &mut write_handle,
                Some(&DEFAULT_SECURITY_ATTRIBUTES),
                0,
            )
        }?;
        let read_handle = read_handle.into_pipe_fd();
        let write_handle = write_handle.into_pipe_fd();

        Ok(OsPipe {
            read_fd: write_handle,
            write_fd: read_handle,
        })
    }

    // fn header() -> String {
    //     format!(
    //         "[{}#{}:{:?}]",
    //         PathBuf::from(std::env::args().next().unwrap_or_default())
    //             .file_name()
    //             .unwrap_or_default()
    //             .to_string_lossy(),
    //         std::process::id(),
    //         std::thread::current().id()
    //     )
    // }

    fn close_pipe(handle: impl AsNativeFd) -> PipeResult<()> {
        trace_pipe!("closing {:?}", handle.as_native_fd());
        unsafe { CloseHandle(handle.as_native_fd()) }?;
        Ok(())
    }

    fn read(handle: impl AsNativeFd, buf: &mut [u8]) -> PipeResult<usize> {
        trace_pipe!("Reading from {:?}", handle.as_native_fd());

        let mut bytes_read = 0;
        let res = unsafe {
            ReadFile(
                handle.as_native_fd(),
                Some(buf),
                Some(&mut bytes_read),
                None,
            )
        };

        match res {
            Ok(_) => Ok(bytes_read as usize),
            Err(e) if e.code() == ERROR_BROKEN_PIPE => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    fn write(handle: impl AsNativeFd, buf: &[u8]) -> PipeResult<usize> {
        // println!(
        //     "{} OsPipe::write for {:?} ({} bytes)",
        //     header(),
        //     handle,
        //     buf.len()
        // );

        let mut bytes_written = 0;
        unsafe {
            WriteFile(
                handle.as_native_fd(),
                Some(buf),
                Some(&mut bytes_written),
                None,
            )
        }?;

        // println!("OsPipe::write: {} bytes", bytes_written);

        Ok(bytes_written as usize)
    }

    fn should_close_other_for_mode(mode: PipeMode) -> bool {
        match mode {
            PipeMode::CrossProcess => true,
            PipeMode::InProcess => false,
        }
    }

    const INVALID_FD: NativeFd = INVALID_HANDLE_VALUE;
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
