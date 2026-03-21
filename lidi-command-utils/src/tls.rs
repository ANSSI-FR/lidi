use std::{fmt, io, net, os};

pub enum Error {
    Config(String),
    Io(io::Error),
    OpensslErrorStack(openssl::error::ErrorStack),
    OpensslHandshakeError(openssl::ssl::HandshakeError<net::TcpStream>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Config(e) => write!(fmt, "config error: {e}"),
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::OpensslErrorStack(e) => write!(fmt, "OpenSSL error: {e}"),
            Self::OpensslHandshakeError(e) => write!(fmt, "OpenSSL handshake error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<openssl::error::ErrorStack> for Error {
    fn from(e: openssl::error::ErrorStack) -> Self {
        Self::OpensslErrorStack(e)
    }
}

impl From<openssl::ssl::HandshakeError<net::TcpStream>> for Error {
    fn from(e: openssl::ssl::HandshakeError<net::TcpStream>) -> Self {
        Self::OpensslHandshakeError(e)
    }
}

pub struct ClientContext(openssl::ssl::SslContext);

impl TryFrom<&crate::config::TlsConfig> for ClientContext {
    type Error = Error;

    fn try_from(config: &crate::config::TlsConfig) -> Result<Self, Self::Error> {
        let method = openssl::ssl::SslMethod::tls_client();

        let mut builder = openssl::ssl::SslContextBuilder::new(method)?;

        let tls_min = match config.tls_min.unwrap_or_default() {
            crate::config::TlsVersion::Tls1_1 => openssl::ssl::SslVersion::TLS1_1,
            crate::config::TlsVersion::Tls1_2 => openssl::ssl::SslVersion::TLS1_2,
            crate::config::TlsVersion::Tls1_3 => openssl::ssl::SslVersion::TLS1_3,
        };
        builder.set_min_proto_version(Some(tls_min))?;

        if let Some(ciphers) = &config.ciphers {
            builder.set_ciphersuites(ciphers)?;
        }

        if let Some(groups) = &config.groups {
            builder.set_groups_list(groups)?;
        }

        let options = builder.options();
        let options = options.union(openssl::ssl::SslOptions::NO_COMPRESSION);
        let options = options.union(openssl::ssl::SslOptions::CIPHER_SERVER_PREFERENCE);
        builder.set_options(options);

        if let Some(certificate) = &config.certificate {
            builder.set_certificate_chain_file(certificate)?;
        }

        if let Some(private_key) = &config.key {
            builder.set_private_key_file(private_key, openssl::ssl::SslFiletype::PEM)?;
            builder.check_private_key()?;
        }

        if let Some(ca) = &config.ca {
            builder.set_ca_file(ca)?;
            builder.set_verify(
                openssl::ssl::SslVerifyMode::PEER
                    | openssl::ssl::SslVerifyMode::FAIL_IF_NO_PEER_CERT,
            );
        }

        let tls = builder.build();

        Ok(Self(tls))
    }
}

pub struct TcpStream(openssl::ssl::SslStream<net::TcpStream>);

impl TcpStream {
    pub fn connect(context: &ClientContext, sockaddr: &net::SocketAddr) -> Result<Self, Error> {
        let stream = net::TcpStream::connect(sockaddr)?;
        let tls = openssl::ssl::Ssl::new(&context.0)?;
        let stream = tls.connect(stream)?;

        Ok(Self(stream))
    }
}

impl io::Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.0.read(buf)
    }
}

impl io::Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.0.flush()
    }
}

impl os::fd::AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> i32 {
        self.0.get_ref().as_raw_fd()
    }
}

pub struct TcpListener {
    listener: net::TcpListener,
    tls: openssl::ssl::SslAcceptor,
}

impl TcpListener {
    pub fn bind(
        config: &crate::config::TlsConfig,
        sockaddr: &net::SocketAddr,
    ) -> Result<Self, Error> {
        let method = openssl::ssl::SslMethod::tls_server();

        let mut builder = match config.tls_method.unwrap_or_default() {
            crate::config::TlsMethod::Mozilla_Intermediate_v4 => {
                openssl::ssl::SslAcceptor::mozilla_intermediate(method)?
            }
            crate::config::TlsMethod::Mozilla_Intermediate_v5 => {
                openssl::ssl::SslAcceptor::mozilla_intermediate_v5(method)?
            }
            crate::config::TlsMethod::Mozilla_Modern_v4 => {
                openssl::ssl::SslAcceptor::mozilla_modern(method)?
            }
            crate::config::TlsMethod::Mozilla_Modern_v5 => {
                openssl::ssl::SslAcceptor::mozilla_modern_v5(method)?
            }
        };

        let tls_min = match config.tls_min.unwrap_or_default() {
            crate::config::TlsVersion::Tls1_1 => openssl::ssl::SslVersion::TLS1_1,
            crate::config::TlsVersion::Tls1_2 => openssl::ssl::SslVersion::TLS1_2,
            crate::config::TlsVersion::Tls1_3 => openssl::ssl::SslVersion::TLS1_3,
        };
        builder.set_min_proto_version(Some(tls_min))?;

        if let Some(ciphers) = &config.ciphers {
            builder.set_ciphersuites(ciphers)?;
        }

        if let Some(groups) = &config.groups {
            builder.set_groups_list(groups)?;
        }

        let options = builder.options();
        let options = options.union(openssl::ssl::SslOptions::NO_COMPRESSION);
        let options = options.union(openssl::ssl::SslOptions::CIPHER_SERVER_PREFERENCE);
        builder.set_options(options);

        if let Some(certificate) = &config.certificate {
            builder.set_certificate_chain_file(certificate)?;
        }

        if let Some(private_key) = &config.key {
            builder.set_private_key_file(private_key, openssl::ssl::SslFiletype::PEM)?;
            builder.check_private_key()?;
        }

        if let Some(ca) = &config.ca {
            builder.set_ca_file(ca)?;
            builder.set_verify(
                openssl::ssl::SslVerifyMode::PEER
                    | openssl::ssl::SslVerifyMode::FAIL_IF_NO_PEER_CERT,
            );
        }

        let listener = net::TcpListener::bind(sockaddr)?;
        let tls = builder.build();

        Ok(Self { listener, tls })
    }

    pub fn accept(&self) -> Result<Result<(TcpStream, net::SocketAddr), Error>, Error> {
        let (stream, client_addr) = self.listener.accept()?;
        Ok(self
            .tls
            .accept(stream)
            .map(|stream| (TcpStream(stream), client_addr))
            .map_err(Error::OpensslHandshakeError))
    }
}

pub(crate) fn init() {
    log::debug!("{}", openssl::version::version());
    openssl::init();
}
