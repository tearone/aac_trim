use adts::Adts;
use fdk_aac::dec::{Decoder, DecoderError, Transport};
use std::fs::File;
use std::io::{BufReader, Read, Seek, Write};
use std::{env, vec};

mod adts;

fn main() {
    let args = env::args().collect::<Vec<String>>();
    let input_path = args[1].as_str();
    let output_path = args[2].as_str();
    let file = File::open(input_path).expect("Error opening file");

    let metadata = file.metadata().expect("Error getting file metadata");
    let size = metadata.len();
    let buf = BufReader::new(file);

    let mut decoder = MpegAacDecoder::new(buf, size).expect("Error creating decoder");
    decoder.decode_adts_stream_waveform_silence(output_path);
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

    pub fn decode_adts_stream_waveform_silence(&mut self, output_path: &str) -> Option<()> {
        let tracks = self.mp4_reader.tracks();
        let track = tracks.get(&self.track_id).expect("Track ID not found");
        let timescale = track.timescale();
        let duration = self.mp4_reader.duration().as_secs();
        let audio_profile = track.audio_profile().unwrap();
        let sample_freq_index = track.sample_freq_index().unwrap();
        let channel_config = track.channel_config().unwrap();

        let mut output = File::create(output_path).unwrap();

        let mut pcm = vec![0; 2048];
        let mut frame: Vec<u8>;

        loop {
            let result = match self.decoder.decode_frame(&mut pcm) {
                Err(DecoderError::NOT_ENOUGH_BITS) => {
                    let sample_result = self.mp4_reader.read_sample(self.track_id, self.position);
                    let sample = sample_result.expect("Error reading sample")?;
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
                        Err(_) => return None,
                    };
                    let res = self.decoder.decode_frame(&mut pcm);
                    let silent_samples = pcm.iter().filter(|e| e == &&0).count();

                    let position = self.position * self.mp4_reader.timescale() / timescale;

                    if silent_samples < pcm.len() / 2
                        || position > 15 && (position as u64) < duration - 15
                    {
                        output.write_all(&frame).expect("Could not write file");
                    }
                    res
                }
                val => val,
            };
            if let Err(err) = result {
                println!("DecoderError: {}", err);
                return None;
            }
            let decoded_fram_size = self.decoder.decoded_frame_size();
            if decoded_fram_size < pcm.len() {
                let _ = pcm.split_off(decoded_fram_size);
            }
        }
    }
}
