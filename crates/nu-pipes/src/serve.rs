use std::thread::JoinHandle;

use log::trace;
use nu_protocol::ShellError;

use crate::{
    errors::MaybeRawStream,
    unidirectional::{PipeWrite, UnOpenedPipe},
    Closeable, HandleWriter,
};

impl UnOpenedPipe<PipeWrite> {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn send(
        &self,
        input: &mut impl MaybeRawStream,
    ) -> Result<Option<JoinHandle<()>>, ShellError> {
        match input.take_stream() {
            Some(stdout) => {
                let pipe = self.open().unwrap();

                let handle = std::thread::spawn(move || {
                    let pipe = pipe;
                    let mut writer = HandleWriter::new(&pipe);
                    let mut stdout = stdout;

                    match std::io::copy(&mut stdout, &mut writer) {
                        Ok(_) => {
                            trace!("OsPipe::start_pipe thread finished writing");
                        }
                        Err(e) => {
                            trace!(
                                "OsPipe::start_pipe thread error: failed to write to pipe: {:?}",
                                e
                            );
                        }
                    }

                    match writer.close() {
                        Ok(_) => {
                            trace!("OsPipe::start_pipe thread flushed pipe");
                        }
                        Err(e) => {
                            trace!(
                                "OsPipe::start_pipe thread error: failed to flush pipe: {:?}",
                                e
                            );
                        }
                    }

                    trace!("OsPipe::start_pipe thread finished writing, closing pipe");
                    // close the pipe when the stream is finished
                });

                Ok(Some(handle))
            }
            _ => Ok(None),
        }
    }
}
