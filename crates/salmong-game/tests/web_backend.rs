#![cfg(target_arch = "wasm32")]

use bevy::render::settings::{Backends, WgpuSettings};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn web_build_selects_webgpu_only() {
    assert_eq!(
        WgpuSettings::default().backends,
        Some(Backends::BROWSER_WEBGPU)
    );
}
