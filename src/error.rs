use thiserror::Error;

#[derive(Debug, Error)]
pub enum SerialError {
    #[error("open failed: {0}")]
    Open(String),
    #[error("read failed: {0}")]
    Read(String),
    #[error("write failed: {0}")]
    Write(String),
    #[error("disconnected")]
    Disconnected,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("config serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("log io error: {0}")]
    Io(#[from] std::io::Error),
}
