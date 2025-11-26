use std::collections::HashMap;

use database::graph::RelationshipType;
use parser_core::java::types::{
    JavaDefinitionInfo, JavaDefinitionType, JavaFqn, JavaFqnPartType, JavaImportedSymbolInfo,
};

use crate::{
    analysis::{
        languages::java::{
            constant_resolver::ConstantResolver,
            endpoint_extractor::{EndpointExtractor, ExtractionContext},
            expression_resolver::ExpressionResolver,
            service_call_extractor::ServiceCallExtractor,
            utils::full_import_path
        },
        types::{
            ConsolidatedRelationship, DefinitionNode, DefinitionType, EndpointNode, FqnType, ImportIdentifier,
            ImportType, ImportedSymbolLocation, ImportedSymbolNode, ServiceCallNode,
        },
    },
    parsing::processor::{FileProcessingResult, References},
};
use internment::ArcIntern;
use parser_core::utils::{Position, Range};

#[derive(Default)]
pub struct JavaAnalyzer {
    expression_resolver: ExpressionResolver,
    constant_resolver: ConstantResolver,
    endpoint_nodes: Vec<EndpointNode>,
    service_call_nodes: Vec<ServiceCallNode>,
}

impl JavaAnalyzer {
    pub fn new() -> Self {
        Self {
            expression_resolver: ExpressionResolver::new(),
            constant_resolver: ConstantResolver::new(),
            endpoint_nodes: Vec::new(),
            service_call_nodes: Vec::new(),
        }
    }

    pub fn process_definitions(
        &mut self,
        file_result: &FileProcessingResult,
        relative_file_path: &str,
        definition_map: &mut HashMap<(String, String), (DefinitionNode, FqnType)>,
        relationships: &mut Vec<ConsolidatedRelationship>,
    ) {
        // Collect all Java definitions for constant resolution
        // Need to collect owned values, not references
        let all_definitions: Vec<JavaDefinitionInfo> = if let Some(defs) = file_result.definitions.iter_java() {
            defs.cloned().collect()
        } else {
            Vec::new()
        };

        // Create extraction context for constant resolution
        let extraction_context = ExtractionContext {
            constant_resolver: &self.constant_resolver,
            all_definitions: &all_definitions,
        };

        if let Some(defs) = file_result.definitions.iter_java() {
            for definition in defs {
                if matches!(definition.definition_type, JavaDefinitionType::Package) {
                    self.expression_resolver
                        .add_file(definition.name.clone(), relative_file_path.to_string());
                    continue;
                }

                let fqn = FqnType::Java(definition.fqn.clone());
                let path = ArcIntern::new(relative_file_path.to_string());

                // Extract annotations if available
                let annotations_json = if let Some(metadata) = &definition.metadata {
                    log::debug!(
                        "[ANALYZER] Definition '{}' has metadata, checking for annotations",
                        definition.name
                    );
                    match metadata {
                        parser_core::java::types::JavaDefinitionMetadata::Class { annotations, .. } |
                        parser_core::java::types::JavaDefinitionMetadata::Method { annotations, .. } |
                        parser_core::java::types::JavaDefinitionMetadata::Constructor { annotations, .. } |
                        parser_core::java::types::JavaDefinitionMetadata::Field { annotations, .. } |
                        parser_core::java::types::JavaDefinitionMetadata::Parameter { annotations, .. } |
                        parser_core::java::types::JavaDefinitionMetadata::LocalVariable { annotations, .. } => {
                            log::debug!(
                                "[ANALYZER] Definition '{}' has {} annotations in metadata",
                                definition.name,
                                annotations.len()
                            );
                            if !annotations.is_empty() {
                                match serde_json::to_string(annotations) {
                                    Ok(json) => {
                                        log::info!(
                                            "[ANALYZER] Serialized {} annotations for '{}': {}",
                                            annotations.len(),
                                            definition.name,
                                            &json
                                        );
                                        Some(json)
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "[ANALYZER] Failed to serialize annotations for '{}': {}",
                                            definition.name,
                                            e
                                        );
                                        None
                                    }
                                }
                            } else {
                                log::debug!(
                                    "[ANALYZER] Definition '{}' has empty annotations",
                                    definition.name
                                );
                                None
                            }
                        }
                    }
                } else {
                    log::debug!(
                        "[ANALYZER] Definition '{}' has no metadata",
                        definition.name
                    );
                    None
                };

                log::debug!(
                    "[ANALYZER] Creating DefinitionNode for '{}' with annotations_json={:?}",
                    definition.name,
                    annotations_json.as_ref().map(|s| s.len())
                );

                let definition_node = DefinitionNode::new_with_annotations(
                    fqn.clone(),
                    DefinitionType::Java(definition.definition_type),
                    definition.range,
                    path.clone(),
                    annotations_json.clone(),
                );

                // Extract endpoints from method annotations if this is a method
                if definition.definition_type == JavaDefinitionType::Method {
                    if let Some(ref annot_json) = annotations_json {
                        // Get class-level annotations for base path extraction
                        let class_annotations_json = self.get_class_annotations(&definition.fqn, file_result);

                        // Use context-aware extraction to resolve constant references
                        let endpoints = EndpointExtractor::extract_endpoints_with_context(
                            annot_json,
                            &fqn.to_string(),
                            relative_file_path,
                            definition.range.start.line as i32,
                            definition.range.end.line as i32,
                            class_annotations_json.as_deref(),
                            &extraction_context,
                        );

                        // Store endpoints and create relationships
                        for endpoint in endpoints {
                            // Create relationship from definition to endpoint
                            let mut def_to_endpoint = ConsolidatedRelationship::definition_to_endpoint(
                                path.clone(),
                                path.clone(),
                            );
                            def_to_endpoint.relationship_type = RelationshipType::DefinitionDefinesEndpoint;
                            def_to_endpoint.source_range = ArcIntern::new(definition.range);
                            def_to_endpoint.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: endpoint.start_line as usize, column: 0 },
                                end: Position { line: endpoint.end_line as usize, column: 0 },
                            });
                            relationships.push(def_to_endpoint);

                            // Create relationship from file to endpoint
                            let mut file_to_endpoint = ConsolidatedRelationship::file_to_endpoint(
                                path.clone(),
                                path.clone(),
                            );
                            file_to_endpoint.relationship_type = RelationshipType::FileHasEndpoint;
                            file_to_endpoint.source_range = ArcIntern::new(Range::empty());
                            file_to_endpoint.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: endpoint.start_line as usize, column: 0 },
                                end: Position { line: endpoint.end_line as usize, column: 0 },
                            });
                            relationships.push(file_to_endpoint);

                            self.endpoint_nodes.push(endpoint);
                        }

                        // Extract service calls from FeignClient method annotations
                        let class_annotations_json = self.get_class_annotations(&definition.fqn, file_result);
                        let (class_fqn_str, class_name) = self.get_class_info(&definition.fqn);

                        let service_calls = ServiceCallExtractor::extract_service_call_from_method(
                            annot_json,
                            &fqn.to_string(),
                            &definition.name,
                            &class_fqn_str,
                            &class_name,
                            class_annotations_json.as_deref(),
                            relative_file_path,
                            definition.range.start.line as i32,
                            definition.range.end.line as i32,
                        );

                        // Store service calls and create relationships
                        for service_call in service_calls {
                            // Create relationship from definition to service call
                            let mut def_to_service_call = ConsolidatedRelationship::definition_to_service_call(
                                path.clone(),
                                path.clone(),
                            );
                            def_to_service_call.relationship_type = RelationshipType::DefinitionCallsService;
                            def_to_service_call.source_range = ArcIntern::new(definition.range);
                            def_to_service_call.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: service_call.start_line as usize, column: 0 },
                                end: Position { line: service_call.end_line as usize, column: 0 },
                            });
                            relationships.push(def_to_service_call);

                            // Create relationship from file to service call
                            let mut file_to_service_call = ConsolidatedRelationship::file_to_service_call(
                                path.clone(),
                                path.clone(),
                            );
                            file_to_service_call.relationship_type = RelationshipType::FileHasServiceCall;
                            file_to_service_call.source_range = ArcIntern::new(Range::empty());
                            file_to_service_call.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: service_call.start_line as usize, column: 0 },
                                end: Position { line: service_call.end_line as usize, column: 0 },
                            });
                            relationships.push(file_to_service_call);

                            self.service_call_nodes.push(service_call);
                        }
                    }
                }

                // Extract service calls from class/interface annotations (for @FeignClient)
                if matches!(
                    definition.definition_type,
                    JavaDefinitionType::Class | JavaDefinitionType::Interface
                ) {
                    if let Some(ref annot_json) = annotations_json {
                        let service_calls = ServiceCallExtractor::extract_service_calls_from_annotations(
                            annot_json,
                            &fqn.to_string(),
                            &definition.name,
                            relative_file_path,
                            definition.range.start.line as i32,
                            definition.range.end.line as i32,
                        );

                        // Store service calls and create relationships
                        for service_call in service_calls {
                            // Create relationship from definition to service call
                            let mut def_to_service_call = ConsolidatedRelationship::definition_to_service_call(
                                path.clone(),
                                path.clone(),
                            );
                            def_to_service_call.relationship_type = RelationshipType::DefinitionCallsService;
                            def_to_service_call.source_range = ArcIntern::new(definition.range);
                            def_to_service_call.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: service_call.start_line as usize, column: 0 },
                                end: Position { line: service_call.end_line as usize, column: 0 },
                            });
                            relationships.push(def_to_service_call);

                            // Create relationship from file to service call
                            let mut file_to_service_call = ConsolidatedRelationship::file_to_service_call(
                                path.clone(),
                                path.clone(),
                            );
                            file_to_service_call.relationship_type = RelationshipType::FileHasServiceCall;
                            file_to_service_call.source_range = ArcIntern::new(Range::empty());
                            file_to_service_call.target_range = ArcIntern::new(Range {
                                byte_offset: (0, 0),
                                start: Position { line: service_call.start_line as usize, column: 0 },
                                end: Position { line: service_call.end_line as usize, column: 0 },
                            });
                            relationships.push(file_to_service_call);

                            self.service_call_nodes.push(service_call);
                        }
                    }
                }

                self.expression_resolver.add_definition(
                    relative_file_path.to_string(),
                    definition.clone(),
                    definition_node.clone(),
                );

                // We don't want to index local variables, parameters, or fields
                if definition.definition_type == JavaDefinitionType::LocalVariable
                    || definition.definition_type == JavaDefinitionType::Parameter
                    || definition.definition_type == JavaDefinitionType::Field
                {
                    continue;
                }

                // Only add file definition relationship for top-level definitions
                if self.is_top_level_definition(&definition.fqn) {
                    let mut relationship =
                        ConsolidatedRelationship::file_to_definition(path.clone(), path.clone());
                    relationship.relationship_type = RelationshipType::FileDefines;
                    relationship.source_range = ArcIntern::new(Range::empty());
                    relationship.target_range = ArcIntern::new(definition.range);
                    relationships.push(relationship);
                }

                definition_map.insert(
                    (fqn.to_string(), relative_file_path.to_string()),
                    (definition_node, fqn),
                );
            }
        }
    }

    /// Process imported symbols from a file result and update the import map
    pub fn process_imports(
        &mut self,
        file_result: &FileProcessingResult,
        relative_file_path: &str,
        imported_symbol_map: &mut HashMap<(String, String), Vec<ImportedSymbolNode>>,
        relationships: &mut Vec<ConsolidatedRelationship>,
    ) {
        if let Some(imported_symbols) = &file_result.imported_symbols
            && let Some(imports) = imported_symbols.iter_java()
        {
            for imported_symbol in imports {
                let location =
                    self.create_imported_symbol_location(imported_symbol, relative_file_path);
                let identifier = self.create_imported_symbol_identifier(imported_symbol);

                let imported_symbol_node = ImportedSymbolNode::new(
                    ImportType::Java(imported_symbol.import_type),
                    imported_symbol.import_path.clone(),
                    identifier,
                    location.clone(),
                );

                let (_, full_import_path) = full_import_path(&imported_symbol_node);
                imported_symbol_map.insert(
                    (full_import_path, relative_file_path.to_string()),
                    vec![imported_symbol_node.clone()],
                );

                let mut relationship = ConsolidatedRelationship::file_to_imported_symbol(
                    ArcIntern::new(relative_file_path.to_string()),
                    ArcIntern::new(location.file_path.clone()),
                );
                relationship.relationship_type = RelationshipType::FileImports;
                relationship.source_range = ArcIntern::new(Range::empty());
                relationship.target_range = ArcIntern::new(location.range());
                relationships.push(relationship);

                self.expression_resolver
                    .add_import(relative_file_path.to_string(), &imported_symbol_node);
            }
        }
    }

    /// Process Java references (calls and creations) and create definition relationships
    pub fn process_references(
        &mut self,
        references: &References,
        file_path: &str,
        relationships: &mut Vec<ConsolidatedRelationship>,
    ) {
        self.expression_resolver
            .resolve_references(file_path, references, relationships);
    }

    /// Create definition-to-definition relationships using definitions map
    pub fn add_definition_relationships(
        &self,
        definition_map: &HashMap<(String, String), (DefinitionNode, FqnType)>,
        relationships: &mut Vec<ConsolidatedRelationship>,
    ) {
        for ((_child_fqn_string, child_file_path), (child_def, child_fqn)) in definition_map {
            if let Some(parent_fqn_string) = self.get_parent_fqn_string(child_fqn)
                && let Some((parent_def, _)) =
                    definition_map.get(&(parent_fqn_string.clone(), child_file_path.to_string()))
                && let Some(relationship_type) = self.get_definition_relationship_type(
                    &parent_def.definition_type,
                    &child_def.definition_type,
                )
            {
                // FIXME: https://gitlab.com/gitlab-org/rust/knowledge-graph/-/issues/177
                // Definition Heirarchy relationships should have their own struct
                let mut relationship = ConsolidatedRelationship::definition_to_definition(
                    parent_def.file_path.clone(),
                    child_def.file_path.clone(),
                );
                relationship.relationship_type = relationship_type;
                relationship.source_range = ArcIntern::new(parent_def.range);
                relationship.target_range = ArcIntern::new(child_def.range);
                relationships.push(relationship);
            }
        }
    }

    fn get_parent_fqn_string(&self, fqn: &FqnType) -> Option<String> {
        match fqn {
            FqnType::Java(java_fqn) => {
                if java_fqn.len() <= 1 {
                    return None;
                }

                let parent_parts: Vec<String> = java_fqn[..java_fqn.len() - 1]
                    .iter()
                    .map(|part| part.node_name.clone())
                    .collect();

                if parent_parts.is_empty() {
                    None
                } else {
                    Some(parent_parts.join("."))
                }
            }
            _ => None,
        }
    }

    fn get_definition_relationship_type(
        &self,
        parent_type: &DefinitionType,
        child_type: &DefinitionType,
    ) -> Option<RelationshipType> {
        use JavaDefinitionType::*;

        let parent_type = self.simplify_definition_type(parent_type)?;
        let child_type = self.simplify_definition_type(child_type)?;

        match (parent_type, child_type) {
            // Class relationships
            (DefinitionType::Java(Class), DefinitionType::Java(Class)) => {
                Some(RelationshipType::ClassToClass)
            }
            (DefinitionType::Java(Class), DefinitionType::Java(Interface)) => {
                Some(RelationshipType::ClassToInterface)
            }
            (DefinitionType::Java(Class), DefinitionType::Java(EnumConstant)) => {
                Some(RelationshipType::ClassToEnumEntry)
            }
            (DefinitionType::Java(Class), DefinitionType::Java(Method)) => {
                Some(RelationshipType::ClassToMethod)
            }
            (DefinitionType::Java(Class), DefinitionType::Java(Lambda)) => {
                Some(RelationshipType::ClassToLambda)
            }
            // Interface relationships
            (DefinitionType::Java(Interface), DefinitionType::Java(Interface)) => {
                Some(RelationshipType::InterfaceToInterface)
            }
            (DefinitionType::Java(Interface), DefinitionType::Java(Class)) => {
                Some(RelationshipType::InterfaceToClass)
            }
            (DefinitionType::Java(Interface), DefinitionType::Java(Method)) => {
                Some(RelationshipType::InterfaceToMethod)
            }
            (DefinitionType::Java(Interface), DefinitionType::Java(Lambda)) => {
                Some(RelationshipType::InterfaceToLambda)
            }
            // Method relationships
            (DefinitionType::Java(Method), DefinitionType::Java(Method)) => {
                Some(RelationshipType::MethodToMethod)
            }
            (DefinitionType::Java(Method), DefinitionType::Java(Class)) => {
                Some(RelationshipType::MethodToClass)
            }
            (DefinitionType::Java(Method), DefinitionType::Java(Interface)) => {
                Some(RelationshipType::MethodToInterface)
            }
            (DefinitionType::Java(Method), DefinitionType::Java(Lambda)) => {
                Some(RelationshipType::MethodToLambda)
            }
            // Lambda relationships
            (DefinitionType::Java(Lambda), DefinitionType::Java(Lambda)) => {
                Some(RelationshipType::LambdaToLambda)
            }
            (DefinitionType::Java(Lambda), DefinitionType::Java(Class)) => {
                Some(RelationshipType::LambdaToClass)
            }
            (DefinitionType::Java(Lambda), DefinitionType::Java(Method)) => {
                Some(RelationshipType::LambdaToMethod)
            }
            (DefinitionType::Java(Lambda), DefinitionType::Java(Interface)) => {
                Some(RelationshipType::LambdaToInterface)
            }
            _ => None,
        }
    }

    fn simplify_definition_type(&self, definition_type: &DefinitionType) -> Option<DefinitionType> {
        use JavaDefinitionType::*;

        match definition_type {
            DefinitionType::Java(Class) => Some(DefinitionType::Java(Class)),
            DefinitionType::Java(Enum) => Some(DefinitionType::Java(Class)),
            DefinitionType::Java(AnnotationDeclaration) => Some(DefinitionType::Java(Class)),
            DefinitionType::Java(Record) => Some(DefinitionType::Java(Class)),
            DefinitionType::Java(Interface) => Some(DefinitionType::Java(Interface)),
            DefinitionType::Java(EnumConstant) => Some(DefinitionType::Java(EnumConstant)),
            DefinitionType::Java(Method) => Some(DefinitionType::Java(Method)),
            DefinitionType::Java(Constructor) => Some(DefinitionType::Java(Method)),
            DefinitionType::Java(Lambda) => Some(DefinitionType::Java(Lambda)),
            _ => None,
        }
    }

    fn is_top_level_definition(&self, fqn: &JavaFqn) -> bool {
        fqn.len() == 1 || (fqn.len() == 2 && fqn[0].node_type == JavaFqnPartType::Package)
    }

    /// Create an imported symbol location from an imported symbol info
    fn create_imported_symbol_location(
        &self,
        imported_symbol: &JavaImportedSymbolInfo,
        file_path: &str,
    ) -> ImportedSymbolLocation {
        ImportedSymbolLocation {
            file_path: file_path.to_string(),
            start_byte: imported_symbol.range.byte_offset.0 as i64,
            end_byte: imported_symbol.range.byte_offset.1 as i64,
            start_line: imported_symbol.range.start.line as i32,
            end_line: imported_symbol.range.end.line as i32,
            start_col: imported_symbol.range.start.column as i32,
            end_col: imported_symbol.range.end.column as i32,
        }
    }

    fn create_imported_symbol_identifier(
        &self,
        imported_symbol: &JavaImportedSymbolInfo,
    ) -> Option<ImportIdentifier> {
        if imported_symbol.identifier.is_some() {
            return Some(ImportIdentifier {
                name: imported_symbol.identifier.as_ref().unwrap().name.clone(),
                alias: imported_symbol.identifier.as_ref().unwrap().alias.clone(),
            });
        }

        None
    }

    /// Get class-level annotations from the parent class of a method
    fn get_class_annotations(
        &self,
        method_fqn: &JavaFqn,
        file_result: &FileProcessingResult,
    ) -> Option<String> {
        // Get the parent class FQN (remove the last part which is the method name)
        if method_fqn.len() < 2 {
            return None;
        }

        let class_fqn_parts: Vec<String> = method_fqn[..method_fqn.len() - 1]
            .iter()
            .map(|part| part.node_name.clone())
            .collect();
        let class_fqn_str = class_fqn_parts.join(".");

        // Find the class definition in file_result
        if let Some(defs) = file_result.definitions.iter_java() {
            for definition in defs {
                let def_fqn_str = definition
                    .fqn
                    .iter()
                    .map(|part| part.node_name.clone())
                    .collect::<Vec<_>>()
                    .join(".");

                if def_fqn_str == class_fqn_str {
                    // Found the class, extract its annotations
                    if let Some(metadata) = &definition.metadata {
                        match metadata {
                            parser_core::java::types::JavaDefinitionMetadata::Class { annotations, .. } => {
                                if !annotations.is_empty() {
                                    match serde_json::to_string(annotations) {
                                        Ok(json) => {
                                            log::debug!(
                                                "[ANALYZER] Found class annotations for '{}': {}",
                                                class_fqn_str,
                                                &json
                                            );
                                            return Some(json);
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "[ANALYZER] Failed to serialize class annotations for '{}': {}",
                                                class_fqn_str,
                                                e
                                            );
                                            return None;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        None
    }

    /// Get collected endpoint nodes and clear the internal collection
    pub fn take_endpoints(&mut self) -> Vec<EndpointNode> {
        std::mem::take(&mut self.endpoint_nodes)
    }

    /// Get collected service call nodes and clear the internal collection
    pub fn take_service_calls(&mut self) -> Vec<ServiceCallNode> {
        std::mem::take(&mut self.service_call_nodes)
    }

    /// Extract class FQN and name from a method FQN
    fn get_class_info(&self, method_fqn: &JavaFqn) -> (String, String) {
        let parts: Vec<String> = method_fqn
            .iter()
            .filter(|part| part.node_type != JavaFqnPartType::Package)
            .map(|part| part.node_name.to_string())
            .collect();

        if parts.len() >= 2 {
            // Last part is method, second-to-last is class
            let class_name = parts[parts.len() - 2].clone();
            let class_fqn = parts[..parts.len() - 1].join(".");
            (class_fqn, class_name)
        } else if !parts.is_empty() {
            // Only one part - might be a top-level method (unlikely but handle it)
            (parts[0].clone(), parts[0].clone())
        } else {
            (String::new(), String::new())
        }
    }
}
