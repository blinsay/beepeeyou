#![allow(dead_code, unused_variables, unused_imports, unused_mut)]
extern crate cpal;
extern crate libc;

use std::f32::consts::PI;
use libc::c_int;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::Duration;

#[derive(Debug)]
enum BeepError {
    NoOutputDevice,
    DeviceError { reason: &'static str },
}

fn main() {
    let load_mult: f32 = 10.0;
    let load = Arc::new(AtomicUsize::new(1));
    let current_frequency = Arc::new(AtomicUsize::new(440));

    let write_load = load.clone();
    spawn(move || loop {
        set_load(load_mult as f64, &write_load);
        sleep(Duration::from_secs(1));
    });

    let (event_loop, stream_id, format) =
        default_device_loop().expect("couldn't set up the default audio loop!");

    let read_load = load.clone();
    let sample_rate = format.sample_rate.0 as f32;
    output_beepeeyou(event_loop, stream_id, &format, move || {
        get_load(load_mult as f64, &read_load) as f32
    });
}

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

    event_loop.run(move |id, data| {
        if id != output_stream {
            return;
        }

        use cpal::StreamData::*;
        use cpal::UnknownTypeOutputBuffer::*;

        match data {
            Output { buffer: U16(mut buf) } => {
                for sample in buf.chunks_mut(channels) {
                    let next_sample = ((sample_clock / sample_rate) * frequency * 2.0 * PI).sin();
                    sample_clock = (sample_clock + 1.0) % sample_rate;

                    if sample_clock % 1000.0 == 0.0 {
                        frequency = 220f32 * get_frequency();
                    }

                    for out in sample.iter_mut() {
                        *out = ((next_sample * 0.5 + 0.5) * (std::u16::MAX as f32)) as u16;
                    }
                }
            },
            Output { buffer: F32(mut buf) } => {
                for sample in buf.chunks_mut(channels) {
                    let next_sample = ((sample_clock / sample_rate) * frequency * 2.0 * PI).sin();
                    sample_clock = (sample_clock + 1.0) % sample_rate;

                    if sample_clock % 1000.0 == 0.0 {
                        frequency = 220f32 * get_frequency();
                    }

                    for out in sample.iter_mut() {
                        *out = next_sample;
                    }
                }
            },
            _ => (),
        }
    })
}

// fn write_samples<S, T>(
//     buf: &mut cpal::OutputBuffer<S>,
//     clock: f32,
//     sample_rate: f32,
//     channels: usize,
//     frequency: f32
// ) -> f32
// where
//     S: cpal::Sample,
//     T: FnMut(f32) -> f32,
// {
//     let mut clock = clock;
//     for sample in buf.chunks_mut(channels) {
//         let value = next_value(clock);
//         for out in sample.iter_mut() {
//             *out = S::from(&value);
//         }
//         clock = (clock + 1.0) % sample_rate;
//     }
//     clock
// }

fn set_load(mult: f64, into: &AtomicUsize) {
    if let Ok((one_min, _five, _fifteen)) = load_avg() {
        let rounded = (one_min * mult).round() as usize;
        into.store(rounded, Ordering::Release);
    }
}

fn get_load(mult: f64, from: &AtomicUsize) -> f64 {
    (from.load(Ordering::Acquire) as f64) / mult
}

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
