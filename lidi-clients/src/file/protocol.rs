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
    FilePathTooLong,
    PathNameTooLong,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::StringFormatError(e) => write!(fmt, "string format error: {e}"),
            Self::InvalidFileSize(s1, s2) => write!(fmt, "invalid file size: {s1} != {s2}"),
            Self::InvalidHash(h1, h2) => write!(fmt, "invalid hash: {h1:x} != {h2:x}"),
            Self::FilePathTooLong => write!(fmt, "file path too long"),
            Self::PathNameTooLong => write!(fmt, "path name too long"),
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

pub(crate) struct Header {
    pub(crate) file_path: Vec<String>,
    pub(crate) mode: u32,
    pub(crate) file_length: u64,
}

impl Header {
    pub(crate) fn serialize_to<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        let file_path_len =
            u16::try_from(self.file_path.len()).map_err(|_| Error::FilePathTooLong)?;
        w.write_all(&file_path_len.to_le_bytes())?;

        for path in &self.file_path {
            let bytes = path.as_bytes();
            let path_len = u16::try_from(bytes.len()).map_err(|_| Error::PathNameTooLong)?;
            w.write_all(&path_len.to_le_bytes())?;
            w.write_all(bytes)?;
        }

        w.write_all(&self.mode.to_le_bytes())?;
        w.write_all(&self.file_length.to_le_bytes())?;

        Ok(())
    }

    pub(crate) fn deserialize_from<R: Read>(r: &mut R) -> Result<Self, Error> {
        let mut file_path_len = [0u8; 2];
        r.read_exact(&mut file_path_len)?;
        let file_path_len = u16::from_le_bytes(file_path_len);

        let mut file_path = Vec::new();

        for _ in 0..file_path_len {
            let mut path_len = [0u8; 2];
            r.read_exact(&mut path_len)?;
            let path_len = u16::from_le_bytes(path_len);

            let mut file_name = vec![0; usize::from(path_len)];
            r.read_exact(&mut file_name)?;
            let file_name = String::from_utf8(file_name)?;

            file_path.push(file_name);
        }

        let mut mode = [0u8; 4];
        r.read_exact(&mut mode)?;
        let mode = u32::from_le_bytes(mode);

        let mut file_length = [0u8; 8];
        r.read_exact(&mut file_length)?;
        let file_length = u64::from_le_bytes(file_length);

        Ok(Self {
            file_path,
            mode,
            file_length,
        })
    }
}

pub(crate) struct Footer {
    pub(crate) hash: u128,
}

impl Footer {
    pub fn serialize_to<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        w.write_all(&self.hash.to_le_bytes())?;
        Ok(())
    }

    #[cfg(feature = "hash")]
    pub fn deserialize_from<R: Read>(r: &mut R) -> Result<Self, Error> {
        let mut hash = [0u8; 16];
        r.read_exact(&mut hash)?;
        let hash = u128::from_le_bytes(hash);

        Ok(Self { hash })
    }
}
