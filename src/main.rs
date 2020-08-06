#![feature(get_mut_unchecked)]

use rusb::{Context, request_type, RequestType, Recipient, Direction, DeviceHandle, AsyncGroup, Transfer};
use std::time::Duration;
use byteorder::{LittleEndian, BigEndian, ReadBytesExt, ByteOrder, WriteBytesExt};
use failure::format_err;
use std::io::{Read, Write, Cursor};
use std::time::SystemTime;
use std::sync::{Arc, mpsc::{Sender, Receiver}};
use bitflags::bitflags;
use std::pin::Pin;

const VID: u16 = 0x0403;
const PID: u16 = 0x601f;

type Result<T> = std::result::Result<T, failure::Error>;

#[derive(Debug)]
struct FT60xConfig {
    vid: u16,
    pid: u16,
    manufacturer: String,
    product_description: String,
    serial_number: String,
    power_attributes: u8,
    power_consumption: u16,
    fifo_clock: FT60xFifoClock,
    fifo_mode: FT60xFifoMode,
    channel_config: FT60xChannelConfig,
    optional_features_support: u16,
    battery_charging_gpio_config: u8,
    flash_eeprom_detection: FT60xFlashRomDetection::FT60xFlashRomDetection,
    msio_config: u32,
    gpio_config: u32,
    reserved1: u8,
    reserved2: u8,
}

#[derive(Debug)]
enum FT60xFifoClock {
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
            _ => Err(format_err!("Unknown fifo clock configuration {}", num))
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
enum FT60xFifoMode {
    Mode245,
    Mode600
}

impl FT60xFifoMode  {
    fn parse(num: u8) -> Result<Self> {
        match num {
            0 => Ok(Self::Mode245),
            1 => Ok(Self::Mode600),
            _ => Err(format_err!("Unknown fifo mode configuration {}", num))
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
enum FT60xChannelConfig {
    FourChannels,
    TwoChannels,
    OneChannel,
    OneChannelOutPipe,
    OneChannelInPipe
}

impl FT60xChannelConfig  {
    fn parse(num: u8) -> Result<Self> {
        match num {
            0 => Ok(Self::FourChannels),
            1 => Ok(Self::TwoChannels),
            2 => Ok(Self::OneChannel),
            3 => Ok(Self::OneChannelOutPipe),
            4 => Ok(Self::OneChannelInPipe),
            _ => Err(format_err!("Unknown channel configuration {}", num))
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

mod FT60xFlashRomDetection {
    #[derive(Debug)]
    enum MemoryType {
        Flash,
        ROM
    }

    #[derive(Debug)]
    enum MemoryStatus {
        Exists,
        ExistsNot,
    }

    #[derive(Debug)]
    enum CustomConfigValidity {
        Valid,
        Invalid
    }

    #[derive(Debug)]
    enum CustomConfigChecksum {
        Valid,
        Invalid
    }

    #[derive(Debug)]
    enum GPIOInput {
        Ignore,
        Used
    }

    #[derive(Debug)]
    enum ConfigUsed {
        Default,
        Custom
    }

    #[derive(Debug)]
    enum GPIO0 {
        Low,
        High
    }

    #[derive(Debug)]
    enum GPIO1 {
        Low,
        High
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
        pub fn parse(flags: u8) -> Result<FT60xFlashRomDetection, failure::Error> {
            let memory_type = match flags & (1 << 0) {
                0 => MemoryType::Flash,
                _ => MemoryType::ROM
            };

            let memory_status = match flags & (1 << 1) {
                0 => MemoryStatus::Exists,
                _ => MemoryStatus::ExistsNot
            };

            let custom_config_validity = match flags & (1 << 2) {
                0 => CustomConfigValidity::Valid,
                _ => CustomConfigValidity::Invalid
            };

            let custom_config_checksum = match flags & (1 << 3) {
                0 => CustomConfigChecksum::Valid,
                _ => CustomConfigChecksum::Invalid
            };

            let config_used = match flags & (1 << 4) {
                0 => ConfigUsed::Default,
                _ => ConfigUsed::Custom
            };

            let gpio_input = match flags & (1 << 5) {
                0 => GPIOInput::Ignore,
                _ => GPIOInput::Used,
            };

            let gpio0 = match flags & (1 << 6) {
                0 => GPIO0::Low,
                _ => GPIO0::High
            };

            let gpio1 = match flags & (1 << 7) {
                0 => GPIO1::Low,
                _ => GPIO1::High
            };


            Ok(FT60xFlashRomDetection {
                memory_type, memory_status, custom_config_validity,
                custom_config_checksum, gpio_input, config_used, gpio0, gpio1
            })
        }

        pub fn encode(&self) -> u8 {
            let mut flags = 0;

            match self.memory_type {
                MemoryType::ROM => flags |= 1 << 0,
                _ => {}
            }

            match self.memory_status {
                MemoryStatus::ExistsNot => flags |= 1 << 1,
                _ => {}
            }

            match self.custom_config_validity {
                CustomConfigValidity::Invalid => flags |= 1 << 2,
                _ => {}
            }

            match self.custom_config_checksum {
                CustomConfigChecksum::Invalid => flags |= 1 << 3,
                _ => {}
            }

            match self.config_used {
                ConfigUsed::Custom => flags |= 1 << 4,
                _ => {}
            }

            match self.gpio_input {
                GPIOInput::Used => flags |= 1 << 5,
                _ => {}
            }

            match self.gpio0 {
                GPIO0::High => flags |= 1 << 6,
                _ => {}
            }

            match self.gpio1 {
                GPIO1::High => flags |= 1 << 7,
                _ => {}
            }

            flags
        }
    }
}

impl FT60xConfig {
    fn parse(bytes: [u8; 152]) -> Result<FT60xConfig> {
        let mut data = Cursor::new(&bytes[..]);

        let vid = data.read_u16::<LittleEndian>()?;
        let pid = data.read_u16::<LittleEndian>()?;

        let mut strings_buf = [0u8; 128];
        assert_eq!(data.read(&mut strings_buf)?, 128);

        fn parse_string(bytes: &[u8]) -> Result<(String, u8)> {
            let offset = bytes[0];
            let length = (offset - 2) >> 1;

            assert_eq!(bytes[1], 0x3);

            let mut res = String::new();

            for i in 0..(length as usize) {
                res += std::str::from_utf8(&[bytes[2 * i + 2]])?;
                assert_eq!(bytes[2 * i + 2 + 1], 0);
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
        let flash_eeprom_detection = FT60xFlashRomDetection::FT60xFlashRomDetection::parse(flash_eeprom_detection)?;

        let msio_config = data.read_u32::<LittleEndian>()?;
        let gpio_config = data.read_u32::<LittleEndian>()?;

        Ok(FT60xConfig { vid, pid, manufacturer, product_description, serial_number,
                      power_attributes, power_consumption, fifo_clock, fifo_mode, channel_config,
                      optional_features_support, battery_charging_gpio_config, flash_eeprom_detection,
                      msio_config, gpio_config, reserved1, reserved2
        })
    }

    fn encode(&self) -> Result<[u8; 152]> {
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

        cursor.write(&strings_buf)?;

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

struct FT60x {
    vid: u16,
    pid: u16,
    context: Context,
}

struct RingBufProducer<T> {
    ringbuf: Arc<RingBuf<T>>,
    next_write_pos_sink: Sender<usize>,
    last_read_pos: Receiver<usize>,
    next_write_pos: usize,
    lastknown_last_read_pos: usize,
}

struct RingBufConsumer<T> {
    ringbuf: Arc<RingBuf<T>>,
    next_write_pos: Receiver<usize>,
    last_read_pos: Sender<usize>,
    next_read_pos: usize,
    lastknown_next_write_pos: usize,
}

impl<T> RingBufProducer<T> {
    fn new(ringbuf: Arc<RingBuf<T>>, next_write_pos_sink: Sender<usize>, last_read_pos: Receiver<usize>) -> Self {
        let cap = ringbuf.capacity - 1;
        Self {
            ringbuf,
            next_write_pos_sink,
            last_read_pos,
            next_write_pos: 0,
            lastknown_last_read_pos: 0,
        }
    }

    fn with_next_buffer<F: FnMut(&mut T)>(&mut self, mut func: F) {
        self.lastknown_last_read_pos = std::iter::once(self.lastknown_last_read_pos).chain(self.last_read_pos.iter()).find(|last_read_pos| {
           self.next_write_pos - last_read_pos < self.ringbuf.capacity
        }).unwrap();

        let pos = self.next_write_pos % self.ringbuf.capacity;

        unsafe {
            &mut func(&mut Arc::get_mut_unchecked(&mut self.ringbuf).buffer[pos]);
        }

        self.next_write_pos += 1;
        self.next_write_pos_sink.send(self.next_write_pos).unwrap();
    }
}

impl<T> RingBufConsumer<T> {
    fn new(ringbuf: Arc<RingBuf<T>>, next_write_pos: Receiver<usize>, last_read_pos: Sender<usize>) -> Self {
        Self {
            ringbuf,
            next_write_pos,
            last_read_pos,
            next_read_pos: 0,
            lastknown_next_write_pos: 0
        }
    }

    fn with_next_buffer<F: FnMut(&T)>(&mut self, mut func: F) {
        self.lastknown_next_write_pos = std::iter::once(self.lastknown_next_write_pos).chain(self.next_write_pos.iter()).find(|next_write_pos| {
            *next_write_pos > self.next_read_pos
        }).unwrap();

        let pos = self.next_read_pos % self.ringbuf.capacity;
        &mut func(&self.ringbuf.buffer[pos]);

        self.last_read_pos.send(self.next_read_pos).unwrap();

        self.next_read_pos += 1;
    }
}


struct RingBuf<T> {
    buffer: Vec<T>,
    capacity: usize
}

impl<T: Default + Clone> RingBuf<T> {
    fn new(capacity: usize) -> Self {
        assert!(capacity != 1, "Use a RwLock for capacity 1");

        RingBuf {
            buffer: vec![Default::default(); capacity],
            capacity
        }
    }

    fn create_channel(capacity: usize) -> (RingBufProducer<T>, RingBufConsumer<T>) {
        let ringbuf = Arc::new(RingBuf::new(capacity));

        let (next_write_pos_sink, next_write_pos_receiver) = std::sync::mpsc::channel();
        let (last_read_pos_sender, last_read_pos_receiver) = std::sync::mpsc::channel();

        let producer = RingBufProducer::new(ringbuf.clone(), next_write_pos_sink, last_read_pos_receiver);
        let consumer = RingBufConsumer::new(ringbuf.clone(), next_write_pos_receiver, last_read_pos_sender);

        (producer, consumer)
    }
}

impl FT60x {
    fn new(vid: u16, pid: u16) -> Result<FT60x> {
        let context = Context::new()?;

        Ok(FT60x {
            vid,
            pid,
            context,
        })
    }

    fn with_device<F: FnMut(&mut DeviceHandle) -> Result<T>, T>(&self, mut func: F) -> Result<T> {
        let mut device = self.context
            .open_device_with_vid_pid(self.vid, self.pid)
            .ok_or_else(|| format_err!("No device with VID {:#x} and PID {:#x} was found", VID, PID))?;

        Ok(func(&mut device)?)
    }

    fn as_device<F: FnMut(Context) -> Result<T>, T>(self, mut func: F) -> Result<T> {
        Ok(func(self.context)?)
    }

    fn get_config(&self) -> Result<FT60xConfig> {
        let mut buf = [0; 152];
        let read = self.with_device(|device| Ok(device.read_control(
            request_type(Direction::In, RequestType::Vendor, Recipient::Device),
            0xcf, 1, 0, &mut buf, Duration::new(1, 0))?))?;
        println!("{:?}", buf.to_vec());

        assert_eq!(read, 152);

        FT60xConfig::parse(buf)
    }

    fn set_config(&self, config: FT60xConfig) -> Result<()> {
        let mut buf = config.encode()?;
        println!("{:?}", buf.to_vec());
        let written = self.with_device(|device| Ok(device.write_control(
            request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
            0xcf, 0, 0, &mut buf, Duration::new(1, 0))?))?;

        assert_eq!(written, 152);

        Ok(())
    }

    // bufsize is in 32kb blocks
    fn on_data<F: FnMut(&[u8])>(self, mut func: F) -> Result<()> {
        let vid = self.vid;
        let pid = self.pid;

        let _ = self.as_device(|mut context| {
            let context = Arc::new(context);
            {
                let mut device = context
                    .open_device_with_vid_pid(vid, pid)
                    .ok_or_else(|| format_err!("No device with VID {:#x} and PID {:#x} was found", VID, PID)).unwrap();

                device.claim_interface(0)?;
                device.claim_interface(1)?;

                let ctrlreq = [
                    0x00, 0x00, 0x00, 0x00, 0x82, 0x02, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
                ];

                device.write_bulk(0x01, &ctrlreq, Duration::new(1, 0))?;
            }


            const blocksize: usize = 32 * 1024; // 32 Kb
            const bufsize: usize = blocksize * 1024 * 16 / 32; // 16Mb


            #[derive(Clone)]
            struct DataSlice {
                data: Vec<u8>,
            }

            impl Default for DataSlice {
                fn default() -> Self {
                    DataSlice {
                        data: vec![0; bufsize],
                    }
                }
            }

            let (mut producer, mut consumer) = RingBuf::<DataSlice>::create_channel(4);

            let other_context = context.clone();
            std::thread::spawn(move || {
                let mut device = other_context
                    .open_device_with_vid_pid(vid, pid)
                    .ok_or_else(|| format_err!("No device with VID {:#x} and PID {:#x} was found", VID, PID)).unwrap();
                // let context = context;
                loop {
                    producer.with_next_buffer(|buf| {
                        let mut async_group = AsyncGroup::new(&other_context);
                        let mut i = 0;
                        for chunk in buf.data.chunks_mut(blocksize) {
                            // println!("{}", i);
                            i += 1;
                            async_group.submit(Transfer::bulk(&device, 0x82, chunk, Duration::new(1, 0))).unwrap();

                            if i > 100 {
                                assert_eq!(async_group.wait_any().unwrap().actual().len(), blocksize);
                            }
                        }

                        while let Ok(mut transfer) = async_group.wait_any() {
                            assert_eq!(transfer.actual().len(), blocksize);
                        }
                    })
                }

            });

            loop {
                consumer.with_next_buffer(|buf| {
                    &mut func(&buf.data[..]);
                })
            }

            // device.write_bulk(0x01, &ctrlreq, Duration::new(1, 0))?;
            Ok(())
        })?;

        Ok(())
    }
}

// struct FT60xCtrlReq {
//     idx: u32,
//     pipe: u8,
//     cmd: u8,
//     unknown1: u8,
//     unknown2: u8,
//     len: u32,
//     unknown4: u32,
//     unknown5: u32,
// }

// impl FT60xCtrlReq {
//     fn encode(&self) -> [u8; 20]
// }

fn main() -> Result<()> {
    let ft60x = FT60x::new(VID, PID)?;
    let mut config = ft60x.get_config()?;

    let mut start = SystemTime::now();
    let mut last = 0u16;
    ft60x.on_data(|buf| {
        let elapsed = start.elapsed().unwrap().as_secs_f64();
        start = SystemTime::now();
        let bytes = buf.len() as f64;

        // let mut cursor = Cursor::new(&buf[..]);

        // while let (Ok(i), Ok(_)) = (cursor.read_u16::<LittleEndian>(), cursor.read_u16::<LittleEndian>()) {
        //     if last.overflowing_add(1).0 != i {
        //         println!("miss {} {}", last, i);
        //     }

        //     last = i;
        // }

        println!("elapsed (for {} Mb) {}s = {} MB/s", bytes / 1024. / 1024., elapsed, bytes / 1024. / 1024. / elapsed);
    })?;

    // let mut buf = vec![0u8; 32 * 1024];

    // let mut last: u16 = 0;
    // for _ in 0..(1024 * 1024 / kb) {
    //     assert_eq!(1024 * kb, read);

        // let mut cursor = Cursor::new(&buf[..]);

        // while let (Ok(i), Ok(_)) = (cursor.read_u16::<LittleEndian>(), cursor.read_u16::<LittleEndian>()) {
        //     if last.overflowing_add(1).0 != i {
        //         println!("miss {} {}", last, i);
        //     }

        //     last = i;
        // }
    // }




    // let (mut producer, mut consumer) = RingBuf::<u64>::create_channel(5);

    // std::thread::spawn(move || {
    //     let mut last = 0;
    //     loop {
    //         consumer.with_next_buffer(|val| {
    //             if last + 1 != *val {
    //                 println!("miss last: {} val: {}", last, val);
    //             }

    //             last = *val;
    //         })
    //     }
    // });

    // std::thread::spawn(move || {
    //     // let mut i = 0;

    //     for i in 1..10_000_000 {
    //         producer.with_next_buffer(|val| {
    //             // i += 1;
    //             *val = i;
    //         })
    //     }
    // }).join();

    // println!("done");


    // for i in 0..6 {
    //     producer.with_next_buffer(|val| {
    //         *val = i;
    //     });

    // }

    // for i in 0..5 {
    //     consumer.with_next_buffer(|val| {
    //         println!("{}", *val);
    //     });
    // }

/*

    producer.with_next_buffer(|val| {
        *val = 1;
    });

    consumer.with_next_buffer(|val| {
        println!("{}", *val);
    });

    producer.with_next_buffer(|val| {
        *val = 1;
    });

    consumer.with_next_buffer(|val| {
        println!("{}", *val);
    });
*/

    // let ft60x = FT60x::new(VID, PID)?;
    // ft60x.test()?;
    // let mut config = ft60x.get_config()?;
    // println!("{:#?}", config);

    // config.channel_config = FT60xChannelConfig::OneChannelInPipe;
    // config.channel_config = FT60xChannelConfig::OneChannel;
    // config.fifo_mode = FT60xFifoMode::Mode245;
    // ft60x.set_config(config)?;

    // let config = ft60x.get_config()?;
    // println!("{:#?}", config);

    Ok(())
}
