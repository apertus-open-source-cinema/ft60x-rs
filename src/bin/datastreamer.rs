// prints the received data from the ft60x to stdout.
// useful for building quick experiments in bash or storing the data.

use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use std::io::{self, Write};
use std::time::SystemTime;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    let ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;
    let mut consumer = ft60x.data_stream(1024 * 16 / 32)?;

    let mut start = SystemTime::now();
    loop {
        match consumer.with_next_buffer(|buf| {
            io::stdout().write_all(buf).unwrap();

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
