use crate::schema::types::{ColumnDefinition, NodeTable, RelationshipKind, RelationshipTable};

// Directory nodes
pub static DIRECTORY_TABLE: NodeTable = NodeTable {
    name: "DirectoryNode",
    parquet_filename: "directories.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("path"),
        ColumnDefinition::new("absolute_path"),
        ColumnDefinition::new("repository_name"),
        ColumnDefinition::new("name"),
    ],
};

pub static FILE_TABLE: NodeTable = NodeTable {
    name: "FileNode",
    parquet_filename: "files.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("path"),
        ColumnDefinition::new("absolute_path"),
        ColumnDefinition::new("language"),
        ColumnDefinition::new("repository_name"),
        ColumnDefinition::new("extension"),
        ColumnDefinition::new("name"),
    ],
};

pub static DEFINITION_TABLE: NodeTable = NodeTable {
    name: "DefinitionNode",
    parquet_filename: "definitions.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("fqn"),
        ColumnDefinition::new("name"),
        ColumnDefinition::new("definition_type"),
        ColumnDefinition::new("primary_file_path"),
        ColumnDefinition::new("primary_start_byte").int64(),
        ColumnDefinition::new("primary_end_byte").int64(),
        ColumnDefinition::new("start_line").int32(),
        ColumnDefinition::new("end_line").int32(),
        ColumnDefinition::new("start_col").int32(),
        ColumnDefinition::new("end_col").int32(),
        ColumnDefinition::new("total_locations").int32(),
        ColumnDefinition::new("annotations_json").nullable(),
    ],
};

// Imported symbol nodes
pub static IMPORTED_SYMBOL_TABLE: NodeTable = NodeTable {
    name: "ImportedSymbolNode",
    parquet_filename: "imported_symbols.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("import_type"),
        ColumnDefinition::new("import_path"),
        ColumnDefinition::new("name"),
        ColumnDefinition::new("alias"),
        ColumnDefinition::new("file_path"),
        ColumnDefinition::new("start_byte").int64(),
        ColumnDefinition::new("end_byte").int64(),
        ColumnDefinition::new("start_line").int32(),
        ColumnDefinition::new("end_line").int32(),
        ColumnDefinition::new("start_col").int32(),
        ColumnDefinition::new("end_col").int32(),
    ],
};

// Endpoint nodes - represents API endpoints (REST, GraphQL, etc.)
pub static ENDPOINT_TABLE: NodeTable = NodeTable {
    name: "EndpointNode",
    parquet_filename: "endpoints.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("http_method"),           // GET, POST, PUT, DELETE, PATCH, etc.
        ColumnDefinition::new("path"),                  // Endpoint path: /api/users/{id}
        ColumnDefinition::new("full_path"),             // Complete path with base: /api/v1/users/{id}
        ColumnDefinition::new("consumes").nullable(),   // Content-Type consumed (application/json)
        ColumnDefinition::new("produces").nullable(),   // Content-Type produced (application/json)
        ColumnDefinition::new("description").nullable(), // Optional description
        ColumnDefinition::new("deprecated").boolean(),  // Is endpoint deprecated?
        ColumnDefinition::new("path_params_json").nullable(),  // JSON array of path parameters
        ColumnDefinition::new("query_params_json").nullable(), // JSON array of query parameters
        ColumnDefinition::new("request_body_json").nullable(), // JSON schema of request body
        ColumnDefinition::new("response_body_json").nullable(), // JSON schema of response body
        ColumnDefinition::new("file_path"),             // File where endpoint is defined
        ColumnDefinition::new("start_line").int32(),    // Line number where endpoint starts
        ColumnDefinition::new("end_line").int32(),      // Line number where endpoint ends
    ],
};

// Service call nodes - represents external service calls (FeignClient, RestTemplate, WebClient, etc.)
pub static SERVICE_CALL_TABLE: NodeTable = NodeTable {
    name: "ServiceCallNode",
    parquet_filename: "service_calls.parquet",
    columns: &[
        ColumnDefinition::new("id").uint32().primary_key(),
        ColumnDefinition::new("service_type"),          // FeignClient, RestTemplate, WebClient, HttpClient, OkHttp, Retrofit
        ColumnDefinition::new("service_name"),          // Service or client name
        ColumnDefinition::new("service_url"),           // Base URL or service identifier
        ColumnDefinition::new("http_method"),           // GET, POST, PUT, DELETE, PATCH, etc.
        ColumnDefinition::new("path"),                  // Endpoint path: /api/users/{id}
        ColumnDefinition::new("full_path"),             // Complete path with base: /api/v1/users/{id}
        ColumnDefinition::new("class_name"),            // Class name where service call is defined
        ColumnDefinition::new("class_fqn"),             // Fully qualified class name
        ColumnDefinition::new("method_name"),           // Method name
        ColumnDefinition::new("method_fqn"),            // Fully qualified method name
        ColumnDefinition::new("file_path"),             // File where service call is defined
        ColumnDefinition::new("start_line").int32(),    // Line number where service call starts
        ColumnDefinition::new("end_line").int32(),      // Line number where service call ends
    ],
};

// Node tables
pub static NODE_TABLES: &[NodeTable] = &[
    DIRECTORY_TABLE,
    FILE_TABLE,
    DEFINITION_TABLE,
    IMPORTED_SYMBOL_TABLE,
    ENDPOINT_TABLE,
    SERVICE_CALL_TABLE,
];

// If we have unused columns, they take up no space by kuzu
// Source id and target id are implicit columns in Kuzu relationships
pub static RELATIONSHIP_TABLE_COLUMNS: &[ColumnDefinition] = &[
    ColumnDefinition::new("type").string(),
    // Optional source location fields for imports and calls
    ColumnDefinition::new("source_start_byte")
        .int64()
        .nullable(),
    ColumnDefinition::new("source_end_byte").int64().nullable(),
    ColumnDefinition::new("source_start_line")
        .int32()
        .nullable(),
    ColumnDefinition::new("source_end_line").int32().nullable(),
    ColumnDefinition::new("source_start_col").int32().nullable(),
    ColumnDefinition::new("source_end_col").int32().nullable(),
];

// Directory relationships (DIR_CONTAINS_DIR + DIR_CONTAINS_FILE)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static DIRECTORY_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "DIRECTORY_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &DIRECTORY_TABLE,
            &DIRECTORY_TABLE,
            Some(&RelationshipKind::DirectoryToDirectory),
        ),
        (
            &DIRECTORY_TABLE,
            &FILE_TABLE,
            Some(&RelationshipKind::DirectoryToFile),
        ),
    ],
};

// File relationships (FILE_DEFINES + FILE_IMPORTS)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static FILE_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "FILE_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &FILE_TABLE,
            &DEFINITION_TABLE,
            Some(&RelationshipKind::FileToDefinition),
        ),
        (
            &FILE_TABLE,
            &IMPORTED_SYMBOL_TABLE,
            Some(&RelationshipKind::FileToImportedSymbol),
        ),
    ],
};

// Definition relationships (DEFINES_IMPORTED_SYMBOL, all MODULE_TO_*, CLASS_TO_*, METHOD_*)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static DEFINITION_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "DEFINITION_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &DEFINITION_TABLE,
            &DEFINITION_TABLE,
            Some(&RelationshipKind::DefinitionToDefinition),
        ),
        (
            &DEFINITION_TABLE,
            &IMPORTED_SYMBOL_TABLE,
            Some(&RelationshipKind::DefinitionToImportedSymbol),
        ),
    ],
};

// Imported symbol relationships (IMPORTED_SYMBOL_TO_IMPORTED_SYMBOL, IMPORTED_SYMBOL_TO_DEFINITION, IMPORTED_SYMBOL_TO_FILE)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static IMPORTED_SYMBOL_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "IMPORTED_SYMBOL_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &IMPORTED_SYMBOL_TABLE,
            &IMPORTED_SYMBOL_TABLE,
            Some(&RelationshipKind::ImportedSymbolToImportedSymbol),
        ),
        (
            &IMPORTED_SYMBOL_TABLE,
            &DEFINITION_TABLE,
            Some(&RelationshipKind::ImportedSymbolToDefinition),
        ),
        (
            &IMPORTED_SYMBOL_TABLE,
            &FILE_TABLE,
            Some(&RelationshipKind::ImportedSymbolToFile),
        ),
    ],
};

// Endpoint relationships (DEFINITION_TO_ENDPOINT, FILE_TO_ENDPOINT)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static ENDPOINT_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "ENDPOINT_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &DEFINITION_TABLE,
            &ENDPOINT_TABLE,
            Some(&RelationshipKind::DefinitionToEndpoint),
        ),
        (
            &FILE_TABLE,
            &ENDPOINT_TABLE,
            Some(&RelationshipKind::FileToEndpoint),
        ),
    ],
};

// Service call relationships (DEFINITION_TO_SERVICE_CALL, FILE_TO_SERVICE_CALL)
// Note: Kuzu automatically handles FROM-TO connections, we only need custom properties
pub static SERVICE_CALL_RELATIONSHIPS: RelationshipTable = RelationshipTable {
    name: "SERVICE_CALL_RELATIONSHIPS",
    columns: RELATIONSHIP_TABLE_COLUMNS,
    from_to_pairs: &[
        (
            &DEFINITION_TABLE,
            &SERVICE_CALL_TABLE,
            Some(&RelationshipKind::DefinitionToServiceCall),
        ),
        (
            &FILE_TABLE,
            &SERVICE_CALL_TABLE,
            Some(&RelationshipKind::FileToServiceCall),
        ),
    ],
};

pub static RELATIONSHIP_TABLES: &[RelationshipTable] = &[
    DIRECTORY_RELATIONSHIPS,
    FILE_RELATIONSHIPS,
    DEFINITION_RELATIONSHIPS,
    IMPORTED_SYMBOL_RELATIONSHIPS,
    ENDPOINT_RELATIONSHIPS,
    SERVICE_CALL_RELATIONSHIPS,
];
