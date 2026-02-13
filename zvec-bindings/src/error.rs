#![allow(non_upper_case_globals)]

use thiserror::Error;

/// Error type for zvec operations.
#[derive(Debug, Clone, Error)]
pub enum Error {
    /// Resource not found (collection, document, index, etc.)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Resource already exists
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Operation not supported
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Internal error in the underlying library
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Permission denied for the operation
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Precondition for the operation was not met
    #[error("Failed precondition: {0}")]
    FailedPrecondition(String),

    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),

    /// Null pointer encountered
    #[error("Null pointer")]
    NullPointer,

    /// Collection not found
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),

    /// Index not found
    #[error("Index not found: {0}")]
    IndexNotFound(String),

    /// Field not found in document or schema
    #[error("Field not found: {0}")]
    FieldNotFound(String),

    /// Vector dimension mismatch
    #[error("Vector dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected dimension
        expected: usize,
        /// Actual dimension provided
        actual: usize,
    },

    /// UTF-8 conversion error
    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// FFI string conversion error
    #[error("IO error: {0}")]
    IoError(#[from] std::ffi::NulError),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<crate::ffi::zvec_status_code> for Error {
    fn from(code: crate::ffi::zvec_status_code) -> Self {
        use crate::ffi::*;
        match code {
            zvec_status_code_ZVEC_STATUS_NOT_FOUND => Error::NotFound(String::new()),
            zvec_status_code_ZVEC_STATUS_ALREADY_EXISTS => Error::AlreadyExists(String::new()),
            zvec_status_code_ZVEC_STATUS_INVALID_ARGUMENT => Error::InvalidArgument(String::new()),
            zvec_status_code_ZVEC_STATUS_NOT_SUPPORTED => Error::NotSupported(String::new()),
            zvec_status_code_ZVEC_STATUS_INTERNAL_ERROR => Error::InternalError(String::new()),
            zvec_status_code_ZVEC_STATUS_PERMISSION_DENIED => {
                Error::PermissionDenied(String::new())
            }
            zvec_status_code_ZVEC_STATUS_FAILED_PRECONDITION => {
                Error::FailedPrecondition(String::new())
            }
            _ => Error::Unknown(String::new()),
        }
    }
}

pub(crate) fn check_status(status: crate::ffi::zvec_status_t) -> Result<()> {
    use crate::ffi::*;

    if status.code == zvec_status_code_ZVEC_STATUS_OK {
        return Ok(());
    }

    let message = if status.message.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(status.message) }
            .to_string_lossy()
            .into_owned()
    };

    let mut error: Error = status.code.into();

    error = match error {
        Error::NotFound(_) => Error::NotFound(message),
        Error::AlreadyExists(_) => Error::AlreadyExists(message),
        Error::InvalidArgument(_) => Error::InvalidArgument(message),
        Error::NotSupported(_) => Error::NotSupported(message),
        Error::InternalError(_) => Error::InternalError(message),
        Error::PermissionDenied(_) => Error::PermissionDenied(message),
        Error::FailedPrecondition(_) => Error::FailedPrecondition(message),
        Error::Unknown(_) => Error::Unknown(message),
        other => other,
    };

    Err(error)
}
