use ogg::{PacketWriter, writing::PacketWriteEndInfo};
use opus_rs::{Application, OpusEncoder as CodecEncoder};
use std::fs::File;
use std::io::BufWriter;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OpusEncodingMode {
    Cbr,
    Vbr,
}

#[derive(Clone)]
pub struct OpusEncoder {
    bitrate: i32,
    mode: OpusEncodingMode,
    vbr_quality: i32,
}

impl OpusEncoder {
    pub fn new() -> Self {
        Self {
            bitrate: 12000, // Default 12kbps
            mode: OpusEncodingMode::Cbr,
            vbr_quality: 5,
        }
    }

    // Add setter for bitrate
    pub fn set_bitrate(&mut self, bitrate: i32) {
        self.bitrate = bitrate;
    }

    // Get current bitrate
    pub fn get_bitrate(&self) -> i32 {
        self.bitrate
    }

    pub fn set_mode(&mut self, mode: OpusEncodingMode) {
        self.mode = mode;
    }

    pub fn get_mode(&self) -> OpusEncodingMode {
        self.mode
    }

    pub fn set_vbr_quality(&mut self, quality: i32) {
        self.vbr_quality = quality.clamp(0, 10);
    }

    pub fn get_vbr_quality(&self) -> i32 {
        self.vbr_quality
    }

    pub fn encode_wav_to_opus(&self, input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Read the WAV file
        let mut reader = hound::WavReader::open(input_path)?;
        let spec = reader.spec();
        
        // Convert to 48kHz mono if needed
        let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
            reader.samples::<f32>().map(|s| s.unwrap()).collect()
        } else {
            reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
        };
        
        // Convert to mono if stereo
        let mono_samples: Vec<f32> = if spec.channels == 2 {
            samples.chunks(2)
                .map(|chunk| chunk[0]) // Take left channel
                .collect()
        } else {
            samples
        };
        
        // Resample to 48kHz if needed
        let resampled_samples = if spec.sample_rate != 48000 {
            let input_duration = mono_samples.len() as f32 / spec.sample_rate as f32;
            let output_len = (input_duration * 48000.0) as usize;
            let scale = (mono_samples.len() - 1) as f32 / (output_len - 1) as f32;
            
            let mut output = Vec::with_capacity(output_len);
            for i in 0..output_len {
                let pos = i as f32 * scale;
                let index = pos as usize;
                let frac = pos - index as f32;
                
                let sample = if index + 1 < mono_samples.len() {
                    mono_samples[index] * (1.0 - frac) + mono_samples[index + 1] * frac
                } else {
                    mono_samples[index]
                };
                
                output.push(sample);
            }
            output
        } else {
            mono_samples
        };
        
        let mut encoder = CodecEncoder::new(48_000, 1, Application::Audio)
            .map_err(std::io::Error::other)?;
        encoder.bitrate_bps = self.bitrate;
        encoder.use_cbr = matches!(self.mode, OpusEncodingMode::Cbr);
        encoder.complexity = self.vbr_quality;
        
        println!("Converting to Opus:");
        println!("  Mode: {:?}", self.mode_name());
        println!("  Bitrate target: {} bps", self.bitrate);
        println!("  VBR quality: {}", self.vbr_quality);
        println!("  Frame size: 960 samples (20ms at 48kHz)");
        println!("  Total frames: {}", resampled_samples.len() / 960);

        let file = BufWriter::new(File::create(output_path)?);
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        let mut packet_writer = PacketWriter::new(file);

        // Opus header
        let mut id_header = Vec::new();
        id_header.extend_from_slice(b"OpusHead");
        id_header.push(1);  // Version
        id_header.push(1);  // Channel count
        id_header.extend_from_slice(&(0u16).to_le_bytes());  // Pre-skip
        id_header.extend_from_slice(&(48000u32).to_le_bytes());  // Input sample rate
        id_header.extend_from_slice(&[0, 0]);  // Output gain
        id_header.push(0);  // Channel mapping family

        packet_writer.write_packet(
            id_header,
            serial,
            PacketWriteEndInfo::EndPage,
            0
        )?;

        // Comment header
        let mut comment_header = Vec::new();
        comment_header.extend_from_slice(b"OpusTags");
        let vendor = b"rustic_audio";
        comment_header.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        comment_header.extend_from_slice(vendor);
        comment_header.extend_from_slice(&[0, 0, 0, 0]);

        packet_writer.write_packet(
            comment_header,
            serial,
            PacketWriteEndInfo::EndPage,
            0
        )?;

        let frame_size = 960;  // 20ms at 48kHz
        let mut input_buffer = vec![0.0f32; frame_size];
        let mut encoded_data = vec![0u8; 1275];
        let mut granulepos = 0i64;
        let frames: Vec<&[f32]> = resampled_samples.chunks(frame_size).collect();

        for (index, chunk) in frames.iter().enumerate() {
            input_buffer.clear();
            input_buffer.extend(*chunk);
            if input_buffer.len() < frame_size {
                input_buffer.resize(frame_size, 0.0);
            }

            let encoded_len = encoder
                .encode(&input_buffer, frame_size, &mut encoded_data)
                .map_err(std::io::Error::other)?;
            let encoded_packet = &encoded_data[..encoded_len];

            granulepos += frame_size as i64;

            let end_info = if index + 1 == frames.len() {
                PacketWriteEndInfo::EndStream
            } else {
                PacketWriteEndInfo::NormalPacket
            };

            packet_writer.write_packet(
                encoded_packet.to_vec(),
                serial,
                end_info,
                granulepos as u64
            )?;
        }

        let final_duration = granulepos as f32 / 48000.0;
        println!("Final Opus duration: {} seconds", final_duration);

        Ok(())
    }

    fn mode_name(&self) -> &'static str {
        match self.mode {
            OpusEncodingMode::Cbr => "CBR",
            OpusEncodingMode::Vbr => "VBR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OpusEncoder;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use ogg::reading::PacketReader;
    use std::fs;
    use std::io::BufReader;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn encode_wav_to_opus_does_not_write_trailing_empty_packet() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "rustic-audio-encoder-test-{unique_suffix}"
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

        let file = BufReader::new(fs::File::open(&opus_path).unwrap());
        let mut packet_reader = PacketReader::new(file);
        packet_reader.read_packet().unwrap();
        packet_reader.read_packet().unwrap();

        let mut last_audio_packet = None;
        while let Ok(Some(packet)) = packet_reader.read_packet() {
            last_audio_packet = Some(packet);
        }

        let last_packet = last_audio_packet.expect("expected at least one audio packet");
        assert!(
            !last_packet.data.is_empty(),
            "last audio packet should not be empty"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }
}