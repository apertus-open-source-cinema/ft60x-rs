use rusb::{
    request_type, AsyncGroup, Context, DeviceHandle, Direction, Recipient, RequestType, Transfer,
};
use std::time::Duration;

use crate::ft60x_config::FT60xConfig;
use crate::ringbuf::{RingBuf, RingBufConsumer};
use crate::Result;
use owning_ref::OwningHandle;
use std::iter::once;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

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

    /// it is recommended to read multiples of 32Kb
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.set_streaming_mode()?;

        let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601
        let mut_chunks = buf.chunks_mut(blocksize);
        let mut_chunks_len = mut_chunks.len();
        let mut collected = 0;

        let mut async_group = AsyncGroup::new(&self.context);
        for (i, chunk) in mut_chunks.enumerate() {
            // The FT60x doesn't seem to like too many outstanding requests
            if i > 500 {
                let mut transfer = async_group.wait_any()?;
                ensure!(
                    transfer.buffer().len() == transfer.actual().len(),
                    "FT60x did not return enough data. requested {} got {}",
                    transfer.buffer().len(),
                    transfer.actual().len()
                );
                collected += 1;
            }

            async_group.submit(Transfer::bulk(
                &self.device,
                0x82,
                chunk,
                Duration::new(1, 0),
            ))?;
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

    // starts a thread with which you can send empty buffers and receive full buffers from
    // allows for interleaved data transfers (without loosing data)
    pub fn data_stream_mpsc(
        mut self,
        in_flight_buffers: usize,
    ) -> (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) {
        let (empty_buffer_tx, empty_buffer_rx) = sync_channel::<Vec<u8>>(in_flight_buffers);
        let (full_buffer_tx, full_buffer_rx) = sync_channel(in_flight_buffers);

        let mut thread_fn = move || {
            self.set_streaming_mode().unwrap();

            let blocksize: usize = 32 * 1024; // 32 Kb seems to be the sweet spot for the ft601

            let mut async_group_buffer: Vec<(Vec<u8>, AsyncGroup)> = Vec::new();

            let mut outstanding = 0;
            for mut current_buffer in empty_buffer_rx.iter() {
                let mut current_async_group = AsyncGroup::new(&self.context);
                for chunk in unsafe {
                    // the rust compiler cant prove the lifetime here.
                    // we are dropping the async group together with ending to write to the buffer
                    // so for the relevant timeframe, the pointers to the chunks of that buffer are valid.
                    std::mem::transmute::<&mut _, &'static mut Vec<u8>>(&mut current_buffer)
                }
                .chunks_mut(blocksize)
                {
                    // The FT60x doesn't seem to like too many outstanding requests
                    if outstanding > 500 {
                        let mut to_ship = None;
                        for (i, async_group) in async_group_buffer
                            .iter_mut()
                            .map(|v| &mut v.1)
                            .chain(once(&mut current_async_group))
                            .enumerate()
                        {
                            match async_group.wait_any() {
                                Ok(mut transfer) => {
                                    ensure!(
                                        transfer.buffer().len() == transfer.actual().len(),
                                        "FT60x did not return enough data. requested {} got {}",
                                        transfer.buffer().len(),
                                        transfer.actual().len()
                                    );
                                    outstanding -= 1;
                                    break;
                                }
                                Err(rusb::Error::NotFound) => {
                                    assert_eq!(to_ship, None);
                                    assert_eq!(i, 0);
                                    to_ship = Some(i);
                                }
                                Err(_) => panic!(),
                            }
                        }

                        if let Some(i) = to_ship {
                            if i < async_group_buffer.len() {
                                full_buffer_tx.send(async_group_buffer.remove(i).0).unwrap();
                            }
                        }
                    }

                    current_async_group.submit(Transfer::bulk(
                        &self.device,
                        0x82,
                        chunk,
                        Duration::new(1, 0),
                    ))?;
                    outstanding += 1;
                }
                async_group_buffer.push((current_buffer, current_async_group));
            }

            let mut to_ship = None;
            for (i, async_group) in async_group_buffer.iter_mut().map(|v| &mut v.1).enumerate() {
                match async_group.wait_any() {
                    Ok(mut transfer) => {
                        ensure!(
                            transfer.buffer().len() == transfer.actual().len(),
                            "FT60x did not return enough data. requested {} got {}",
                            transfer.buffer().len(),
                            transfer.actual().len()
                        );
                        break;
                    }
                    Err(rusb::Error::NotFound) => {
                        assert_eq!(to_ship, None);
                        assert_eq!(i, 0);
                        to_ship = Some(i);
                    }
                    Err(_) => panic!(),
                }
            }

            if let Some(i) = to_ship {
                if i < async_group_buffer.len() {
                    full_buffer_tx.send(async_group_buffer.remove(i).0).unwrap();
                }
            }

            Ok(())
        };

        thread::Builder::new()
            .name("ft60x-rx".to_string())
            .spawn(move || {
                thread_fn().unwrap();
            })
            .unwrap();

        (empty_buffer_tx, full_buffer_rx)
    }

    /// it is recommended to request multiples of 32Kb
    pub fn data_stream_ringbuf(mut self, bufsize: usize) -> Result<RingBufConsumer<Vec<u8>>> {
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
