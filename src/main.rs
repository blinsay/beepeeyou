extern crate cpal;
extern crate libc;

use libc::c_int;
use std::f32::consts::PI;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::Duration;

const LOAD_MULT: f64 = 10.0;

fn main() {
    let load = Arc::new(AtomicUsize::new(LOAD_MULT as usize));

    // start a thread that gets one minute loadavg from the system and stores it
    // as an atomic uint. to convert from the float to an int, multiply by
    // LOAD_MULT and round.
    let write_load = load.clone();
    let set_load = move || loop {
        if let Ok((one_min, _five, _fifteen)) = load_avg() {
            let rounded = (one_min * LOAD_MULT as f64).round() as usize;
            write_load.store(rounded, Ordering::Release);
        }
        sleep(Duration::from_secs(1));
    };

    // read load averages from the same atomic uint by pulling them out and
    // dividing by the LOAD_MULT constant.
    let read_load = load.clone();
    let get_load = move || (read_load.load(Ordering::Acquire) as f64) / LOAD_MULT;

    // in a background thread, get load and set it every second
    spawn(set_load);

    // start the audio event loop and write beeps to it
    let (event_loop, stream_id, format) =
        default_device_loop().expect("couldn't set up the default audio loop!");
    output_beepeeyou(event_loop, stream_id, &format, || get_load() as f32);
}

#[derive(Debug)]
enum BeepError {
    NoOutputDevice,
    DeviceError { reason: &'static str },
}

// get an event loop configured to output to the default audio device
fn default_device_loop() -> Result<(cpal::EventLoop, cpal::StreamId, cpal::Format), BeepError> {
    use cpal::{CreationError, DefaultFormatError};
    use BeepError::*;

    let device = cpal::default_output_device().ok_or(NoOutputDevice)?;
    let output_format = device.default_output_format().map_err(|e| match e {
        DefaultFormatError::DeviceNotAvailable => DeviceError {
            reason: "checking output format: device is gone",
        },
        DefaultFormatError::StreamTypeNotSupported => DeviceError {
            reason: "checking output format: device is not supported",
        },
    })?;

    let event_loop = cpal::EventLoop::new();
    let stream_id = event_loop
        .build_output_stream(&device, &output_format)
        .map_err(|e| match e {
            CreationError::DeviceNotAvailable => DeviceError {
                reason: "can't build event loop: device is gone",
            },
            CreationError::FormatNotSupported => DeviceError {
                reason: "can't build event loop: device is not supported",
            },
        })?;

    Ok((event_loop, stream_id, output_format))
}

// beep beep, i'm a jeep
fn output_beepeeyou<T>(
    event_loop: cpal::EventLoop,
    output_stream: cpal::StreamId,
    output_format: &cpal::Format,
    mut get_frequency: T,
) -> ()
where
    T: FnMut() -> f32 + Send,
{
    let sample_rate = output_format.sample_rate.0 as f32;
    let channels = output_format.channels as usize;

    let mut sample_clock = 0f32;
    let mut frequency = 220f32 * get_frequency();

    let mut calculate_next_sample = move || {
        let next_sample = ((sample_clock / sample_rate) * frequency * 2.0 * PI).sin();
        sample_clock = (sample_clock + 1.0) % sample_rate;

        if sample_clock % 1000.0 == 0.0 {
            frequency = 220f32 * get_frequency();
        }

        next_sample
    };

    event_loop.run(move |id, data| {
        if id != output_stream {
            return;
        }

        use cpal::StreamData::*;
        use cpal::UnknownTypeOutputBuffer::*;

        match data {
            Output { buffer: U16(buf) } => {
                write_samples(buf, channels, &mut calculate_next_sample);
            }
            Output { buffer: I16(buf) } => {
                write_samples(buf, channels, &mut calculate_next_sample);
            }
            Output { buffer: F32(buf) } => {
                write_samples(buf, channels, &mut calculate_next_sample);
            }
            _ => (),
        }
    })
}

// call get_next_sample to fill the sample buffer
fn write_samples<S, F>(mut buf: cpal::OutputBuffer<S>, channels: usize, get_next_sample: &mut F)
where
    S: cpal::Sample,
    F: FnMut() -> f32,
{
    for sample in buf.chunks_mut(channels) {
        let next_sample = get_next_sample();
        for out in sample.iter_mut() {
            *out = cpal::Sample::from(&next_sample);
        }
    }
}

// use libc to get loadavg
fn load_avg() -> io::Result<(f64, f64, f64)> {
    let mut loads: [f64; 3] = [0.0; 3];
    let loaded = unsafe { getloadavg(&mut loads[0], 3) };
    if loaded != 3 {
        return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"));
    }
    Ok((loads[0], loads[1], loads[2]))
}

#[link(name = "c")]
extern "C" {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
}
