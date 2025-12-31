# GitLab Knowledge Graph

The GitLab Knowledge Graph is a system to create a structured, queryable representation of code repositories. It captures entities like files, directories, classes, functions, and their relationships (imports, calls, inheritance, etc.), enabling advanced code understanding and AI features.

> Note: This project is now in public beta. While core functionality is stable, some features may still be evolving.

## Key Features

### Core Capabilities
- **Multi-language Support**: TypeScript, JavaScript, Python, Ruby, Java, Kotlin, C#
- **Graph Database Storage**: Embedded KuzuDB with Cypher query support
- **Relationship Tracking**: Imports, calls, inheritance, endpoint definitions, service calls
- **Web UI**: Interactive Vue 3 frontend for exploring code relationships
- **MCP Integration**: Model Context Protocol for AI tool integration

### Java-Specific Features

#### 1. REST Endpoint Extraction
Automatically extracts Spring Boot REST endpoints with full metadata:

- **Annotation Support**: `@GetMapping`, `@PostMapping`, `@PutMapping`, `@DeleteMapping`, `@PatchMapping`, `@RequestMapping`
- **Multi-Path Support**: Handles arrays in annotations (e.g., `@RequestMapping({"/api/v1", "/api/v2"})`) with Cartesian product expansion
- **Context-Path Resolution**: Automatically extracts `server.servlet.context-path` from configuration files
- **Full Path Construction**: Combines context-path + class base path + method path
- **Metadata Extraction**: HTTP method, path parameters, consumes/produces types, deprecation status

#### 2. External Service Call Detection
Identifies and tracks calls to external services:

- **Client Library Support**:
  - **FeignClient** (full endpoint details with annotation parsing)
  - **RestTemplate** (Spring synchronous HTTP client)
  - **WebClient** (Spring reactive HTTP client)
  - **HttpClient** (Apache HTTP client)
  - **OkHttp** (Square's HTTP client)
  - **Retrofit** (Type-safe HTTP client)

- **URL Resolution**: Two-stage resolution pipeline
  1. Field reference resolution (e.g., `Config.BASE_URL` → `"${api.host}"`)
  2. Property placeholder resolution (e.g., `"${api.host}"` → `"http://localhost:8080"`)

#### 3. Configuration File Integration
Supports Spring Boot property file formats for dynamic value resolution:

- **File Formats**: `.properties`, `.yml`/`.yaml`, `.json`
- **Property Resolution**: Spring placeholder syntax `${key}` and `${key:default}`
- **Multi-File Loading**: Later files override earlier ones (profile support)
- **Context-Path Extraction**: Automatic detection from:
  - `server.servlet.context-path` (Spring Boot 2.x+)
  - `server.context-path` (Spring Boot 1.x, deprecated)

#### 4. Field Reference Resolution
Resolves Java constant and field references to their literal values:

- **Supported Types**:
  - Static final constants: `public static final String API_URL = "..."`
  - Static fields: `public static String baseUrl = "..."`
  - Instance fields: `private String endpoint = "..."`
  - Enum constants: `Environment.PRODUCTION`

- **Resolution Context**: Handles both simple and fully qualified references with package context

#### 5. Endpoint Flow Visualization
Trace method call chains from REST endpoints:

- **Interactive Tree View**: Expand/collapse method call hierarchy
- **Configurable Depth**: Control traversal depth (1-10 levels)
- **Call Type Classification**: Direct, Ambiguous, Interface, Abstract, External
- **Cycle Detection**: Identifies recursive method calls
- **Statistics**: Total methods, max depth, call type breakdown

### ETL Pipeline

The indexing process follows an Extract-Transform-Load architecture:

1. **Extract - Workspace Discovery**: Scan for Git repositories, register as projects
2. **Extract - File Discovery**: Enumerate files respecting `.gitignore`, filter by language
3. **Transform - Semantic Analysis**: Extract definitions/imports/relationships from AST
4. **Transform - Resolution**: Resolve references into in-memory graph
5. **Load - Graph Storage**: Write Parquet files and load into KuzuDB

### Threading Model
- **Async I/O Pool**: High-concurrency file reading (`worker_threads * 2`, min 8)
- **CPU Worker Pool**: Thread pool for parsing/analysis (sized to CPU cores or `--threads`)

### Data Storage
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

## Documentation

We use [GitLab Pages](https://gitlab-org.gitlab.io/rust/knowledge-graph) to host our full documentation.

See [CLAUDE.md](CLAUDE.md) for detailed development guidance and architecture documentation.

## Quick Start

### Prerequisites
- [mise](https://mise.jdx.dev/) for toolchain management
- Git LFS (for gitlab-code-parser Go bindings)
- Rust stable (managed via mise)
- Node.js 22 (managed via mise)

### Installation
```bash
# Trust and install toolchains
mise trust && mise install

# Install git hooks (lefthook)
lefthook install

# Build and install CLI globally
mise run build-cli-release
```

### Usage
```bash
# Index a workspace
gkg index /path/to/workspace

# Index with Java configuration files
gkg index /path/to/workspace --java-property-files config/application.properties,config/application-prod.yml

# Start web UI (default: http://localhost:27495)
gkg server

# Start server with custom port
gkg server --port 8080
```

### Java Property File Configuration

When indexing Java projects, you can provide property files for accurate endpoint and service call URL resolution:

```bash
# Single property file
gkg index /path/to/java-project --java-property-files src/main/resources/application.properties

# Multiple files (later files override earlier)
gkg index /path/to/java-project --java-property-files \
  src/main/resources/application.properties,\
  src/main/resources/application-prod.yml,\
  config/custom.json

# Files are auto-detected by extension:
# - .properties → Java properties format
# - .yml/.yaml → YAML format (nested keys flattened to dot notation)
# - .json → JSON format (nested objects flattened)
```

**Property Resolution Example:**
```properties
# application.properties
server.servlet.context-path=/retail-api
api.host=http://api.example.com
api.version=/v1
```

```java
// Config.java
public class ApiConfig {
    public static final String BASE_URL = "${api.host}${api.version}";
}

// UserController.java
@RestController
@RequestMapping(ApiConfig.BASE_URL)
public class UserController {
    @GetMapping("/users")
    public List<User> getUsers() { ... }
}

// Result: Endpoint extracted with full path
// GET /retail-api/http://api.example.com/v1/users
// (context-path + resolved BASE_URL + method path)
```

## Development

See [CLAUDE.md](CLAUDE.md) for comprehensive development setup, commands, and architecture details.

**Quick Commands:**
```bash
# Run tests
cargo test

# Run server in dev mode
mise run server-dev

# Build frontend
mise run frontend-build

# Lint and format
mise run rust-clippy
mise run rust-fmt
```

## Roadmap

Follow progress in the 👉 [Knowledge Graph First Iteration epic](https://gitlab.com/groups/gitlab-org/-/epics/17514).
