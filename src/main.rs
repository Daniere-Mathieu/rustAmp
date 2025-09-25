fn main() {
    println!("Hello, world!");
    println!("This is a simple Rust program to demonstrate the structure of a main function.");
    configure_cpal();
}

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::PI;
use std::time::Duration;

fn configure_cpal() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let mut supported_configs_range = device
        .supported_output_configs()
        .expect("error while querying configs");
    let supported_config = supported_configs_range
        .next()
        .expect("no supported config?!")
        .with_max_sample_rate();

    let config = supported_config.config();
    let sample_rate = config.sample_rate.0 as f32;
    let freq = 440.0; // A4 note
    let mut phase = 0.0f32;
    let step = 2.0 * PI * freq / sample_rate;

    print!("starting stream");
    let stream = device.build_output_stream(
        &supported_config.config(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            println!("data: {:?}", data);
            for sample in data.iter_mut() {
                *sample = (phase).sin() * 0.2; // 0.2 to avoid clipping
                phase += step;
                if phase > 2.0 * PI {
                    phase -= 2.0 * PI;
                }
            }
        },
        move |err| {
            eprintln!("an error occurred on stream: {}", err);
        },
        None,
    );
    println!(" - done");
    stream.unwrap().play().unwrap();

    std::thread::sleep(Duration::from_secs(30));
}
