use std::fmt;

/// Everything that can go wrong in the `yuv` crate.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// A file header (PPM, Y4M) we could not understand.
    BadHeader(String),
    /// A buffer whose length does not match width × height × format.
    WrongSize {
        expected: usize,
        actual: usize,
    },
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::BadHeader(msg) => write!(f, "bad header: {msg}"),
            Error::WrongSize { expected, actual } => {
                write!(
                    f,
                    "wrong buffer size: expected {expected} bytes, got {actual}"
                )
            }
            Error::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

// Lets us use `?` on std::io results inside functions returning our Result.
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
