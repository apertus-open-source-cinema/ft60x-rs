use rusb::{
    request_type, AsyncGroup, Context, DeviceHandle, Direction, Recipient, RequestType, Transfer,
};
use std::time::Duration;

use crate::ft60x_config::FT60xConfig;
use crate::ringbuf::{RingBuf, RingBufConsumer};
use crate::Result;
use failure::format_err;
use lazy_static::lazy_static;

pub const DEFAULT_PID: u16 = 0x601f;
pub const DEFAULT_VID: u16 = 0x0403;

lazy_static! {
    static ref CONTEXT: Context = Context::new().unwrap();
}

pub struct FT60x {
    device: DeviceHandle<'static>,
    streaming_mode: bool,
}
impl FT60x {
    pub fn new(vid: u16, pid: u16) -> Result<Self> {
        let device =
            CONTEXT
                .open_device_with_vid_pid(vid, pid)
                .ok_or_else(|| format_err!("No device with VID {:#x} and PID {:#x} was found", vid, pid))?;
        Ok(Self { device, streaming_mode: false })
    }

    pub fn get_config(&mut self) -> Result<FT60xConfig> {
        let mut buf = [0; 152];
        let read = self.device.read_control(
            request_type(Direction::In, RequestType::Vendor, Recipient::Device),
            0xcf,
            1,
            0,
            &mut buf,
            Duration::new(1, 0),
        )?;

        assert_eq!(read, 152);
        FT60xConfig::parse(buf)
    }

    pub fn set_config(&mut self, config: FT60xConfig) -> Result<()> {
        let buf = config.encode()?;
        let written = self.device.write_control(
            request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
            0xcf,
            0,
            0,
            &buf,
            Duration::new(1, 0),
        )?;

        assert_eq!(written, 152);
        Ok(())
    }

    fn set_streaming_mode(&mut self) -> Result<()> {
        if !self.streaming_mode {
            self.device.claim_interface(0)?;
            self.device.claim_interface(1)?;

            let ctrlreq = [
                0x00, 0x00, 0x00, 0x00, 0x82, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ];

            self.device.write_bulk(0x01, &ctrlreq, Duration::new(1, 0))?;
            self.streaming_mode = true;
        }
        Ok(())
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.set_streaming_mode()?;

        let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601
        let mut async_group = AsyncGroup::new(&CONTEXT);
        for (i, chunk) in buf.chunks_mut(blocksize).enumerate() {
            async_group
                .submit(Transfer::bulk(&self.device, 0x82, chunk, Duration::new(1, 0)))
                .unwrap();

            // The FT60x doesn't seem to like too many outstanding requests
            if i > 50 {
                let real_len = async_group.wait_any().unwrap().actual().len();
                //eprintln!("{}", real_len);
                assert_eq!(
                    real_len,
                    blocksize
                );
            }
        }

        while let Ok(mut transfer) = async_group.wait_any() {
            assert_eq!(transfer.actual().len(), blocksize);
        }

        Ok(())
    }

    /// bufsize is in 32kb blocks
    pub fn data_stream(mut self, bufsize: usize) -> Result<RingBufConsumer<Vec<u8>>> {
        let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601
        let bufsize: usize = blocksize * bufsize;

        let (mut producer, consumer) =
            RingBuf::<Vec<u8>>::create_channel_with_default_value(4, vec![0u8; bufsize]);

        std::thread::spawn(move || {
            while producer.with_next_buffer(|buf| {
                    self.read_exact(buf).unwrap();
                })
                .is_ok()
            {}
        });

        Ok(consumer)
    }
}
