use std::collections::HashMap;

use crate::querying::mappers::{
    INT_MAPPER, QueryResultMapper, RELATIONSHIP_TYPE_MAPPER, STRING_MAPPER,
};

pub struct QueryLibrary;

#[derive(Debug, Clone)]
pub struct Query {
    pub query: String,
    pub parameters: HashMap<&'static str, QueryParameter>,
    pub result: HashMap<&'static str, QueryResultMapper>,
}

#[derive(Debug, Clone)]
pub struct QueryParameter {
    pub name: &'static str,
    pub definition: QueryParameterDefinition,
}

#[derive(Debug, Clone)]
pub enum QueryParameterDefinition {
    String(Option<String>),
    Int(Option<i64>),
    Float(Option<f64>),
    Boolean(Option<bool>),
    Array(Option<Vec<String>>),
}

// TODO: Handle new ID for definitions (file_path + fqn)
// TODO: Add queries for imported symbols

#[derive(Debug, Clone)]
pub struct RelationshipConfig {
    pub source_type: &'static str,
    pub relationship_name: &'static str,
    pub target_type: &'static str,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct ImportUsageQueryOptions {
    pub include_name: bool,
    pub include_alias: bool,
}

impl QueryLibrary {
    // Combined query that returns both imports and their references in one query
    pub fn get_import_usage(options: ImportUsageQueryOptions) -> Query {
        let name_clause = if options.include_name {
            "\n                  AND imp.name = $import_name"
        } else {
            ""
        };
        let alias_clause = if options.include_alias {
            "\n                  AND imp.alias = $import_alias"
        } else {
            ""
        };
        let query = format!(
            r#"
                MATCH (f:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode)
                WHERE toLower(imp.import_path) IN $paths_lc{name_clause}{alias_clause}
                OPTIONAL MATCH (imp)<-[r:DEFINITION_RELATIONSHIPS]-(src:DefinitionNode)
                WHERE r IS NULL OR r.type IN [$calls_type_id, $ambiguous_calls_type_id]
                RETURN
                  f.path AS file_path,
                  imp.import_path AS import_path,
                  imp.name AS name,
                  imp.alias AS alias,
                  imp.start_line AS import_start_line,
                  imp.end_line AS import_end_line,
                  src.primary_file_path AS ref_file,
                  COALESCE(r.source_start_line, src.start_line) AS ref_start_line,
                  COALESCE(r.source_end_line, src.end_line) AS ref_end_line,
                  src.fqn AS ref_fqn,
                  src.start_line AS def_start_line
                LIMIT $limit
            "#
        );

        let mut parameters = HashMap::from([
            (
                "paths_lc",
                QueryParameter {
                    name: "paths_lc",
                    definition: QueryParameterDefinition::Array(None),
                },
            ),
            (
                "calls_type_id",
                QueryParameter {
                    name: "calls_type_id",
                    definition: QueryParameterDefinition::Int(None),
                },
            ),
            (
                "ambiguous_calls_type_id",
                QueryParameter {
                    name: "ambiguous_calls_type_id",
                    definition: QueryParameterDefinition::Int(None),
                },
            ),
            (
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(50)),
                },
            ),
        ]);

        if options.include_name {
            parameters.insert(
                "import_name",
                QueryParameter {
                    name: "import_name",
                    definition: QueryParameterDefinition::String(None),
                },
            );
        }

        if options.include_alias {
            parameters.insert(
                "import_alias",
                QueryParameter {
                    name: "import_alias",
                    definition: QueryParameterDefinition::String(None),
                },
            );
        }

        Query {
            query,
            parameters,
            result: HashMap::from([
                ("file_path", STRING_MAPPER),
                ("import_path", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("alias", STRING_MAPPER),
                ("import_start_line", INT_MAPPER),
                ("import_end_line", INT_MAPPER),
                ("ref_file", STRING_MAPPER),
                ("ref_start_line", INT_MAPPER),
                ("ref_end_line", INT_MAPPER),
                ("ref_fqn", STRING_MAPPER),
                ("def_start_line", INT_MAPPER),
            ]),
        }
    }

    // Common result mappers used by import-path and import-name import queries
    fn import_hit_result_mappers() -> HashMap<&'static str, QueryResultMapper> {
        HashMap::from([
            ("file_path", STRING_MAPPER),
            ("import_path", STRING_MAPPER),
            ("name", STRING_MAPPER),
            ("alias", STRING_MAPPER),
            ("start_line", INT_MAPPER),
            ("end_line", INT_MAPPER),
        ])
    }

    pub fn get_dependency_import_paths_query() -> Query {
        Query {
            query: r#"
                MATCH (f:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode)
                WHERE toLower(COALESCE(imp.import_path, '')) IN $paths_lc
                RETURN f.path AS file_path,
                       imp.import_path AS import_path,
                       imp.name AS name,
                       imp.alias AS alias,
                       imp.start_line AS start_line,
                       imp.end_line AS end_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "paths_lc",
                    QueryParameter {
                        name: "paths_lc",
                        definition: QueryParameterDefinition::Array(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(50)),
                    },
                ),
            ]),
            result: Self::import_hit_result_mappers(),
        }
    }
    /// Returns all supported relationship configurations for the graph
    pub fn get_all_relationship_configs() -> Vec<RelationshipConfig> {
        vec![
            // Directory relationships (priority 1)
            RelationshipConfig {
                source_type: "DirectoryNode",
                relationship_name: "DIRECTORY_RELATIONSHIPS",
                target_type: "DirectoryNode",
                priority: 1,
            },
            RelationshipConfig {
                source_type: "DirectoryNode",
                relationship_name: "DIRECTORY_RELATIONSHIPS",
                target_type: "FileNode",
                priority: 1,
            },
            // File relationships (priority 2)
            RelationshipConfig {
                source_type: "FileNode",
                relationship_name: "FILE_RELATIONSHIPS",
                target_type: "DefinitionNode",
                priority: 2,
            },
            RelationshipConfig {
                source_type: "FileNode",
                relationship_name: "FILE_RELATIONSHIPS",
                target_type: "ImportedSymbolNode",
                priority: 2,
            },
            // Definition relationships (priority 3)
            RelationshipConfig {
                source_type: "DefinitionNode",
                relationship_name: "DEFINITION_RELATIONSHIPS",
                target_type: "DefinitionNode",
                priority: 3,
            },
            RelationshipConfig {
                source_type: "DefinitionNode",
                relationship_name: "DEFINITION_RELATIONSHIPS",
                target_type: "ImportedSymbolNode",
                priority: 3,
            },
            // Import relationships (priority 4)
            RelationshipConfig {
                source_type: "ImportedSymbolNode",
                relationship_name: "IMPORTED_SYMBOL_RELATIONSHIPS",
                target_type: "ImportedSymbolNode",
                priority: 4,
            },
            RelationshipConfig {
                source_type: "ImportedSymbolNode",
                relationship_name: "IMPORTED_SYMBOL_RELATIONSHIPS",
                target_type: "DefinitionNode",
                priority: 4,
            },
            RelationshipConfig {
                source_type: "ImportedSymbolNode",
                relationship_name: "IMPORTED_SYMBOL_RELATIONSHIPS",
                target_type: "FileNode",
                priority: 4,
            },
        ]
    }

    /// Builds a UNION query section for a given relationship configuration
    pub fn build_relationship_query_section(
        config: &RelationshipConfig,
        limit_param: &str,
    ) -> String {
        let source_return = Self::get_node_neighbors_return_values(config.source_type, "source");
        let target_return = Self::get_node_neighbors_return_values(config.target_type, "target");

        format!(
            r#"
            MATCH (source:{source_type})-[r:{relationship_name}]-(target:{target_type})
            RETURN
                {source_return}
                {target_return}
                '{relationship_name}' as relationship_name,
                CAST(id(r) AS STRING) as relationship_id,
                r.type as relationship_type,
                {priority} as order_priority
            LIMIT ${limit_param}
            "#,
            source_type = config.source_type,
            relationship_name = config.relationship_name,
            target_type = config.target_type,
            priority = config.priority,
            source_return = source_return,
            target_return = target_return,
            limit_param = limit_param
        )
    }

    pub fn get_definition_relations_query() -> Query {
        Query {
            query: r#"
                MATCH (n:DefinitionNode)-[r]-(related:DefinitionNode)
                WHERE n.fqn = $fqn
                RETURN
                    related.fqn as fqn,
                    r.type as relationship_type,
                    related.name as name,
                    related.definition_type as definition_type,
                    related.primary_file_path as file_path,
                    related.start_line as line_number
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "fqn",
                    QueryParameter {
                        name: "fqn",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("fqn", STRING_MAPPER),
                ("relationship_type", RELATIONSHIP_TYPE_MAPPER),
                ("name", STRING_MAPPER),
                ("definition_type", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("line_number", INT_MAPPER),
            ]),
        }
    }

    pub fn get_file_definitions_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[r:FILE_RELATIONSHIPS]->(definition:DefinitionNode)
                WHERE file.path = $file_path OR file.absolute_path = $file_path
                RETURN
                    definition.fqn as fqn,
                    definition.name as name,
                    definition.definition_type as definition_type,
                    definition.start_line as line_number,
                    definition.primary_file_path as file_path
                ORDER BY definition.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "file_path",
                    QueryParameter {
                        name: "file_path",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("fqn", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("definition_type", STRING_MAPPER),
                ("line_number", INT_MAPPER),
                ("file_path", STRING_MAPPER),
            ]),
        }
    }

    pub fn get_file_imports_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode)
                WHERE file.path = $file_path OR file.absolute_path = $file_path
                RETURN
                    imp.name as name,
                    imp.import_path as import_path,
                    imp.alias as alias,
                    imp.start_line as line_number
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "file_path",
                    QueryParameter {
                        name: "file_path",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("name", STRING_MAPPER),
                ("import_path", STRING_MAPPER),
                ("alias", STRING_MAPPER),
                ("line_number", INT_MAPPER),
            ]),
        }
    }

    pub fn get_list_matches_query() -> Query {
        Query {
            query: r#"
                MATCH (n:DefinitionNode)
                WHERE toLower(n.fqn) CONTAINS toLower($search_string)
                RETURN
                    n.fqn as fqn,
                    n.name as name,
                    n.definition_type as definition_type,
                    n.primary_file_path as file_path,
                    n.start_line as line_number
                ORDER BY n.fqn
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "search_string",
                    QueryParameter {
                        name: "search_string",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("fqn", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("definition_type", STRING_MAPPER),
                ("line_number", INT_MAPPER),
                ("file_path", STRING_MAPPER),
            ]),
        }
    }

    pub fn get_initial_project_graph_query() -> Query {
        let relationships = Self::get_all_relationship_configs();

        // Group relationships by priority to use appropriate limits
        let mut directory_queries = Vec::new();
        let mut file_queries = Vec::new();
        let mut definition_queries = Vec::new();
        let mut import_queries = Vec::new();

        for config in relationships {
            let limit_param = match config.priority {
                1 => "directory_limit",
                2 => "file_limit",
                3 => match config.target_type {
                    "ImportedSymbolNode" => "imported_symbol_limit",
                    _ => "definition_limit",
                },
                4 => "imported_symbol_limit", // Reuse existing parameter for import relationships
                _ => "definition_limit",
            };

            let query_section = Self::build_relationship_query_section(&config, limit_param);

            match config.priority {
                1 => directory_queries.push(query_section),
                2 => file_queries.push(query_section),
                3 => definition_queries.push(query_section),
                4 => import_queries.push(query_section),
                _ => definition_queries.push(query_section),
            }
        }

        // Combine all queries with UNION
        let mut all_queries = Vec::new();
        all_queries.extend(directory_queries);
        all_queries.extend(file_queries);
        all_queries.extend(definition_queries);
        all_queries.extend(import_queries);

        let query = all_queries.join("\nUNION\n");

        Query {
            query,
            parameters: HashMap::from([
                (
                    "directory_limit",
                    QueryParameter {
                        name: "directory_limit",
                        definition: QueryParameterDefinition::Int(Some(50)),
                    },
                ),
                (
                    "file_limit",
                    QueryParameter {
                        name: "file_limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
                (
                    "definition_limit",
                    QueryParameter {
                        name: "definition_limit",
                        definition: QueryParameterDefinition::Int(Some(200)),
                    },
                ),
                (
                    "imported_symbol_limit",
                    QueryParameter {
                        name: "imported_symbol_limit",
                        definition: QueryParameterDefinition::Int(Some(50)),
                    },
                ),
            ]),
            result: Self::get_graph_result_mappers(),
        }
    }

    /// Returns the standard result mappers used by both initial graph and node neighbors queries
    pub fn get_graph_result_mappers() -> HashMap<&'static str, QueryResultMapper> {
        HashMap::from([
            ("source_id", STRING_MAPPER),
            ("source_type", STRING_MAPPER),
            ("source_name", STRING_MAPPER),
            ("source_path", STRING_MAPPER),
            ("source_absolute_path", STRING_MAPPER),
            ("source_repository_name", STRING_MAPPER),
            ("source_fqn", STRING_MAPPER),
            ("source_definition_type", STRING_MAPPER),
            ("source_language", STRING_MAPPER),
            ("source_extension", STRING_MAPPER),
            ("source_start_line", INT_MAPPER),
            ("source_primary_start_byte", INT_MAPPER),
            ("source_primary_end_byte", INT_MAPPER),
            ("source_total_locations", INT_MAPPER),
            ("source_import_type", STRING_MAPPER),
            ("source_import_path", STRING_MAPPER),
            ("source_import_alias", STRING_MAPPER),
            ("source_annotations_json", STRING_MAPPER),
            ("target_id", STRING_MAPPER),
            ("target_type", STRING_MAPPER),
            ("target_name", STRING_MAPPER),
            ("target_path", STRING_MAPPER),
            ("target_absolute_path", STRING_MAPPER),
            ("target_repository_name", STRING_MAPPER),
            ("target_fqn", STRING_MAPPER),
            ("target_definition_type", STRING_MAPPER),
            ("target_language", STRING_MAPPER),
            ("target_extension", STRING_MAPPER),
            ("target_start_line", INT_MAPPER),
            ("target_primary_start_byte", INT_MAPPER),
            ("target_primary_end_byte", INT_MAPPER),
            ("target_total_locations", INT_MAPPER),
            ("target_import_type", STRING_MAPPER),
            ("target_import_path", STRING_MAPPER),
            ("target_import_alias", STRING_MAPPER),
            ("target_annotations_json", STRING_MAPPER),
            ("relationship_name", STRING_MAPPER),
            ("relationship_id", STRING_MAPPER),
            ("relationship_type", STRING_MAPPER),
            ("relationship_name", RELATIONSHIP_TYPE_MAPPER),
            ("order_priority", INT_MAPPER),
        ])
    }

    fn get_node_neighbors_return_values(node_type: &str, alias: &str) -> String {
        match node_type {
            "DirectoryNode" => format!(
                r#"
                CAST({alias}.id AS STRING) as {alias}_id,
                'DirectoryNode' as {alias}_type,
                {alias}.name as {alias}_name,
                {alias}.path as {alias}_path,
                {alias}.absolute_path as {alias}_absolute_path,
                {alias}.repository_name as {alias}_repository_name,
                '' as {alias}_fqn,
                '' as {alias}_definition_type,
                '' as {alias}_language,
                '' as {alias}_extension,
                CAST(0 AS INT64) as {alias}_start_line,
                CAST(0 AS INT64) as {alias}_primary_start_byte,
                CAST(0 AS INT64) as {alias}_primary_end_byte,
                CAST(0 AS INT64) as {alias}_total_locations,
                '' as {alias}_import_type,
                '' as {alias}_import_path,
                '' as {alias}_import_alias,
                '' as {alias}_annotations_json,
            "#
            ),
            "FileNode" => format!(
                r#"
                    CAST({alias}.id AS STRING) as {alias}_id,
                    'FileNode' as {alias}_type,
                    {alias}.name as {alias}_name,
                    {alias}.path as {alias}_path,
                    {alias}.absolute_path as {alias}_absolute_path,
                    {alias}.repository_name as {alias}_repository_name,
                    '' as {alias}_fqn,
                    '' as {alias}_definition_type,
                    {alias}.language as {alias}_language,
                    {alias}.extension as {alias}_extension,
                    CAST(0 AS INT64) as {alias}_start_line,
                    CAST(0 AS INT64) as {alias}_primary_start_byte,
                    CAST(0 AS INT64) as {alias}_primary_end_byte,
                    CAST(0 AS INT64) as {alias}_total_locations,
                    '' as {alias}_import_type,
                    '' as {alias}_import_path,
                    '' as {alias}_import_alias,
                    '' as {alias}_annotations_json,
                "#
            ),
            "DefinitionNode" => format!(
                r#"
                    CAST({alias}.id AS STRING) as {alias}_id,
                    'DefinitionNode' as {alias}_type,
                    {alias}.name as {alias}_name,
                    {alias}.primary_file_path as {alias}_path,
                    '' as {alias}_absolute_path,
                    '' as {alias}_repository_name,
                    {alias}.fqn as {alias}_fqn,
                    {alias}.definition_type as {alias}_definition_type,
                    '' as {alias}_language,
                    '' as {alias}_extension,
                    CAST({alias}.start_line AS INT64) as {alias}_start_line,
                    {alias}.primary_start_byte as {alias}_primary_start_byte,
                    {alias}.primary_end_byte as {alias}_primary_end_byte,
                    CAST({alias}.total_locations AS INT64) as {alias}_total_locations,
                    '' as {alias}_import_type,
                    '' as {alias}_import_path,
                    '' as {alias}_import_alias,
                    COALESCE({alias}.annotations_json, '') as {alias}_annotations_json,
                "#
            ),
            "ImportedSymbolNode" => format!(
                r#"
                    CAST({alias}.id AS STRING) as {alias}_id,
                    'ImportedSymbolNode' as {alias}_type,
                    {alias}.name as {alias}_name,
                    {alias}.file_path as {alias}_path,
                    '' as {alias}_absolute_path,
                    '' as {alias}_repository_name,
                    '' as {alias}_fqn,
                    '' as {alias}_definition_type,
                    '' as {alias}_language,
                    '' as {alias}_extension,
                    CAST({alias}.start_line AS INT64) as {alias}_start_line,
                    {alias}.start_byte as {alias}_primary_start_byte,
                    {alias}.end_byte as {alias}_primary_end_byte,
                    CAST(0 AS INT64) as {alias}_total_locations,
                    {alias}.import_type as {alias}_import_type,
                    {alias}.import_path as {alias}_import_path,
                    {alias}.alias as {alias}_import_alias,
                    '' as {alias}_annotations_json,
                "#
            ),
            _ => "".to_string(),
        }
    }

    /// Returns relationship configs relevant to a specific node type for neighbor queries
    pub fn get_neighbor_relationship_configs(node_type: &str) -> Vec<RelationshipConfig> {
        let all_relationships = Self::get_all_relationship_configs();

        match node_type {
            "DirectoryNode" => all_relationships
                .into_iter()
                .filter(|config| config.source_type == "DirectoryNode")
                .collect(),
            "FileNode" => all_relationships
                .into_iter()
                .filter(|config| {
                    config.source_type == "FileNode" || config.target_type == "FileNode"
                })
                .collect(),
            "DefinitionNode" => all_relationships
                .into_iter()
                .filter(|config| {
                    config.source_type == "DefinitionNode" || config.target_type == "DefinitionNode"
                })
                .collect(),
            "ImportedSymbolNode" => all_relationships
                .into_iter()
                .filter(|config| {
                    config.target_type == "ImportedSymbolNode"
                        || config.source_type == "ImportedSymbolNode"
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Builds a neighbor query section with the appropriate WHERE clause for node filtering
    pub fn build_neighbor_query_section(config: &RelationshipConfig, node_type: &str) -> String {
        let source_return = Self::get_node_neighbors_return_values(config.source_type, "source");
        let target_return = Self::get_node_neighbors_return_values(config.target_type, "target");

        // Determine the WHERE clause based on which node type we're filtering by
        let where_clause = match node_type {
            "DirectoryNode" => "WHERE source.id = $node_id",
            "FileNode" => {
                if config.source_type == "FileNode" {
                    "WHERE source.id = $node_id"
                } else {
                    "WHERE target.id = $node_id"
                }
            }
            "DefinitionNode" => {
                if config.source_type == "DefinitionNode" {
                    "WHERE source.id = $node_id"
                } else {
                    "WHERE target.id = $node_id"
                }
            }
            "ImportedSymbolNode" => {
                if config.source_type == "ImportedSymbolNode" {
                    "WHERE source.id = $node_id"
                } else {
                    "WHERE target.id = $node_id"
                }
            }
            _ => "",
        };

        format!(
            r#"
            MATCH (source:{source_type})-[r:{relationship_name}]-(target:{target_type}) {where_clause}
            RETURN
                {source_return}
                {target_return}
                '{relationship_name}' as relationship_name,
                CAST(id(r) AS STRING) as relationship_id,
                r.type as relationship_type,
                {priority} as order_priority
            "#,
            source_type = config.source_type,
            relationship_name = config.relationship_name,
            target_type = config.target_type,
            priority = config.priority,
            where_clause = where_clause,
            source_return = source_return,
            target_return = target_return,
        )
    }

    pub fn get_node_neighbors_query(node_type: &str) -> Option<Query> {
        let relationship_configs = Self::get_neighbor_relationship_configs(node_type);

        if relationship_configs.is_empty() {
            return None;
        }

        let query_sections: Vec<String> = relationship_configs
            .iter()
            .map(|config| Self::build_neighbor_query_section(config, node_type))
            .collect();

        let query = format!("{} LIMIT $limit", query_sections.join("\nUNION\n"));

        Some(Query {
            query,
            parameters: HashMap::from([
                (
                    "node_id",
                    QueryParameter {
                        name: "node_id",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: Self::get_graph_result_mappers(),
        })
    }

    pub fn get_search_nodes_query() -> Query {
        Query {
            query: r#"
                MATCH (d:DirectoryNode)
                WHERE toLower(d.name) CONTAINS toLower($search_term)
                   OR toLower(d.path) CONTAINS toLower($search_term)
                RETURN
                    CAST(d.id AS STRING) as id,
                    'DirectoryNode' as node_type,
                    d.name as name,
                    d.path as path,
                    d.absolute_path as absolute_path,
                    d.repository_name as repository_name,
                    '' as fqn,
                    '' as definition_type,
                    '' as language,
                    '' as extension,
                    CAST(0 AS INT64) as start_line,
                    CAST(0 AS INT64) as primary_start_byte,
                    CAST(0 AS INT64) as primary_end_byte,
                    CAST(0 AS INT64) as total_locations,
                    '' as import_type,
                    '' as import_path,
                    '' as import_alias,
                    '' as annotations_json
                UNION
                MATCH (f:FileNode)
                WHERE toLower(f.name) CONTAINS toLower($search_term)
                   OR toLower(f.path) CONTAINS toLower($search_term)
                RETURN
                    CAST(f.id AS STRING) as id,
                    'FileNode' as node_type,
                    f.name as name,
                    f.path as path,
                    f.absolute_path as absolute_path,
                    f.repository_name as repository_name,
                    '' as fqn,
                    '' as definition_type,
                    f.language as language,
                    f.extension as extension,
                    CAST(0 AS INT64) as start_line,
                    CAST(0 AS INT64) as primary_start_byte,
                    CAST(0 AS INT64) as primary_end_byte,
                    CAST(0 AS INT64) as total_locations,
                    '' as import_type,
                    '' as import_path,
                    '' as import_alias,
                    '' as annotations_json
                UNION
                MATCH (def:DefinitionNode)
                WHERE toLower(def.name) CONTAINS toLower($search_term)
                   OR toLower(def.fqn) CONTAINS toLower($search_term)
                RETURN
                    CAST(def.id AS STRING) as id,
                    'DefinitionNode' as node_type,
                    def.name as name,
                    def.primary_file_path as path,
                    '' as absolute_path,
                    '' as repository_name,
                    def.fqn as fqn,
                    def.definition_type as definition_type,
                    '' as language,
                    '' as extension,
                    CAST(def.start_line AS INT64) as start_line,
                    def.primary_start_byte as primary_start_byte,
                    def.primary_end_byte as primary_end_byte,
                    CAST(def.total_locations AS INT64) as total_locations,
                    '' as import_type,
                    '' as import_path,
                    '' as import_alias,
                    COALESCE(def.annotations_json, '') as annotations_json
                UNION
                MATCH (imp:ImportedSymbolNode)
                WHERE toLower(imp.name) CONTAINS toLower($search_term)
                   OR toLower(imp.import_path) CONTAINS toLower($search_term)
                   OR toLower(imp.alias) CONTAINS toLower($search_term)
                RETURN
                    CAST(imp.id AS STRING) as id,
                    'ImportedSymbolNode' as node_type,
                    imp.name as name,
                    imp.file_path as path,
                    '' as absolute_path,
                    '' as repository_name,
                    '' as fqn,
                    '' as definition_type,
                    '' as language,
                    '' as extension,
                    CAST(imp.start_line AS INT64) as start_line,
                    imp.start_byte as primary_start_byte,
                    imp.end_byte as primary_end_byte,
                    CAST(0 AS INT64) as total_locations,
                    imp.import_type as import_type,
                    imp.import_path as import_path,
                    imp.alias as import_alias,
                    '' as annotations_json
                ORDER BY node_type, name
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "search_term",
                    QueryParameter {
                        name: "search_term",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(100)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", STRING_MAPPER),
                ("node_type", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("absolute_path", STRING_MAPPER),
                ("repository_name", STRING_MAPPER),
                ("fqn", STRING_MAPPER),
                ("definition_type", STRING_MAPPER),
                ("language", STRING_MAPPER),
                ("extension", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("primary_start_byte", INT_MAPPER),
                ("primary_end_byte", INT_MAPPER),
                ("total_locations", INT_MAPPER),
                ("import_type", STRING_MAPPER),
                ("import_path", STRING_MAPPER),
                ("import_alias", STRING_MAPPER),
                ("annotations_json", STRING_MAPPER),
            ]),
        }
    }

    pub fn get_search_definitions_query() -> Query {
        Query {
            query: r#"
                MATCH (d:DefinitionNode)
                WHERE ANY(term IN $search_terms WHERE toLower(d.name) CONTAINS term)
                RETURN 
                    d.name as name,
                    d.fqn as fqn,
                    d.definition_type as definition_type,
                    d.primary_file_path as file_path,
                    d.start_line as start_line,
                    d.end_line as end_line
                ORDER BY d.name
                SKIP $skip
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "search_terms",
                    QueryParameter {
                        name: "search_terms",
                        definition: QueryParameterDefinition::Array(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(10)),
                    },
                ),
                (
                    "skip",
                    QueryParameter {
                        name: "skip",
                        definition: QueryParameterDefinition::Int(Some(0)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    pub fn get_definitions_by_fqn_or_name_query() -> Query {
        Query {
            query: r#"
                MATCH (d:DefinitionNode)
                WHERE
                    d.primary_file_path = $file_path
                    AND (
                        toLower(d.name) CONTAINS toLower($name_or_fqn)
                        OR toLower(d.fqn) CONTAINS toLower($name_or_fqn)
                    )
                RETURN
                    d.id as id,
                    d.name as name,
                    d.fqn as fqn,
                    d.primary_file_path as file_path,
                    d.start_line as line_number
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "file_path",
                    QueryParameter {
                        name: "file_path",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "name_or_fqn",
                    QueryParameter {
                        name: "name_or_fqn",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", STRING_MAPPER),
                ("name", STRING_MAPPER),
                ("fqn", STRING_MAPPER),
                ("line_number", INT_MAPPER),
                ("file_path", STRING_MAPPER),
            ]),
        }
    }


    /// Query to get Java external service calls from FeignClient interfaces
    /// Returns interfaces and methods that define external service calls via @FeignClient
    pub fn get_java_service_calls_query() -> Query {
        Query {
            query: r#"
                MATCH (interface:DefinitionNode)-[:DEFINITION_RELATIONSHIPS]-(method:DefinitionNode)
                WHERE interface.definition_type = 'Interface'
                  AND interface.annotations_json CONTAINS 'FeignClient'
                  AND method.definition_type = 'Method'
                  AND method.annotations_json IS NOT NULL
                  AND method.annotations_json <> ''
                  AND (
                    method.annotations_json CONTAINS 'GetMapping'
                    OR method.annotations_json CONTAINS 'PostMapping'
                    OR method.annotations_json CONTAINS 'PutMapping'
                    OR method.annotations_json CONTAINS 'DeleteMapping'
                    OR method.annotations_json CONTAINS 'PatchMapping'
                    OR method.annotations_json CONTAINS 'RequestMapping'
                  )
                RETURN
                    'FeignClient' as service_type,
                    interface.name as class_name,
                    interface.fqn as class_fqn,
                    interface.primary_file_path as file_path,
                    COALESCE(interface.annotations_json, '') as class_annotations,
                    method.name as method_name,
                    method.fqn as method_fqn,
                    method.start_line as method_start_line,
                    COALESCE(method.annotations_json, '') as method_annotations
                ORDER BY interface.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get Java classes that use RestTemplate for HTTP calls
    /// Detects classes that import org.springframework.web.client.RestTemplate
    pub fn get_java_rest_template_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode),
                      (file)-[:FILE_RELATIONSHIPS]->(class:DefinitionNode),
                      (class)-[:DEFINITION_RELATIONSHIPS]->(method:DefinitionNode)
                WHERE imp.name = 'RestTemplate'
                  AND imp.import_path CONTAINS 'springframework'
                  AND class.definition_type = 'Class'
                  AND method.definition_type = 'Method'
                RETURN 'RestTemplate' AS service_type,
                    class.name AS class_name,
                    class.fqn AS class_fqn,
                    class.primary_file_path AS file_path,
                    class.annotations_json AS class_annotations,
                    method.name AS method_name,
                    method.fqn AS method_fqn,
                    method.start_line AS method_start_line,
                    method.annotations_json AS method_annotations
                ORDER BY class.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get Java classes that use WebClient for HTTP calls
    /// Detects classes that import org.springframework.web.reactive.function.client.WebClient
    pub fn get_java_web_client_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode),
                      (file)-[:FILE_RELATIONSHIPS]->(class:DefinitionNode),
                      (class)-[:DEFINITION_RELATIONSHIPS]->(method:DefinitionNode)
                WHERE imp.name = 'WebClient'
                  AND imp.import_path CONTAINS 'springframework'
                  AND class.definition_type = 'Class'
                  AND method.definition_type = 'Method'
                RETURN 'WebClient' AS service_type,
                    class.name AS class_name,
                    class.fqn AS class_fqn,
                    class.primary_file_path AS file_path,
                    class.annotations_json AS class_annotations,
                    method.name AS method_name,
                    method.fqn AS method_fqn,
                    method.start_line AS method_start_line,
                    method.annotations_json AS method_annotations
                ORDER BY class.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get Java classes that use Apache HttpClient for HTTP calls
    /// Detects classes that import org.apache.http.client.HttpClient
    pub fn get_java_http_client_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode),
                      (file)-[:FILE_RELATIONSHIPS]->(class:DefinitionNode),
                      (class)-[:DEFINITION_RELATIONSHIPS]->(method:DefinitionNode)
                WHERE (imp.name = 'HttpClient' OR imp.name = 'CloseableHttpClient')
                  AND imp.import_path CONTAINS 'apache'
                  AND class.definition_type = 'Class'
                  AND method.definition_type = 'Method'
                RETURN 'HttpClient' AS service_type,
                    class.name AS class_name,
                    class.fqn AS class_fqn,
                    class.primary_file_path AS file_path,
                    class.annotations_json AS class_annotations,
                    method.name AS method_name,
                    method.fqn AS method_fqn,
                    method.start_line AS method_start_line,
                    method.annotations_json AS method_annotations
                ORDER BY class.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get Java classes that use OkHttp for HTTP calls
    /// Detects classes that import okhttp3.OkHttpClient
    pub fn get_java_okhttp_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode),
                      (file)-[:FILE_RELATIONSHIPS]->(class:DefinitionNode),
                      (class)-[:DEFINITION_RELATIONSHIPS]->(method:DefinitionNode)
                WHERE imp.name = 'OkHttpClient'
                  AND imp.import_path CONTAINS 'okhttp'
                  AND class.definition_type = 'Class'
                  AND method.definition_type = 'Method'
                RETURN 'OkHttp' AS service_type,
                    class.name AS class_name,
                    class.fqn AS class_fqn,
                    class.primary_file_path AS file_path,
                    class.annotations_json AS class_annotations,
                    method.name AS method_name,
                    method.fqn AS method_fqn,
                    method.start_line AS method_start_line,
                    method.annotations_json AS method_annotations
                ORDER BY class.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get Java classes that use Retrofit for HTTP calls
    /// Detects classes that import retrofit2.Retrofit
    pub fn get_java_retrofit_query() -> Query {
        Query {
            query: r#"
                MATCH (file:FileNode)-[:FILE_RELATIONSHIPS]->(imp:ImportedSymbolNode),
                      (file)-[:FILE_RELATIONSHIPS]->(class:DefinitionNode),
                      (class)-[:DEFINITION_RELATIONSHIPS]->(method:DefinitionNode)
                WHERE imp.name = 'Retrofit'
                  AND imp.import_path CONTAINS 'retrofit'
                  AND class.definition_type = 'Class'
                  AND method.definition_type = 'Method'
                RETURN 'Retrofit' AS service_type,
                    class.name AS class_name,
                    class.fqn AS class_fqn,
                    class.primary_file_path AS file_path,
                    class.annotations_json AS class_annotations,
                    method.name AS method_name,
                    method.fqn AS method_fqn,
                    method.start_line AS method_start_line,
                    method.annotations_json AS method_annotations
                ORDER BY class.fqn, method.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("service_type", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("class_annotations", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
                ("method_annotations", STRING_MAPPER),
            ]),
        }
    }

    /// Query to get all endpoints
    /// Returns all EndpointNode entries from the graph
    pub fn get_endpoints_query() -> Query {
        Query {
            query: r#"
                MATCH (ep:EndpointNode)
                RETURN
                    ep.id as id,
                    ep.http_method as http_method,
                    ep.path as path,
                    ep.full_path as full_path,
                    ep.consumes as consumes,
                    ep.produces as produces,
                    ep.description as description,
                    ep.deprecated as deprecated,
                    ep.path_params_json as path_params_json,
                    ep.query_params_json as query_params_json,
                    ep.request_body_json as request_body_json,
                    ep.response_body_json as response_body_json,
                    ep.file_path as file_path,
                    ep.start_line as start_line,
                    ep.end_line as end_line
                ORDER BY ep.file_path, ep.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("consumes", STRING_MAPPER),
                ("produces", STRING_MAPPER),
                ("description", STRING_MAPPER),
                ("deprecated", INT_MAPPER),
                ("path_params_json", STRING_MAPPER),
                ("query_params_json", STRING_MAPPER),
                ("request_body_json", STRING_MAPPER),
                ("response_body_json", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get endpoints by file path
    /// Returns all endpoints defined in a specific file
    pub fn get_endpoints_by_file_query() -> Query {
        Query {
            query: r#"
                MATCH (f:FileNode)-[:ENDPOINT_RELATIONSHIPS]->(ep:EndpointNode)
                WHERE f.path = $file_path OR f.absolute_path = $file_path
                RETURN
                    ep.id as id,
                    ep.http_method as http_method,
                    ep.path as path,
                    ep.full_path as full_path,
                    ep.consumes as consumes,
                    ep.produces as produces,
                    ep.description as description,
                    ep.deprecated as deprecated,
                    ep.path_params_json as path_params_json,
                    ep.query_params_json as query_params_json,
                    ep.request_body_json as request_body_json,
                    ep.response_body_json as response_body_json,
                    ep.file_path as file_path,
                    ep.start_line as start_line,
                    ep.end_line as end_line
                ORDER BY ep.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "file_path",
                    QueryParameter {
                        name: "file_path",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(1000)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("consumes", STRING_MAPPER),
                ("produces", STRING_MAPPER),
                ("description", STRING_MAPPER),
                ("deprecated", INT_MAPPER),
                ("path_params_json", STRING_MAPPER),
                ("query_params_json", STRING_MAPPER),
                ("request_body_json", STRING_MAPPER),
                ("response_body_json", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get endpoints by HTTP method
    /// Returns all endpoints filtered by HTTP method (GET, POST, etc.)
    pub fn get_endpoints_by_method_query() -> Query {
        Query {
            query: r#"
                MATCH (ep:EndpointNode)
                WHERE ep.http_method = $http_method
                RETURN
                    ep.id as id,
                    ep.http_method as http_method,
                    ep.path as path,
                    ep.full_path as full_path,
                    ep.consumes as consumes,
                    ep.produces as produces,
                    ep.description as description,
                    ep.deprecated as deprecated,
                    ep.path_params_json as path_params_json,
                    ep.query_params_json as query_params_json,
                    ep.request_body_json as request_body_json,
                    ep.response_body_json as response_body_json,
                    ep.file_path as file_path,
                    ep.start_line as start_line,
                    ep.end_line as end_line
                ORDER BY ep.file_path, ep.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "http_method",
                    QueryParameter {
                        name: "http_method",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(1000)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("consumes", STRING_MAPPER),
                ("produces", STRING_MAPPER),
                ("description", STRING_MAPPER),
                ("deprecated", INT_MAPPER),
                ("path_params_json", STRING_MAPPER),
                ("query_params_json", STRING_MAPPER),
                ("request_body_json", STRING_MAPPER),
                ("response_body_json", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get endpoint with its defining method
    /// Returns endpoint along with the method that defines it
    pub fn get_endpoint_with_definition_query() -> Query {
        Query {
            query: r#"
                MATCH (method:DefinitionNode)-[:ENDPOINT_RELATIONSHIPS]->(ep:EndpointNode)
                WHERE ep.id = $endpoint_id
                RETURN
                    ep.id as endpoint_id,
                    ep.http_method as http_method,
                    ep.path as path,
                    ep.full_path as full_path,
                    ep.consumes as consumes,
                    ep.produces as produces,
                    ep.description as description,
                    ep.deprecated as deprecated,
                    ep.path_params_json as path_params_json,
                    ep.query_params_json as query_params_json,
                    ep.request_body_json as request_body_json,
                    ep.response_body_json as response_body_json,
                    ep.file_path as endpoint_file_path,
                    ep.start_line as endpoint_start_line,
                    ep.end_line as endpoint_end_line,
                    method.fqn as method_fqn,
                    method.name as method_name,
                    method.definition_type as method_type,
                    method.primary_file_path as method_file_path,
                    method.start_line as method_start_line
                LIMIT 1
            "#
            .to_string(),
            parameters: HashMap::from([(
                "endpoint_id",
                QueryParameter {
                    name: "endpoint_id",
                    definition: QueryParameterDefinition::Int(None),
                },
            )]),
            result: HashMap::from([
                ("endpoint_id", INT_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("consumes", STRING_MAPPER),
                ("produces", STRING_MAPPER),
                ("description", STRING_MAPPER),
                ("deprecated", INT_MAPPER),
                ("path_params_json", STRING_MAPPER),
                ("query_params_json", STRING_MAPPER),
                ("request_body_json", STRING_MAPPER),
                ("response_body_json", STRING_MAPPER),
                ("endpoint_file_path", STRING_MAPPER),
                ("endpoint_start_line", INT_MAPPER),
                ("endpoint_end_line", INT_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_type", STRING_MAPPER),
                ("method_file_path", STRING_MAPPER),
                ("method_start_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get all service call nodes
    /// Returns all service calls extracted from Java code
    pub fn get_service_calls_query() -> Query {
        Query {
            query: r#"
                MATCH (sc:ServiceCallNode)
                RETURN
                    sc.id as id,
                    sc.service_type as service_type,
                    sc.service_name as service_name,
                    sc.service_url as service_url,
                    sc.http_method as http_method,
                    sc.path as path,
                    sc.full_path as full_path,
                    sc.class_name as class_name,
                    sc.class_fqn as class_fqn,
                    sc.method_name as method_name,
                    sc.method_fqn as method_fqn,
                    sc.file_path as file_path,
                    sc.start_line as start_line,
                    sc.end_line as end_line
                ORDER BY sc.file_path, sc.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([(
                "limit",
                QueryParameter {
                    name: "limit",
                    definition: QueryParameterDefinition::Int(Some(1000)),
                },
            )]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("service_type", STRING_MAPPER),
                ("service_name", STRING_MAPPER),
                ("service_url", STRING_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get service calls by file path
    /// Returns service calls defined in a specific file
    pub fn get_service_calls_by_file_query() -> Query {
        Query {
            query: r#"
                MATCH (f:FileNode)-[:SERVICE_CALL_RELATIONSHIPS]->(sc:ServiceCallNode)
                WHERE f.path = $file_path OR f.absolute_path = $file_path
                RETURN
                    sc.id as id,
                    sc.service_type as service_type,
                    sc.service_name as service_name,
                    sc.service_url as service_url,
                    sc.http_method as http_method,
                    sc.path as path,
                    sc.full_path as full_path,
                    sc.class_name as class_name,
                    sc.class_fqn as class_fqn,
                    sc.method_name as method_name,
                    sc.method_fqn as method_fqn,
                    sc.file_path as file_path,
                    sc.start_line as start_line,
                    sc.end_line as end_line
                ORDER BY sc.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "file_path",
                    QueryParameter {
                        name: "file_path",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(1000)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("service_type", STRING_MAPPER),
                ("service_name", STRING_MAPPER),
                ("service_url", STRING_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get service calls by service type
    /// Returns service calls filtered by service type (FeignClient, RestTemplate, etc.)
    pub fn get_service_calls_by_type_query() -> Query {
        Query {
            query: r#"
                MATCH (sc:ServiceCallNode)
                WHERE sc.service_type = $service_type
                RETURN
                    sc.id as id,
                    sc.service_type as service_type,
                    sc.service_name as service_name,
                    sc.service_url as service_url,
                    sc.http_method as http_method,
                    sc.path as path,
                    sc.full_path as full_path,
                    sc.class_name as class_name,
                    sc.class_fqn as class_fqn,
                    sc.method_name as method_name,
                    sc.method_fqn as method_fqn,
                    sc.file_path as file_path,
                    sc.start_line as start_line,
                    sc.end_line as end_line
                ORDER BY sc.file_path, sc.start_line
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "service_type",
                    QueryParameter {
                        name: "service_type",
                        definition: QueryParameterDefinition::String(None),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(1000)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("id", INT_MAPPER),
                ("service_type", STRING_MAPPER),
                ("service_name", STRING_MAPPER),
                ("service_url", STRING_MAPPER),
                ("http_method", STRING_MAPPER),
                ("path", STRING_MAPPER),
                ("full_path", STRING_MAPPER),
                ("class_name", STRING_MAPPER),
                ("class_fqn", STRING_MAPPER),
                ("method_name", STRING_MAPPER),
                ("method_fqn", STRING_MAPPER),
                ("file_path", STRING_MAPPER),
                ("start_line", INT_MAPPER),
                ("end_line", INT_MAPPER),
            ]),
        }
    }

    /// Query to get endpoint execution flow
    /// Returns the method call chain for a specific endpoint
    pub fn get_endpoint_flow_query() -> Query {
        Query {
            query: r#"
                MATCH (method:DefinitionNode)-[:ENDPOINT_RELATIONSHIPS]->(ep:EndpointNode)
                WHERE ep.id = $endpoint_id AND method.definition_type = 'Method'
                RETURN
                    ep.id as endpoint_id,
                    ep.http_method as endpoint_http_method,
                    ep.path as endpoint_path,
                    ep.full_path as endpoint_full_path,
                    ep.file_path as endpoint_file_path,
                    ep.start_line as endpoint_start_line,
                    ep.end_line as endpoint_end_line,
                    '' as caller_method_fqn,
                    '' as called_method_fqn,
                    '' as called_method_name,
                    '' as called_method_class_name,
                    '' as called_method_file_path,
                    CAST(0 AS INT64) as called_method_start_line,
                    CAST(0 AS INT64) as called_method_end_line,
                    CAST(0 AS INT64) as depth,
                    '' as relationship_type
                UNION ALL
                MATCH (method:DefinitionNode)-[:ENDPOINT_RELATIONSHIPS]->(ep:EndpointNode),
                      p = (method)-[rels:DEFINITION_RELATIONSHIPS*1..5]->(called:DefinitionNode)
                WHERE ep.id = $endpoint_id
                  AND method.definition_type = 'Method'
                  AND called.definition_type = 'Method'
                WITH ep, method, p as path_match, called,
                     nodes(p) as path_nodes,
                     relationships(p) as path_rels,
                     length(p) as depth
                WHERE depth <= $max_depth
                WITH ep, method, called, depth,
                     path_nodes[size(path_nodes)-1] as caller,
                     path_rels[size(path_rels)] as last_rel
                WHERE last_rel.type IN ['CALLS', 'AMBIGUOUSLY_CALLS']
                RETURN DISTINCT
                    ep.id as endpoint_id,
                    ep.http_method as endpoint_http_method,
                    ep.path as endpoint_path,
                    ep.full_path as endpoint_full_path,
                    ep.file_path as endpoint_file_path,
                    ep.start_line as endpoint_start_line,
                    ep.end_line as endpoint_end_line,
                    caller.fqn as caller_method_fqn,
                    called.fqn as called_method_fqn,
                    called.name as called_method_name,
                    substring(called.fqn, 1, size(called.fqn) - size(called.name) - 1) as called_method_class_name,
                    called.primary_file_path as called_method_file_path,
                    CAST(called.start_line AS INT64) as called_method_start_line,
                    CAST(called.end_line AS INT64) as called_method_end_line,
                    CAST(depth AS INT64) as depth,
                    last_rel.type as relationship_type
                LIMIT $limit
            "#
            .to_string(),
            parameters: HashMap::from([
                (
                    "endpoint_id",
                    QueryParameter {
                        name: "endpoint_id",
                        definition: QueryParameterDefinition::Int(None),
                    },
                ),
                (
                    "max_depth",
                    QueryParameter {
                        name: "max_depth",
                        definition: QueryParameterDefinition::Int(Some(5)),
                    },
                ),
                (
                    "limit",
                    QueryParameter {
                        name: "limit",
                        definition: QueryParameterDefinition::Int(Some(1000)),
                    },
                ),
            ]),
            result: HashMap::from([
                ("endpoint_id", INT_MAPPER),
                ("endpoint_http_method", STRING_MAPPER),
                ("endpoint_path", STRING_MAPPER),
                ("endpoint_full_path", STRING_MAPPER),
                ("endpoint_file_path", STRING_MAPPER),
                ("endpoint_start_line", INT_MAPPER),
                ("endpoint_end_line", INT_MAPPER),
                ("caller_method_fqn", STRING_MAPPER),
                ("called_method_fqn", STRING_MAPPER),
                ("called_method_name", STRING_MAPPER),
                ("called_method_class_name", STRING_MAPPER),
                ("called_method_file_path", STRING_MAPPER),
                ("called_method_start_line", INT_MAPPER),
                ("called_method_end_line", INT_MAPPER),
                ("depth", INT_MAPPER),
                ("relationship_type", STRING_MAPPER),
            ]),
        }
    }
}
