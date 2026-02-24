use std::fmt;

use blueberry_serde::{deserialize_message, empty_message, serialize_packet, MessageHeader};
use derive_more::Debug;
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Module {
    Blueberry,
    Test,
    Unknown(u16),
}

impl Module {
    pub fn as_u16(self) -> u16 {
        match self {
            Self::Blueberry => 0x4242,
            Self::Test => 0x4243,
            Self::Unknown(v) => v,
        }
    }

    pub fn from_u16(v: u16) -> Self {
        match v {
            0x4242 => Self::Blueberry,
            0x4243 => Self::Test,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKey {
    Id,
    AppData,
    Version,
    WhoseThere,
    Test,
    Unknown(u16),
}

impl MessageKey {
    pub fn as_u16(self) -> u16 {
        match self {
            Self::Id => 0x0000,
            Self::AppData => 0x0001,
            Self::Version => 0x0002,
            Self::WhoseThere => 0x0003,
            Self::Test => 0x0000,
            Self::Unknown(v) => v,
        }
    }

    pub fn from_u16(module: u16, key: u16) -> Self {
        match (Module::from_u16(module), key) {
            (Module::Blueberry, 0x0000) => Self::Id,
            (Module::Blueberry, 0x0001) => Self::AppData,
            (Module::Blueberry, 0x0002) => Self::Version,
            (Module::Blueberry, 0x0003) => Self::WhoseThere,
            (Module::Test, 0x0000) => Self::Test,
            (_, k) => Self::Unknown(k),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdFields {
    pub id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDataFields {
    pub floats: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionFields {
    pub firmware_version: u32,
    pub hardware_rev: u8,
    pub mcu_type: u8,
    pub hardware_type: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFields {
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HwType {
    Sfdq,
    BlueServo,
    Lumen,
    Nucleo,
    BlueEsc,
    Gigaboard,
    BlueBridge,
    Undefined,
    Unknown(u16),
}

impl From<u16> for HwType {
    fn from(v: u16) -> Self {
        match v {
            0x0000 => Self::Sfdq,
            0x0001 => Self::BlueServo,
            0x0002 => Self::Lumen,
            0x0003 => Self::Nucleo,
            0x0004 => Self::BlueEsc,
            0x0005 => Self::Gigaboard,
            0x0006 => Self::BlueBridge,
            0xFFFF => Self::Undefined,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for HwType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(v) => write!(f, "Unknown(0x{v:04X})"),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McuType {
    Stm32F446,
    Stm32H563,
    Stm32H573,
    Stm32G071,
    Undefined,
    Unknown(u8),
}

impl From<u8> for McuType {
    fn from(v: u8) -> Self {
        match v {
            0x01 => Self::Stm32F446,
            0x02 => Self::Stm32H563,
            0x03 => Self::Stm32H573,
            0x04 => Self::Stm32G071,
            0xFF => Self::Undefined,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for McuType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stm32F446 => write!(f, "STM32F446"),
            Self::Stm32H563 => write!(f, "STM32H563"),
            Self::Stm32H573 => write!(f, "STM32H573"),
            Self::Stm32G071 => write!(f, "STM32G071"),
            Self::Undefined => write!(f, "Undefined"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Id {
        #[debug("{:#010x}", id)]
        id: u32,
    },
    AppData {
        floats: Vec<f32>,
    },
    Version {
        #[debug("{:#010x}", firmware_version)]
        firmware_version: u32,
        #[debug("{:#010x}", hardware_rev)]
        hardware_rev: u8,
        hardware_type: HwType,
        mcu_type: McuType,
    },
    WhoseThere,
    Test {
        filename: String,
    },
    Unknown {
        #[debug("{:#010x}", module)]
        module: u16,
        #[debug("{:#010x}", key)]
        key: u16,
        body: Vec<u8>,
    },
}

impl Message {
    /// Parse a `Message` from raw message bytes using `blueberry_serde::deserialize_message`.
    pub fn from_raw(raw: &[u8]) -> Self {
        let Some(hdr) = MessageHeader::decode(raw) else {
            return Self::Unknown {
                module: 0,
                key: 0,
                body: raw.to_vec(),
            };
        };

        let mk = MessageKey::from_u16(hdr.module_key, hdr.message_key);
        let fallback = || Self::Unknown {
            module: hdr.module_key,
            key: hdr.message_key,
            body: raw[8..].to_vec(),
        };

        match mk {
            MessageKey::Id => match deserialize_message::<IdFields>(raw) {
                Ok((_, fields)) => Self::Id { id: fields.id },
                Err(e) => {
                    warn!("Failed to deserialize ID message: {e}");
                    fallback()
                }
            },
            MessageKey::AppData => match deserialize_message::<AppDataFields>(raw) {
                Ok((_, fields)) => Self::AppData {
                    floats: fields.floats,
                },
                Err(e) => {
                    warn!("Failed to deserialize APP_DATA message: {e}");
                    fallback()
                }
            },
            MessageKey::Version => match deserialize_message::<VersionFields>(raw) {
                Ok((_, fields)) => Self::Version {
                    firmware_version: fields.firmware_version,
                    hardware_rev: fields.hardware_rev,
                    hardware_type: HwType::from(fields.hardware_type),
                    mcu_type: McuType::from(fields.mcu_type),
                },
                Err(e) => {
                    warn!("Failed to deserialize VERSION message: {e}");
                    fallback()
                }
            },
            MessageKey::WhoseThere => Self::WhoseThere,
            MessageKey::Test => match deserialize_message::<TestFields>(raw) {
                Ok((_, fields)) => Self::Test {
                    filename: fields.filename,
                },
                Err(e) => {
                    warn!("Failed to deserialize TEST message: {e}");
                    fallback()
                }
            },
            _ => fallback(),
        }
    }

    pub fn request_packet(
        module: Module,
        key: MessageKey,
    ) -> Result<Vec<u8>, blueberry_serde::Error> {
        serialize_packet(&[&empty_message(module.as_u16(), key.as_u16())])
    }
}
