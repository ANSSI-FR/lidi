use std::error;
use std::fmt;
use std::result;

#[derive(Debug)]
pub struct Error {
    inner: String,
}

pub type Result<T> = result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        &self.inner
    }
}

impl From<String> for Error {
    fn from(err: String) -> Error {
        Error {
            inner: err,
        }
    }
}
