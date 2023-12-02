use serde::{Deserialize, Serialize};

use crate::{pipe_impl, PipeError, PipeResult, StreamEncoding};



pub struct BidirectionalPipeResult {
    ours: UnidirectionalReadPipe,
    theirs: UnidirectionalReadPipe,
    mode: PipeMode,
}

impl BidirectionalPipe {
    pub fn create(mode: PipeMode) -> PipeResult<BidirectionalPipe> {
        Self::with_mode_and_encoding(mode, StreamEncoding::None)
    }

    pub fn with_mode_and_encoding(
        mode: PipeMode,
        encoding: StreamEncoding,
    ) -> PipeResult<BidirectionalPipe> {
        let pipe1 = pipe_impl::create_pipe()?;
        let pipe2 = pipe_impl::create_pipe()?;
        let ours = UnidirectionalReadPipe {
            datatype: nu_protocol::StreamDataType::Binary,
            encoding,
            read_handle: pipe1.read_handle,
            write_handle: pipe2.write_handle,
            mode,
        };
        let theirs = UnidirectionalReadPipe {
            datatype: nu_protocol::StreamDataType::Binary,
            encoding,
            read_handle: pipe2.read_handle,
            write_handle: pipe1.write_handle,
            mode,
        };
        if pipe_impl::should_close_other_for_mode(mode) {
            // close both their ends of the pipe in our process
            theirs.close_read()?;
            theirs.close_write()?;
        }

        Ok(BidirectionalPipeResult { ours, theirs, mode })
    }

    pub fn mode(&self) -> PipeMode {
        self.mode
    }
}

pub struct BiDirectionalPipeOptions {
    encoding: StreamEncoding,
    mode: PipeMode,
}

impl Default for BiDirectionalPipeOptions {
    fn default() -> Self {
        Self {
            encoding: StreamEncoding::None,
            mode: PipeMode::CrossProcess,
        }
    }
}

impl BiDirectionalPipeOptions {
    pub fn encoding(mut self, encoding: StreamEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn mode(mut self, mode: PipeMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn create(&self) -> Result<BidirectionalPipe, PipeError> {
        BidirectionalPipe::with_mode_and_encoding(self.mode, self.encoding)
    }
}
