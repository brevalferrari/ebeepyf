use crate::BEEPS_FREQ_RANGE;
use rodio::{
    source::{Amplify, Mix, SineWave},
    Source,
};
type SineMix =
    Mix<Mix<Mix<Amplify<SineWave>, Amplify<SineWave>>, Amplify<SineWave>>, Amplify<SineWave>>;

pub(super) fn per_ip_sine(ip: [u8; 4]) -> SineMix {
    SineWave::new(u8_to_freq(ip[0]))
        .amplify(0.2)
        .mix(SineWave::new(u8_to_freq(ip[1])).amplify(0.2))
        .mix(SineWave::new(u8_to_freq(ip[2])).amplify(0.2))
        .mix(SineWave::new(u8_to_freq(ip[3])).amplify(0.2))
}

fn u8_to_freq(n: u8) -> f32 {
    n as f32 * ((BEEPS_FREQ_RANGE.1 - BEEPS_FREQ_RANGE.0) / u8::MAX as f32) + BEEPS_FREQ_RANGE.0
}
