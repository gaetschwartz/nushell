#![warn(missing_docs)]

//! Nu-pipes is a library for working with pipes in a cross-platform way.
//! It utilizes pipe(2) on Unix and CreatePipe on Windows.
mod errors;
mod os_pipes;
mod stream_writer;
pub mod utils;

use errors::*;
pub use io::{PipeReader, PipeWriter};
pub use os_pipes::*;
pub use stream_writer::StreamWriter;
