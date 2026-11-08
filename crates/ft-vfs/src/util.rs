use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(crate) fn encode_segment(value: &str) -> String {
    utf8_percent_encode(value, SEGMENT).to_string()
}

pub(crate) fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn decode(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().into_owned()
}
