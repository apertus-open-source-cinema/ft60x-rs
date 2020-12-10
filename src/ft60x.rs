use rusb::{
    request_type, AsyncGroup, Context, DeviceHandle, Direction, Recipient, RequestType, Transfer,
};
use std::time::Duration;

use crate::ft60x_config::FT60xConfig;
use crate::ringbuf::{RingBuf, RingBufConsumer};
use crate::Result;
use owning_ref::OwningHandle;
use std::sync::Arc;

pub const DEFAULT_PID: u16 = 0x601f;
pub const DEFAULT_VID: u16 = 0x0403;

pub struct FT60x {
    context: Arc<Context>,
    device: OwningHandle<Arc<Context>, Box<DeviceHandle<'static>>>,
    streaming_mode: bool,
}
impl FT60x {
    pub fn new(vid: u16, pid: u16) -> Result<Self> {
        let context = Arc::new(Context::new()?);
        let device: Result<_> = OwningHandle::try_new(context.clone(), |context| unsafe {
            Ok(Box::new(
                context
                    .as_ref()
                    .ok_or_else(|| format_general_err!("null pointer for context received"))?
                    .open_device_with_vid_pid(vid, pid)
                    .ok_or_else(|| {
                        format_general_err!(
                            "No device with VID {:#x} and PID {:#x} was found",
                            vid,
                            pid
                        )
                    })?,
            ))
        });
        Ok(FT60x {
            context,
            device: device?,
            streaming_mode: false,
        })
    }

    pub fn get_config(&self) -> Result<FT60xConfig> {
        let mut buf = [0; 152];
        let read = self.device.read_control(
            request_type(Direction::In, RequestType::Vendor, Recipient::Device),
            0xcf,
            1,
            0,
            &mut buf,
            Duration::new(1, 0),
        )?;

        ensure!(read == 152, "got wrong number of config bytes");
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

        ensure!(written == 152, "wrote wrong number of config bytes");
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

            self.device
                .write_bulk(0x01, &ctrlreq, Duration::new(1, 0))?;
            self.streaming_mode = true;
        }
        Ok(())
    }

    /// it es recommended to read multiples of 32Kb
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.set_streaming_mode()?;

        let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601
        let mut_chunks = buf.chunks_mut(blocksize);
        let mut_chunks_len = mut_chunks.len();
        let mut collected = 0;

        let mut async_group = AsyncGroup::new(&self.context);
        for (i, chunk) in mut_chunks.enumerate() {
            async_group.submit(Transfer::bulk(
                &self.device,
                0x82,
                chunk,
                Duration::new(1, 0),
            ))?;

            // The FT60x doesn't seem to like too many outstanding requests
            if i > 50 {
                let mut transfer = async_group.wait_any()?;
                ensure!(
                    transfer.buffer().len() == transfer.actual().len(),
                    "FT60x did not return enough data. requested {} got {}",
                    transfer.buffer().len(),
                    transfer.actual().len()
                );
                collected += 1;
            }
        }
        while let Ok(mut transfer) = async_group.wait_any() {
            ensure!(
                transfer.buffer().len() == transfer.actual().len(),
                "FT60x did not return enough data. requested {} got {}",
                transfer.buffer().len(),
                transfer.actual().len()
            );
            collected += 1;
        }
        ensure!(
            collected == mut_chunks_len,
            "FT60x did not answer all chunks within timeout. Requested {} got an answer for {}",
            mut_chunks_len,
            collected
        );
        Ok(())
    }

    /// it es recommended to request multiples of 32Kb
    pub fn data_stream(mut self, bufsize: usize) -> Result<RingBufConsumer<Vec<u8>>> {
        let (mut producer, consumer) =
            RingBuf::<Vec<u8>>::create_channel_with_default_value(4, vec![0u8; bufsize]);

        std::thread::spawn(move || {
            while producer
                .with_next_buffer(|buf| self.read_exact(buf).unwrap())
                .is_ok()
            {}
        });

        Ok(consumer)
    }
}
