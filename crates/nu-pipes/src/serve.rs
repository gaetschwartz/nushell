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
                let mut writer = self.open().unwrap();
                if let Some(size) = stdout.known_size {
                    _ = writer.set_pledged_src_size(Some(size));
                }
                let mut stdout = stdout;

                match std::io::copy(&mut stdout, &mut writer) {
                    Ok(_) => {
                        trace_pipe!("finished writing");
                    }
                    Err(e) => {
                        trace_pipe!("error: failed to write to pipe: {:?}", e);
                    }
                }

                match writer.close() {
                    Ok(_) => {
                        trace_pipe!("lushed pipe");
                    }
                    Err(e) => {
                        trace_pipe!("error: failed to flush pipe: {:?}", e);
                    }
                }

                trace_pipe!("finished writing, closing pipe");
                // close the pipe when the stream is finished
            })
            .expect("failed to spawn thread");

        Ok(Some(handle))
    }
}
