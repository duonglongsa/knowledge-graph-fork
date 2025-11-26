use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use internment::ArcIntern;

use database::graph::RelationshipType;
use database::schema::types::{NodeFieldAccess, RelationshipKind};
use parser_core::{
    csharp::types::{CSharpDefinitionType, CSharpFqn, CSharpImportType},
    definitions::DefinitionTypeInfo,
    imports::ImportTypeInfo,
    java::ast::java_fqn_to_string,
    java::types::{JavaDefinitionType, JavaFqn, JavaImportType},
    kotlin::ast::kotlin_fqn_to_string,
    kotlin::types::{KotlinDefinitionType, KotlinFqn, KotlinImportType},
    python::fqn::python_fqn_to_string,
    python::types::{PythonDefinitionType, PythonFqn, PythonImportType},
    ruby::{
        fqn::ruby_fqn_to_string,
        types::{RubyDefinitionType, RubyFqn},
    },
    rust::fqn::rust_fqn_to_string,
    rust::types::{RustDefinitionType, RustFqn, RustImportType},
    typescript::ast::typescript_fqn_to_string,
    typescript::types::{TypeScriptDefinitionType, TypeScriptFqn, TypeScriptImportType},
    utils::{HasRange, Position, Range},
};
use serde::{Deserialize, Serialize};

/// Consolidated relationship data for efficient storage
#[derive(Debug, Clone)]
pub struct ConsolidatedRelationship {
    pub kind: RelationshipKind,
    pub source_id: Option<u32>,
    pub target_id: Option<u32>,
    pub relationship_type: RelationshipType,
    pub source_path: Option<ArcIntern<String>>,
    pub target_path: Option<ArcIntern<String>>,
    pub source_range: ArcIntern<Range>,
    pub target_range: ArcIntern<Range>,
    /// Definition location for source node (used for ID lookup)
    pub source_definition_range: Option<ArcIntern<Range>>,
    /// Definition location for target node (used for ID lookup)  
    pub target_definition_range: Option<ArcIntern<Range>>,
}

impl Default for ConsolidatedRelationship {
    fn default() -> Self {
        Self {
            kind: RelationshipKind::Empty,
            source_id: None,
            target_id: None,
            relationship_type: RelationshipType::Empty,
            source_path: None,
            target_path: None,
            source_range: ArcIntern::new(Range::empty()),
            target_range: ArcIntern::new(Range::empty()),
            source_definition_range: None,
            target_definition_range: None,
        }
    }
}

impl ConsolidatedRelationship {
    pub fn dir_to_dir(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DirectoryToDirectory,
            ..Default::default()
        }
    }

    pub fn dir_to_file(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DirectoryToFile,
            ..Default::default()
        }
    }

    pub fn import_to_import(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::ImportedSymbolToImportedSymbol,
            ..Default::default()
        }
    }

    pub fn import_to_definition(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::ImportedSymbolToDefinition,
            ..Default::default()
        }
    }

    pub fn import_to_file(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::ImportedSymbolToFile,
            ..Default::default()
        }
    }

    pub fn definition_to_definition(
        from_path: ArcIntern<String>,
        to_path: ArcIntern<String>,
    ) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DefinitionToDefinition,
            ..Default::default()
        }
    }

    pub fn file_to_definition(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::FileToDefinition,
            ..Default::default()
        }
    }

    pub fn file_to_imported_symbol(
        from_path: ArcIntern<String>,
        to_path: ArcIntern<String>,
    ) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::FileToImportedSymbol,
            ..Default::default()
        }
    }

    pub fn definition_to_imported_symbol(
        from_path: ArcIntern<String>,
        to_path: ArcIntern<String>,
    ) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DefinitionToImportedSymbol,
            ..Default::default()
        }
    }

    pub fn definition_to_endpoint(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DefinitionToEndpoint,
            ..Default::default()
        }
    }

    pub fn file_to_endpoint(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::FileToEndpoint,
            ..Default::default()
        }
    }

    pub fn definition_to_service_call(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::DefinitionToServiceCall,
            ..Default::default()
        }
    }

    pub fn file_to_service_call(from_path: ArcIntern<String>, to_path: ArcIntern<String>) -> Self {
        Self {
            source_path: Some(from_path),
            target_path: Some(to_path),
            kind: RelationshipKind::FileToServiceCall,
            ..Default::default()
        }
    }
}

pub fn rels_by_kind(
    relationships: &[ConsolidatedRelationship],
    kind: RelationshipKind,
) -> impl Iterator<Item = ConsolidatedRelationship> + '_ {
    relationships
        .iter()
        .filter(move |rel| rel.kind == kind)
        .cloned()
}

impl NodeFieldAccess for ConsolidatedRelationship {
    fn get_u32_field(&self, field_name: &str) -> Option<u32> {
        match field_name {
            "source_id" => self.source_id,
            "target_id" => self.target_id,
            _ => None,
        }
    }

    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "type" => Some(self.relationship_type.as_string()),
            "source_path" => self.source_path.as_ref().map(|p| p.as_ref().clone()),
            "target_path" => self.target_path.as_ref().map(|p| p.as_ref().clone()),
            _ => None,
        }
    }

    fn get_i64_field(&self, field_name: &str) -> Option<i64> {
        match field_name {
            "source_start_byte" => Some(self.source_range.byte_offset.0 as i64),
            "source_end_byte" => Some(self.source_range.byte_offset.1 as i64),
            _ => None,
        }
    }

    fn get_i32_field(&self, field_name: &str) -> Option<i32> {
        match field_name {
            "source_start_line" => Some(self.source_range.start.line as i32),
            "source_end_line" => Some(self.source_range.end.line as i32),
            "source_start_col" => Some(self.source_range.start.column as i32),
            "source_end_col" => Some(self.source_range.end.column as i32),
            _ => None,
        }
    }
}

/// Structured graph data ready for writing to Parquet files
#[derive(Debug)]
pub struct GraphData {
    /// Directory nodes to be written to directories.parquet
    pub directory_nodes: Vec<DirectoryNode>,
    /// File nodes to be written to files.parquet
    pub file_nodes: Vec<FileNode>,
    /// Definition nodes to be written to definitions.parquet
    pub definition_nodes: Vec<DefinitionNode>,
    /// Imported symbol nodes to be written to imported_symbols.parquet
    pub imported_symbol_nodes: Vec<ImportedSymbolNode>,
    /// Endpoint nodes to be written to endpoints.parquet
    pub endpoint_nodes: Vec<EndpointNode>,
    /// Service call nodes to be written to service_calls.parquet
    pub service_call_nodes: Vec<ServiceCallNode>,
    /// Relationships to be written to parquet files based on their kind
    pub relationships: Vec<ConsolidatedRelationship>,
}

/// Represents a directory node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryNode {
    /// Relative path from repository root
    pub path: String,
    /// Absolute path on filesystem
    pub absolute_path: String,
    /// Repository name
    pub repository_name: String,
    /// Directory name (last component of path)
    pub name: String,
}

/// Implementation of NodeFieldAccess for DirectoryNode
impl NodeFieldAccess for DirectoryNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "path" => Some(self.path.clone()),
            "absolute_path" => Some(self.absolute_path.clone()),
            "repository_name" => Some(self.repository_name.clone()),
            "name" => Some(self.name.clone()),
            _ => None,
        }
    }

    fn get_i32_field(&self, _field_name: &str) -> Option<i32> {
        None // DirectoryNode has no i32 fields
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}

/// Represents a file node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Relative path from repository root
    pub path: String,
    /// Absolute path on filesystem
    pub absolute_path: String,
    /// Programming language detected
    pub language: String,
    /// Repository name
    pub repository_name: String,
    /// File extension
    pub extension: String,
    /// File name (last component of path)
    pub name: String,
}

/// Implementation of NodeFieldAccess for FileNode
impl NodeFieldAccess for FileNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "path" => Some(self.path.clone()),
            "absolute_path" => Some(self.absolute_path.clone()),
            "language" => Some(self.language.clone()),
            "repository_name" => Some(self.repository_name.clone()),
            "extension" => Some(self.extension.clone()),
            "name" => Some(self.name.clone()),
            _ => None,
        }
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}

/// Represents a language-specific definition type (e.g. class, module, method, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DefinitionType {
    Ruby(RubyDefinitionType),
    Python(PythonDefinitionType),
    Kotlin(KotlinDefinitionType),
    Java(JavaDefinitionType),
    CSharp(CSharpDefinitionType),
    TypeScript(TypeScriptDefinitionType),
    Rust(RustDefinitionType),
    Unsupported(),
}

impl DefinitionType {
    pub fn as_str(&self) -> &str {
        match self {
            DefinitionType::Ruby(ruby_type) => ruby_type.as_str(),
            DefinitionType::Python(python_type) => python_type.as_str(),
            DefinitionType::Kotlin(kotlin_type) => kotlin_type.as_str(),
            DefinitionType::Java(java_type) => java_type.as_str(),
            DefinitionType::CSharp(csharp_type) => csharp_type.as_str(),
            DefinitionType::TypeScript(typescript_type) => typescript_type.as_str(),
            DefinitionType::Rust(rust_type) => rust_type.as_str(),
            DefinitionType::Unsupported() => "unsupported",
        }
    }
}

/// Represents a language-specific FQN type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FqnType {
    Ruby(RubyFqn),
    Python(PythonFqn),
    Kotlin(KotlinFqn),
    Java(JavaFqn),
    CSharp(CSharpFqn),
    TypeScript(TypeScriptFqn),
    Rust(RustFqn),
}

impl std::fmt::Display for FqnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FqnType::Ruby(ruby_type) => write!(f, "{}", ruby_fqn_to_string(ruby_type)),
            FqnType::Python(python_type) => write!(f, "{}", python_fqn_to_string(python_type)),
            FqnType::Kotlin(kotlin_type) => write!(f, "{}", kotlin_fqn_to_string(kotlin_type)),
            FqnType::Java(java_type) => write!(f, "{}", java_fqn_to_string(java_type)),
            FqnType::CSharp(csharp_type) => write!(
                f,
                "{}",
                csharp_type
                    .iter()
                    .map(|part| part.node_name.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
            FqnType::TypeScript(typescript_type) => {
                write!(f, "{}", typescript_fqn_to_string(typescript_type))
            }
            FqnType::Rust(rust_type) => write!(f, "{}", rust_fqn_to_string(rust_type)),
        }
    }
}

impl FqnType {
    #[inline(always)]
    pub fn name(&self) -> &str {
        match self {
            FqnType::Ruby(ruby_type) => ruby_type.parts.last().unwrap().node_name(),
            FqnType::Python(python_type) => python_type.parts.last().unwrap().node_name(),
            FqnType::Kotlin(kotlin_type) => kotlin_type.last().unwrap().node_name(),
            FqnType::Java(java_type) => java_type.last().unwrap().node_name(),
            FqnType::CSharp(csharp_type) => csharp_type.last().unwrap().node_name(),
            FqnType::TypeScript(typescript_type) => typescript_type.last().unwrap().node_name(),
            FqnType::Rust(rust_type) => rust_type.parts.last().unwrap().node_name(),
        }
    }
}
/// Represents a definition node in the graph
#[derive(Debug, Clone)]
pub struct DefinitionNode {
    /// Fully qualified name (unique identifier)
    pub fqn: FqnType,
    /// Type of definition
    pub definition_type: DefinitionType,
    // Lines, cols, byte offsets
    pub range: Range,
    // File location of the definition
    pub file_path: ArcIntern<String>,
    /// Annotations as JSON string (for Java definitions)
    pub annotations_json: Option<String>,
}

impl HasRange for DefinitionNode {
    fn range(&self) -> Range {
        self.range
    }
}

impl DefinitionNode {
    /// Create a new DefinitionNode
    pub fn new(
        fqn: FqnType,
        definition_type: DefinitionType,
        range: Range,
        file_path: ArcIntern<String>,
    ) -> Self {
        Self {
            fqn,
            definition_type,
            range,
            file_path,
            annotations_json: None,
        }
    }

    /// Create a new DefinitionNode with annotations
    pub fn new_with_annotations(
        fqn: FqnType,
        definition_type: DefinitionType,
        range: Range,
        file_path: ArcIntern<String>,
        annotations_json: Option<String>,
    ) -> Self {
        Self {
            fqn,
            definition_type,
            range,
            file_path,
            annotations_json,
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        self.fqn.name()
    }
}

/// Implementation of NodeFieldAccess for DefinitionNode
impl NodeFieldAccess for DefinitionNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "fqn" => Some(self.fqn.to_string()),
            "name" => Some(self.name().to_string()),
            "definition_type" => Some(self.definition_type.as_str().to_string()),
            "primary_file_path" => Some(self.file_path.as_ref().clone()),
            "annotations_json" => self.annotations_json.clone(),
            _ => None,
        }
    }

    fn get_i32_field(&self, field_name: &str) -> Option<i32> {
        match field_name {
            "start_line" => Some(self.range.start.line as i32),
            "end_line" => Some(self.range.end.line as i32),
            "start_col" => Some(self.range.start.column as i32),
            "end_col" => Some(self.range.end.column as i32),
            "total_locations" => Some(1), // Default to 1 for single location
            _ => None,
        }
    }

    fn get_i64_field(&self, field_name: &str) -> Option<i64> {
        match field_name {
            "primary_start_byte" => Some(self.range.byte_offset.0 as i64),
            "primary_end_byte" => Some(self.range.byte_offset.1 as i64),
            _ => None,
        }
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}

/// Represents a single location where an imported symbol is found
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ImportedSymbolLocation {
    /// File path where this symbol was imported
    pub file_path: String,
    /// Start byte position in the file
    pub start_byte: i64,
    /// End byte position in the file  
    pub end_byte: i64,
    /// Start line number
    pub start_line: i32,
    /// End line number
    pub end_line: i32,
    /// Start column
    pub start_col: i32,
    /// End column
    pub end_col: i32,
}

impl ImportedSymbolLocation {
    pub fn range(&self) -> Range {
        let start_pos = Position::new(self.start_line as usize, self.start_col as usize);
        let end_pos = Position::new(self.end_line as usize, self.end_col as usize);
        Range::new(
            start_pos,
            end_pos,
            (self.start_byte as usize, self.end_byte as usize),
        )
    }
}

/// Represents a language-specific import type
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImportType {
    Java(JavaImportType),
    Kotlin(KotlinImportType),
    Python(PythonImportType),
    CSharp(CSharpImportType),
    TypeScript(TypeScriptImportType),
    Rust(RustImportType),
}

impl ImportType {
    pub fn as_str(&self) -> &str {
        match self {
            ImportType::Java(java_type) => java_type.as_str(),
            ImportType::Kotlin(kotlin_type) => kotlin_type.as_str(),
            ImportType::Python(python_type) => python_type.as_str(),
            ImportType::CSharp(csharp_type) => csharp_type.as_str(),
            ImportType::TypeScript(typescript_type) => typescript_type.as_str(),
            ImportType::Rust(rust_type) => rust_type.as_str(),
        }
    }
}

/// Represents an identifier associated with an imported symbol
#[derive(Debug, Clone)]
pub struct ImportIdentifier {
    /// Original name, e.g. "foo" in `from module import foo as bar`
    pub name: String,
    /// Alias, e.g. "bar" in `from module import foo as bar`
    pub alias: Option<String>,
}

/// Represents an imported symbol node in the graph
#[derive(Debug, Clone)]
pub struct ImportedSymbolNode {
    /// Language-specific type of import (regular, from, aliased, wildcard, etc.)
    pub import_type: ImportType,
    /// The import path as specified in the source code
    /// e.g., "./my_module", "react", "../utils"
    pub import_path: String,
    /// Information about the imported identifier(s)
    /// None for side-effect imports like `import "./styles.css"`
    pub identifier: Option<ImportIdentifier>,
    /// Location of the enclosing import statement
    pub location: ImportedSymbolLocation,
}

impl ImportedSymbolNode {
    /// Create a new ImportedSymbolNode
    pub fn new(
        import_type: ImportType,
        import_path: String,
        identifier: Option<ImportIdentifier>,
        location: ImportedSymbolLocation,
    ) -> Self {
        Self {
            import_type,
            import_path,
            identifier,
            location,
        }
    }
}

/// Implementation of NodeFieldAccess for ImportedSymbolNode
impl NodeFieldAccess for ImportedSymbolNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "import_type" => Some(self.import_type.as_str().to_string()),
            "import_path" => Some(self.import_path.clone()),
            "name" => self.identifier.as_ref().map(|id| id.name.clone()),
            "alias" => self.identifier.as_ref().and_then(|id| id.alias.clone()),
            "file_path" => Some(self.location.file_path.clone()),
            _ => None,
        }
    }

    fn get_i32_field(&self, field_name: &str) -> Option<i32> {
        match field_name {
            "start_line" => Some(self.location.start_line),
            "end_line" => Some(self.location.end_line),
            "start_col" => Some(self.location.start_col),
            "end_col" => Some(self.location.end_col),
            _ => None,
        }
    }

    fn get_i64_field(&self, field_name: &str) -> Option<i64> {
        match field_name {
            "start_byte" => Some(self.location.start_byte),
            "end_byte" => Some(self.location.end_byte),
            _ => None,
        }
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}

/// Optimized file tree structure for fast lookups
#[derive(Debug, Clone)]
pub struct OptimizedFileTree {
    /// File paths
    normalized_files: HashMap<String, String>, // Normalized file path -> Original file path
    /// Precomputed root directories
    root_dirs: HashSet<PathBuf>,
    /// Directory structure for efficient path operations
    dirs: HashSet<PathBuf>,
}

impl OptimizedFileTree {
    pub fn new<'a>(files: impl Iterator<Item = &'a String>) -> Self {
        let mut dirs = HashSet::new();
        let mut normalized_files = HashMap::new();

        // Precompute normalized files and directory structure
        for file_path in files {
            normalized_files.insert(file_path.to_lowercase(), file_path.clone());

            let path = Path::new(&file_path);
            if let Some(parent) = path.parent() {
                dirs.insert(parent.to_path_buf());
            }
        }

        // Precompute root directories
        let root_dirs = Self::compute_root_dirs(&normalized_files, &dirs);

        Self {
            normalized_files,
            root_dirs,
            dirs,
        }
    }

    fn compute_root_dirs(
        files: &HashMap<String, String>,
        dirs: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let mut root_dirs = HashSet::new();

        // Find the most common root directory (shortest path)
        if let Some(common_root) = dirs.iter().min_by_key(|p| p.as_os_str().len()) {
            root_dirs.insert(common_root.clone());
        }

        // Look for directories that might be package roots (contain __init__.py)
        for (file_path, norm_file_path) in files {
            if norm_file_path.ends_with("__init__.py") {
                let path = Path::new(file_path);
                if let Some(package_dir) = path.parent()
                    && let Some(package_parent) = package_dir.parent()
                {
                    root_dirs.insert(package_parent.to_path_buf());
                }
            }
        }

        root_dirs
    }

    /// Get the original file path if it exists (case-insensitive)
    pub fn get_denormalized_file(&self, norm_file_path: &str) -> Option<&String> {
        self.normalized_files.get(norm_file_path)
    }

    /// Get root directories
    pub fn get_root_dirs(&self) -> &HashSet<PathBuf> {
        &self.root_dirs
    }

    /// Get all directories
    pub fn get_dirs(&self) -> &HashSet<PathBuf> {
        &self.dirs
    }
}

/// Represents an API endpoint node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointNode {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub http_method: String,
    /// Endpoint path (e.g., /api/users/{id})
    pub path: String,
    /// Complete path with base (e.g., /api/v1/users/{id})
    pub full_path: String,
    /// Content-Type consumed (e.g., application/json)
    pub consumes: Option<String>,
    /// Content-Type produced (e.g., application/json)
    pub produces: Option<String>,
    /// Optional description
    pub description: Option<String>,
    /// Is endpoint deprecated?
    pub deprecated: bool,
    /// JSON array of path parameters
    pub path_params_json: Option<String>,
    /// JSON array of query parameters
    pub query_params_json: Option<String>,
    /// JSON schema of request body
    pub request_body_json: Option<String>,
    /// JSON schema of response body
    pub response_body_json: Option<String>,
    /// File where endpoint is defined
    pub file_path: String,
    /// Start line number
    pub start_line: i32,
    /// End line number
    pub end_line: i32,
}

impl EndpointNode {
    /// Create a new EndpointNode
    pub fn new(
        http_method: String,
        path: String,
        full_path: String,
        file_path: String,
        start_line: i32,
        end_line: i32,
    ) -> Self {
        Self {
            http_method,
            path,
            full_path,
            consumes: None,
            produces: None,
            description: None,
            deprecated: false,
            path_params_json: None,
            query_params_json: None,
            request_body_json: None,
            response_body_json: None,
            file_path,
            start_line,
            end_line,
        }
    }
}

/// Implementation of NodeFieldAccess for EndpointNode
impl NodeFieldAccess for EndpointNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "http_method" => Some(self.http_method.clone()),
            "path" => Some(self.path.clone()),
            "full_path" => Some(self.full_path.clone()),
            "consumes" => self.consumes.clone(),
            "produces" => self.produces.clone(),
            "description" => self.description.clone(),
            "path_params_json" => self.path_params_json.clone(),
            "query_params_json" => self.query_params_json.clone(),
            "request_body_json" => self.request_body_json.clone(),
            "response_body_json" => self.response_body_json.clone(),
            "file_path" => Some(self.file_path.clone()),
            _ => None,
        }
    }

    fn get_i32_field(&self, field_name: &str) -> Option<i32> {
        match field_name {
            "start_line" => Some(self.start_line),
            "end_line" => Some(self.end_line),
            _ => None,
        }
    }

    fn get_u8_field(&self, field_name: &str) -> Option<u8> {
        match field_name {
            "deprecated" => Some(if self.deprecated { 1 } else { 0 }),
            _ => None,
        }
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}

/// Represents an external service call (FeignClient, RestTemplate, WebClient, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCallNode {
    /// Type of HTTP client (FeignClient, RestTemplate, WebClient, HttpClient, OkHttp, Retrofit)
    pub service_type: String,
    /// Service or client name
    pub service_name: String,
    /// Base URL or service identifier
    pub service_url: String,
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub http_method: String,
    /// Endpoint path (e.g., /api/users/{id})
    pub path: String,
    /// Complete path with base (e.g., /api/v1/users/{id})
    pub full_path: String,
    /// Class name where service call is defined
    pub class_name: String,
    /// Fully qualified class name
    pub class_fqn: String,
    /// Method name
    pub method_name: String,
    /// Fully qualified method name
    pub method_fqn: String,
    /// File where service call is defined
    pub file_path: String,
    /// Start line number
    pub start_line: i32,
    /// End line number
    pub end_line: i32,
}

impl ServiceCallNode {
    /// Create a new ServiceCallNode
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service_type: String,
        service_name: String,
        service_url: String,
        http_method: String,
        path: String,
        full_path: String,
        class_name: String,
        class_fqn: String,
        method_name: String,
        method_fqn: String,
        file_path: String,
        start_line: i32,
        end_line: i32,
    ) -> Self {
        Self {
            service_type,
            service_name,
            service_url,
            http_method,
            path,
            full_path,
            class_name,
            class_fqn,
            method_name,
            method_fqn,
            file_path,
            start_line,
            end_line,
        }
    }
}

/// Implementation of NodeFieldAccess for ServiceCallNode
impl NodeFieldAccess for ServiceCallNode {
    fn get_string_field(&self, field_name: &str) -> Option<String> {
        match field_name {
            "service_type" => Some(self.service_type.clone()),
            "service_name" => Some(self.service_name.clone()),
            "service_url" => Some(self.service_url.clone()),
            "http_method" => Some(self.http_method.clone()),
            "path" => Some(self.path.clone()),
            "full_path" => Some(self.full_path.clone()),
            "class_name" => Some(self.class_name.clone()),
            "class_fqn" => Some(self.class_fqn.clone()),
            "method_name" => Some(self.method_name.clone()),
            "method_fqn" => Some(self.method_fqn.clone()),
            "file_path" => Some(self.file_path.clone()),
            _ => None,
        }
    }

    fn get_i32_field(&self, field_name: &str) -> Option<i32> {
        match field_name {
            "start_line" => Some(self.start_line),
            "end_line" => Some(self.end_line),
            _ => None,
        }
    }

    fn get_u8_field(&self, _field_name: &str) -> Option<u8> {
        None
    }

    fn get_id_field<F>(&self, field_name: &str, id_callback: F) -> Option<u32>
    where
        F: FnOnce(&Self) -> u32,
    {
        match field_name {
            "id" => Some(id_callback(self)),
            _ => None,
        }
    }
}
