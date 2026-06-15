use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ogg::reading::PacketReader;
use opus_rs::OpusDecoder;
use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SAMPLE_RATE: u32 = 48_000;
const FRAME_SIZE: usize = 960;
const FRAME_QUEUE_CAPACITY: usize = 8;
const STARTUP_FRAME_WAIT: Duration = Duration::from_secs(1);

const PRODUCER_RUNNING: u8 = 0;
const PRODUCER_COMPLETED: u8 = 1;
const PRODUCER_FAILED: u8 = 2;

type PcmFrame = Vec<f32>;

pub fn get_opus_info(file_path: &str) -> Result<(u64, f64), Box<dyn std::error::Error>> {
    let file = File::open(file_path)?;
    let file_size = file.metadata()?.len();

    // Count the number of audio packets to estimate duration
    let reader = BufReader::new(file);
    let mut packet_reader = PacketReader::new(reader);

    // Skip headers
    packet_reader.read_packet()?; // OpusHead
    packet_reader.read_packet()?; // OpusTags

    let mut packet_count = 0;
    while let Ok(Some(_)) = packet_reader.read_packet() {
        packet_count += 1;
    }

    // Each packet is 20ms of audio
    let duration = (packet_count as f64) * 0.02;

    Ok((file_size, duration))
}

pub fn playback_opus(file_path: &str, is_playing_flag: Arc<AtomicBool>) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| "Failed to get default output device".to_string())?;
    let config = device.default_output_config().map_err(|err| err.to_string())?;

    let output_config = cpal::StreamConfig {
        channels: config.channels(),
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };
    let output_channels = output_config.channels as usize;

    let (frame_tx, frame_rx) = sync_channel(FRAME_QUEUE_CAPACITY);
    let producer_state = Arc::new(AtomicU8::new(PRODUCER_RUNNING));
    let producer_error = Arc::new(Mutex::new(None));
    let producer_handle = spawn_frame_producer(
        file_path,
        frame_tx,
        Arc::clone(&is_playing_flag),
        Arc::clone(&producer_state),
        Arc::clone(&producer_error),
    );

    let mut current_frame = wait_for_startup_frame(
        &frame_rx,
        &producer_state,
        &producer_error,
        &is_playing_flag,
    )?;
    if current_frame.is_none() {
        join_frame_producer(producer_handle)?;
        return Ok(());
    }
    let mut frame_position = 0usize;

    let callback_error = Arc::new(Mutex::new(None));

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let is_playing = Arc::clone(&is_playing_flag);
            let is_playing_errors = Arc::clone(&is_playing_flag);
            let producer_state = Arc::clone(&producer_state);
            let callback_error = Arc::clone(&callback_error);
            let callback_error_errors = Arc::clone(&callback_error);

            device.build_output_stream(
                &output_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if !is_playing.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }

                    let mut pos = 0;
                    while pos < data.len() {
                        let needs_frame = current_frame
                            .as_ref()
                            .map(|frame| frame_position >= frame.len())
                            .unwrap_or(true);
                        if needs_frame {
                            match frame_rx.try_recv() {
                                Ok(frame) => {
                                    current_frame = Some(frame);
                                    frame_position = 0;
                                    continue;
                                }
                                Err(TryRecvError::Empty) => {
                                    let state = producer_state.load(Ordering::Relaxed);
                                    if state == PRODUCER_COMPLETED || state == PRODUCER_FAILED {
                                        is_playing.store(false, Ordering::Relaxed);
                                    }
                                    break;
                                }
                                Err(TryRecvError::Disconnected) => {
                                    if producer_state.load(Ordering::Relaxed) == PRODUCER_RUNNING {
                                        store_error_once(
                                            callback_error.as_ref(),
                                            "playback producer disconnected unexpectedly"
                                                .to_string(),
                                        );
                                    }
                                    is_playing.store(false, Ordering::Relaxed);
                                    break;
                                }
                            }
                        }

                        let Some(frame) = current_frame.as_ref() else {
                            break;
                        };
                        let copied = copy_frame_to_output(
                            &mut data[pos..],
                            output_channels,
                            frame,
                            &mut frame_position,
                        );
                        if copied == 0 {
                            current_frame = None;
                            continue;
                        }
                        pos += copied;
                    }

                    data[pos..].fill(0.0);
                },
                move |err| {
                    store_error_once(
                        callback_error_errors.as_ref(),
                        format!("stream callback error: {err:?}"),
                    );
                    is_playing_errors.store(false, Ordering::Relaxed);
                },
                None,
            )
            .map_err(|err| err.to_string())?
        },
        _ => return Err("Unsupported output format".to_string()),
    };

    stream.play().map_err(|err| err.to_string())?;

    while is_playing_flag.load(Ordering::Relaxed) {
        if callback_error.lock().is_ok_and(|error| error.is_some()) {
            is_playing_flag.store(false, Ordering::Relaxed);
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    drop(stream);
    join_frame_producer(producer_handle)?;

    if let Some(error) = take_error(callback_error.as_ref()) {
        return Err(error);
    }

    if producer_state.load(Ordering::Relaxed) == PRODUCER_FAILED {
        return Err(
            take_error(producer_error.as_ref())
                .unwrap_or_else(|| "playback producer failed".to_string()),
        );
    }

    Ok(())
} 

fn spawn_frame_producer(
    file_path: &str,
    frame_tx: SyncSender<PcmFrame>,
    is_playing: Arc<AtomicBool>,
    producer_state: Arc<AtomicU8>,
    producer_error: Arc<Mutex<Option<String>>>,
) -> thread::JoinHandle<()> {
    let file_path = file_path.to_string();
    thread::spawn(move || {
        match produce_frames(&file_path, frame_tx, is_playing) {
            Ok(()) => producer_state.store(PRODUCER_COMPLETED, Ordering::Relaxed),
            Err(err) => {
                store_error_once(producer_error.as_ref(), err);
                producer_state.store(PRODUCER_FAILED, Ordering::Relaxed);
            }
        }
    })
}

fn produce_frames(
    file_path: &str,
    frame_tx: SyncSender<PcmFrame>,
    is_playing: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut decoder = OpusDecoder::new(SAMPLE_RATE as i32, 1).map_err(|err| err.to_string())?;
    let file = BufReader::new(File::open(file_path).map_err(|err| err.to_string())?);
    let mut packet_reader = PacketReader::new(file);
    let mut decode_buffer = vec![0f32; FRAME_SIZE];

    packet_reader
        .read_packet()
        .map_err(|err| err.to_string())?;
    packet_reader
        .read_packet()
        .map_err(|err| err.to_string())?;

    while is_playing.load(Ordering::Relaxed) {
        let Some(packet) = packet_reader.read_packet().map_err(|err| err.to_string())? else {
            return Ok(());
        };
        if packet.data.is_empty() {
            continue;
        }
        let n_samples = decoder
            .decode(&packet.data, FRAME_SIZE, &mut decode_buffer)
            .map_err(|err| err.to_string())?;
        if n_samples == 0 {
            continue;
        }
        if frame_tx.send(decode_buffer[..n_samples].to_vec()).is_err() {
            return Ok(());
        }
    }

    Ok(())
}

fn wait_for_startup_frame(
    frame_rx: &Receiver<PcmFrame>,
    producer_state: &Arc<AtomicU8>,
    producer_error: &Arc<Mutex<Option<String>>>,
    is_playing: &Arc<AtomicBool>,
) -> Result<Option<PcmFrame>, String> {
    loop {
        if !is_playing.load(Ordering::Relaxed) {
            return Ok(None);
        }

        match frame_rx.recv_timeout(STARTUP_FRAME_WAIT) {
            Ok(frame) => return Ok(Some(frame)),
            Err(RecvTimeoutError::Timeout) => match producer_state.load(Ordering::Relaxed) {
                PRODUCER_COMPLETED => return Ok(None),
                PRODUCER_FAILED => {
                    return Err(
                        take_error(producer_error.as_ref())
                            .unwrap_or_else(|| "playback producer failed".to_string()),
                    );
                }
                _ => continue,
            },
            Err(RecvTimeoutError::Disconnected) => match producer_state.load(Ordering::Relaxed) {
                PRODUCER_COMPLETED => return Ok(None),
                PRODUCER_FAILED => {
                    return Err(
                        take_error(producer_error.as_ref())
                            .unwrap_or_else(|| "playback producer failed".to_string()),
                    );
                }
                _ => {
                    return Err("playback producer disconnected unexpectedly".to_string())
                }
            },
        }
    }
}

fn join_frame_producer(handle: thread::JoinHandle<()>) -> Result<(), String> {
    handle
        .join()
        .map_err(|_| "failed to join playback producer thread".to_string())
}

fn copy_frame_to_output(
    data: &mut [f32],
    channels: usize,
    frame: &[f32],
    frame_position: &mut usize,
) -> usize {
    let remaining_samples = frame.len().saturating_sub(*frame_position);
    if remaining_samples == 0 {
        return 0;
    }

    let samples_to_copy = (data.len() / channels).min(remaining_samples);
    for i in 0..samples_to_copy {
        let sample = frame[*frame_position + i];
        for c in 0..channels {
            data[i * channels + c] = sample;
        }
    }

    *frame_position += samples_to_copy;
    samples_to_copy * channels
}

fn store_error_once(slot: &Mutex<Option<String>>, message: String) {
    if let Ok(mut error) = slot.lock() {
        if error.is_none() {
            *error = Some(message);
        }
    }
}

fn take_error(slot: &Mutex<Option<String>>) -> Option<String> {
    slot.lock().ok()?.take()
}

#[cfg(test)]
mod tests {
    use super::{copy_frame_to_output, produce_frames};
    use crate::opus_encoder::OpusEncoder;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::sync_channel;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn produce_frames_skips_empty_end_of_stream_packet() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "rustic-audio-eos-test-{unique_suffix}"
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wav_path = temp_dir.join("test.wav");
        let opus_path = temp_dir.join("test.opus");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        for _ in 0..(960 * 2) {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        OpusEncoder::new()
            .encode_wav_to_opus(
                wav_path.to_str().unwrap(),
                opus_path.to_str().unwrap(),
            )
            .unwrap();

        let (frame_tx, frame_rx) = sync_channel(8);
        let is_playing = Arc::new(AtomicBool::new(true));
        produce_frames(
            opus_path.to_str().unwrap(),
            frame_tx,
            Arc::clone(&is_playing),
        )
        .expect("playback should not fail on empty EOS packet");

        let frame_count = frame_rx.iter().count();
        assert!(
            frame_count > 0,
            "expected at least one decoded audio frame, got {frame_count}"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn copy_frame_to_output_expands_mono_samples_across_channels() {
        let mut output = [0.0; 6];
        let mut frame_position = 0;

        let copied = copy_frame_to_output(&mut output, 2, &[0.25, -0.5, 1.0], &mut frame_position);

        assert_eq!(copied, 6);
        assert_eq!(frame_position, 3);
        assert_eq!(output, [0.25, 0.25, -0.5, -0.5, 1.0, 1.0]);
    }

    #[test]
    fn copy_frame_to_output_respects_existing_frame_offset() {
        let mut output = [0.0; 4];
        let mut frame_position = 1;

        let copied = copy_frame_to_output(&mut output, 2, &[0.1, 0.2, 0.3], &mut frame_position);

        assert_eq!(copied, 4);
        assert_eq!(frame_position, 3);
        assert_eq!(output, [0.2, 0.2, 0.3, 0.3]);
    }
}