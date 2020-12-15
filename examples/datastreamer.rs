// prints the received data from the ft60x to stdout.
// useful for building quick experiments in bash or storing the data.

use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use std::io::{self, Write};
use std::thread;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, ft60x::Error>;

fn main() -> Result<()> {
    let ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;
    let (empty_buffer_tx, full_buffer_rx) = ft60x.data_stream_mpsc(10);

    thread::spawn(move || loop {
        empty_buffer_tx.send(vec![0; 1024 * 1024 * 128]).unwrap();
    });

    let mut start = SystemTime::now();
    for buf in full_buffer_rx.iter() {
        io::stdout().write_all(&*buf).unwrap();

        let bytes = buf.len() as f64;
        let elapsed = start.elapsed().unwrap().as_secs_f64();
        start = SystemTime::now();
        eprintln!(
            "elapsed (for {} Mb) {}s = {} MB/s",
            bytes / 1024. / 1024.,
            elapsed,
            bytes / 1024. / 1024. / elapsed
        );
    }

    Ok(())
}
