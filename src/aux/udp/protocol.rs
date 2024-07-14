use std::{
    fmt, io,
    io::{Read, Write},
};

pub enum Error {
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub(crate) struct Header {
    pub(crate) size: usize,
}

impl Header {
    pub fn serialize_to<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        w.write_all(&self.size.to_le_bytes())?;
        Ok(())
    }

    pub fn deserialize_from<R: Read>(r: &mut R) -> Result<Self, Error> {
        let mut size = [0u8; 8];
        r.read_exact(&mut size)?;
        let size = usize::from_le_bytes(size);

        Ok(Self { size })
    }
}
