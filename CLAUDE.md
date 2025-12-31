# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a monorepo containing two related Rust projects:

1. **gitlab-code-parser**: A static code analysis library that parses source code using tree-sitter and extracts structured entities (classes, functions, imports, etc.) and their relationships.

2. **knowledge-graph**: A framework that uses gitlab-code-parser to build a queryable graph database of code repositories, enabling advanced code understanding and AI features.

The knowledge-graph depends on gitlab-code-parser (via path dependency: `../gitlab-code-parser/crates/parser-core`).

## Architecture

### gitlab-code-parser

**Crates:**
- `parser-core`: Core parsing logic using tree-sitter and ast-grep
- `chunker`: Code chunking functionality
- `parser-c-bindings`: C FFI bindings
- `cli`: Command-line interface for parsing
- `testing`: Test utilities

**Key characteristics:**
- Stateless API: give it file path + contents, get structured entities & relationships
- Multi-runtime bindings: Rust • Node.js (napi-rs) • Go (cgo FFI) • WASM • Ruby (FFI)
- Supports multiple languages: TypeScript, JavaScript, Python, Ruby, Java, Kotlin, C#

### knowledge-graph

**Crates:**
- `gkg`: Main CLI that coordinates all components (entry point: [crates/gkg/src/main.rs](crates/gkg/src/main.rs))
- `indexer`: Core ETL pipeline that processes repositories and extracts structured data
- `database`: Persistence layer with abstraction over KuzuDB (embedded)
- `workspace-manager`: Tracks projects and their indexing status
- `http-server-desktop`: Web server (Axum) providing HTTP API and embedded frontend
- `http-server-deployed`: Server-side version of HTTP server
- `mcp`: Model-Context-Provider protocol for AI tool integration
- `event-bus`: Real-time event system for progress/status updates
- `monitoring`: Observability and metrics collection

**Packages (Node.js workspace):**
- `@gitlab-org/gkg` ([packages/gkg](packages/gkg)): TypeScript bindings generated from Rust via ts-rs
- `@gitlab-org/gkg-frontend` ([packages/frontend](packages/frontend)): Vue 3 + Vite UI
- `docs` ([packages/docs](packages/docs)): Astro + Starlight documentation

**Indexing Pipeline (ETL process):**
1. **Extract - Workspace Discovery**: Scan for Git repositories, register as projects
2. **Extract - File Discovery**: Enumerate files respecting `.gitignore`, filter by language
3. **Transform - Semantic Analysis**: Extract definitions/imports/relationships from AST
4. **Transform - Resolution**: Resolve references into in-memory graph
5. **Load - Graph Storage**: Write Parquet files and load into KuzuDB

**Threading Model:**
- Async I/O Pool: High-concurrency file reading (`worker_threads * 2`, min 8)
- CPU Worker Pool: Thread pool for parsing/analysis (sized to CPU cores or `--threads`)

**Data Storage:**
```
~/.gkg/workspace_folders/
  └── {workspace_hash}/         # SHA-256 of workspace path
      └── {project_hash}/       # SHA-256 of project path
          ├── database.kz          # KuzuDB graph database
          └── parquet_files/
              ├── directories.parquet
              ├── files.parquet
              ├── definitions.parquet
              ├── imported_symbols.parquet
              ├── endpoints.parquet
              └── relationships/
                  ├── DIRECTORY_RELATIONSHIPS.parquet
                  ├── FILE_RELATIONSHIPS.parquet
                  ├── DEFINITION_RELATIONSHIPS.parquet
                  ├── IMPORTED_SYMBOL_RELATIONSHIPS.parquet
                  └── ENDPOINT_RELATIONSHIPS.parquet
```

**Database Abstraction:**
- Unified `QueryingService` trait supports KuzuDB (embedded, file-based) 
- Query library provides predefined queries compatible with both backends
- See [docs/database-abstraction-layer.md](docs/database-abstraction-layer.md) for details

## Development Setup

**Prerequisites:**
- [mise](https://mise.jdx.dev/) for toolchain management (Rust stable, Node.js 22)
- Git LFS (for gitlab-code-parser Go bindings)
- Kuzu build prerequisites OR [dynamic linking setup](https://gitlab-org.gitlab.io/rust/knowledge-graph/contribute/build#speed-up-your-builds) (recommended)

**Setup:**
```bash
# Trust and install toolchains
mise trust && mise install

# Install git hooks (lefthook)
lefthook install
```

**Optional dependencies:**
- `nextest` for faster test execution
- `gitlab-xtasks` for newline verification
- `glab` for GitLab CI validation
- `cargo-watch` for auto-reload during development

## Common Commands

### gitlab-code-parser

```bash
cd gitlab-code-parser

# Build
cargo build --release

# Test
cargo test --all
mise run cargo-test      # same as above
mise run nextest         # faster with nextest

# Lint
mise run lint            # clippy with -D warnings
mise run lint-fix        # auto-fix linting issues

# Format
cargo fmt
mise run fix-all         # format + clippy fixes

# Benchmark/analyze a directory
cargo run --release --package cli -- analyze --directory ../gdk/gitlab --threads 16
cargo run --release --package cli -- analyze --directory ../gdk/gitlab --languages "javascript,typescript"

# Profile (macOS)
cargo build --release --package cli
samply record --rate 9999 target/release/cli analyze --directory ../gdk/gitlab

# Benchmark with hyperfine
hyperfine --warmup 3 --runs 10 'cargo run --release --package cli -- analyze --directory ../gdk/gitlab'

# View available tasks
mise tasks
```

### knowledge-graph

```bash
cd knowledge-graph

# Full build (requires frontend)
mise run bindings-gen      # generate TypeScript bindings
npm ci
npm run build --workspace=@gitlab-org/gkg-frontend
cargo build --release --bin gkg

# Build without frontend
cargo build --release --bin gkg --features no-frontend

# Install CLI globally
mise run build-cli-release  # builds and installs to ~/.cargo/bin

# Test
cargo test
mise run cargo-test         # regular cargo test
mise run nextest            # faster with nextest
mise run rust-test          # CI profile: bindings + nextest fallback

# Lint & format (matches CI)
mise run rust-fmt           # check formatting
mise run rust-fmt:fix       # auto-fix formatting
mise run rust-clippy        # lint with -D warnings
mise run rust-clippy:fix    # auto-fix linting
mise run newlines-check     # verify file endings
mise run newlines-check:fix # fix file endings

# Development servers
mise run server-dev         # debug mode at http://localhost:27495
mise run server-watch       # auto-reload on crate changes
mise run server-dev-reindexing       # with reindexing enabled
mise run server-watch-reindexing     # auto-reload + reindexing

# Server-side version
mise run server-deployed

# Frontend development
mise run frontend-dev       # Vite dev server
mise run frontend-build     # production build
mise run frontend-lint      # ESLint
mise run frontend-lint:fix  # auto-fix frontend issues
mise run frontend-format    # Prettier

# Documentation
mise run docs-dev           # Astro dev server
mise run docs-lint          # check docs
mise run docs-lint:fix      # auto-fix docs

# Observability stack (Docker)
mise run observability:up   # start Prometheus/Grafana/Mimir
mise run observability:down
mise run observability:logs
mise run observability:clean  # remove all data volumes

# View all available tasks
mise tasks
```

## Code Quality Standards

Both projects enforce strict standards:

- **Unused imports fail builds**: `unused_imports = "deny"` in workspace lints
- **Clippy warnings treated as errors**: `-D warnings`
- **Formatting required**: Must pass `rustfmt` checks
- **Newline verification**: All files must end with proper newlines

The pre-commit hooks (lefthook) automatically fix most issues. Run manually:
```bash
cargo check --workspace     # check unused imports
cargo clippy --workspace -- -D warnings
cargo fmt
```

## Testing

**Run specific test:**
```bash
# knowledge-graph
cargo test --package indexer test_name
cargo test --package gkg test_name

# gitlab-code-parser
cargo test --package parser-core test_name
```

**Run tests for a crate:**
```bash
cargo test --package indexer
cargo nextest run --package indexer  # faster
```

**Export TypeScript bindings (knowledge-graph only):**
```bash
cargo test export_bindings_  # generates files in packages/gkg/src/
```

## Debugging

**VS Code configurations** (knowledge-graph only, see [.vscode/launch.json](knowledge-graph/.vscode/launch.json)):

1. **Debug gkg index**: Debug the indexing command
   - Adjust workspace path in launch.json
   - Set breakpoints
   - Select "Debug gkg index" in Run and Debug

2. **Debug server**: Debug HTTP/MCP server
   - Ensure no gkg servers running
   - Set breakpoints
   - Select "Debug server" in Run and Debug
   - Start IDE with gkg integration to trigger breakpoints

Note: gkg ensures single-instance, so starting in debug mode captures all client connections.

## Build Optimization

**Speed up knowledge-graph builds with Kuzu dynamic linking:**

1. Download [Kuzu binaries](https://github.com/kuzudb/kuzu/releases) matching version in Cargo.toml
2. Set environment variables:
   ```bash
   export KUZU_SHARED=1
   export KUZU_INCLUDE_DIR=/path/to/kuzu/include
   export KUZU_LIBRARY_DIR=/path/to/kuzu/lib
   ```
3. Build skips complex Kuzu compilation step

**Prefer debug builds** for testing (release builds are slow unless benchmarking).

## Git Workflow

**Hooks (lefthook):**
- **Pre-commit**: Rust format/lint (auto-fix), newline fixes, frontend lint, docs lint, GitLab CI validation
- **Pre-push**: All pre-commit checks + full test suite + frontend build

**Skip hooks** (not recommended):
```bash
git commit --no-verify
git push --no-verify
```

## Key Implementation Notes

**When modifying Rust types that drive the API** (primarily in `knowledge-graph/crates/http-server-desktop`):
1. Rebuild TypeScript bindings: `mise run bindings-gen` or `cargo test export_bindings_ --features no-frontend`
2. Bindings are committed to git (in [packages/gkg/src](packages/gkg/src))

**The http-server-desktop embeds the frontend:**
- Without `--features no-frontend`, binary requires `packages/frontend/dist` to exist
- Use `--features no-frontend` to skip web UI entirely

**Database switching:**
- Default: KuzuDB (embedded, file-based)
- Both implement unified `QueryingService` trait

**Release profile configuration:**
- gitlab-code-parser: LTO "fat", opt-level 3, codegen-units 1
- knowledge-graph: LTO "thin", panic "abort", opt-level 3, strip symbols, codegen-units 1

## Java Endpoints Feature

The knowledge-graph supports extracting and viewing Java REST endpoints from Spring Boot applications as first-class graph nodes.

**Architecture:**
The endpoint extraction follows an ETL pipeline with endpoints stored as dedicated `EndpointNode` entities:

1. **Extract (Indexing Phase)**: During indexing, `endpoint_extractor.rs` analyzes Java annotations on methods to detect API endpoint mappings
2. **Transform (Endpoint Extraction)**: For each method with endpoint annotations:
   - Parses `@GetMapping`, `@PostMapping`, `@PutMapping`, `@DeleteMapping`, `@PatchMapping`, `@RequestMapping`
   - Extracts HTTP method, path, consumes/produces, deprecation status, and other metadata
   - Resolves class-level `@RequestMapping` to build full endpoint paths
   - Creates `EndpointNode` instances with all metadata
3. **Load (Graph Storage)**: EndpointNode entities are:
   - Written to `endpoints.parquet` via Arrow batch conversion
   - Loaded into KuzuDB as `EndpointNode` graph nodes
   - Linked to files and methods via `ENDPOINT_RELATIONSHIPS`

**Database Schema:**
EndpointNode is defined in [crates/database/src/schema/init.rs](knowledge-graph/crates/database/src/schema/init.rs) with 15 fields:
- `id` (uint32, primary key): Auto-generated unique identifier
- `http_method` (string): GET, POST, PUT, DELETE, PATCH, etc.
- `path` (string): Endpoint path from annotation (e.g., `/users/{id}`)
- `full_path` (string): Complete path with base (e.g., `/api/v1/users/{id}`)
- `consumes` (nullable string): Content-Type consumed (e.g., `application/json`)
- `produces` (nullable string): Content-Type produced (e.g., `application/json`)
- `description` (nullable string): Optional description from JavaDoc (future)
- `deprecated` (uint8): 0 or 1 indicating if endpoint is deprecated
- `path_params_json` (nullable string): JSON array of path parameters (future)
- `query_params_json` (nullable string): JSON array of query parameters (future)
- `request_body_json` (nullable string): JSON schema of request body (future)
- `response_body_json` (nullable string): JSON schema of response body (future)
- `file_path` (string): File where endpoint is defined
- `start_line` (int32): Line number where endpoint starts
- `end_line` (int32): Line number where endpoint ends

**Relationships:**
- `FileNode` → `ENDPOINT_RELATIONSHIPS` → `EndpointNode`: File contains endpoint
- `DefinitionNode` → `ENDPOINT_RELATIONSHIPS` → `EndpointNode`: Method defines endpoint

**Supported annotations:**
- `@GetMapping`, `@PostMapping`, `@PutMapping`, `@DeleteMapping`, `@PatchMapping`
- `@RequestMapping` (defaults to GET if method not specified)
- Class-level `@RequestMapping` for base path resolution

**Query Layer:**
Query functions in [crates/database/src/querying/library.rs](knowledge-graph/crates/database/src/querying/library.rs):
- `get_endpoints_query()`: Returns all EndpointNode entities
- `get_endpoints_by_file_query()`: Filter endpoints by file path
- `get_endpoints_by_method_query()`: Filter by HTTP method (GET, POST, etc.)
- `get_endpoint_with_definition_query()`: Get endpoint with its defining method

**Key files:**
- `crates/indexer/src/analysis/languages/java/endpoint_extractor.rs` - Endpoint extraction from annotations
- `crates/indexer/src/analysis/types.rs` - EndpointNode type definition and NodeFieldAccess impl
- `crates/database/src/schema/init.rs` - EndpointNode schema and ENDPOINT_RELATIONSHIPS
- `crates/database/src/querying/library.rs` - Query functions for endpoints
- `crates/http-server-desktop/src/endpoints/graph/graph_endpoints.rs` - API handler
- `packages/frontend/src/components/endpoints/JavaEndpointsView.vue` - UI component
- `packages/frontend/src/api/client.ts` - `fetchJavaEndpoints()` API client method

**API usage:**
```bash
# Fetch Java endpoints for a project
curl "http://localhost:27495/api/graph/java-endpoints/{workspace_folder_path}/{project_path}?limit=1000"
```

**Frontend access:**
Navigate to Project Explorer → Select a project → Click "Endpoints" tab to view all REST endpoints grouped by file path with:
- HTTP method badges (color-coded: GET=green, POST=blue, PUT=yellow, DELETE=red, PATCH=purple)
- Deprecated badge for `@Deprecated` endpoints
- Description (if available)
- Consumes/Produces content types
- Line range for quick navigation

**Data flow example:**
```
Java Source Code:
  @RestController
  @RequestMapping("/api/v1")
  class UserController {
    @GetMapping("/users/{id}")
    @Deprecated
    User getUser(@PathVariable Long id) { ... }
  }

↓ Indexing (endpoint_extractor.rs)

EndpointNode {
  http_method: "GET",
  path: "/users/{id}",
  full_path: "/api/v1/users/{id}",
  deprecated: 1,
  file_path: "src/main/java/.../ UserController.java",
  start_line: 25,
  end_line: 27,
  ...
}

↓ Graph Storage (Parquet → KuzuDB)

MATCH (ep:EndpointNode) WHERE ep.file_path CONTAINS "UserController"
RETURN ep.http_method, ep.full_path, ep.deprecated

↓ API Layer (graph_endpoints.rs)

GET /api/graph/java-endpoints/{workspace}/{project}
→ JavaEndpointsSuccessResponse with endpoints array

↓ Frontend (JavaEndpointsView.vue)

Grouped by file, displays:
  UserController.java
    [GET] /api/v1/users/{id} [Deprecated] :25-27
```

## Java Service Calls Feature

The knowledge-graph supports extracting and viewing external service calls from Java applications using various HTTP client libraries.

**How it works:**
1. During indexing, Java annotations and import statements are extracted and stored
2. Multiple queries detect different HTTP client patterns via import-based detection:
   - `get_java_service_calls_query()` - FeignClient interfaces with `@FeignClient` annotation
   - `get_java_rest_template_query()` - Classes importing `org.springframework.web.client.RestTemplate`
   - `get_java_web_client_query()` - Classes importing `org.springframework.web.reactive.function.client.WebClient`
   - `get_java_http_client_query()` - Classes importing Apache HttpClient (`org.apache.http.client.HttpClient`)
   - `get_java_okhttp_query()` - Classes importing OkHttp (`okhttp3.OkHttpClient`)
   - `get_java_retrofit_query()` - Classes importing Retrofit (`retrofit2.Retrofit`)
3. The API endpoint `/api/graph/java-service-calls/{workspace_folder_path}/{project_path}` returns combined service call data
4. The frontend displays service calls grouped by class with HTTP method and service type badges

**Supported patterns:**
- `@FeignClient` interfaces with HTTP mapping annotations (`@GetMapping`, `@PostMapping`, etc.) - Full HTTP method and path extraction
- `RestTemplate` - Spring synchronous HTTP client (import-based detection)
- `WebClient` - Spring reactive HTTP client (import-based detection)
- `HttpClient` - Apache HTTP client (import-based detection)
- `OkHttp` - Square's HTTP client (import-based detection)
- `Retrofit` - Type-safe HTTP client (import-based detection)

**Key files:**
- `crates/database/src/querying/library.rs` - Query functions for each service type
- `crates/http-server-desktop/src/endpoints/graph/service_calls.rs` - API handler and annotation parsing
- `packages/frontend/src/components/endpoints/JavaServiceCallsView.vue` - UI component
- `packages/frontend/src/api/client.ts` - `fetchJavaServiceCalls()` API client method

**API usage:**
```bash
# Fetch Java service calls for a project
curl "http://localhost:27495/api/graph/java-service-calls/{workspace_folder_path}/{project_path}?limit=1000"
```

**Frontend access:**
Navigate to Project Explorer → Select a project → Click "Service Calls" tab to view all external service calls grouped by class/interface.

**Service type badges:**
- FeignClient: Indigo badge - Full endpoint details available
- RestTemplate: Emerald badge - Import-based detection
- WebClient: Cyan badge - Import-based detection
- HttpClient: Orange badge - Import-based detection
- OkHttp: Rose badge - Import-based detection
- Retrofit: Violet badge - Import-based detection

**See also:** `docs/java-service-calls-feature-plan.md` for detailed implementation plan and future enhancements

## Java Endpoint Flow Feature

The knowledge-graph supports visualizing the complete method call chain for Java REST endpoints, enabling developers to understand the logical execution flow from API entry point through all downstream method calls.

**Architecture:**
The Endpoint Flow feature leverages the existing graph database relationships to trace method calls and present them in an interactive hierarchical tree format:

1. **Query Phase**: Execute `get_endpoint_flow_query()` to traverse method call relationships up to configurable depth (1-10, default 5)
2. **Tree Building**: Parse flat query results into hierarchical call tree structure with cycle detection
3. **Analysis**: Calculate statistics including total methods, max depth, call type breakdown, and recursion detection
4. **Visualization**: Render interactive tree with expand/collapse controls and visual indicators for call types

**Database Query:**
The `get_endpoint_flow_query()` function in [crates/database/src/querying/library.rs](knowledge-graph/crates/database/src/querying/library.rs) executes a Cypher query that:
- Finds the root method defining the endpoint via `ENDPOINT_RELATIONSHIPS`
- Traverses `CALLS` and `AMBIGUOUSLY_CALLS` relationships up to max_depth
- Returns endpoint metadata, method nodes, and their call relationships
- Limits results to prevent excessive data (default 1000)

**API Endpoint:**
```
GET /api/graph/endpoint-flow/{workspace_folder_path}/{project_path}/{endpoint_id}?max_depth=5
```

**Response Structure:**
```typescript
interface EndpointFlowResponse {
  endpoint_id: number;
  endpoint_http_method: string;
  endpoint_path: string;
  endpoint_full_path: string;
  root_method: MethodNode;
  call_tree: CallTreeNode[];
  statistics: FlowStatistics;
  project_info: ProjectInfo;
}

interface CallTreeNode {
  method_id: number;
  method_name: string;
  class_name: string;
  file_path: string;
  start_line: number;
  end_line: number;
  depth: number;
  call_type: "Direct" | "Ambiguous" | "Interface" | "Abstract";
  is_external: boolean;
  is_recursive: boolean;
  children: CallTreeNode[];
}

interface FlowStatistics {
  total_methods: number;
  max_depth: number;
  direct_calls: number;
  ambiguous_calls: number;
  external_calls: number;
  has_recursion: boolean;
}
```

**Call Type Classification:**
- **Direct**: Concrete method calls with known target
- **Ambiguous**: Calls that could resolve to multiple methods (e.g., via polymorphism)
- **Interface**: Calls to interface methods
- **Abstract**: Calls to abstract methods

**Backend Implementation:**

File: [crates/http-server-desktop/src/endpoints/graph/endpoint_flow.rs](knowledge-graph/crates/http-server-desktop/src/endpoints/graph/endpoint_flow.rs)

Key functions:
- `endpoint_flow_handler()`: Main API handler that validates parameters, executes queries, builds tree
- `build_flow_response()`: Constructs hierarchical tree from flat query results
- `build_tree_recursive()`: Recursive tree builder with cycle detection
- `detect_cycles()`: Identifies recursive method calls using DFS with visited/recursion stack
- `is_external_call()`: Determines if method is outside project path

**Frontend Components:**

**EndpointFlowView.vue** ([packages/frontend/src/components/endpoints/EndpointFlowView.vue](knowledge-graph/packages/frontend/src/components/endpoints/EndpointFlowView.vue)):
- Displays endpoint metadata with HTTP method badge
- Statistics panel showing:
  - Total methods in call chain
  - Maximum depth reached
  - Breakdown by call type (direct/ambiguous/external)
  - Recursive call detection warning
- Interactive controls:
  - Depth selector (3, 5, 7, 10 levels)
  - Search/filter by method name, FQN, or file path
  - Expand All / Collapse All buttons
- Auto-loads flow data on mount

**CallTreeNodeComponent.vue** ([packages/frontend/src/components/endpoints/CallTreeNodeComponent.vue](knowledge-graph/packages/frontend/src/components/endpoints/CallTreeNodeComponent.vue)):
- Recursive component for rendering tree nodes
- Visual indicators by call type:
  - 🔵 **Direct** (blue): Concrete method call
  - 🟠 **Ambiguous** (orange): Polymorphic call with multiple possible targets
  - 🟡 **Interface/Abstract** (yellow): Call to interface/abstract method
  - 🔴 **External** (red): Call to library/framework code outside project
  - 🔁 **Recursive** (purple): Method involved in call cycle
- Displays method signature, file location, and line range
- Expand/collapse chevrons for nodes with children
- Hover effects and click navigation to method definitions

**API Client:**

File: [packages/frontend/src/api/client.ts](knowledge-graph/packages/frontend/src/api/client.ts) (line 393+)

```typescript
async fetchEndpointFlow(
  workspaceFolderPath: string,
  projectPath: string,
  endpointId: number,
  maxDepth: number = 5,
): Promise<EndpointFlowResponse>
```

**Key Features:**

1. **Configurable Depth**: Control how many levels deep to traverse (1-10)
2. **Cycle Detection**: Identifies and marks recursive method calls
3. **External Call Identification**: Distinguishes project code from library/framework calls
4. **Search & Filter**: Find specific methods in large call trees
5. **Performance Optimized**: Limits results and uses efficient graph traversal
6. **Error Handling**: Graceful handling of missing endpoints, database errors
7. **Interactive UI**: Expand/collapse nodes, adjust depth, clear visual indicators

**Key files:**
- `crates/http-server-desktop/src/endpoints/graph/endpoint_flow.rs` - API handler and tree building
- `crates/database/src/querying/library.rs` - `get_endpoint_flow_query()` function
- `packages/frontend/src/components/endpoints/EndpointFlowView.vue` - Main flow visualization
- `packages/frontend/src/components/endpoints/CallTreeNodeComponent.vue` - Recursive tree node
- `packages/frontend/src/api/client.ts` - `fetchEndpointFlow()` API client method
- `packages/gkg/src/api.ts` - Auto-generated TypeScript types

**API usage:**
```bash
# Fetch endpoint flow for a specific endpoint
curl "http://localhost:27495/api/graph/endpoint-flow/{workspace_folder_path}/{project_path}/123?max_depth=5"
```

**Frontend access:**
Navigate to Project Explorer → Select a project → Click "Endpoints" tab → Select an endpoint → Click "View Flow" button to visualize the complete method call chain with:
- Interactive tree visualization with expand/collapse
- Color-coded badges for different call types
- Statistics about call chain complexity
- Search functionality to find specific methods
- Adjustable depth control (3, 5, 7, 10 levels)
- Line range information for quick navigation to source code

**Data flow example:**
```
User clicks "View Flow" on endpoint
    ↓
Frontend: apiClient.fetchEndpointFlow(workspace, project, endpointId, maxDepth)
    ↓
HTTP: GET /api/graph/endpoint-flow/{workspace}/{project}/{endpoint_id}?max_depth=5
    ↓
Backend: endpoint_flow_handler()
    ├─ Decode URL parameters and validate max_depth (1-10)
    ├─ Get project info from workspace manager
    ├─ Execute get_endpoint_flow_query() to traverse call graph
    └─ build_flow_response()
        ├─ Parse query results into method nodes
        ├─ Build hierarchical tree using recursive algorithm
        ├─ Detect cycles for recursion identification
        ├─ Identify external calls (outside project path)
        └─ Calculate statistics
    ↓
HTTP Response: EndpointFlowResponse JSON
    ↓
Frontend: Render EndpointFlowView component
    ├─ Display endpoint metadata and HTTP method badge
    ├─ Show statistics panel
    ├─ Render call tree with CallTreeNodeComponent (recursive)
    │   ├─ Color-code nodes by call type
    │   ├─ Show recursion indicators
    │   └─ Enable expand/collapse interaction
    ├─ Enable search/filter functionality
    └─ Provide depth adjustment controls
```

**Performance Considerations:**
- Query limited to 1000 results by default
- Depth clamped to maximum of 10 levels
- Tree building uses efficient algorithms with cycle detection
- Frontend lazy-renders tree nodes (only expanded nodes rendered)
- Search/filter operates on client-side for instant feedback

**Edge Cases Handled:**
1. **Recursive Calls**: Detected via DFS cycle detection, marked with special indicator
2. **Ambiguous Calls**: Polymorphic calls displayed with orange badge
3. **External Calls**: Library/framework methods shown with red badge
4. **Deep Call Chains**: Limited by max_depth parameter to prevent excessive data
5. **Missing Endpoints**: Returns 404 with descriptive error message
6. **Database Errors**: Returns 500 with error details for debugging

**See also:** `endpoint-flow-feature-plan.md` for detailed implementation plan and architecture decisions
