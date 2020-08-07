use rusb::{
    request_type, AsyncGroup, Context, DeviceHandle, Direction, Recipient, RequestType, Transfer,
};
use std::time::Duration;

use crate::ft60x_config::FT60xConfig;
use crate::ringbuf::{RingBuf, RingBufConsumer};
use crate::Result;
use failure::format_err;

pub const DEFAULT_PID: u16 = 0x601f;
pub const DEFAULT_VID: u16 = 0x0403;

pub struct FT60x {
    vid: u16,
    pid: u16,
    context: Context,
}

impl FT60x {
    pub fn new(vid: u16, pid: u16) -> Result<FT60x> {
        let context = Context::new()?;

        Ok(FT60x { vid, pid, context })
    }

    fn with_device_helper<F: FnMut(&mut DeviceHandle) -> Result<T>, T>(
        context: &Context,
        vid: u16,
        pid: u16,
        mut func: F,
    ) -> Result<T> {
        let mut device = context.open_device_with_vid_pid(vid, pid).ok_or_else(|| {
            format_err!("No device with VID {:#x} and PID {:#x} was found", vid, pid)
        })?;

        Ok(func(&mut device)?)
    }

    fn with_device<F: FnMut(&mut DeviceHandle) -> Result<T>, T>(&self, func: F) -> Result<T> {
        Self::with_device_helper(&self.context, self.vid, self.pid, func)
    }

    pub fn get_config(&self) -> Result<FT60xConfig> {
        let mut buf = [0; 152];
        let read = self.with_device(|device| {
            Ok(device.read_control(
                request_type(Direction::In, RequestType::Vendor, Recipient::Device),
                0xcf,
                1,
                0,
                &mut buf,
                Duration::new(1, 0),
            )?)
        })?;

        assert_eq!(read, 152);

        FT60xConfig::parse(buf)
    }

    pub fn set_config(&self, config: FT60xConfig) -> Result<()> {
        let buf = config.encode()?;
        println!("{:?}", buf.to_vec());
        let written = self.with_device(|device| {
            Ok(device.write_control(
                request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
                0xcf,
                0,
                0,
                &buf,
                Duration::new(1, 0),
            )?)
        })?;

        assert_eq!(written, 152);

        Ok(())
    }

    /// bufsize is in 32kb blocks
    pub fn data_stream(self, bufsize: usize) -> Result<RingBufConsumer<Vec<u8>>> {
        let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601
        let bufsize: usize = blocksize * bufsize;

        self.set_streaming_mode()?;

        let (mut producer, consumer) =
            RingBuf::<Vec<u8>>::create_channel_with_default_value(4, vec![0u8; bufsize]);

        std::thread::spawn(move || {
            Self::with_device_helper(&self.context, self.vid, self.pid, |device| {
                while producer
                    .with_next_buffer(|buf| {
                        let mut async_group = AsyncGroup::new(&self.context);
                        for (i, chunk) in buf.chunks_mut(blocksize).enumerate() {
                            async_group
                                .submit(Transfer::bulk(&device, 0x82, chunk, Duration::new(1, 0)))
                                .unwrap();

                            // The FT60x doesn't seem to like too many outstanding requests
                            if i > 50 {
                                assert_eq!(
                                    async_group.wait_any().unwrap().actual().len(),
                                    blocksize
                                );
                            }
                        }

                        while let Ok(mut transfer) = async_group.wait_any() {
                            assert_eq!(transfer.actual().len(), blocksize);
                        }
                    })
                    .is_ok()
                {}

                Ok(())
            })
            .unwrap();
        });

        Ok(consumer)
    }

    fn set_streaming_mode(&self) -> Result<()> {
        self.with_device(|device| {
            device.claim_interface(0)?;
            device.claim_interface(1)?;

            let ctrlreq = [
                0x00, 0x00, 0x00, 0x00, 0x82, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ];

            device.write_bulk(0x01, &ctrlreq, Duration::new(1, 0))?;

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
