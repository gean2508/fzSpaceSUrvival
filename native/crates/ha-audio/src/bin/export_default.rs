//! Imprime el `AudioSpec` default como JSON — para (re)generar
//! `native/assets/audio.json`:
//!   cargo run -p ha-audio --bin export_default > assets/audio.json

fn main() {
    println!("{}", ha_audio::AudioSpec::default_spec().to_json());
}
