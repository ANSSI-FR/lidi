// SPDX-License-Identifier: LGPL-3.0

use std::borrow::Cow;

use std::error::Error as StdError;
use std::result::Result as StdResult;

pub fn cast_result<'a, C, T, E>(res: StdResult<T, E>, message: C) -> Result<T>
where
    C: Into<Cow<'a, str>>,
{
    res.map_err(|_| Error::CustomError(message.into().to_string()))
}

pub fn cast_option<'a, C, T>(res: Option<T>, message: C) -> Result<T>
where
    C: Into<Cow<'a, str>>,
{
    res.ok_or(Error::CustomError(message.into().to_string()))
}

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug)]
pub enum Error {
    CustomError(String),
    IoError(std::io::Error),
    CookieFactoryError(cookie_factory::GenError),
    UnixError(nix::Error),
    BincodeError(bincode::Error),
}

impl StdError for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "Diode I/O error: {}", e),
            Self::CookieFactoryError(e) => write!(f, "Diode serialization error: {}", e),
            Self::UnixError(e) => write!(f, "Diode unix error: {}", e),
            Self::BincodeError(e) => write!(f, "Diode bincode error: {}", e),
            Self::CustomError(s) => write!(f, "Diode custom error: {}", s),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<cookie_factory::GenError> for Error {
    fn from(e: cookie_factory::GenError) -> Self {
        Self::CookieFactoryError(e)
    }
}

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        Self::UnixError(e)
    }
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Self {
        Self::BincodeError(e)
    }
}