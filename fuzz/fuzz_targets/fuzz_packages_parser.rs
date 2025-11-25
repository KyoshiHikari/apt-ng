#![no_main]
use libfuzzer_sys::fuzz_target;
use apt_ng::apt_parser::parse_packages_file;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(content) = std::str::from_utf8(data) {
        // Try to parse the Packages file
        // We don't care about the result, just that it doesn't crash
        let _ = parse_packages_file(content);
    }
});

