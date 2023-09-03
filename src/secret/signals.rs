use std::convert::Infallible;
use std::unimplemented;

use futures::Stream;
use signal_hook::consts::signal;
use signal_hook::iterator::exfiltrator::SignalOnly;
use signal_hook::iterator::{Signals, SignalsInfo};
use tokio::signal::unix::SignalKind;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub fn await_signals() -> impl Stream<Item = i32> {
    // TODO: Support all the signals
    let _info = SignalsInfo::<SignalOnly>::new([signal::SIGINT, signal::SIGHUP]).unwrap();

    let (mut tx, rx) = mpsc::channel(6);

    // TODO: signal-hook-tokio
    // https://docs.rs/signal-hook-tokio
    // tokio::spawn(async move {
    //     let signal = info.wait()
    // });

    ReceiverStream::new(rx)
}
