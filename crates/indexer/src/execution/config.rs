use crate::indexer::IndexingConfig;

pub struct IndexingConfigBuilder;

impl IndexingConfigBuilder {
    /// Build indexing config without property files
    pub fn build(threads: usize) -> IndexingConfig {
        Self::build_with_properties(threads, Vec::new())
    }

    /// Build indexing config with property files
    pub fn build_with_properties(threads: usize, java_property_files: Vec<String>) -> IndexingConfig {
        let effective_threads = IndexingConfigBuilder::get_effective_threads(threads);
        IndexingConfig {
            worker_threads: effective_threads,
            max_file_size: 5_000_000,
            respect_gitignore: true,
            java_property_files,
        }
    }

    pub fn get_effective_threads(threads: usize) -> usize {
        if threads == 0 {
            num_cpus::get()
        } else {
            threads
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_with_zero_threads() {
        let config = IndexingConfigBuilder::build(0);

        assert!(config.worker_threads > 0);
        assert_eq!(config.max_file_size, 5_000_000);
        assert!(config.respect_gitignore);
        assert!(config.java_property_files.is_empty());
    }

    #[test]
    fn test_build_with_property_files() {
        let property_files = vec![
            "config/application.properties".to_string(),
            "config/application-prod.yml".to_string(),
        ];
        let config = IndexingConfigBuilder::build_with_properties(4, property_files.clone());

        assert_eq!(config.worker_threads, 4);
        assert_eq!(config.java_property_files, property_files);
    }
}
