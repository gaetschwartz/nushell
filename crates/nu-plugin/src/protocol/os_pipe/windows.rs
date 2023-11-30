use std::{io, ptr};

// use log::trace;
use nu_protocol::{Span, StreamDataType};

use crate::{protocol::os_pipe::OSError, Handle, OsPipe};
use windows::{
    core::HRESULT,
    Win32::{
        Foundation::{CloseHandle, BOOL, INVALID_HANDLE_VALUE},
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{ReadFile, WriteFile},
        System::Pipes::CreatePipe,
    },
};

use super::{PipeError, PipeResult};

const DEFAULT_SECURITY_ATTRIBUTES: SECURITY_ATTRIBUTES = SECURITY_ATTRIBUTES {
    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
    lpSecurityDescriptor: ptr::null_mut(),
    // bInheritHandle: BOOL::from(true),
    bInheritHandle: BOOL(1),
};

pub fn create_pipe(span: Span) -> Result<OsPipe, PipeError> {
    let mut read_handle = INVALID_HANDLE_VALUE;
    let mut write_handle = INVALID_HANDLE_VALUE;

    unsafe {
        CreatePipe(
            &mut read_handle,
            &mut write_handle,
            Some(&DEFAULT_SECURITY_ATTRIBUTES),
            0,
        )
    }
    .map_err(|e| PipeError::FailedToCreatePipe(OSError(e)))?;
    let read_handle = Handle::Read(read_handle);
    let write_handle = Handle::Write(write_handle);

    Ok(OsPipe {
        span,
        read_handle,
        write_handle,
        datatype: StreamDataType::Binary,
        handle_policy: super::HandlePolicy::Inlusive,
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

pub fn close_handle(handle: Handle) -> PipeResult<()> {
    // println!("{} OsPipe::close for {:?}", header(), handle,);
    unsafe { CloseHandle(handle.native()) }
        .map_err(|e| PipeError::FailedToCloseHandle(handle, OSError(e)))
}

pub fn read_handle(handle: Handle, buf: &mut [u8]) -> PipeResult<usize> {
    // eprintln!("{} OsPipe::read for {:?}", header(), handle,);

    let mut bytes_read = 0;
    let res = unsafe { ReadFile(handle.native(), Some(buf), Some(&mut bytes_read), None) };

    match res {
        Ok(_) => Ok(bytes_read as usize),
        Err(e) if e.code() == ERROR_BROKEN_PIPE => Ok(0),
        Err(e) => Err(PipeError::FailedToRead(handle, e.into())),
    }
}

#[derive(Debug)]
#[repr(transparent)]
struct WindowsErrorCode(i32);

impl PartialEq<WindowsErrorCode> for HRESULT {
    fn eq(&self, other: &WindowsErrorCode) -> bool {
        self.0 as u16 == other.0 as u16
    }
}

const ERROR_BROKEN_PIPE: WindowsErrorCode = WindowsErrorCode(0x0000_006D);

pub(crate) fn write_handle(handle: Handle, buf: &[u8]) -> io::Result<usize> {
    // println!(
    //     "{} OsPipe::write for {:?} ({} bytes)",
    //     header(),
    //     handle,
    //     buf.len()
    // );

    let mut bytes_written = 0;
    unsafe { WriteFile(handle.native(), Some(buf), Some(&mut bytes_written), None) }
        .map_err(|e| PipeError::FailedToWrite(handle, e.into()))?;

    // println!("OsPipe::write: {} bytes", bytes_written);

    Ok(bytes_written as usize)
}

impl From<windows::core::Error> for OSError {
    fn from(error: windows::core::Error) -> Self {
        OSError(error)
    }
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
