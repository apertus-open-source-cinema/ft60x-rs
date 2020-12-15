# `ft60x-rs`
A rust interface library for the ftdi FT60x usb3.0 superspeed FIFO bridge chip series.

## Current State
`ft60x-rs` can sucessfully stream data from the FT601 to the host in 245 fifo mode.
Streaming in the other direction is not implemented yet.
FT600 should work as well but is untested.

## Binaries / Utilities
Shipped with `ft60x-rs` are some examples (found in [`examples/`](examples/)).

* `datastreamer` streams the data it recieves to stdout while printing performance information to stderr. This can be used to record the datastream to disk or process it further using other tools.
* `stream_checker` checks that the 32bit words recieved from the FT60x form a consecutive counter. If anything is missed, a warning is printed to stderr. This can be used to verify that no data is missed (and therefore to verify gateware). Example gateware that can be used in companion with this tool can be found [in the apertus nmigen-gateware repo](https://github.com/apertus-open-source-cinema/nmigen-gateware/blob/c75fffe/src/experiments/usb3_test.py)
* `config` configures the ft601 to be used as a fifo in 254 mode.
* `perf_debug` can help debugging performance issues.


## Performance
Using the FT601 in 245 FIFO mode, we were able to read ~360Mbyte/s continiously.
This is pretty exactly the same performance as we achieved using the proprietary
D3XX library while this code uses less cpu time.

Further performance optimization might be possible using the 600 FIFO mode. However
this was not investigated further.
