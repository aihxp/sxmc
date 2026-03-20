use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SxmcError};

/// A file-based cache with TTL support.
/// Stores entries in ~/.cache/sxmc/
pub struct Cache {
    dir: PathBuf,
    default_ttl: Duration,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    data: String,
    created_at: u64,
    ttl_secs: u64,
}

impl Cache {
    /// Create a new cache with the given TTL.
    pub fn new(ttl_secs: u64) -> Result<Self> {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("sxmc");

        std::fs::create_dir_all(&dir)
            .map_err(|e| SxmcError::Other(format!("Failed to create cache dir: {}", e)))?;

        Ok(Self {
            dir,
            default_ttl: Duration::from_secs(ttl_secs),
        })
    }

    /// Get a cached value by key, if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<String> {
        let path = self.key_path(key);
        let content = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&content).ok()?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();

        if now - entry.created_at > entry.ttl_secs {
            // Expired — clean up
            let _ = std::fs::remove_file(&path);
            return None;
        }

        Some(entry.data)
    }

    /// Store a value in the cache.
    pub fn set(&self, key: &str, data: &str) -> Result<()> {
        self.set_with_ttl(key, data, self.default_ttl.as_secs())
    }

    /// Store a value with a custom TTL.
    pub fn set_with_ttl(&self, key: &str, data: &str, ttl_secs: u64) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| SxmcError::Other(format!("System time error: {}", e)))?
            .as_secs();

        let entry = CacheEntry {
            data: data.to_string(),
            created_at: now,
            ttl_secs,
        };

        let json = serde_json::to_string(&entry)?;
        let path = self.key_path(key);
        std::fs::write(&path, json)
            .map_err(|e| SxmcError::Other(format!("Failed to write cache: {}", e)))?;

        Ok(())
    }

    /// Remove a cached entry.
    pub fn remove(&self, key: &str) {
        let _ = std::fs::remove_file(self.key_path(key));
    }

    /// Clear all cached entries.
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(|e| SxmcError::Other(format!("Failed to read cache dir: {}", e)))?
                .flatten()
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    fn key_path(&self, key: &str) -> PathBuf {
        // Hash the key for safe filenames
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        self.dir.join(format!("{:x}.json", hasher.finish()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_set_get() {
        let cache = Cache::new(3600).unwrap();
        let key = "test_cache_set_get";
        cache.set(key, "hello world").unwrap();
        assert_eq!(cache.get(key), Some("hello world".to_string()));
        cache.remove(key);
    }

    #[test]
    fn test_cache_miss() {
        let cache = Cache::new(3600).unwrap();
        assert_eq!(cache.get("nonexistent_key_12345"), None);
    }

    #[test]
    fn test_cache_expired() {
        let cache = Cache::new(3600).unwrap();
        let key = "test_cache_expired";
        // Set with 0 TTL — immediately expired
        cache.set_with_ttl(key, "expired data", 0).unwrap();
        // Sleep over 1 second to ensure the second-granularity timestamp advances
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(cache.get(key), None);
    }
}
