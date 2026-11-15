#![cfg(target_arch = "wasm32")]

use bevy::audio::{AudioSource, Decodable, Source};
use salmong_game::sound::{Cue, wav};
use wasm_bindgen_test::wasm_bindgen_test;

const SOUNDS: [(Cue, usize); 6] = [
    (Cue::Board, 1_102),
    (Cue::Deliver, 2_646),
    (Cue::Station, 6_615),
    (Cue::Overflow, 6_615),
    (Cue::Reject, 3_307),
    (Cue::Over, 19_845),
];

#[wasm_bindgen_test]
fn each_wav_decodes_to_its_sample_count() {
    for (cue, samples) in SOUNDS {
        let source = AudioSource {
            bytes: wav(cue).into(),
        };
        let decoder = source.decoder();
        assert_eq!(decoder.sample_rate().get(), 22_050, "{cue:?}");
        assert_eq!(decoder.channels().get(), 1, "{cue:?}");
        assert_eq!(decoder.count(), samples, "{cue:?}");
    }
}
