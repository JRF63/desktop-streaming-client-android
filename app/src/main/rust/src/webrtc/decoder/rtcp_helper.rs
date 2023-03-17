use super::DecoderError;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use webrtc::rtcp::{self, payload_feedbacks::picture_loss_indication::PictureLossIndication};
use webrtc_helper::WebRtcPeer;

pub struct RateLimitedPli {
    rtcp_packets: [Box<dyn rtcp::packet::Packet + Send + Sync>; 1],
    last_pli_time: SystemTime,
    pli_interval: Duration,
}

impl RateLimitedPli {
    pub fn new(media_ssrc: u32, pli_interval: Duration) -> RateLimitedPli {
        let pli = PictureLossIndication {
            sender_ssrc: 0,
            media_ssrc,
        };
        RateLimitedPli {
            rtcp_packets: [Box::new(pli) as _],
            last_pli_time: SystemTime::UNIX_EPOCH,
            pli_interval,
        }
    }

    pub async fn send(&mut self, peer: &Arc<WebRtcPeer>) -> Result<(), DecoderError> {
        let now = SystemTime::now();
        if let Ok(duration) = now.duration_since(self.last_pli_time) {
            if duration > self.pli_interval {
                peer.write_rtcp(&self.rtcp_packets).await?;
                self.last_pli_time = now;
            }
        }
        Ok(())
    }
}
