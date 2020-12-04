// prints the recieved stream as 2 16 bit values.
// usefull for debugging performance

use byteorder::{LittleEndian, ReadBytesExt};
use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use std::io::Cursor;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    let ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;
    let mut consumer = ft60x.data_stream(1024 * 16 / 32)?;

    let mut start = SystemTime::now();
    let mut last_i = 0;
    while consumer
        .with_next_buffer(|buf| {
            let mut cursor = Cursor::new(&buf[..]);
            while let (Ok(i), Ok(j)) = (
                cursor.read_u16::<LittleEndian>(),
                cursor.read_u16::<LittleEndian>(),
            ) {
                if j > 0 {
                    eprintln!("{}, {}", last_i, j);
                }
                last_i = i;
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
