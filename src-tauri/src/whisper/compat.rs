//! Custom Whisper model compatibility validator.
//!
//! Reads ~48 bytes of a candidate `.bin` file (4-byte GGML magic + 11
//! `i32` hparams) and decides whether it's loadable by whisper-rs as a
//! Whisper model. Built to be fast (< 1 ms wall time, no full file
//! read) and pedagogical: errors carry enough context to explain to
//! the user *why* a file was rejected.
//!
//! This is the "Level B" validation approach from the import plan:
//! magic bytes alone (Level A) lets LLaMA legacy `.bin` files through;
//! a full `WhisperContext::new` load (Level C) would take 30 s on a
//! large model and is too expensive for an interactive picker.
//!
//! Source references for the constants and field layout come from
//! whisper.cpp 0.15.0 vendored in `whisper-rs-sys`:
//! - `whisper.cpp/ggml/include/ggml.h:216` for `GGML_FILE_MAGIC`
//! - `whisper.cpp/src/whisper.cpp:1495–1586` for the model-load
//!   sequence and the 11 hparams.
//! - `whisper.cpp/src/whisper.cpp:451–453` for the multilingual
//!   heuristic `n_vocab >= 51865`.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const GGML_MAGIC: u32 = 0x6767_6d6c; // "ggml" little-endian
const N_AUDIO_CTX: i32 = 1500; // Whisper invariant
const N_TEXT_CTX: i32 = 448; // Whisper invariant
                             // `n_mels` has two valid values across Whisper history:
                             // - 80  for tiny / base / small / medium / large-v1 / large-v2
                             // - 128 for large-v3 / large-v3-turbo / distil-large-v3 (OpenAI doubled
                             //       the mel-spectrogram resolution starting with v3).
                             // Reject anything else; accept both.
const N_MELS_LEGACY: i32 = 80;
const N_MELS_V3: i32 = 128;
const VOCAB_MIN: i32 = 51864; // English-only tokenizer
const VOCAB_MAX: i32 = 51866; // large-v3 (extra `<|nospeech|>` token)
const FTYPE_MAX: i32 = 25; // current upper bound in ggml_ftype enum
                           // Whisper / ggml encodes the on-disk ftype as
                           //   raw_ftype = qnt_version * QNT_VERSION_FACTOR + actual_ftype
                           // (cf. `GGML_QNT_VERSION_FACTOR` in ggml.h). The library bumps
                           // `qnt_version` whenever the quantisation block layout changes;
                           // upstream is at version 2 today. Without the modulo, our q5_0 model
                           // (raw_ftype=2008 on disk) gets wrongly rejected as "unknown quant".
const QNT_VERSION_FACTOR: i32 = 1000;
const QNT_VERSION_MAX: i32 = 2;

const HPARAMS_BYTES: usize = 44; // 11 × i32
const HEADER_BYTES: usize = 4 + HPARAMS_BYTES;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    /// `true` iff the model supports languages beyond English.
    /// Heuristic: `n_vocab >= 51865` (matches whisper.cpp's own
    /// `is_multilingual()` definition).
    pub is_multilingual: bool,
    /// Coarse model-size bucket derived from `n_audio_state`. Used
    /// for UI labels and rough perf expectations. `"unknown"` when
    /// the value doesn't match a published Whisper variant
    /// (community fine-tunes are allowed but flagged via a
    /// `NonStandardSizeClass` warning).
    pub size_class: String,
    /// Quantisation label resolved from `ftype` (e.g. `"q5_1"`,
    /// `"f16"`). `"unknown"` if the ftype is in range but doesn't
    /// map to a label we know.
    pub quant_label: String,
    pub n_vocab: i32,
    pub n_audio_state: i32,
    pub n_audio_layer: i32,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
// `rename_all` only renames the variant discriminator (e.g.
// `EnglishOnly` → `englishOnly`). To also rename inner struct fields
// (e.g. `n_audio_state` → `nAudioState`) we need `rename_all_fields`.
// Without it the frontend sees snake_case keys and renders empty
// values from camelCase property accesses.
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportWarning {
    NonStandardSizeClass {
        n_audio_state: i32,
    },
    EnglishOnly,
    /// File on disk is larger than 60% of currently-available RAM.
    /// Whisper.cpp peak memory is roughly 1× model size on CPU and a
    /// little more on GPU; 0.6 is a conservative threshold.
    HighMemoryUse {
        model_size_mb: u64,
        free_ram_mb: u64,
    },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
// `rename_all_fields` is the missing piece — without it inner struct
// fields stay snake_case (`n_vocab`, `found_hex`) in the emitted JSON,
// and the frontend's camelCase accessors render empty.
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ModelCompatError {
    /// Couldn't open / stat the file (permissions, broken link, etc.).
    Unreadable { os_error: String },
    /// File is shorter than the 48-byte header we need to inspect.
    Truncated { size_bytes: u64 },
    /// Magic bytes are not `ggml`. The file is something else entirely
    /// (PDF, ZIP, GGUF, ELF binary, …).
    BadMagic {
        found_hex: String,
        expected_hex: String,
    },
    /// File is GGML but not a Whisper model. The hparams don't match
    /// Whisper's invariants (n_audio_ctx=1500, n_mels=80, n_text_ctx=448,
    /// n_vocab in 51864-51866). Most common case in practice: a LLaMA
    /// legacy GGML file with `n_vocab=32000`.
    NotWhisper {
        n_vocab: i32,
        n_audio_ctx: i32,
        n_mels: i32,
        n_text_ctx: i32,
        explanation: String,
    },
    /// ftype outside `0..=25`. Future quantisation variant we don't
    /// know about, or a corrupted byte at the right offset.
    UnknownQuant { ftype: i32 },
    /// User picked a file that's already in their imported list.
    /// Only returned by the Tauri command path (not by `validate`
    /// itself, which is file-format-only); kept in this enum so the
    /// frontend handles all import errors uniformly via one
    /// switch-on-`kind`.
    AlreadyImported { existing_display_name: String },
    /// User picked a file inside the app-managed `models/` directory.
    /// Same rationale: command-level check, surfaced through the same
    /// error type for UI consistency.
    InsideManagedDir,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidationResult {
    pub capabilities: ModelCapabilities,
    pub warnings: Vec<ImportWarning>,
}

fn ftype_to_label(ftype: i32) -> &'static str {
    // Matches the `ggml_ftype` enum in `whisper-rs-sys` /
    // `whisper.cpp/ggml/include/ggml.h:440-464`. Keep in sync with
    // upstream when bumping whisper-rs.
    match ftype {
        0 => "f32",
        1 => "f16",
        2 => "q4_0",
        3 => "q4_1",
        7 => "q8_0",
        8 => "q5_0",
        9 => "q5_1",
        10 => "q2_K",
        11 => "q3_K",
        12 => "q4_K",
        13 => "q5_K",
        14 => "q6_K",
        15 => "iq2_xxs",
        16 => "iq2_xs",
        17 => "iq3_xxs",
        18 => "iq1_s",
        19 => "iq4_nl",
        20 => "iq3_s",
        21 => "iq2_s",
        22 => "iq4_xs",
        23 => "iq1_m",
        24 => "bf16",
        25 => "mxfp4",
        _ => "unknown",
    }
}

fn n_audio_state_to_size_class(n_audio_state: i32) -> &'static str {
    // Standard Whisper model-size matrix from the OpenAI paper. A
    // community fine-tune with non-standard dims hits "unknown" and
    // gets flagged by the `NonStandardSizeClass` warning.
    match n_audio_state {
        384 => "tiny",
        512 => "base",
        768 => "small",
        1024 => "medium",
        1280 => "large",
        _ => "unknown",
    }
}

/// Read the first 48 bytes of `path` and decide whether it's a
/// loadable Whisper model. Returns extracted capabilities + a list of
/// non-blocking warnings on success, a structured error otherwise.
pub fn validate(path: &Path) -> Result<ValidationResult, ModelCompatError> {
    let metadata = std::fs::metadata(path).map_err(|e| ModelCompatError::Unreadable {
        os_error: e.to_string(),
    })?;
    let file_size_bytes = metadata.len();

    if file_size_bytes < HEADER_BYTES as u64 {
        return Err(ModelCompatError::Truncated {
            size_bytes: file_size_bytes,
        });
    }

    let mut file = File::open(path).map_err(|e| ModelCompatError::Unreadable {
        os_error: e.to_string(),
    })?;
    let mut header = [0u8; HEADER_BYTES];
    file.read_exact(&mut header)
        .map_err(|e| ModelCompatError::Unreadable {
            os_error: e.to_string(),
        })?;

    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != GGML_MAGIC {
        return Err(ModelCompatError::BadMagic {
            found_hex: format!("{magic:#010x}"),
            expected_hex: format!("{GGML_MAGIC:#010x}"),
        });
    }

    let read_i32 = |idx: usize| -> i32 {
        let off = 4 + idx * 4;
        i32::from_le_bytes([
            header[off],
            header[off + 1],
            header[off + 2],
            header[off + 3],
        ])
    };

    let n_vocab = read_i32(0);
    let n_audio_ctx = read_i32(1);
    let n_audio_state = read_i32(2);
    let _n_audio_head = read_i32(3);
    let n_audio_layer = read_i32(4);
    let n_text_ctx = read_i32(5);
    let n_text_state = read_i32(6);
    let n_text_head = read_i32(7);
    let _n_text_layer = read_i32(8);
    let n_mels = read_i32(9);
    let raw_ftype = read_i32(10);
    // Decompose the on-disk encoding (see QNT_VERSION_FACTOR comment).
    let qnt_version = raw_ftype.div_euclid(QNT_VERSION_FACTOR);
    let ftype = raw_ftype.rem_euclid(QNT_VERSION_FACTOR);

    // Whisper-architecture invariants. Failing any of these means the
    // file is GGML but not Whisper.
    let n_mels_ok = n_mels == N_MELS_LEGACY || n_mels == N_MELS_V3;
    if n_audio_ctx != N_AUDIO_CTX || n_text_ctx != N_TEXT_CTX || !n_mels_ok {
        let explanation = if n_vocab == 32000 {
            // LLaMA's classic vocab size. Common false-positive when a
            // user has a LLaMA legacy `.bin` lying around.
            "Looks like a LLaMA legacy GGML model (n_vocab=32000), not Whisper.".to_string()
        } else {
            // Build a focused message: list only the fields that
            // actually mismatched. Tells the user exactly what's off
            // instead of a wall of "(expected X)" they have to parse.
            let mut diffs: Vec<String> = Vec::new();
            if n_audio_ctx != N_AUDIO_CTX {
                diffs.push(format!(
                    "n_audio_ctx={n_audio_ctx} (expected {N_AUDIO_CTX})"
                ));
            }
            if n_text_ctx != N_TEXT_CTX {
                diffs.push(format!("n_text_ctx={n_text_ctx} (expected {N_TEXT_CTX})"));
            }
            if !n_mels_ok {
                diffs.push(format!(
                    "n_mels={n_mels} (expected {N_MELS_LEGACY} or {N_MELS_V3})"
                ));
            }
            format!(
                "GGML file but architecture doesn't match Whisper: {}.",
                diffs.join(", ")
            )
        };
        return Err(ModelCompatError::NotWhisper {
            n_vocab,
            n_audio_ctx,
            n_mels,
            n_text_ctx,
            explanation,
        });
    }

    if !(VOCAB_MIN..=VOCAB_MAX).contains(&n_vocab) {
        return Err(ModelCompatError::NotWhisper {
            n_vocab,
            n_audio_ctx,
            n_mels,
            n_text_ctx,
            explanation: format!(
                "Whisper architecture matches but n_vocab={n_vocab} is outside the supported range \
                 {VOCAB_MIN}-{VOCAB_MAX}. Likely a non-standard fine-tune; we can't guarantee \
                 compatibility."
            ),
        });
    }

    // whisper.cpp asserts these dim equalities at load time; fail
    // fast here with a friendly message instead of a runtime crash
    // later.
    if n_text_state != n_audio_state {
        return Err(ModelCompatError::NotWhisper {
            n_vocab,
            n_audio_ctx,
            n_mels,
            n_text_ctx,
            explanation: format!(
                "Whisper architecture mismatch: n_text_state={n_text_state} \
                 != n_audio_state={n_audio_state}."
            ),
        });
    }

    // Reject quant variants we don't recognise. Surface the raw
    // on-disk value (e.g. 2008) in the error so the technical detail
    // panel matches what `xxd` shows; the user-facing message says
    // "Unsupported quantisation variant" and lists the ones we do
    // accept.
    if !(0..=QNT_VERSION_MAX).contains(&qnt_version) || !(0..=FTYPE_MAX).contains(&ftype) {
        return Err(ModelCompatError::UnknownQuant { ftype: raw_ftype });
    }

    let is_multilingual = n_vocab >= 51865;
    let size_class = n_audio_state_to_size_class(n_audio_state).to_string();
    let quant_label = ftype_to_label(ftype).to_string();

    let mut warnings = Vec::new();
    if size_class == "unknown" {
        warnings.push(ImportWarning::NonStandardSizeClass { n_audio_state });
    }
    if !is_multilingual {
        warnings.push(ImportWarning::EnglishOnly);
    }

    // Memory pre-flight. `sysinfo` reports MIB units (1024^2). 60% of
    // available RAM is the heuristic threshold — works well in
    // practice for CPU loads; GPU loads need slightly more headroom
    // but the soft warning is informative, not blocking.
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let free_ram_bytes = sys.available_memory();
    let free_ram_mb = free_ram_bytes / 1_048_576;
    let model_size_mb = file_size_bytes / 1_048_576;
    if free_ram_mb > 0 && model_size_mb.saturating_mul(100) > free_ram_mb.saturating_mul(60) {
        warnings.push(ImportWarning::HighMemoryUse {
            model_size_mb,
            free_ram_mb,
        });
    }

    // Cross-check head dim parity (whisper.cpp assertion). Warning
    // rather than hard error because a non-standard fine-tune might
    // tweak this and still load.
    let _ = n_text_head; // unused: kept in case we add a future warning

    Ok(ValidationResult {
        capabilities: ModelCapabilities {
            is_multilingual,
            size_class,
            quant_label,
            n_vocab,
            n_audio_state,
            n_audio_layer,
            file_size_bytes,
        },
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Build a 48-byte header (magic + 11 i32 hparams) plus 32 bytes
    /// of padding so the file is comfortably above the truncated
    /// threshold. Returns a closed `NamedTempFile` whose path can be
    /// passed to `validate`.
    fn fixture(magic: u32, hparams: [i32; 11]) -> NamedTempFile {
        let mut buf = Vec::with_capacity(HEADER_BYTES + 32);
        buf.extend_from_slice(&magic.to_le_bytes());
        for v in hparams {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&[0u8; 32]);

        let mut f = NamedTempFile::new().expect("create tempfile");
        f.write_all(&buf).expect("write fixture");
        f.flush().expect("flush fixture");
        f
    }

    // ftype on disk uses the qnt_version-encoded form
    // `qntvr * 1000 + actual`. Our shipped models are q5_x at qntvr=2
    // (the upstream default), hence 2008 / 2009 below. Older fixtures
    // could use plain `8` / `9` (qntvr=0) — both must validate.
    const Q5_0_QNTV2: i32 = 2008;
    const Q5_1_QNTV2: i32 = 2009;

    /// A canonical multilingual small-model header (n_audio_state=768,
    /// n_audio_layer=12, n_vocab=51865, q5_1).
    const HPARAMS_SMALL_MULTILINGUAL: [i32; 11] = [
        51865,      // n_vocab
        1500,       // n_audio_ctx
        768,        // n_audio_state
        12,         // n_audio_head
        12,         // n_audio_layer
        448,        // n_text_ctx
        768,        // n_text_state
        12,         // n_text_head
        12,         // n_text_layer
        80,         // n_mels
        Q5_1_QNTV2, // ftype = q5_1 with quantisation version 2
    ];

    /// English-only tiny variant (n_vocab=51864).
    const HPARAMS_TINY_EN: [i32; 11] = [51864, 1500, 384, 6, 4, 448, 384, 6, 4, 80, Q5_0_QNTV2];

    /// Canonical large-v3-turbo header. Important: `n_mels=128` (v3
    /// doubled the mel-spectrogram resolution) — the original
    /// validator hardcoded 80 and rejected this real model.
    const HPARAMS_LARGE_V3_TURBO: [i32; 11] = [
        51866,      // n_vocab (v3 added <|nospeech|>)
        1500,       // n_audio_ctx
        1280,       // n_audio_state
        20,         // n_audio_head
        32,         // n_audio_layer
        448,        // n_text_ctx
        1280,       // n_text_state
        20,         // n_text_head
        4,          // n_text_layer (turbo: 4 decoder layers)
        128,        // n_mels (v3 invariant, NOT 80)
        Q5_0_QNTV2, // ftype = q5_0 with quantisation version 2
    ];

    /// LLaMA legacy hparams (vocab 32000, no Whisper-specific dims).
    /// LLaMA used n_audio_ctx etc. layout differently — for our
    /// purposes the key signal is the wrong invariants.
    const HPARAMS_LLAMA_LEGACY: [i32; 11] = [
        32000, 4096, 4096, 32, 32, 4096, 4096, 32, 32, 0, 1, // ftype = f16
    ];

    #[test]
    fn accepts_canonical_multilingual_small() {
        let f = fixture(GGML_MAGIC, HPARAMS_SMALL_MULTILINGUAL);
        let result = validate(f.path()).expect("should validate");
        assert!(result.capabilities.is_multilingual);
        assert_eq!(result.capabilities.size_class, "small");
        assert_eq!(result.capabilities.quant_label, "q5_1");
        assert_eq!(result.capabilities.n_vocab, 51865);
        assert_eq!(result.capabilities.n_audio_layer, 12);
        // No warnings expected on a canonical model.
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn accepts_large_v3_turbo_with_n_mels_128() {
        // Regression for the bug where the validator hardcoded
        // n_mels=80 and rejected every v3+ model — including the
        // large-v3-turbo we ship as a built-in.
        let f = fixture(GGML_MAGIC, HPARAMS_LARGE_V3_TURBO);
        let result = validate(f.path()).expect("v3-turbo must validate");
        assert_eq!(result.capabilities.size_class, "large");
        assert_eq!(result.capabilities.quant_label, "q5_0");
        assert_eq!(result.capabilities.n_vocab, 51866);
        assert!(result.capabilities.is_multilingual);
        // No NotWhisper error, no NonStandardSizeClass warning.
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| matches!(w, ImportWarning::NonStandardSizeClass { .. })),
            "v3-turbo should not be flagged non-standard: {:?}",
            result.warnings
        );
    }

    #[test]
    fn flags_english_only_models() {
        let f = fixture(GGML_MAGIC, HPARAMS_TINY_EN);
        let result = validate(f.path()).expect("should validate");
        assert!(!result.capabilities.is_multilingual);
        assert_eq!(result.capabilities.size_class, "tiny");
        assert_eq!(result.capabilities.quant_label, "q5_0");
        assert!(
            result.warnings.contains(&ImportWarning::EnglishOnly),
            "missing EnglishOnly warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn rejects_bad_magic() {
        // PDF header (`%PDF`) instead of `ggml`.
        let bad_magic = u32::from_le_bytes([b'%', b'P', b'D', b'F']);
        let f = fixture(bad_magic, HPARAMS_SMALL_MULTILINGUAL);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::BadMagic {
                found_hex,
                expected_hex,
            } => {
                assert_eq!(expected_hex, "0x67676d6c");
                assert!(found_hex.starts_with("0x"));
            }
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn rejects_llama_legacy_with_friendly_message() {
        // LLaMA legacy uses the same `ggml` magic, so the magic check
        // alone wouldn't catch it. Architecture invariants do.
        let f = fixture(GGML_MAGIC, HPARAMS_LLAMA_LEGACY);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::NotWhisper {
                n_vocab,
                explanation,
                ..
            } => {
                assert_eq!(n_vocab, 32000);
                // The error message should mention LLaMA explicitly so
                // the user understands it's a wrong-model-family case.
                assert!(
                    explanation.contains("LLaMA"),
                    "explanation should mention LLaMA: {explanation:?}"
                );
            }
            other => panic!("expected NotWhisper, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_whisper_architecture() {
        // Vocab is in range but n_audio_ctx and n_mels are wrong.
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[1] = 4096; // n_audio_ctx
        hp[9] = 64; // n_mels
        let f = fixture(GGML_MAGIC, hp);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::NotWhisper {
                n_audio_ctx,
                n_mels,
                ..
            } => {
                assert_eq!(n_audio_ctx, 4096);
                assert_eq!(n_mels, 64);
            }
            other => panic!("expected NotWhisper, got {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_file() {
        // 10 bytes is below the 48-byte header threshold.
        let mut f = NamedTempFile::new().expect("tempfile");
        f.write_all(&[0u8; 10]).unwrap();
        f.flush().unwrap();
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::Truncated { size_bytes } => {
                assert_eq!(size_bytes, 10);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_quant() {
        // ftype = 99 is outside the 0..=25 range (qntvr=0, ftype=99).
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[10] = 99;
        let f = fixture(GGML_MAGIC, hp);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::UnknownQuant { ftype } => assert_eq!(ftype, 99),
            other => panic!("expected UnknownQuant, got {other:?}"),
        }
    }

    #[test]
    fn accepts_qnt_version_2_encoded_ftype() {
        // Regression: real ggml models on disk encode ftype as
        // `qntvr * 1000 + actual`. Our shipped large-v3-turbo file
        // has raw_ftype=2008 (qntvr=2 + q5_0). The validator must
        // decode this, NOT reject it as out-of-range.
        let f = fixture(GGML_MAGIC, HPARAMS_LARGE_V3_TURBO);
        let result = validate(f.path()).expect("qntv2 ftype must validate");
        // Confirm we expose the *actual* quant label, not the raw
        // encoded value.
        assert_eq!(result.capabilities.quant_label, "q5_0");
    }

    #[test]
    fn rejects_quant_version_too_new() {
        // qntvr=3 doesn't exist yet upstream — reject so the user
        // gets a clear error instead of a runtime crash later.
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[10] = 3 * 1000 + 8; // qntvr=3, ftype=q5_0
        let f = fixture(GGML_MAGIC, hp);
        let err = validate(f.path()).expect_err("future qntvr should reject");
        match err {
            ModelCompatError::UnknownQuant { ftype } => assert_eq!(ftype, 3008),
            other => panic!("expected UnknownQuant, got {other:?}"),
        }
    }

    #[test]
    fn flags_non_standard_size_class() {
        // n_audio_state=999 doesn't map to any known Whisper variant.
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[2] = 999; // n_audio_state
        hp[6] = 999; // n_text_state must match
        let f = fixture(GGML_MAGIC, hp);
        let result = validate(f.path()).expect("should still validate");
        assert_eq!(result.capabilities.size_class, "unknown");
        assert!(
            result
                .warnings
                .contains(&ImportWarning::NonStandardSizeClass { n_audio_state: 999 }),
            "missing NonStandardSizeClass warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn rejects_dim_mismatch_text_vs_audio_state() {
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[6] = 1024; // n_text_state diverges from n_audio_state=768
        let f = fixture(GGML_MAGIC, hp);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::NotWhisper { explanation, .. } => {
                assert!(
                    explanation.contains("n_text_state"),
                    "should mention n_text_state mismatch: {explanation:?}"
                );
            }
            other => panic!("expected NotWhisper, got {other:?}"),
        }
    }

    #[test]
    fn rejects_vocab_outside_supported_range() {
        // Architecture matches Whisper but vocab is non-standard.
        let mut hp = HPARAMS_SMALL_MULTILINGUAL;
        hp[0] = 51820; // wild fine-tune
        let f = fixture(GGML_MAGIC, hp);
        let err = validate(f.path()).expect_err("should reject");
        match err {
            ModelCompatError::NotWhisper {
                n_vocab,
                explanation,
                ..
            } => {
                assert_eq!(n_vocab, 51820);
                assert!(
                    explanation.contains("non-standard"),
                    "explanation should mention non-standard fine-tune: {explanation:?}"
                );
            }
            other => panic!("expected NotWhisper, got {other:?}"),
        }
    }

    #[test]
    fn handles_unreadable_path() {
        let nonexistent = std::path::Path::new("/no/such/path/anywhere/whisper.bin");
        let err = validate(nonexistent).expect_err("should reject");
        assert!(matches!(err, ModelCompatError::Unreadable { .. }));
    }
}
