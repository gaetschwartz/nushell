use std::io::Write;
use std::thread::Scope;
use std::thread::ScopedJoinHandle;

use nu_protocol::{RawStream, ShellError};

use crate::trace_pipe;
use crate::unidirectional::{PipeWrite, UnOpenedPipe};

pub trait StreamSender<'a> {
    fn send_stream_scoped<'scope, 'env: 'scope>(
        &'a self,
        scope: &'scope Scope<'scope, 'env>,
        stdout: RawStream,
    ) -> Result<Option<ScopedJoinHandle<'scope, ()>>, ShellError>
    where
        'a: 'env;
}

impl<'a> StreamSender<'a> for UnOpenedPipe<PipeWrite> {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    fn send_stream_scoped<'scope, 'env: 'scope>(
        &'a self,
        scope: &'scope Scope<'scope, 'env>,
        stdout: RawStream,
    ) -> Result<Option<ScopedJoinHandle<'scope, ()>>, ShellError>
    where
        'a: 'env,
    {
        let handle = std::thread::Builder::new()
            .name("serve_stream".to_owned())
            .spawn_scoped(scope, move || {
                trace_pipe!("starting to write");
                let mut writer = self.open().unwrap();

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
