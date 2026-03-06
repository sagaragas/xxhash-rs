//! Known test vectors from the xxHash BSD-licensed reference material.
//!
//! These vectors are extracted from the published sanity test data in the
//! xxHash reference (BSD-2-Clause). They cover edge-case input lengths
//! including the boundaries specified in VAL-HASH-001:
//! 0, 1, 3, 4, 8, 9, 16, 17, 128, 129, 240, 241, plus larger inputs.
//!
//! Each vector specifies: input length (bytes from the canonical test buffer),
//! seed, and expected hash output as a lowercase hex string.

use super::{Algorithm, TestVector};

/// XXH32 test vectors from the reference sanity checks.
///
/// Seed values: 0 and PRIME32 (0x9E3779B1).
pub fn xxh32_vectors() -> Vec<TestVector> {
    // (len, seed_u32, expected_hash_u32)
    let data: &[(usize, u64, u32)] = &[
        // Seed = 0
        (0, 0, 0x02CC5D05),
        (1, 0, 0xCF65B03E),
        (3, 0, 0xC23884F5),
        (4, 0, 0xA9DE7CE9),
        (8, 0, 0xA3F6F44B),
        (9, 0, 0xFFB82A24),
        (16, 0, 0x93BA3759),
        (17, 0, 0x89FDC23E),
        (128, 0, 0x0FD07B71),
        (129, 0, 0x68C9EC37),
        (240, 0, 0xFA6B6557),
        (241, 0, 0xE5F7C54D),
        (256, 0, 0x520CB910),
        (512, 0, 0xD485C30A),
        // Seed = PRIME32 (0x9E3779B1)
        (0, 0x9E3779B1, 0x36B78AE7),
        (1, 0x9E3779B1, 0xB4545AA4),
        (3, 0x9E3779B1, 0x1A269947),
        (4, 0x9E3779B1, 0x2BAAFE83),
        (8, 0x9E3779B1, 0xC2A8E239),
        (9, 0x9E3779B1, 0xD35632C6),
        (16, 0x9E3779B1, 0xA94FC1E1),
        (17, 0x9E3779B1, 0xC9910739),
        (128, 0x9E3779B1, 0x3BD1140E),
        (129, 0x9E3779B1, 0x2A9476A5),
        (240, 0x9E3779B1, 0x55DF41D9),
        (241, 0x9E3779B1, 0x13B52081),
    ];

    data.iter()
        .map(|&(len, seed, expected)| TestVector {
            len,
            seed,
            expected_hex: format!("{expected:08x}"),
            algorithm: Algorithm::XXH32,
        })
        .collect()
}

/// XXH64 test vectors from the reference sanity checks.
///
/// Seed values: 0 and PRIME32 as 64-bit (0x000000009E3779B1).
pub fn xxh64_vectors() -> Vec<TestVector> {
    // (len, seed_u64, expected_hash_u64)
    let data: &[(usize, u64, u64)] = &[
        // Seed = 0
        (0, 0, 0xEF46DB3751D8E999),
        (1, 0, 0xE934A84ADB052768),
        (3, 0, 0xFF7E1959CB50794A),
        (4, 0, 0x9136A0DCA57457EE),
        (8, 0, 0xCDBCF538E71D1348),
        (9, 0, 0x554B1AE991EDA6B6),
        (16, 0, 0x98C90B57FDFCB55C),
        (17, 0, 0x0D39A2D051A30C2C),
        (128, 0, 0x90CA021457D96DC5),
        (129, 0, 0x41C280132D697ABA),
        (240, 0, 0xB81838D483BAEE53),
        (241, 0, 0x95D76C8B4D8FC4D6),
        (256, 0, 0x5E3F5BF94D574981),
        (512, 0, 0x4358D2FDD62B58A7),
        // Seed = PRIME32 as 64-bit (0x000000009E3779B1)
        (0, 0x000000009E3779B1, 0xAC75FDA2929B17EF),
        (1, 0x000000009E3779B1, 0x5014607643A9B4C3),
        (3, 0x000000009E3779B1, 0xAA8584E83660F7D1),
        (4, 0x000000009E3779B1, 0xCAAB286BD8E9FDB5),
        (8, 0x000000009E3779B1, 0xFE0C047A5353CDAC),
        (9, 0x000000009E3779B1, 0x7908265248F6D73F),
        (16, 0x000000009E3779B1, 0xC900AD2D536B607E),
        (17, 0x000000009E3779B1, 0x495CD68A647C7A22),
        (128, 0x000000009E3779B1, 0xED9340A202BCD1CF),
        (129, 0x000000009E3779B1, 0x1668B87489935FF5),
        (240, 0x000000009E3779B1, 0xA4B3F965B6FE67F8),
        (241, 0x000000009E3779B1, 0x19D5AD5F4BD6CB9F),
    ];

    data.iter()
        .map(|&(len, seed, expected)| TestVector {
            len,
            seed,
            expected_hex: format!("{expected:016x}"),
            algorithm: Algorithm::XXH64,
        })
        .collect()
}

/// XXH3_64 test vectors from the reference sanity checks.
///
/// Seed values: 0 and PRIME64_1 (0x9E3779B185EBCA8D).
pub fn xxh3_64_vectors() -> Vec<TestVector> {
    // (len, seed_u64, expected_hash_u64)
    let data: &[(usize, u64, u64)] = &[
        // Seed = 0
        (0, 0, 0x2D06800538D394C2),
        (1, 0, 0xC44BDFF4074EECDB),
        (3, 0, 0x54247382A8D6B94D),
        (4, 0, 0xE5DC74BC51848A51),
        (8, 0, 0x24CCC9ACAA9F65E4),
        (9, 0, 0x14D5001C15DD3F2B),
        (16, 0, 0x981B17D36C7498C9),
        (17, 0, 0x796F5ACD3A60F862),
        (128, 0, 0xFCFF24126754D861),
        (129, 0, 0x98F1B0A679A2CA29),
        (240, 0, 0x81C3C2B67F568CCF),
        (241, 0, 0xC5A639ECD2030E5E),
        (256, 0, 0x55DE574AD89D0AC5),
        (512, 0, 0x617E49599013CB6B),
        // Seed = PRIME64_1 (0x9E3779B185EBCA8D)
        (0, 0x9E3779B185EBCA8D, 0xA8A6B918B2F0364A),
        (1, 0x9E3779B185EBCA8D, 0x032BE332DD766EF8),
        (3, 0x9E3779B185EBCA8D, 0x634B8990B4976373),
        (4, 0x9E3779B185EBCA8D, 0xAA2E7ECCB0C8F747),
        (8, 0x9E3779B185EBCA8D, 0x8F973410999B8F6B),
        (9, 0x9E3779B185EBCA8D, 0xB3AE7333D9013F60),
        (16, 0x9E3779B185EBCA8D, 0x663F29333B4DB6B1),
        (17, 0x9E3779B185EBCA8D, 0xF3EC5067F4306DB3),
        (128, 0x9E3779B185EBCA8D, 0x73FDE75280646649),
        (129, 0x9E3779B185EBCA8D, 0x21FFFDBCA099C844),
        (240, 0x9E3779B185EBCA8D, 0xCC0F58C27EF3D8EE),
        (241, 0x9E3779B185EBCA8D, 0xDDA9B0A161D4829A),
    ];

    data.iter()
        .map(|&(len, seed, expected)| TestVector {
            len,
            seed,
            expected_hex: format!("{expected:016x}"),
            algorithm: Algorithm::XXH3_64,
        })
        .collect()
}

/// XXH3_128 test vectors from the reference sanity checks.
///
/// Seed values: 0, PRIME32 as 64-bit (0x000000009E3779B1), and
/// PRIME64_1 (0x9E3779B185EBCA8D).
/// The 128-bit hash is stored as low-64 || high-64 in hex.
pub fn xxh3_128_vectors() -> Vec<TestVector> {
    // (len, seed_u64, expected_lo_u64, expected_hi_u64)
    let data: &[(usize, u64, u64, u64)] = &[
        // Seed = 0
        (0, 0, 0x6001C324468D497F, 0x99AA06D3014798D8),
        (1, 0, 0xC44BDFF4074EECDB, 0xA6CD5E9392000F6A),
        (3, 0, 0x54247382A8D6B94D, 0x20EFC49FF02422EA),
        (4, 0, 0x2E7D8D6876A39FE9, 0x970D585AC632BF8E),
        (8, 0, 0x64C69CAB4BB21DC5, 0x47A7F080D82BB456),
        (9, 0, 0xED7CCBC501EB7501, 0x564EF6078950D457),
        (16, 0, 0x562980258A998629, 0xC68C368ECF8A9C05),
        (17, 0, 0xABBC12D11973D7DB, 0x955FA78643ED3669),
        (128, 0, 0xEBB15E34A7FB5AB1, 0x39992220E045260A),
        (129, 0, 0x86C9E3BC8F0A3B5C, 0x03815FC91F1B30B6),
        (240, 0, 0x5C9AAE94C8EBE5A0, 0xAA4202DAA2769DC8),
        (241, 0, 0xC5A639ECD2030E5E, 0x99A80ECF0ECFC647),
        (256, 0, 0x55DE574AD89D0AC5, 0x8B1C66091423D288),
        (512, 0, 0x617E49599013CB6B, 0x18D2D110DCC9BCA1),
        // Seed = PRIME64_1 (0x9E3779B185EBCA8D)
        (0, 0x9E3779B185EBCA8D, 0xA986DFC5D7605BFE, 0x00FEAA732A3CE25E),
        (1, 0x9E3779B185EBCA8D, 0x032BE332DD766EF8, 0x20E49ABCC53B3842),
        (3, 0x9E3779B185EBCA8D, 0x634B8990B4976373, 0x1C7ECF6A308CF00E),
        (4, 0x9E3779B185EBCA8D, 0xBFAF51F1E67E0B0F, 0x3D53E5DFD837D927),
        (8, 0x9E3779B185EBCA8D, 0x7B29471DC729B5FF, 0xF50CEC145BCD5C5A),
        (9, 0x9E3779B185EBCA8D, 0xAEF5DFC0AC9F9044, 0x6B380B43FFA61042),
        (16, 0x9E3779B185EBCA8D, 0x0346D13A7A5498C7, 0x6FFCB80CD33085C8),
        (17, 0x9E3779B185EBCA8D, 0x980A14119985A7DF, 0xD77681219E464828),
        (128, 0x9E3779B185EBCA8D, 0x8394F5C51F1D8246, 0xA0F7CCB68EE02ADD),
        (129, 0x9E3779B185EBCA8D, 0xD4AAE26FCEC7DC03, 0xAD559266067C0BF3),
        (240, 0x9E3779B185EBCA8D, 0x604E98DB085C1864, 0x29D2133D6EA58C5B),
        (241, 0x9E3779B185EBCA8D, 0xDDA9B0A161D4829A, 0xEC64AFAE6A137582),
    ];

    data.iter()
        .map(|&(len, seed, lo, hi)| TestVector {
            len,
            seed,
            expected_hex: format!("{lo:016x}{hi:016x}"),
            algorithm: Algorithm::XXH3_128,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh32_vectors_non_empty() {
        let vecs = xxh32_vectors();
        assert!(!vecs.is_empty(), "XXH32 vectors must not be empty");
        assert!(
            vecs.iter().all(|v| v.algorithm == Algorithm::XXH32),
            "All XXH32 vectors must have correct algorithm tag"
        );
        // All hex strings should be 8 chars for 32-bit
        for v in &vecs {
            assert_eq!(v.expected_hex.len(), 8, "XXH32 hex should be 8 chars");
        }
    }

    #[test]
    fn xxh64_vectors_non_empty() {
        let vecs = xxh64_vectors();
        assert!(!vecs.is_empty(), "XXH64 vectors must not be empty");
        assert!(
            vecs.iter().all(|v| v.algorithm == Algorithm::XXH64),
            "All XXH64 vectors must have correct algorithm tag"
        );
        for v in &vecs {
            assert_eq!(v.expected_hex.len(), 16, "XXH64 hex should be 16 chars");
        }
    }

    #[test]
    fn xxh3_64_vectors_non_empty() {
        let vecs = xxh3_64_vectors();
        assert!(!vecs.is_empty(), "XXH3_64 vectors must not be empty");
        assert!(
            vecs.iter().all(|v| v.algorithm == Algorithm::XXH3_64),
            "All XXH3_64 vectors must have correct algorithm tag"
        );
        for v in &vecs {
            assert_eq!(v.expected_hex.len(), 16, "XXH3_64 hex should be 16 chars");
        }
    }

    #[test]
    fn xxh3_128_vectors_non_empty() {
        let vecs = xxh3_128_vectors();
        assert!(!vecs.is_empty(), "XXH3_128 vectors must not be empty");
        assert!(
            vecs.iter().all(|v| v.algorithm == Algorithm::XXH3_128),
            "All XXH3_128 vectors must have correct algorithm tag"
        );
        for v in &vecs {
            assert_eq!(
                v.expected_hex.len(),
                32,
                "XXH3_128 hex should be 32 chars, got {} for len={}",
                v.expected_hex.len(),
                v.len
            );
        }
    }
}
