// checks that the received stream forms a consecutive counter when interpreted as 32 bit unsigned values.
// useful for ensuring that no data is lost

use byteorder::{LittleEndian, ReadBytesExt};
use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use std::io::Cursor;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, ft60x::Error>;

fn main() -> Result<()> {
    let ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;
    let mut consumer = ft60x.data_stream_ringbuf(1024 * 1024 * 128)?;

    let mut start = SystemTime::now();
    let mut last = 0u32;
    while consumer
        .with_next_buffer(|buf| {
            let mut cursor = Cursor::new(&buf[..]);
            while let Ok(i) = cursor.read_u32::<LittleEndian>() {
                if last.overflowing_add(1).0 != i {
                    eprintln!("miss! last: {}; next: {}", last, i);
                }

                last = i;
            }

            let bytes = buf.len() as f64;
            let elapsed = start.elapsed().unwrap().as_secs_f64();
            start = SystemTime::now();
            eprintln!(
                "elapsed (for {} Mb) {}s = {} MB/s",
                bytes / 1024. / 1024.,
                elapsed,
                bytes / 1024. / 1024. / elapsed
            );
        })
        .is_ok()
    {}

    Ok(())
}
