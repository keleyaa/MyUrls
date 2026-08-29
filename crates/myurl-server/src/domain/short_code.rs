use rand::{TryRngCore, rngs::OsRng};
use thiserror::Error;

use crate::config::AUTO_CODE_LENGTH;

pub const BASE62: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

const RANDOM_BYTES_PER_PULL: usize = 16;
// Limits rejection sampling when a faulty test source never emits an accepted byte.
const MAX_RANDOM_BYTE_PULLS: usize = 16;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ShortCodeError {
    #[error("secure random bytes are unavailable")]
    RandomnessUnavailable,
    #[error("random byte source was exhausted")]
    RandomByteSourceExhausted,
    #[error("random byte source exceeded the rejected-byte pull limit")]
    RejectedByteLimitExceeded,
}

/// Supplies random bytes for short-code generation.
pub trait RandomByteSource {
    fn fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), ShortCodeError>;
}

pub fn generate_short_code() -> Result<String, ShortCodeError> {
    generate_short_code_with_source(&mut OsRandomByteSource)
}

pub fn generate_short_code_with_source<S>(source: &mut S) -> Result<String, ShortCodeError>
where
    S: RandomByteSource,
{
    let mut code = String::with_capacity(AUTO_CODE_LENGTH);
    let mut bytes = [0_u8; RANDOM_BYTES_PER_PULL];

    for _ in 0..MAX_RANDOM_BYTE_PULLS {
        source.fill_bytes(&mut bytes)?;
        for byte in bytes {
            if byte < 248 {
                code.push(BASE62.as_bytes()[(byte % BASE62.len() as u8) as usize] as char);
                if code.len() == AUTO_CODE_LENGTH {
                    return Ok(code);
                }
            }
        }
    }

    Err(ShortCodeError::RejectedByteLimitExceeded)
}

pub fn is_valid_code(value: &str) -> bool {
    (4..=32).contains(&value.len())
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

struct OsRandomByteSource;

impl RandomByteSource for OsRandomByteSource {
    fn fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), ShortCodeError> {
        OsRng
            .try_fill_bytes(destination)
            .map_err(|_| ShortCodeError::RandomnessUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BASE62, RandomByteSource, ShortCodeError, generate_short_code,
        generate_short_code_with_source, is_valid_code,
    };

    struct RepeatingByteSource {
        byte: u8,
    }

    impl RandomByteSource for RepeatingByteSource {
        fn fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), ShortCodeError> {
            destination.fill(self.byte);
            Ok(())
        }
    }

    struct FiniteByteSource {
        bytes: Vec<u8>,
        offset: usize,
    }

    impl RandomByteSource for FiniteByteSource {
        fn fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), ShortCodeError> {
            let remaining = self.bytes.len().saturating_sub(self.offset);
            if remaining < destination.len() {
                return Err(ShortCodeError::RandomByteSourceExhausted);
            }

            let end = self.offset + destination.len();
            destination.copy_from_slice(&self.bytes[self.offset..end]);
            self.offset = end;
            Ok(())
        }
    }

    #[test]
    fn generates_a_fixed_length_base62_code_from_an_all_zero_source() {
        let mut source = RepeatingByteSource { byte: 0 };
        let code = generate_short_code_with_source(&mut source).unwrap();

        assert_eq!(code, "0000000000");
        assert_eq!(code.len(), 10);
        assert!(code.chars().all(|character| BASE62.contains(character)));
    }

    #[test]
    fn rejects_out_of_range_bytes_without_modulo_bias() {
        let mut source = FiniteByteSource {
            bytes: [vec![248, 249, 250, 251, 252, 253], (0_u8..10).collect()].concat(),
            offset: 0,
        };

        assert_eq!(
            generate_short_code_with_source(&mut source),
            Ok("0123456789".to_owned())
        );
    }

    #[test]
    fn returns_a_specific_error_when_the_injected_source_is_exhausted() {
        let mut source = FiniteByteSource {
            bytes: vec![248; 16],
            offset: 0,
        };

        assert_eq!(
            generate_short_code_with_source(&mut source),
            Err(ShortCodeError::RandomByteSourceExhausted)
        );
    }

    #[test]
    fn bounds_rejection_sampling_for_a_source_that_only_returns_rejected_bytes() {
        let mut source = RepeatingByteSource { byte: 255 };

        assert_eq!(
            generate_short_code_with_source(&mut source),
            Err(ShortCodeError::RejectedByteLimitExceeded)
        );
    }

    #[test]
    fn production_generation_returns_a_base62_code_of_the_configured_length() {
        let code = generate_short_code().unwrap();

        assert_eq!(code.len(), 10);
        assert!(code.chars().all(|character| BASE62.contains(character)));
    }

    #[test]
    fn accepts_and_rejects_path_code_shapes_from_the_node_policy_tests() {
        for code in ["abcd", "Abcd_123-XYZ"] {
            assert!(is_valid_code(code), "expected {code:?} to be valid");
        }

        for code in [
            "ab",
            "hello.world",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "a bcd",
            "аbcd",
        ] {
            assert!(!is_valid_code(code), "expected {code:?} to be invalid");
        }
    }
}
