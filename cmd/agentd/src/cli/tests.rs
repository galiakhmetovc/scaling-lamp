#[test]
fn decode_repl_line_bytes_uses_cp1251_locale_hint() {
    let bytes = "привет\n".as_bytes();
    let encoded = encoding_rs::WINDOWS_1251.encode("привет\n").0;

    let decoded = super::decode_repl_line_bytes(&encoded, Some("cp1251"))
        .expect("cp1251 input should decode");

    assert_eq!(decoded, String::from_utf8(bytes.to_vec()).expect("utf8"));
}
