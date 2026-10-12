pub(crate) fn token_matches(expected: &str, provided: &str) -> bool {
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;

    Sha256::digest(expected.as_bytes())
        .ct_eq(&Sha256::digest(provided.as_bytes()))
        .into()
}
