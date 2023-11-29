use std::io::Read;
#[cfg(unix)]
use std::process::Command;

use log::trace;
use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};

use crate::OsPipe;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: OsPipe,
}

impl StreamCustomValue {
    pub fn new(os_pipe: OsPipe, span: Span) -> Self {
        Self { span, os_pipe }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        trace!(
            "{}::value_string for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        self.to_base_value(self.span)
            .map(|v| v.as_string().unwrap_or_default())
            .unwrap_or_default()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        trace!(
            "{}::to_base_value for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        let mut reader = self.os_pipe.reader();
        let mut vec = Vec::new();
        _ = reader.read_to_end(&mut vec)?;

        match self.os_pipe.datatype {
            StreamDataType::Binary => Ok(Value::binary(vec, span)),
            StreamDataType::Text => Ok(Value::string(String::from_utf8_lossy(&vec), span)),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn span(&self) -> Span {
        self.span
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        match self.os_pipe.datatype {
            StreamDataType::Binary => "StreamCustomValue::Binary",
            StreamDataType::Text => "StreamCustomValue::Text",
        }
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!("typetag_deserialize")
    }

    fn as_string(&self) -> Result<String, ShellError> {
        // trace!("{}::as_string for {:?}", self.typetag_name(), self.os_pipe);

        #[cfg(all(unix, debug_assertions))]
        {
            let pid = std::process::id();
            let res_self = Command::new("ps")
                .arg("-o")
                .arg("comm=")
                .arg("-p")
                .arg(pid.to_string())
                .output();
            let self_name = match res_self {
                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                Err(_) => "".to_string(),
            };
            trace!("plugin::self: {} {:?}", pid, self_name);
            let ppid = std::os::unix::process::parent_id();
            let res_parent = Command::new("ps")
                .arg("-o")
                .arg("comm=")
                .arg("-p")
                .arg(ppid.to_string())
                .output();
            let parent_name = match res_parent {
                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                Err(_) => "".to_string(),
            };
            trace!("plugin::parent: {} {:?}", ppid, parent_name);
            let open_fds = Command::new("lsof")
                .arg("-p")
                .arg(pid.to_string())
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                .unwrap_or_else(|_| "".to_string());
            trace!("plugin::open fds: \n{}", open_fds);
            // get permissions and other info for read_fd
            let info = unsafe { libc::fcntl(self.os_pipe.read_handle.into(), libc::F_GETFL) };
            if info < 0 {
                trace!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                trace!("plugin::read_fd::access mode: {}", acc_mode);
            }
            let info = unsafe { libc::fcntl(self.os_pipe.write_handle.into(), libc::F_GETFL) };
            if info < 0 {
                trace!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                trace!("plugin::write_fd::access mode: {}", acc_mode);
            }
        }
        // self.os_pipe.close_write()?;
        let mut reader = self.os_pipe.reader();
        let mut vec = Vec::new();
        let time0 = std::time::Instant::now();
        _ = reader.read_to_end(&mut vec)?;
        let time1 = std::time::Instant::now();
        let string = String::from_utf8_lossy(&vec);
        let time2 = std::time::Instant::now();
        eprintln!(
            "plugin::as_string: {} bytes, read: {} ms, decode: {} ms",
            vec.len(),
            (time1 - time0).as_micros() as f64 / 1000.0,
            (time2 - time1).as_micros() as f64 / 1000.0
        );
        self.os_pipe.close_read()?;
        Ok(string.to_string())
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        Ok(Spanned {
            item: self.as_string()?,
            span: self.span,
        })
    }
}
