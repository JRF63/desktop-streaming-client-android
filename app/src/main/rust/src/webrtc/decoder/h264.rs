use super::AndroidDecoder;
use webrtc_helper::codecs::{
    h264::{H264Codec, H264Depacketizer},
    util::nalu_chunks,
};

const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;
const NALU_DELIMITER: [u8; 4] = [0, 0, 0, 1];

#[derive(Default)]
pub struct H264Decoder {
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    codec_config: Option<Vec<u8>>,
    resolution: Option<(i32, i32)>,
}

impl AndroidDecoder for H264Decoder {
    type DepacketizerType<'a> = H264Depacketizer<'a>;

    fn init_done(&self) -> bool {
        self.codec_config.is_some() && self.resolution.is_some()
    }

    fn resolution(&self) -> Option<(i32, i32)> {
        self.resolution
    }

    fn codec_config(&self) -> Option<&[u8]> {
        self.codec_config.as_ref().map(|x| x.as_slice())
    }

    fn read_payload(&mut self, payload: &[u8]) -> Result<(), ()> {
        if payload.is_empty() {
            return Err(());
        }
        for nalu in nalu_chunks(payload) {
            match nalu[0] & NALU_TYPE_BITMASK {
                NALU_TYPE_SPS => {
                    if let Some((width, height)) = H264Codec::get_resolution(nalu) {
                        self.resolution = Some((width as i32, height as i32));
                        self.sps = Some(nalu.to_vec());
                        self.build_codec_config();
                    }
                }
                NALU_TYPE_PPS => {
                    self.pps = Some(nalu.to_vec());
                    self.build_codec_config();
                }
                _ => return Err(()),
            }
        }
        Ok(())
    }
}

impl H264Decoder {
    fn build_codec_config(&mut self) {
        if self.sps.is_some() && self.pps.is_some() && self.resolution.is_some() {
            let sps = self.sps.as_ref().unwrap();
            let pps = self.pps.as_ref().unwrap();
            let mut codec_config =
                Vec::with_capacity(2 * NALU_DELIMITER.len() + sps.len() + pps.len());

            codec_config.extend_from_slice(&NALU_DELIMITER);
            codec_config.extend_from_slice(sps);
            codec_config.extend_from_slice(&NALU_DELIMITER);
            codec_config.extend_from_slice(pps);

            self.codec_config = Some(codec_config);
        }
    }
}
