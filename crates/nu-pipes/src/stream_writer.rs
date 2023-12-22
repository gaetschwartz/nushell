use std::io::Write;
use std::thread;

use nu_protocol::{RawStream, ShellError};

use crate::unidirectional::PipeWrite;
use crate::utils::NamedScopedThreadSpawn;
use crate::PipeFd;
use crate::{trace_pipe, AsPipeFd};

/// Trait for writing streams to pipes.
pub trait StreamWriter<'a>: AsPipeFd<PipeWrite> {
    /// Sends the stdout stream to an external stream within a scoped thread.
    ///
    /// # Arguments
    ///
    /// * `scope` - The thread scope.
    /// * `stdout` - The stdout stream to be sent.
    ///
    /// # Returns
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    fn send_stream_scoped<'scope, 'env: 'scope>(
        self,
        scope: &'scope thread::Scope<'scope, 'env>,
        stdout: RawStream,
    ) -> Result<Option<thread::ScopedJoinHandle<'scope, ()>>, ShellError>
    where
        'a: 'env;
}

impl<'a> StreamWriter<'a> for PipeFd<PipeWrite> {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    fn send_stream_scoped<'scope, 'env: 'scope>(
        self,
        scope: &'scope thread::Scope<'scope, 'env>,
        stdout: RawStream,
    ) -> Result<Option<thread::ScopedJoinHandle<'scope, ()>>, ShellError>
    where
        'a: 'env,
    {
        let handle = scope
            .spawn_named("serve_stream", move || {
                trace_pipe!("starting to write");

                let mut writer = self.into_writer();

                let mut stdout = stdout;

                loop {
                    match stdout.stream.next() {
                        Some(item) => match item {
                            Ok(item) => {
                                trace_pipe!("writing item");
                                match writer.write_all(&item) {
                                    Ok(_) => {
                                        trace_pipe!("wrote {} bytes", item.len());
                                    }
                                    Err(e) => {
                                        trace_pipe!("error: failed to write item: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                trace_pipe!("error: failed to get item: {:?}", e);
                            }
                        },
                        None => {
                            trace_pipe!("no more data to write");
                            break;
                        }
                    }
                }
                trace_pipe!("finished writing, closing pipe");

                match writer.flush() {
                    Ok(_) => {
                        trace_pipe!("flushed pipe");
                    }
                    Err(e) => {
                        trace_pipe!("error: failed to flush pipe: {:?}", e);
                    }
                }
                match writer.close() {
                    Ok(_) => {
                        trace_pipe!("closed pipe");
                    }
                    Err(e) => {
                        trace_pipe!("error: failed to close pipe: {:?}", e);
                    }
                }
            })
            .expect("failed to spawn thread");

        Ok(Some(handle))
    }
}
