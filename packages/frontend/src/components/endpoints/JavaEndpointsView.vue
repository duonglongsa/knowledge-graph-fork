<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import { Activity, ChevronDown, ChevronRight, FileCode, Globe, Search } from 'lucide-vue-next';

import EndpointFlowView from './EndpointFlowView.vue';

import type { JavaEndpoint } from '@/api/client';
import { apiClient } from '@/api/client';
import { Badge } from '@/components/ui/badge';
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';

interface Props {
  workspaceFolderPath: string;
  projectPath: string;
}

const props = defineProps<Props>();

const endpoints = ref<JavaEndpoint[]>([]);
const isLoading = ref(false);
const error = ref<string | null>(null);
const searchQuery = ref('');
const expandedFiles = ref<Set<string>>(new Set());
const isFlowModalOpen = ref(false);
const selectedEndpointId = ref<number | null>(null);

// Group endpoints by file path
const groupedEndpoints = computed(() => {
  const groups: Record<string, JavaEndpoint[]> = {};

  const filtered = endpoints.value.filter((endpoint) => {
    if (!searchQuery.value.trim()) return true;
    const query = searchQuery.value.toLowerCase();
    return (
      endpoint.full_path.toLowerCase().includes(query) ||
      endpoint.file_path.toLowerCase().includes(query) ||
      endpoint.http_method.toLowerCase().includes(query)
    );
  });

  filtered.forEach((endpoint) => {
    const key = endpoint.file_path;
    if (!groups[key]) {
      groups[key] = [];
    }
    groups[key].push(endpoint);
  });

  return groups;
});

const fileNames = computed(() => Object.keys(groupedEndpoints.value).sort());

// Extract file name from path for display
const getFileName = (filePath: string): string => {
  const parts = filePath.split('/');
  return parts[parts.length - 1] || filePath;
};

const totalEndpoints = computed(() => endpoints.value.length);
const filteredEndpoints = computed(() =>
  Object.values(groupedEndpoints.value).reduce((sum, arr) => sum + arr.length, 0),
);

const toggleFile = (filePath: string) => {
  if (expandedFiles.value.has(filePath)) {
    expandedFiles.value.delete(filePath);
  } else {
    expandedFiles.value.add(filePath);
  }
};

const isFileExpanded = (filePath: string) => {
  return expandedFiles.value.has(filePath);
};

const expandAll = () => {
  fileNames.value.forEach((name) => expandedFiles.value.add(name));
};

const collapseAll = () => {
  expandedFiles.value.clear();
};

const getMethodBadgeClass = (method: string): string => {
  switch (method.toUpperCase()) {
    case 'GET':
      return 'bg-green-500/20 text-green-700 dark:text-green-400 border-green-500/30';
    case 'POST':
      return 'bg-blue-500/20 text-blue-700 dark:text-blue-400 border-blue-500/30';
    case 'PUT':
      return 'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30';
    case 'DELETE':
      return 'bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30';
    case 'PATCH':
      return 'bg-purple-500/20 text-purple-700 dark:text-purple-400 border-purple-500/30';
    default:
      return 'bg-gray-500/20 text-gray-700 dark:text-gray-400 border-gray-500/30';
  }
};

const fetchEndpoints = async () => {
  if (!props.workspaceFolderPath || !props.projectPath) return;

  isLoading.value = true;
  error.value = null;

  try {
    const response = await apiClient.fetchJavaEndpoints(
      props.workspaceFolderPath,
      props.projectPath,
    );
    endpoints.value = response.endpoints;
    // Expand all controllers by default
    expandAll();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to fetch endpoints';
    endpoints.value = [];
  } finally {
    isLoading.value = false;
  }
};

const showFlow = (endpointId: number) => {
  selectedEndpointId.value = endpointId;
  isFlowModalOpen.value = true;
};

// Fetch endpoints when props change
watch(
  () => [props.workspaceFolderPath, props.projectPath],
  () => {
    fetchEndpoints();
  },
  { immediate: true },
);
</script>

<template>
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Globe class="h-5 w-5 text-foreground" />
        <h3 class="text-lg font-medium text-foreground">Java Endpoints</h3>
        <Badge variant="outline" class="ml-2">
          {{ filteredEndpoints }} / {{ totalEndpoints }}
        </Badge>
      </div>

      <div class="flex items-center gap-2">
        <button class="text-xs text-muted-foreground hover:text-foreground" @click="expandAll">
          Expand All
        </button>
        <span class="text-muted-foreground">|</span>
        <button class="text-xs text-muted-foreground hover:text-foreground" @click="collapseAll">
          Collapse All
        </button>
      </div>
    </div>

    <!-- Search -->
    <div class="relative">
      <Search
        class="absolute left-2 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground"
      />
      <Input
        v-model="searchQuery"
        placeholder="Search endpoints by path, file, or HTTP method..."
        class="pl-8"
      />
    </div>

    <!-- Loading State -->
    <div v-if="isLoading" class="flex items-center justify-center py-8">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
    </div>

    <!-- Error State -->
    <Card v-else-if="error" class="border-destructive">
      <CardHeader>
        <CardTitle class="text-destructive">Error</CardTitle>
        <CardDescription>{{ error }}</CardDescription>
      </CardHeader>
    </Card>

    <!-- Empty State -->
    <Card v-else-if="endpoints.length === 0">
      <CardHeader>
        <CardTitle class="flex items-center gap-2">
          <Globe class="h-5 w-5" />
          No Endpoints Found
        </CardTitle>
        <CardDescription>
          No Java REST endpoints were found in this project. Make sure the project contains Spring
          Boot controllers with @RequestMapping, @GetMapping, @PostMapping, etc. annotations.
        </CardDescription>
      </CardHeader>
    </Card>

    <!-- Endpoints List -->
    <div v-else class="space-y-2">
      <Collapsible
        v-for="filePath in fileNames"
        :key="filePath"
        :open="isFileExpanded(filePath)"
        class="border rounded-lg"
      >
        <CollapsibleTrigger
          class="flex items-center justify-between w-full p-3 hover:bg-muted/50 transition-colors"
          @click="toggleFile(filePath)"
        >
          <div class="flex items-center gap-2">
            <ChevronRight v-if="!isFileExpanded(filePath)" class="h-4 w-4 text-muted-foreground" />
            <ChevronDown v-else class="h-4 w-4 text-muted-foreground" />
            <FileCode class="h-4 w-4 text-muted-foreground" />
            <span class="font-medium text-sm">
              {{ getFileName(filePath) }}
            </span>
            <Badge variant="outline" class="text-xs">
              {{ groupedEndpoints[filePath].length }} endpoints
            </Badge>
          </div>
          <span class="text-xs text-muted-foreground truncate max-w-[300px]">
            {{ filePath }}
          </span>
        </CollapsibleTrigger>

        <CollapsibleContent>
          <div class="border-t">
            <div
              v-for="endpoint in groupedEndpoints[filePath]"
              :key="`${endpoint.id}-${endpoint.http_method}`"
              class="flex items-start gap-3 px-4 py-3 hover:bg-muted/30 border-b last:border-b-0"
            >
              <div class="flex items-center gap-2 min-w-fit">
                <Badge
                  variant="outline"
                  :class="[
                    'w-16 justify-center font-mono text-xs',
                    getMethodBadgeClass(endpoint.http_method),
                  ]"
                >
                  {{ endpoint.http_method }}
                </Badge>
                <Badge v-if="endpoint.deprecated" variant="destructive" class="text-xs">
                  Deprecated
                </Badge>
              </div>

              <div class="flex-1 space-y-1">
                <code class="text-sm font-mono text-foreground block">
                  {{ endpoint.full_path || '/' }}
                </code>

                <div v-if="endpoint.description" class="text-xs text-muted-foreground">
                  {{ endpoint.description }}
                </div>

                <div class="flex items-center gap-4 text-xs text-muted-foreground">
                  <span v-if="endpoint.consumes">
                    Consumes: <code class="text-xs">{{ endpoint.consumes }}</code>
                  </span>
                  <span v-if="endpoint.produces">
                    Produces: <code class="text-xs">{{ endpoint.produces }}</code>
                  </span>
                </div>
              </div>

              <div class="flex items-center gap-2">
                <button
                  class="flex items-center gap-1 text-xs text-primary hover:underline"
                  title="View execution flow"
                  @click.stop="showFlow(endpoint.id)"
                >
                  <Activity class="h-3 w-3" />
                  View Flow
                </button>
                <span class="text-xs text-muted-foreground whitespace-nowrap">
                  :{{ endpoint.start_line }}-{{ endpoint.end_line }}
                </span>
              </div>
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>

    <!-- Flow Modal -->
    <Dialog v-model:open="isFlowModalOpen">
      <DialogContent
        class="!max-w-[98vw] w-[98vw] max-h-[95vh] h-[95vh] overflow-hidden flex flex-col p-0"
      >
        <DialogHeader class="px-6 py-4 border-b shrink-0">
          <DialogTitle>Endpoint Execution Flow</DialogTitle>
        </DialogHeader>
        <div class="flex-1 overflow-y-auto px-6 py-4">
          <EndpointFlowView
            v-if="selectedEndpointId !== null"
            :workspace-folder-path="workspaceFolderPath"
            :project-path="projectPath"
            :endpoint-id="selectedEndpointId"
          />
        </div>
      </DialogContent>
    </Dialog>
  </div>
</template>
