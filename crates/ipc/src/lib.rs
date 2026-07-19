//! Versioned, bounded IPC envelopes and transport-independent framing.

use std::{
    error::Error,
    fmt,
    io::{self, Read, Write},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const PROTOCOL_MAJOR: u16 = 1;
pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const CURRENT: Self = Self {
        major: PROTOCOL_MAJOR,
        minor: PROTOCOL_MINOR,
    };

    #[must_use]
    pub const fn accepts(self, other: Self) -> bool {
        self.major == other.major && other.minor <= self.minor
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Request,
    Response,
    Event,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl RemoteError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }

    #[must_use]
    pub const fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub version: ProtocolVersion,
    pub kind: MessageKind,
    pub request_id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RemoteError>,
}

impl<T> Envelope<T> {
    #[must_use]
    pub fn request(request_id: RequestId, payload: T) -> Self {
        Self {
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Request,
            request_id,
            payload: Some(payload),
            error: None,
        }
    }

    #[must_use]
    pub fn response(request_id: RequestId, payload: T) -> Self {
        Self {
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Response,
            request_id,
            payload: Some(payload),
            error: None,
        }
    }

    #[must_use]
    pub fn event(request_id: RequestId, payload: T) -> Self {
        Self {
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Event,
            request_id,
            payload: Some(payload),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(request_id: RequestId, error: RemoteError) -> Self {
        Self {
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Response,
            request_id,
            payload: None,
            error: Some(error),
        }
    }

    fn validate(&self) -> Result<(), IpcError> {
        if !ProtocolVersion::CURRENT.accepts(self.version) {
            return Err(IpcError::IncompatibleVersion {
                local: ProtocolVersion::CURRENT,
                remote: self.version,
            });
        }
        match (&self.payload, &self.error) {
            (Some(_), None) => Ok(()),
            (None, Some(_)) if self.kind == MessageKind::Response => Ok(()),
            _ => Err(IpcError::InvalidEnvelope),
        }
    }
}

pub fn encode_envelope<T: Serialize>(message: &Envelope<T>) -> Result<Vec<u8>, IpcError> {
    message.validate()?;
    let encoded = serde_json::to_vec(message).map_err(IpcError::Encode)?;
    if encoded.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge {
            actual: encoded.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    Ok(encoded)
}

pub fn decode_envelope<T: DeserializeOwned>(frame: &[u8]) -> Result<Envelope<T>, IpcError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge {
            actual: frame.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    let message = serde_json::from_slice::<Envelope<T>>(frame).map_err(IpcError::Decode)?;
    message.validate()?;
    Ok(message)
}

pub trait FrameTransport {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), IpcError>;
    fn receive_frame(&mut self) -> Result<Vec<u8>, IpcError>;
}

#[derive(Debug)]
pub struct StreamTransport<S> {
    stream: S,
}

impl<S> StreamTransport<S> {
    #[must_use]
    pub const fn new(stream: S) -> Self {
        Self { stream }
    }

    #[must_use]
    pub const fn stream(&self) -> &S {
        &self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: Read + Write> FrameTransport for StreamTransport<S> {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), IpcError> {
        if frame.len() > MAX_FRAME_BYTES {
            return Err(IpcError::FrameTooLarge {
                actual: frame.len(),
                limit: MAX_FRAME_BYTES,
            });
        }
        let length = u32::try_from(frame.len()).map_err(|_| IpcError::FrameTooLarge {
            actual: frame.len(),
            limit: MAX_FRAME_BYTES,
        })?;
        self.stream.write_all(&length.to_be_bytes())?;
        self.stream.write_all(frame)?;
        self.stream.flush()?;
        Ok(())
    }

    fn receive_frame(&mut self) -> Result<Vec<u8>, IpcError> {
        let mut prefix = [0_u8; 4];
        self.stream.read_exact(&mut prefix)?;
        let length = u32::from_be_bytes(prefix) as usize;
        if length > MAX_FRAME_BYTES {
            return Err(IpcError::FrameTooLarge {
                actual: length,
                limit: MAX_FRAME_BYTES,
            });
        }
        let mut frame = vec![0_u8; length];
        self.stream.read_exact(&mut frame)?;
        Ok(frame)
    }
}

#[derive(Debug)]
pub struct Connection<T> {
    transport: T,
}

impl<T: FrameTransport> Connection<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn send<M: Serialize>(&mut self, message: &Envelope<M>) -> Result<(), IpcError> {
        self.transport.send_frame(&encode_envelope(message)?)
    }

    pub fn receive<M: DeserializeOwned>(&mut self) -> Result<Envelope<M>, IpcError> {
        decode_envelope(&self.transport.receive_frame()?)
    }

    pub fn into_inner(self) -> T {
        self.transport
    }
}

#[derive(Debug)]
pub enum IpcError {
    Io(io::Error),
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    FrameTooLarge {
        actual: usize,
        limit: usize,
    },
    IncompatibleVersion {
        local: ProtocolVersion,
        remote: ProtocolVersion,
    },
    InvalidEnvelope,
    UnexpectedMessage {
        expected: MessageKind,
        actual: MessageKind,
    },
    RequestIdMismatch {
        expected: RequestId,
        actual: RequestId,
    },
}

impl fmt::Display for IpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IPC transport failed: {error}"),
            Self::Encode(error) => write!(formatter, "IPC encoding failed: {error}"),
            Self::Decode(error) => write!(formatter, "IPC decoding failed: {error}"),
            Self::FrameTooLarge { actual, limit } => {
                write!(formatter, "IPC frame is {actual} bytes, limit is {limit}")
            }
            Self::IncompatibleVersion { local, remote } => write!(
                formatter,
                "incompatible IPC protocol local {}.{}, remote {}.{}",
                local.major, local.minor, remote.major, remote.minor
            ),
            Self::InvalidEnvelope => {
                formatter.write_str("invalid IPC envelope payload/error shape")
            }
            Self::UnexpectedMessage { expected, actual } => write!(
                formatter,
                "expected {expected:?} IPC message, got {actual:?}"
            ),
            Self::RequestIdMismatch { expected, actual } => write!(
                formatter,
                "IPC request ID mismatch: expected {}, got {}",
                expected.0, actual.0
            ),
        }
    }
}

impl Error for IpcError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Encode(error) | Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for IpcError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Ping {
        value: String,
    }

    #[test]
    fn request_round_trip_is_stable() {
        let message = Envelope::request(
            RequestId(42),
            Ping {
                value: "meow".to_owned(),
            },
        );
        let decoded = decode_envelope::<Ping>(&encode_envelope(&message).unwrap()).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn compatibility_accepts_older_minor_and_rejects_major_or_future_minor() {
        assert!(ProtocolVersion::CURRENT.accepts(ProtocolVersion { major: 1, minor: 0 }));
        assert!(!ProtocolVersion::CURRENT.accepts(ProtocolVersion { major: 2, minor: 0 }));
        assert!(!ProtocolVersion::CURRENT.accepts(ProtocolVersion { major: 1, minor: 1 }));
    }

    #[test]
    fn stream_transport_uses_bounded_big_endian_frames() {
        let mut transport = StreamTransport::new(io::Cursor::new(Vec::<u8>::new()));
        transport.send_frame(b"cat").unwrap();
        let bytes = transport.into_inner().into_inner();
        assert_eq!(&bytes[..4], &3_u32.to_be_bytes());
        assert_eq!(&bytes[4..], b"cat");
    }

    #[test]
    fn decoder_fuzz_corpus_never_panics() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for length in 0..2048_usize {
            let mut input = vec![0_u8; length];
            for byte in &mut input {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            let result = std::panic::catch_unwind(|| decode_envelope::<serde_json::Value>(&input));
            assert!(result.is_ok(), "decoder panicked for {length}-byte input");
        }
    }

    #[test]
    fn declared_oversized_frame_is_rejected_before_allocation() {
        let mut bytes = io::Cursor::new(((MAX_FRAME_BYTES as u32) + 1).to_be_bytes().to_vec());
        let error = StreamTransport::new(&mut bytes)
            .receive_frame()
            .unwrap_err();
        assert!(matches!(error, IpcError::FrameTooLarge { .. }));
    }
}
