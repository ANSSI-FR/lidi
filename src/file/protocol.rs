use std::{
    fmt, io,
    io::{Read, Write},
    string::FromUtf8Error,
};

pub enum Error {
    Io(io::Error),
    StringFormatError(FromUtf8Error),
    InvalidFileSize(usize, usize),
    InvalidHash(u128, u128),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::StringFormatError(e) => write!(fmt, "string format error: {e}"),
            Self::InvalidFileSize(s1, s2) => write!(fmt, "invalid file size: {s1} != {s2}"),
            Self::InvalidHash(h1, h2) => write!(fmt, "invalid hash: {h1:x} != {h2:x}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(e: FromUtf8Error) -> Self {
        Self::StringFormatError(e)
    }
}

pub struct Header {
    pub file_name: String,
    pub mode: u32,
    pub file_length: u64,
}

impl Header {
    pub fn serialize_to<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        w.write_all(&self.file_name.len().to_le_bytes())?;
        w.write_all(self.file_name.as_bytes())?;
        w.write_all(&self.mode.to_le_bytes())?;
        w.write_all(&self.file_length.to_le_bytes())?;
        Ok(())
    }

    pub fn deserialize_from<R: Read>(r: &mut R) -> Result<Self, Error> {
        let mut file_name_len = [0u8; 8];

        log::trace!("read header");
        r.read_exact(&mut file_name_len)?;
        let file_name_len = usize::from_le_bytes(file_name_len);

        if file_name_len > 1024 {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid name length",
            )));
        }

        let mut file_name = vec![0; file_name_len];
        r.read_exact(&mut file_name)?;
        let file_name = String::from_utf8(file_name)?;

        let mut mode = [0u8; 4];
        r.read_exact(&mut mode)?;
        let mode = u32::from_le_bytes(mode);

        let mut file_length = [0u8; 8];
        r.read_exact(&mut file_length)?;
        let file_length = u64::from_le_bytes(file_length);

        Ok(Self {
            file_name,
            mode,
            file_length,
        })
    }
}

pub struct Footer {
    pub hash: u128,
    pub stream_end: bool,
}

impl Footer {
    pub fn serialize_to<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        w.write_all(&self.hash.to_le_bytes())?;
        let mut stream_end = [0u8; 1];
        if self.stream_end {
            stream_end[0] = 1;
        } else {
            stream_end[0] = 0;
        }
        w.write_all(&stream_end)?;

        Ok(())
    }

    pub fn deserialize_from<R: Read>(r: &mut R) -> Result<Self, Error> {
        let mut hash = [0u8; 16];
        r.read_exact(&mut hash)?;
        let hash = u128::from_le_bytes(hash);

        let mut stream_end = [0u8; 1];
        r.read_exact(&mut stream_end)?;

        Ok(Self {
            hash,
            stream_end: stream_end[0] == 1,
        })
    }
}
