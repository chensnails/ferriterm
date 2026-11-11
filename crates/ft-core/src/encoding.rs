//! Byte-to-text decoding for remote hosts that do not speak UTF-8.
//!
//! The transcript and SFTP names arrive as raw bytes. Legacy servers send GBK,
//! Big5 or Shift_JIS, so every consumer that turns bytes into text resolves the
//! host's configured label here instead of calling `from_utf8_lossy`.

use encoding_rs::{CoderResult, Encoding, UTF_8};

/// Resolves a label such as `gbk` or `shift_jis`; unknown labels fall back to
/// UTF-8 so a typo in a host setting can never make output unreadable.
pub fn encoding_for_label(label: &str) -> &'static Encoding {
    Encoding::for_label(label.trim().as_bytes()).unwrap_or(UTF_8)
}

pub fn is_utf8_label(label: &str) -> bool {
    encoding_for_label(label) == UTF_8
}

/// Text from the webview is UTF-8; a host that speaks another charset needs it
/// re-encoded before it reaches the shell, names or file contents.
pub fn encode_text(label: &str, text: &str) -> Vec<u8> {
    let encoding = encoding_for_label(label);
    if encoding == UTF_8 {
        return text.as_bytes().to_vec();
    }
    let (bytes, _, _) = encoding.encode(text);
    bytes.into_owned()
}

/// Streaming decoder that keeps a partial multibyte character between calls.
pub struct StreamingDecoder {
    inner: encoding_rs::Decoder,
}

impl StreamingDecoder {
    pub fn new(label: &str) -> Self {
        Self {
            inner: encoding_for_label(label).new_decoder(),
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.decode(bytes, false)
    }

    /// Flushes a trailing partial character at end of stream.
    pub fn finish(&mut self) -> String {
        self.decode(&[], true)
    }

    fn decode(&mut self, mut input: &[u8], last: bool) -> String {
        let mut out = String::with_capacity(input.len() + 16);
        loop {
            let (result, read, _) = self.inner.decode_to_string(input, &mut out, last);
            input = &input[read..];
            match result {
                CoderResult::InputEmpty => break,
                CoderResult::OutputFull => out.reserve(512),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_and_unknown_labels() {
        assert_eq!(encoding_for_label("utf-8"), UTF_8);
        assert_eq!(encoding_for_label("GBK").name(), "GBK");
        assert_eq!(encoding_for_label("not-a-charset"), UTF_8);
    }

    #[test]
    fn decodes_gbk_split_across_chunks() {
        // "测试" in GBK is B2 E2 CA D4.
        let mut decoder = StreamingDecoder::new("gbk");
        assert_eq!(decoder.push(&[0xB2]), "");
        assert_eq!(decoder.push(&[0xE2, 0xCA]), "测");
        assert_eq!(decoder.push(&[0xD4]), "试");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn utf8_passes_through() {
        let mut decoder = StreamingDecoder::new("utf-8");
        assert_eq!(decoder.push("héllo".as_bytes()), "héllo");
    }

    #[test]
    fn encodes_input_for_the_host_charset() {
        assert_eq!(encode_text("gbk", "测试"), vec![0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(encode_text("utf-8", "测试"), "测试".as_bytes());
    }
}
