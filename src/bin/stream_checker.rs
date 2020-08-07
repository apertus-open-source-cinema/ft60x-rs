// checks the first 16 bits of each received packet to be a consecutive counter.
// useful for ensuring that no data is lost

use byteorder::{LittleEndian, ReadBytesExt};
use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use std::io::Cursor;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    let ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;
    let mut consumer = ft60x.data_stream(1024 * 16 / 32)?;

    let mut start = SystemTime::now();
    let mut last = 0u16;
    loop {
        match consumer.with_next_buffer(|buf| {
            let mut cursor = Cursor::new(&buf[..]);
            while let (Ok(i), Ok(_)) = (
                cursor.read_u16::<LittleEndian>(),
                cursor.read_u16::<LittleEndian>(),
            ) {
                if last.overflowing_add(1).0 != i {
                    eprintln!("miss {} {}", last, i);
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
        }) {
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    Ok(())
}