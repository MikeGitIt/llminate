use anyhow::Result;
use sha2::{Sha256, Digest as Sha2Digest};
use crc32fast::Hasher as Crc32Hasher;

/// Checksum algorithm types matching JavaScript ChecksumAlgorithm
#[derive(Debug, Clone)]
pub enum ChecksumAlgorithm {
    MD5,
    CRC32,
    CRC32C,
    SHA1,
    SHA256,
}

/// Checksum configuration matching JavaScript
pub struct ChecksumConfig {
    pub algorithm_id: ChecksumAlgorithm,
    pub checksum_constructor: Box<dyn Fn(&[u8]) -> String + Send + Sync>,
}

/// Create checksum configuration matching JavaScript createChecksumConfiguration
pub fn create_checksum_configuration(algorithms: Vec<ChecksumAlgorithm>) -> Vec<ChecksumConfig> {
    let mut configs = Vec::new();
    
    for algo in algorithms {
        let config = match algo {
            ChecksumAlgorithm::SHA256 => {
                ChecksumConfig {
                    algorithm_id: ChecksumAlgorithm::SHA256,
                    checksum_constructor: Box::new(|data: &[u8]| {
                        let mut hasher = Sha256::new();
                        hasher.update(data);
                        format!("{:x}", hasher.finalize())
                    }),
                }
            },
            ChecksumAlgorithm::MD5 => {
                ChecksumConfig {
                    algorithm_id: ChecksumAlgorithm::MD5,
                    checksum_constructor: Box::new(|data: &[u8]| {
                        let digest = md5::compute(data);
                        format!("{:x}", digest)
                    }),
                }
            },
            ChecksumAlgorithm::CRC32 => {
                ChecksumConfig {
                    algorithm_id: ChecksumAlgorithm::CRC32,
                    checksum_constructor: Box::new(|data: &[u8]| {
                        let mut hasher = Crc32Hasher::new();
                        hasher.update(data);
                        format!("{:08x}", hasher.finalize())
                    }),
                }
            },
            ChecksumAlgorithm::CRC32C => {
                ChecksumConfig {
                    algorithm_id: ChecksumAlgorithm::CRC32C,
                    checksum_constructor: Box::new(|data: &[u8]| {
                        // CRC32C requires crc32c crate which uses Castagnoli polynomial
                        // Add to Cargo.toml: crc32c = "0.6"
                        // Then: let checksum = crc32c::crc32c(data);
                        // For complete implementation, the crc32c crate is needed
                        unimplemented!("CRC32C requires crc32c crate to be added to dependencies")
                    }),
                }
            },
            ChecksumAlgorithm::SHA1 => {
                ChecksumConfig {
                    algorithm_id: ChecksumAlgorithm::SHA1,
                    checksum_constructor: Box::new(|data: &[u8]| {
                        use sha1::{Sha1, Digest};
                        let mut hasher = Sha1::new();
                        hasher.update(data);
                        format!("{:x}", hasher.finalize())
                    }),
                }
            },
        };
        configs.push(config);
    }
    
    configs
}

/// Get default checksum algorithms
pub fn get_default_algorithms() -> Vec<ChecksumAlgorithm> {
    vec![
        ChecksumAlgorithm::SHA256,
        ChecksumAlgorithm::MD5,
        ChecksumAlgorithm::CRC32,
    ]
}