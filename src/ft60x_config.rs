use crate::Result;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write};

#[derive(Debug)]
pub struct FT60xConfig {
    vid: u16,
    pid: u16,
    manufacturer: String,
    product_description: String,
    serial_number: String,
    power_attributes: u8,
    power_consumption: u16,
    pub fifo_clock: FT60xFifoClock,
    pub fifo_mode: FT60xFifoMode,
    pub channel_config: FT60xChannelConfig,
    optional_features_support: u16,
    battery_charging_gpio_config: u8,
    flash_eeprom_detection: ft60x_flash_rom_detection::FT60xFlashRomDetection,
    msio_config: u32,
    gpio_config: u32,
    reserved1: u8,
    reserved2: u8,
}

impl FT60xConfig {
    pub fn parse(bytes: [u8; 152]) -> Result<FT60xConfig> {
        let mut data = Cursor::new(&bytes[..]);

        let vid = data.read_u16::<LittleEndian>()?;
        let pid = data.read_u16::<LittleEndian>()?;

        let mut strings_buf = [0u8; 128];
        ensure!(data.read(&mut strings_buf)? == 128);

        fn parse_string(bytes: &[u8]) -> Result<(String, u8)> {
            let offset = bytes[0];
            let length = (offset - 2) >> 1;

            ensure!(bytes[1] == 0x3);

            let mut res = String::new();

            for i in 0..(length as usize) {
                res += std::str::from_utf8(&[bytes[2 * i + 2]])?;
                ensure!(bytes[2 * i + 2 + 1] == 0);
            }

            Ok((res, offset))
        }

        let mut offset = 0usize;
        let (manufacturer, new_offset) = parse_string(&strings_buf[offset..])?;
        offset += new_offset as usize;
        let (product_description, new_offset) = parse_string(&strings_buf[offset..])?;
        offset += new_offset as usize;
        let (serial_number, _) = parse_string(&strings_buf[offset..])?;

        let reserved1 = data.read_u8()?;

        let power_attributes = data.read_u8()?;
        let power_consumption = data.read_u16::<LittleEndian>()?;

        let reserved2 = data.read_u8()?;

        let fifo_clock = FT60xFifoClock::parse(data.read_u8()?)?;
        let fifo_mode = FT60xFifoMode::parse(data.read_u8()?)?;
        let channel_config = FT60xChannelConfig::parse(data.read_u8()?)?;

        let optional_features_support = data.read_u16::<LittleEndian>()?;
        let battery_charging_gpio_config = data.read_u8()?;
        let flash_eeprom_detection = data.read_u8()?;
        let flash_eeprom_detection =
            ft60x_flash_rom_detection::FT60xFlashRomDetection::parse(flash_eeprom_detection)?;

        let msio_config = data.read_u32::<LittleEndian>()?;
        let gpio_config = data.read_u32::<LittleEndian>()?;

        Ok(FT60xConfig {
            vid,
            pid,
            manufacturer,
            product_description,
            serial_number,
            power_attributes,
            power_consumption,
            fifo_clock,
            fifo_mode,
            channel_config,
            optional_features_support,
            battery_charging_gpio_config,
            flash_eeprom_detection,
            msio_config,
            gpio_config,
            reserved1,
            reserved2,
        })
    }

    pub fn encode(&self) -> Result<[u8; 152]> {
        let mut buf = [0; 152];
        let mut cursor = Cursor::new(&mut buf[..]);

        cursor.write_u16::<LittleEndian>(self.vid)?;
        cursor.write_u16::<LittleEndian>(self.pid)?;

        let mut strings_buf = [0u8; 128];
        let mut strings_cursor = Cursor::new(&mut strings_buf[..]);

        fn encode_string(string: &str, cursor: &mut Cursor<&mut [u8]>) -> Result<()> {
            let length = string.len();
            let offset = (length + 1) << 1;

            cursor.write_u8(offset as u8)?;
            cursor.write_u8(0x3)?;

            for i in 0..length {
                cursor.write_u8(string.as_bytes()[i] as u8)?;
                cursor.write_u8(0x0)?;
            }

            Ok(())
        }

        encode_string(&self.manufacturer, &mut strings_cursor)?;
        encode_string(&self.product_description, &mut strings_cursor)?;
        encode_string(&self.serial_number, &mut strings_cursor)?;

        cursor.write_all(&strings_buf)?;

        cursor.write_u8(self.reserved1)?;

        cursor.write_u8(self.power_attributes)?;
        cursor.write_u16::<LittleEndian>(self.power_consumption)?;

        cursor.write_u8(self.reserved2)?;

        cursor.write_u8(self.fifo_clock.encode())?;
        cursor.write_u8(self.fifo_mode.encode())?;
        cursor.write_u8(self.channel_config.encode())?;
        cursor.write_u16::<LittleEndian>(self.optional_features_support)?;
        cursor.write_u8(self.battery_charging_gpio_config)?;
        cursor.write_u8(self.flash_eeprom_detection.encode())?;

        cursor.write_u32::<LittleEndian>(self.msio_config)?;
        cursor.write_u32::<LittleEndian>(self.gpio_config)?;

        Ok(buf)
    }
}

#[derive(Debug)]
pub enum FT60xFifoMode {
    Mode245,
    Mode600,
}

impl FT60xFifoMode {
    fn parse(num: u8) -> Result<Self> {
        match num {
            0 => Ok(Self::Mode245),
            1 => Ok(Self::Mode600),
            _ => Err(format_general_err!(
                "Unknown fifo mode configuration {}",
                num
            )),
        }
    }

    fn encode(&self) -> u8 {
        match self {
            Self::Mode245 => 0,
            Self::Mode600 => 1,
        }
    }
}

#[derive(Debug)]
pub enum FT60xFifoClock {
    Clock100MHz,
    Clock66MHz,
    Clock50MHz,
    Clock40MHz,
}

impl FT60xFifoClock {
    fn parse(num: u8) -> Result<Self> {
        match num {
            0 => Ok(Self::Clock100MHz),
            1 => Ok(Self::Clock66MHz),
            2 => Ok(Self::Clock50MHz),
            3 => Ok(Self::Clock40MHz),
            _ => Err(format_general_err!(
                "Unknown fifo clock configuration {}",
                num
            )),
        }
    }

    fn encode(&self) -> u8 {
        match self {
            Self::Clock100MHz => 0,
            Self::Clock66MHz => 1,
            Self::Clock50MHz => 2,
            Self::Clock40MHz => 3,
        }
    }
}

#[derive(Debug)]
pub enum FT60xChannelConfig {
    FourChannels,
    TwoChannels,
    OneChannel,
    OneChannelOutPipe,
    OneChannelInPipe,
}

impl FT60xChannelConfig {
    fn parse(num: u8) -> Result<Self> {
        match num {
            0 => Ok(Self::FourChannels),
            1 => Ok(Self::TwoChannels),
            2 => Ok(Self::OneChannel),
            3 => Ok(Self::OneChannelOutPipe),
            4 => Ok(Self::OneChannelInPipe),
            _ => Err(format_general_err!("Unknown channel configuration {}", num)),
        }
    }

    fn encode(&self) -> u8 {
        match self {
            Self::FourChannels => 0,
            Self::TwoChannels => 1,
            Self::OneChannel => 2,
            Self::OneChannelOutPipe => 3,
            Self::OneChannelInPipe => 4,
        }
    }
}

pub mod ft60x_flash_rom_detection {
    use crate::Result;

    #[derive(Debug)]
    pub enum MemoryType {
        Flash,
        ROM,
    }

    #[derive(Debug)]
    pub enum MemoryStatus {
        Exists,
        ExistsNot,
    }

    #[derive(Debug)]
    pub enum CustomConfigValidity {
        Valid,
        Invalid,
    }

    #[derive(Debug)]
    pub enum CustomConfigChecksum {
        Valid,
        Invalid,
    }

    #[derive(Debug)]
    pub enum GPIOInput {
        Ignore,
        Used,
    }

    #[derive(Debug)]
    pub enum ConfigUsed {
        Default,
        Custom,
    }

    #[derive(Debug)]
    pub enum GPIO0 {
        Low,
        High,
    }

    #[derive(Debug)]
    pub enum GPIO1 {
        Low,
        High,
    }

    #[derive(Debug)]
    pub struct FT60xFlashRomDetection {
        memory_type: MemoryType,
        memory_status: MemoryStatus,
        custom_config_validity: CustomConfigValidity,
        custom_config_checksum: CustomConfigChecksum,
        gpio_input: GPIOInput,
        config_used: ConfigUsed,
        gpio0: GPIO0,
        gpio1: GPIO1,
    }

    impl FT60xFlashRomDetection {
        pub fn parse(flags: u8) -> Result<FT60xFlashRomDetection> {
            let memory_type = match flags & (1 << 0) {
                0 => MemoryType::Flash,
                _ => MemoryType::ROM,
            };

            let memory_status = match flags & (1 << 1) {
                0 => MemoryStatus::Exists,
                _ => MemoryStatus::ExistsNot,
            };

            let custom_config_validity = match flags & (1 << 2) {
                0 => CustomConfigValidity::Valid,
                _ => CustomConfigValidity::Invalid,
            };

            let custom_config_checksum = match flags & (1 << 3) {
                0 => CustomConfigChecksum::Valid,
                _ => CustomConfigChecksum::Invalid,
            };

            let config_used = match flags & (1 << 4) {
                0 => ConfigUsed::Default,
                _ => ConfigUsed::Custom,
            };

            let gpio_input = match flags & (1 << 5) {
                0 => GPIOInput::Ignore,
                _ => GPIOInput::Used,
            };

            let gpio0 = match flags & (1 << 6) {
                0 => GPIO0::Low,
                _ => GPIO0::High,
            };

            let gpio1 = match flags & (1 << 7) {
                0 => GPIO1::Low,
                _ => GPIO1::High,
            };

            Ok(FT60xFlashRomDetection {
                memory_type,
                memory_status,
                custom_config_validity,
                custom_config_checksum,
                gpio_input,
                config_used,
                gpio0,
                gpio1,
            })
        }

        pub fn encode(&self) -> u8 {
            let mut flags = 0;
            if let MemoryType::ROM = self.memory_type {
                flags |= 1 << 0
            }
            if let MemoryStatus::ExistsNot = self.memory_status {
                flags |= 1 << 1
            }
            if let CustomConfigValidity::Invalid = self.custom_config_validity {
                flags |= 1 << 2
            }
            if let CustomConfigChecksum::Invalid = self.custom_config_checksum {
                flags |= 1 << 3
            }
            if let ConfigUsed::Custom = self.config_used {
                flags |= 1 << 4
            }
            if let GPIOInput::Used = self.gpio_input {
                flags |= 1 << 5
            }
            if let GPIO0::High = self.gpio0 {
                flags |= 1 << 6
            }
            if let GPIO1::High = self.gpio1 {
                flags |= 1 << 7
            }

            flags
        }
    }
}
