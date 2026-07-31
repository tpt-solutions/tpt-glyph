// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — fuzz target: PostScript interpreter.
//
// Fuzzes the untrusted PostScript input path with strict resource limits
// (Phase 10). A crash, hang, or unbounded memory growth here is a security bug.
// Run with: `cargo +nightly fuzz run ps_interpreter`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Treat the input as UTF-8 text (PostScript is a text language). Invalid
    // bytes are replaced, which still exercises the tokenizer's error paths.
    let src = String::from_utf8_lossy(data);
    let mut interp = tpt_glyph_ps::Interpreter::with_limits(
        600,
        800,
        tpt_glyph_ps::ResourceLimits::strict(),
    );
    // Errors are expected and must NOT panic/abort. We only care about crashes,
    // hangs, or OOM.
    let _ = interp.run_source(&src);
});
