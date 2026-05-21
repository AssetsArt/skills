use codegraph_core::hash::FileHash;

#[test]
fn file_hash_display_is_lowercase_hex() {
    let bytes = [0xab_u8; 32];
    let hash = FileHash::new(bytes);
    let s = format!("{hash}");
    assert_eq!(s.len(), 64);
    assert!(s.chars().all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_lowercase())));
    assert_eq!(s, "ab".repeat(32));
}

#[test]
fn file_hash_as_ref_returns_underlying_bytes() {
    let bytes = [0x42_u8; 32];
    let hash = FileHash::new(bytes);
    let slice: &[u8] = hash.as_ref();
    assert_eq!(slice, &bytes[..]);
}

#[test]
fn file_hash_equality() {
    assert_eq!(FileHash::new([0; 32]), FileHash::new([0; 32]));
    assert_ne!(FileHash::new([0; 32]), FileHash::new([1; 32]));
}
