use serde::{Deserialize, Serialize};

use crate::errors::PipeError;

use super::{NXPCEndpoint, NXPCRole, NxpcMessage};

#[derive(Debug)]
pub enum NXPCError {
    NotStarted,
    AlreadyStarted,
    PipeError(PipeError),
    SendError,
    RecvError,
    SerilizationError(rmp_serde::encode::Error),
    DeserializationError(rmp_serde::decode::Error),
    Runtime(NXPCRuntimeError),
    IoError(std::io::Error),
}

impl std::fmt::Display for NXPCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NXPCError::NotStarted => write!(f, "Endpoint not started"),
            NXPCError::AlreadyStarted => write!(f, "Endpoint already started"),
            NXPCError::PipeError(e) => write!(f, "Pipe error: {}", e),
            NXPCError::SendError => write!(f, "Send error"),
            NXPCError::RecvError => write!(f, "Recv error"),
            NXPCError::SerilizationError(e) => write!(f, "Serilization error: {}", e),
            NXPCError::DeserializationError(e) => write!(f, "Deserilization error: {}", e),
            NXPCError::Runtime(e) => write!(f, "Runtime error: {}", e.message),
            NXPCError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl From<PipeError> for NXPCError {
    fn from(e: PipeError) -> Self {
        NXPCError::PipeError(e)
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for NXPCError {
    fn from(_: std::sync::mpsc::SendError<T>) -> Self {
        NXPCError::SendError
    }
}

impl From<std::sync::mpsc::RecvError> for NXPCError {
    fn from(_: std::sync::mpsc::RecvError) -> Self {
        NXPCError::RecvError
    }
}

impl From<rmp_serde::encode::Error> for NXPCError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        NXPCError::SerilizationError(e)
    }
}

impl From<rmp_serde::decode::Error> for NXPCError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        NXPCError::DeserializationError(e)
    }
}

impl From<std::io::Error> for NXPCError {
    fn from(e: std::io::Error) -> Self {
        NXPCError::IoError(e)
    }
}

impl std::error::Error for NXPCError {}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct NXPCRuntimeError {
    pub message: String,
}

impl std::fmt::Display for NXPCRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", self.message)
    }
}

impl std::error::Error for NXPCRuntimeError {}

impl From<NXPCRuntimeError> for NXPCError {
    fn from(e: NXPCRuntimeError) -> Self {
        NXPCError::Runtime(e)
    }
}

impl<'scope, 'a: 'scope, R: NXPCRole, M: NxpcMessage, P: NxpcMessage> std::fmt::Debug
    for NXPCEndpoint<'scope, 'a, R, M, P>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NXPCEndpoint").finish()
    }
}
