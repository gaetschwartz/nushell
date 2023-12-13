use nu_protocol::ShellError;

#[derive(Debug)]
pub struct PipeError {
    pub kind: OSErrorKind,
    pub message: String,
}

impl PipeError {
    pub fn new(kind: OSErrorKind, message: String) -> Self {
        Self { kind, message }
    }

    pub fn os_error<S: Into<String>>(message: S) -> Self {
        Self {
            kind: OSErrorKind::from_last_os_error(),
            message: message.into(),
        }
    }
}

impl std::error::Error for PipeError {}
impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

#[allow(dead_code)]
pub type PipeResult<T> = Result<T, PipeError>;

impl From<PipeError> for std::io::Error {
    fn from(error: PipeError) -> Self {
        std::io::Error::new(error.kind.into(), error)
    }
}

impl From<PipeError> for ShellError {
    fn from(error: PipeError) -> Self {
        ShellError::IOError {
            msg: error.to_string(),
        }
    }
}

/// All the libc errors.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OSErrorKind {
    None,
    AccessDenied,
    BadFileDescriptor,
    FileExists,
    InvalidInput,
    TooManyOpenFiles,
    TooManyOpenFilesInSystem,
    FileNotFound,
    NoSpace,
    NotConnected,
    BrokenPipe,
    ConnectionReset,
    ConnectionAborted,
    ConnectionRefused,
    NotSocket,
    AddressInUse,
    AddressNotAvailable,
    AddressFamilyNotSupported,
    AlreadyConnected,
    DestinationAddressRequired,
    HostUnreachable,
    MessageTooLong,
    Unknown(i32),
}

impl OSErrorKind {
    pub fn from_last_os_error() -> Self {
        if let Some(code) = std::io::Error::last_os_error().raw_os_error() {
            OSErrorKind::from(code)
        } else {
            OSErrorKind::None
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for OSErrorKind {
    fn from(error: windows::core::Error) -> Self {
        let Some(error) = windows::Win32::Foundation::WIN32_ERROR::from_error(&error) else {
            return OSErrorKind::None;
        };
        OSErrorKind::from(error)
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for PipeError {
    fn from(error: windows::core::Error) -> Self {
        Self {
            message: error.message().to_string(),
            kind: OSErrorKind::from(error),
        }
    }
}

#[cfg(windows)]
impl From<windows::Win32::Foundation::WIN32_ERROR> for OSErrorKind {
    fn from(error: windows::Win32::Foundation::WIN32_ERROR) -> Self {
        match error {
            windows::Win32::Foundation::ERROR_SUCCESS => OSErrorKind::None,
            windows::Win32::Foundation::ERROR_ACCESS_DENIED => OSErrorKind::AccessDenied,
            windows::Win32::Foundation::ERROR_BAD_FILE_TYPE => OSErrorKind::BadFileDescriptor,
            windows::Win32::Foundation::ERROR_FILE_EXISTS => OSErrorKind::FileExists,
            windows::Win32::Foundation::ERROR_INVALID_DATA => OSErrorKind::InvalidInput,
            windows::Win32::Foundation::ERROR_TOO_MANY_OPEN_FILES => OSErrorKind::TooManyOpenFiles,
            windows::Win32::Foundation::ERROR_NOT_ENOUGH_MEMORY => {
                OSErrorKind::TooManyOpenFilesInSystem
            }
            windows::Win32::Foundation::ERROR_FILE_NOT_FOUND => OSErrorKind::FileNotFound,
            windows::Win32::Foundation::ERROR_DISK_FULL => OSErrorKind::NoSpace,
            windows::Win32::Foundation::ERROR_NOT_CONNECTED => OSErrorKind::NotConnected,
            windows::Win32::Foundation::ERROR_BROKEN_PIPE => OSErrorKind::BrokenPipe,
            windows::Win32::Foundation::ERROR_NO_DATA => OSErrorKind::BrokenPipe,
            windows::Win32::Foundation::ERROR_CONNECTION_ABORTED => OSErrorKind::ConnectionAborted,
            windows::Win32::Foundation::ERROR_CONNECTION_REFUSED => OSErrorKind::ConnectionRefused,
            windows::Win32::Foundation::ERROR_ADDRESS_ALREADY_ASSOCIATED => {
                OSErrorKind::AddressInUse
            }
            windows::Win32::Foundation::ERROR_ADDRESS_NOT_ASSOCIATED => {
                OSErrorKind::AddressNotAvailable
            }
            windows::Win32::Foundation::ERROR_CONNECTION_INVALID => OSErrorKind::NotSocket,
            windows::Win32::Foundation::ERROR_CONNECTION_ACTIVE => OSErrorKind::AlreadyConnected,
            _ => OSErrorKind::Unknown(error.0 as i32),
        }
    }
}

impl From<i32> for OSErrorKind {
    fn from(code: i32) -> Self {
        #[cfg(unix)]
        match code {
            libc::EACCES => OSErrorKind::AccessDenied,
            libc::EBADF => OSErrorKind::BadFileDescriptor,
            libc::EEXIST => OSErrorKind::FileExists,
            libc::EINVAL => OSErrorKind::InvalidInput,
            libc::EMFILE => OSErrorKind::TooManyOpenFiles,
            libc::ENFILE => OSErrorKind::TooManyOpenFilesInSystem,
            libc::ENOENT => OSErrorKind::FileNotFound,
            libc::ENOSPC => OSErrorKind::NoSpace,
            libc::ENOTCONN => OSErrorKind::NotConnected,
            libc::EPIPE => OSErrorKind::BrokenPipe,
            libc::ECONNRESET => OSErrorKind::ConnectionReset,
            libc::ECONNABORTED => OSErrorKind::ConnectionAborted,
            libc::ECONNREFUSED => OSErrorKind::ConnectionRefused,
            libc::ENOTSOCK => OSErrorKind::NotSocket,
            libc::EADDRINUSE => OSErrorKind::AddressInUse,
            libc::EADDRNOTAVAIL => OSErrorKind::AddressNotAvailable,
            libc::EAFNOSUPPORT => OSErrorKind::AddressFamilyNotSupported,
            libc::EISCONN => OSErrorKind::AlreadyConnected,
            libc::EDESTADDRREQ => OSErrorKind::DestinationAddressRequired,
            libc::EHOSTUNREACH => OSErrorKind::HostUnreachable,
            libc::EMSGSIZE => OSErrorKind::MessageTooLong,
            e => OSErrorKind::Unknown(e),
        }
        #[cfg(windows)]
        windows::Win32::Foundation::WIN32_ERROR(code as u32).into()
    }
}

impl From<OSErrorKind> for std::io::ErrorKind {
    fn from(kind: OSErrorKind) -> Self {
        match kind {
            OSErrorKind::None => std::io::ErrorKind::Other,
            OSErrorKind::AccessDenied => std::io::ErrorKind::PermissionDenied,
            OSErrorKind::BadFileDescriptor => std::io::ErrorKind::InvalidInput,
            OSErrorKind::FileExists => std::io::ErrorKind::AlreadyExists,
            OSErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
            OSErrorKind::TooManyOpenFiles => std::io::ErrorKind::Other,
            OSErrorKind::TooManyOpenFilesInSystem => std::io::ErrorKind::Other,
            OSErrorKind::FileNotFound => std::io::ErrorKind::NotFound,
            OSErrorKind::NoSpace => std::io::ErrorKind::Other,
            OSErrorKind::NotConnected => std::io::ErrorKind::NotConnected,
            OSErrorKind::BrokenPipe => std::io::ErrorKind::BrokenPipe,
            OSErrorKind::ConnectionReset => std::io::ErrorKind::ConnectionReset,
            OSErrorKind::ConnectionAborted => std::io::ErrorKind::ConnectionAborted,
            OSErrorKind::ConnectionRefused => std::io::ErrorKind::ConnectionRefused,
            OSErrorKind::NotSocket => std::io::ErrorKind::NotConnected,
            OSErrorKind::AddressInUse => std::io::ErrorKind::AddrInUse,
            OSErrorKind::AddressNotAvailable => std::io::ErrorKind::AddrNotAvailable,
            OSErrorKind::AlreadyConnected => std::io::ErrorKind::AlreadyExists,
            OSErrorKind::DestinationAddressRequired => std::io::ErrorKind::AddrNotAvailable,
            OSErrorKind::HostUnreachable => std::io::ErrorKind::AddrNotAvailable,
            OSErrorKind::MessageTooLong => std::io::ErrorKind::InvalidInput,
            OSErrorKind::AddressFamilyNotSupported => std::io::ErrorKind::AddrNotAvailable,
            OSErrorKind::Unknown(_) => std::io::ErrorKind::Other,
        }
    }
}
