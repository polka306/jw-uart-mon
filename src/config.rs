use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::ConfigError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Parity { None, Odd, Even }
impl Default for Parity { fn default() -> Self { Parity::None } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl { None, Software, Hardware }
impl Default for FlowControl { fn default() -> Self { FlowControl::None } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding { None, Cr, Lf, Crlf }
impl Default for LineEnding { fn default() -> Self { LineEnding::Lf } }

impl LineEnding {
    pub fn bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::None => b"",
            LineEnding::Cr => b"\r",
            LineEnding::Lf => b"\n",
            LineEnding::Crlf => b"\r\n",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SerialConfig {
    #[serde(default)]
    pub port: Option<String>,
    pub baud: u32,
    pub data_bits: u8,
    #[serde(default)]
    pub parity: Parity,
    pub stop_bits: u8,
    #[serde(default)]
    pub flow: FlowControl,
    #[serde(default)]
    pub line_ending: LineEnding,
}
impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: None,
            baud: 115200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow: FlowControl::None,
            line_ending: LineEnding::Lf,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    pub hex: bool,
    pub timestamps: bool,
    pub ring_capacity: usize,
}
impl Default for UiConfig {
    fn default() -> Self { Self { hex: false, timestamps: true, ring_capacity: 10000 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Macro {
    pub slot: u8,
    pub name: String,
    pub payload: String,
    #[serde(default)]
    pub hex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    #[serde(default)]
    pub serial: SerialConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub macros: Vec<Macro>,
}

impl Config {
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        if !path.exists() { return Ok(Self::default()); }
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub fn default_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "uart-mon")
        .map(|p| p.config_dir().join("config.toml"))
}
pub fn default_log_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "uart-mon")
        .map(|p| p.data_local_dir().join("logs"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_default() {
        let c = Config::default();
        let s = toml::to_string(&c).unwrap();
        let parsed: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, parsed);
    }
    #[test]
    fn missing_fields_use_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.ui.ring_capacity, 10000);
        assert_eq!(c.serial.baud, 115200);
    }
    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.toml");
        let mut c = Config::default();
        c.macros.push(Macro { slot: 1, name: "v".into(), payload: "AT\r\n".into(), hex: false });
        c.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(c, loaded);
    }
    #[test]
    fn line_ending_bytes() {
        assert_eq!(LineEnding::Lf.bytes(), b"\n");
        assert_eq!(LineEnding::Crlf.bytes(), b"\r\n");
    }
}
