#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    // Create a temporary file with the fuzzed data
    if let Ok(mut temp_file) = NamedTempFile::new() {
        if temp_file.write_all(data).is_ok() && temp_file.flush().is_ok() {
            let path = temp_file.path();
            
            // Try to open as .apx package
            // We don't care about the result, just that it doesn't crash
            let _ = apt_ng::package::ApxPackage::open(path);
        }
    }
});

