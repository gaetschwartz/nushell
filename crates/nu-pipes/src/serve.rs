use std::thread::{JoinHandle, ScopedJoinHandle};
use std::{sync::mpsc, thread::Scope};

use log::trace;
use nu_protocol::{RawStream, ShellError};

use crate::{
    unidirectional::{Pipe, PipeWrite, UnOpenedPipe},
    Closeable, HandleWriter,
};

// pub struct BetterThread<'p, I: Send, T: Send, R: Send, F: FnOnce(Option<I>) -> R + Send> {
//     handle: Option<JoinHandle<R>>,
//     tx: mpsc::Sender<T>,
//     phantom: std::marker::PhantomData<&'p ()>,
//     initial: Option<I>,
//     func: F,
// }

// pub struct BetterThreadBuilder<'scope, 'env: 'scope, I: Send> {
//     name: Option<String>,
//     scope: Option<&'scope Scope<'scope, 'env>>,
//     initial: Option<I>,
// }

// impl<'scope, 'env: 'scope, I: Send> BetterThreadBuilder<'scope, 'env, I> {
//     pub fn new() -> Self {
//         Self {
//             name: None,
//             scope: None,
//             initial: None,
//         }
//     }

//     pub fn name(mut self, name: String) -> Self {
//         self.name = Some(name);
//         self
//     }

//     pub fn scope(mut self, scope: &'scope Scope<'scope, 'env>) -> Self {
//         self.scope = Some(scope);
//         self
//     }

//     pub fn initial(mut self, initial: Option<I>) -> Self {
//         self.initial = initial;
//         self
//     }

//     pub fn spawn<T: Send, R: Send, F: FnOnce(Option<I>) -> R + Send>(
//         &self,
//         func: F,
//     ) -> Result<BetterThread<I, T, R, F>, ShellError> {
//         let (sv, rv) = mpsc::channel::<T>();
//         let mut builder = std::thread::Builder::new();
//         if let Some(name) = &self.name {
//             builder = builder.name(name.clone());
//         }
//         let handle = if let Some(scope) = self.scope {
//             builder.spawn_scoped(scope, move || {
//                 let initial = self.initial;
//                 let func = func;
//                 let data = rv.recv();
//                 return data.map(|v| func(initial));
//             })?
//         } else {
//             let static_scope = std::thread::current().scope();
//             builder.spawn(move || {
//                 let initial = self.initial;
//                 let func = func;
//                 let data = rv.recv();
//                 return data.map(|v| func(initial));
//             })?
//         };
//         let t = BetterThread {
//             initial: self.initial,
//             func,
//             tx: sv,
//             phantom: std::marker::PhantomData,
//         };
//         Ok(t)
//     }
// }

// impl<'p, I: Send, T: Send, R: Send, F: FnOnce(Option<I>) -> R + Send> BetterThread<'p, I, T, R, F> {}

impl<'a> UnOpenedPipe<PipeWrite> {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn send<'scope, 'env: 'scope>(
        &'a self,
        scope: &'scope Scope<'scope, 'env>,
        stdout: RawStream,
    ) -> Result<Option<ScopedJoinHandle<'scope, ()>>, ShellError>
    where
        'a: 'env,
    {
        let handle = scope.spawn(move || {
            let mut writer = self.open().unwrap();
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
}
