// configures the ft601 in a way that can be used with this lib.
// run this tool before running any other tools.

use ft60x::ft60x::{FT60x, DEFAULT_PID, DEFAULT_VID};
use ft60x::ft60x_config::{FT60xChannelConfig, FT60xFifoClock, FT60xFifoMode};

type Result<T> = std::result::Result<T, ft60x::Error>;

fn main() -> Result<()> {
    let mut ft60x = FT60x::new(DEFAULT_VID, DEFAULT_PID)?;

    let mut config = ft60x.get_config()?;

    config.fifo_clock = FT60xFifoClock::Clock100MHz;
    config.fifo_mode = FT60xFifoMode::Mode245;
    config.channel_config = FT60xChannelConfig::OneChannelInPipe;

    ft60x.set_config(config)?;

    println!("successfully set config :)");

    Ok(())
}
