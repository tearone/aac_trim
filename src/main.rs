use adts::Adts;
use fdk_aac::dec::{Decoder, DecoderError, Transport};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, Write};
use std::{env, vec};
use waveform::Waveform;

mod adts;
mod waveform;

fn main() {
    let args = env::args().collect::<Vec<String>>();
    let input_path = args[1].as_str();
    let output_path = args[2].as_str();
    let waveform_path = args[3].as_str();
    let ffmpeg_path = args[4].as_str();

    let tmp_path = &format!("{}.fixed.m4a", input_path);
    let mut process = subprocess::Popen::create(
        &[
            ffmpeg_path,
            "-y",
            "-hide_banner",
            "-loglevel",
            "repeat+info",
            "-vn",
            "-i",
            input_path,
            "-f",
            "mp4",
            "-map",
            "0",
            "-dn",
            "-ignore_unknown",
            "-c",
            "copy",
            tmp_path,
        ],
        subprocess::PopenConfig::default(),
    )
    .expect("Failed to open ffmpeg");
    process.wait().expect("Failed to run ffmpeg");
    fs::rename(tmp_path, input_path).expect("Failed to rename input file");

    let file = File::open(input_path).expect("Error opening file");

    let metadata = file.metadata().expect("Error getting file metadata");
    let size = metadata.len();
    let buf = BufReader::new(file);

    let mut decoder = MpegAacDecoder::new(buf, size).expect("Error creating decoder");
    if let Some(waveform) = decoder.process(output_path) {
        let mut waveform_file =
            File::create(waveform_path).expect("Failed to create waveform file");
        waveform_file
            .write_all(&waveform)
            .expect("Failed to write waveform file");
    }
}

pub struct MpegAacDecoder<R>
where
    R: Read + Seek,
{
    mp4_reader: mp4::Mp4Reader<R>,
    decoder: Decoder,
    track_id: u32,
    position: u32,
}

impl<R> MpegAacDecoder<R>
where
    R: Read + Seek,
{
    pub fn new(reader: R, size: u64) -> Result<Self, &'static str> {
        let decoder = Decoder::new(Transport::Adts);
        let mp4 = mp4::Mp4Reader::read_header(reader, size).or(Err("Error reading MPEG header"))?;
        let mut track_id: Option<u32> = None;
        {
            for (_, track) in mp4.tracks().iter() {
                let media_type = track.media_type().or(Err("Error getting media type"))?;
                match media_type {
                    mp4::MediaType::AAC => {
                        track_id = Some(track.track_id());
                        break;
                    }
                    _ => {}
                }
            }
        }
        match track_id {
            Some(track_id) => {
                let aac = MpegAacDecoder {
                    mp4_reader: mp4,
                    decoder: decoder,
                    track_id: track_id.clone(),
                    position: 1,
                };

                return Ok(aac);
            }
            None => {
                return Err("No aac track found");
            }
        }
    }

    pub fn process(&mut self, output_path: &str) -> Option<Vec<u8>> {
        let tracks = self.mp4_reader.tracks();
        let track = tracks.get(&self.track_id).expect("Track ID not found");
        let timescale = track.timescale();
        let duration = self.mp4_reader.duration().as_secs();
        let audio_profile = track.audio_profile().unwrap();
        let sample_freq_index = track.sample_freq_index().unwrap();
        let channel_config = track.channel_config().unwrap();

        let mut output = File::create(output_path).unwrap();

        let mut pcm = vec![0; 8192];
        let mut frame: Vec<u8>;
        let mut waveform = Vec::<f64>::new();

        loop {
            let result = match self.decoder.decode_frame(&mut pcm) {
                Err(DecoderError::NOT_ENOUGH_BITS) => {
                    let sample_result = self.mp4_reader.read_sample(self.track_id, self.position);
                    let sample = match sample_result
                        .expect(&format!("Error reading sample at {}", self.position))
                    {
                        Some(sample) => sample,
                        _ => {
                            let mut avg = Vec::<f64>::new();
                            let mut max = 0_f64;
                            for offset in 1..50 {
                                let (val, maxval) = Waveform::avgmax(
                                    &waveform[(offset - 1) * (waveform.len() / 50)
                                        ..offset * (waveform.len() / 50)],
                                );
                                if maxval > max {
                                    max = maxval
                                }
                                avg.push(val);
                            }
                            return Some(Waveform::balance(&avg, max));
                        }
                    };
                    let adts_header = Adts::construct_adts_header(
                        &sample,
                        &audio_profile,
                        &channel_config,
                        &sample_freq_index,
                    )
                    .expect("ADTS bytes");
                    let adts_bytes = mp4::Bytes::copy_from_slice(&adts_header);
                    frame = [adts_bytes, sample.bytes].concat();
                    self.position += 1;
                    let _bytes_read = match self.decoder.fill(&frame) {
                        Ok(bytes_read) => bytes_read,
                        Err(err) => {
                            println!("DecoderError (read): {}", err);
                            return None;
                        }
                    };
                    let res = self.decoder.decode_frame(&mut pcm);
                    let silent_samples = pcm.iter().filter(|e| e == &&0).count();

                    let position = self.position * self.mp4_reader.timescale() / timescale;

                    if silent_samples < pcm.len() / 2
                        || position > 20 && (position as u64) < duration - 30
                    {
                        output
                            .write_all(&frame)
                            .expect("Could not write output file");

                        let mut avg = Vec::<f64>::new();
                        for offset in 1..10 {
                            avg.push(Waveform::avgfilter(
                                &pcm[(offset - 1) * (pcm.len() / 10)..offset * (pcm.len() / 10)],
                            ));
                        }
                        if let Some(chunk) = Waveform::peak_range(&avg) {
                            waveform.push(chunk);
                        }
                    }
                    res
                }
                val => val,
            };
            if let Err(err) = result {
                println!("DecoderError (result): {}", err);
                return None;
            }
            let decoded_fram_size = self.decoder.decoded_frame_size();
            if decoded_fram_size < pcm.len() {
                let _ = pcm.split_off(decoded_fram_size);
            }
        }
    }
}
