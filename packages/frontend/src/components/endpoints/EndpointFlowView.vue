<script setup lang="ts">
import type { CallTreeNode, EndpointFlowResponse } from '@gitlab-org/gkg';
import { computed, onMounted, ref } from 'vue';

import { Activity, AlertCircle, Loader2 } from 'lucide-vue-next';

import CallTreeNodeComponent from './CallTreeNodeComponent.vue';

import { apiClient } from '@/api/client';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';

interface Props {
  workspaceFolderPath: string;
  projectPath: string;
  endpointId: number;
}

const props = defineProps<Props>();

const flowData = ref<EndpointFlowResponse | null>(null);
const isLoading = ref(false);
const error = ref<string | null>(null);
const maxDepth = ref(5);
const searchQuery = ref('');
const expandedNodes = ref<Set<string>>(new Set());

const toggleNode = (nodeFqn: string) => {
  if (expandedNodes.value.has(nodeFqn)) {
    expandedNodes.value.delete(nodeFqn);
  } else {
    expandedNodes.value.add(nodeFqn);
  }
};

const expandAll = () => {
  if (!flowData.value) return;
  const collectFqns = (nodes: CallTreeNode[]): string[] => {
    const fqns: string[] = [];
    nodes.forEach((node) => {
      fqns.push(node.method_fqn);
      if (node.children.length > 0) {
        fqns.push(...collectFqns(node.children));
      }
    });
    return fqns;
  };
  const allFqns = collectFqns(flowData.value.call_tree);
  expandedNodes.value = new Set(allFqns);
};

const loadFlow = async () => {
  isLoading.value = true;
  error.value = null;

  try {
    const response = await apiClient.fetchEndpointFlow(
      props.workspaceFolderPath,
      props.projectPath,
      props.endpointId,
      maxDepth.value,
    );
    flowData.value = response;
    // Expand all nodes by default
    expandAll();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to fetch endpoint flow';
    flowData.value = null;
  } finally {
    isLoading.value = false;
  }
};

const collapseAll = () => {
  expandedNodes.value.clear();
};

const isNodeExpanded = (nodeFqn: string) => {
  return expandedNodes.value.has(nodeFqn);
};

const filteredTree = computed(() => {
  if (!flowData.value || !searchQuery.value.trim()) {
    return flowData.value?.call_tree || [];
  }

  const query = searchQuery.value.toLowerCase();
  const filterNodes = (nodes: CallTreeNode[]): CallTreeNode[] => {
    return nodes
      .map((node) => {
        const matches =
          node.method_name.toLowerCase().includes(query) ||
          node.method_fqn.toLowerCase().includes(query) ||
          node.file_path.toLowerCase().includes(query);

        const filteredChildren = filterNodes(node.children);

        if (matches || filteredChildren.length > 0) {
          return {
            ...node,
            children: filteredChildren,
          };
        }
        return null;
      })
      .filter((node): node is CallTreeNode => node !== null);
  };

  return filterNodes(flowData.value.call_tree);
});

onMounted(() => {
  loadFlow();
});
</script>

<template>
  <div class="w-full space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Activity class="h-5 w-5 text-primary" />
        <h3 class="text-lg font-medium">Endpoint Flow</h3>
      </div>
    </div>

    <!-- Loading State -->
    <div v-if="isLoading" class="flex items-center justify-center py-12">
      <Loader2 class="h-8 w-8 animate-spin text-primary" />
    </div>

    <!-- Error State -->
    <Card v-else-if="error" class="border-destructive">
      <CardHeader>
        <CardTitle class="flex items-center gap-2 text-destructive">
          <AlertCircle class="h-5 w-5" />
          Error
        </CardTitle>
        <CardDescription>{{ error }}</CardDescription>
      </CardHeader>
    </Card>

    <!-- Flow Content -->
    <div v-else-if="flowData" class="space-y-4">
      <!-- Endpoint Info -->
      <Card>
        <CardHeader>
          <div class="flex items-center gap-2">
            <Badge
              variant="outline"
              :class="[
                'font-mono text-xs',
                flowData.endpoint_http_method === 'GET'
                  ? 'bg-green-500/20 text-green-700 dark:text-green-400 border-green-500/30'
                  : flowData.endpoint_http_method === 'POST'
                    ? 'bg-blue-500/20 text-blue-700 dark:text-blue-400 border-blue-500/30'
                    : flowData.endpoint_http_method === 'PUT'
                      ? 'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30'
                      : flowData.endpoint_http_method === 'DELETE'
                        ? 'bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30'
                        : 'bg-gray-500/20 text-gray-700 dark:text-gray-400 border-gray-500/30',
              ]"
            >
              {{ flowData.endpoint_http_method }}
            </Badge>
            <code class="text-sm font-mono">{{ flowData.endpoint_full_path }}</code>
          </div>
          <CardDescription>
            {{ flowData.root_method.name }}() :{{ flowData.root_method.start_line }}-{{
              flowData.root_method.end_line
            }}
          </CardDescription>
        </CardHeader>
      </Card>

      <!-- Statistics -->
      <Card>
        <CardHeader>
          <CardTitle class="text-sm">Flow Statistics</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="grid grid-cols-2 md:grid-cols-6 gap-4 text-sm">
            <div>
              <div class="text-muted-foreground">Total Methods</div>
              <div class="text-lg font-semibold">{{ flowData.statistics.total_methods }}</div>
            </div>
            <div>
              <div class="text-muted-foreground">Max Depth</div>
              <div class="text-lg font-semibold">{{ flowData.statistics.max_depth }}</div>
            </div>
            <div>
              <div class="text-muted-foreground">Direct Calls</div>
              <div class="text-lg font-semibold">{{ flowData.statistics.direct_calls }}</div>
            </div>
            <div>
              <div class="text-muted-foreground">Ambiguous</div>
              <div class="text-lg font-semibold">{{ flowData.statistics.ambiguous_calls }}</div>
            </div>
            <div>
              <div class="text-muted-foreground">External</div>
              <div class="text-lg font-semibold">{{ flowData.statistics.external_calls }}</div>
            </div>
            <div>
              <div class="text-muted-foreground">Recursive</div>
              <div class="flex items-center gap-2">
                <div class="text-lg font-semibold">
                  {{ flowData.statistics.has_recursive_calls ? 'Yes' : 'No' }}
                </div>
                <Badge
                  v-if="flowData.statistics.has_recursive_calls"
                  variant="destructive"
                  class="text-xs"
                >
                  ⚠️ Warning
                </Badge>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- Controls -->
      <div class="flex items-center gap-4">
        <div class="flex-1">
          <Input v-model="searchQuery" placeholder="Search methods..." class="w-full" />
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm">Depth: {{ maxDepth }}</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem
              @click="
                maxDepth = 3;
                loadFlow();
              "
              >Depth: 3</DropdownMenuItem
            >
            <DropdownMenuItem
              @click="
                maxDepth = 5;
                loadFlow();
              "
              >Depth: 5</DropdownMenuItem
            >
            <DropdownMenuItem
              @click="
                maxDepth = 7;
                loadFlow();
              "
              >Depth: 7</DropdownMenuItem
            >
            <DropdownMenuItem
              @click="
                maxDepth = 10;
                loadFlow();
              "
              >Depth: 10</DropdownMenuItem
            >
          </DropdownMenuContent>
        </DropdownMenu>
        <button class="text-xs text-muted-foreground hover:text-foreground" @click="expandAll">
          Expand All
        </button>
        <span class="text-muted-foreground">|</span>
        <button class="text-xs text-muted-foreground hover:text-foreground" @click="collapseAll">
          Collapse All
        </button>
      </div>

      <!-- Call Tree -->
      <Card>
        <CardHeader>
          <CardTitle class="text-sm">Call Tree</CardTitle>
        </CardHeader>
        <CardContent class="overflow-x-auto">
          <div v-if="filteredTree.length === 0" class="text-center py-8 text-muted-foreground">
            <p v-if="searchQuery.trim()">No methods match your search</p>
            <p v-else>No method calls found</p>
          </div>
          <div v-else class="space-y-1 min-w-max">
            <CallTreeNodeComponent
              v-for="node in filteredTree"
              :key="node.method_fqn"
              :node="node"
              :depth="0"
              :is-expanded="isNodeExpanded(node.method_fqn)"
              @toggle="toggleNode"
            />
          </div>
        </CardContent>
      </Card>
    </div>
  </div>
</template>
