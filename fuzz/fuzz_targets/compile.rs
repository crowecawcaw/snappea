#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| snappea_fuzz::compile(data));
