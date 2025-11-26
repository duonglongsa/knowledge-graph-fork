<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { Search, Network, FileCode, ChevronDown, ChevronRight } from 'lucide-vue-next';
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { apiClient, type JavaServiceCall } from '@/api/client';

interface Props {
  workspaceFolderPath: string;
  projectPath: string;
}

const props = defineProps<Props>();

const serviceCalls = ref<JavaServiceCall[]>([]);
const isLoading = ref(false);
const error = ref<string | null>(null);
const searchQuery = ref('');
const expandedClients = ref<Set<string>>(new Set());

// Group service calls by class
const groupedServiceCalls = computed(() => {
  const groups: Record<string, JavaServiceCall[]> = {};

  const filtered = serviceCalls.value.filter((call) => {
    if (!searchQuery.value.trim()) return true;
    const query = searchQuery.value.toLowerCase();
    return (
      call.full_path.toLowerCase().includes(query) ||
      call.method_name.toLowerCase().includes(query) ||
      call.class_name.toLowerCase().includes(query) ||
      call.http_method.toLowerCase().includes(query) ||
      call.service_name.toLowerCase().includes(query) ||
      call.service_type.toLowerCase().includes(query)
    );
  });

  filtered.forEach((call) => {
    const key = call.class_fqn;
    if (!groups[key]) {
      groups[key] = [];
    }
    groups[key].push(call);
  });

  return groups;
});

const clientNames = computed(() => Object.keys(groupedServiceCalls.value).sort());

const totalServiceCalls = computed(() => serviceCalls.value.length);
const filteredServiceCalls = computed(() =>
  Object.values(groupedServiceCalls.value).reduce((sum, arr) => sum + arr.length, 0),
);

const toggleClient = (clientFqn: string) => {
  if (expandedClients.value.has(clientFqn)) {
    expandedClients.value.delete(clientFqn);
  } else {
    expandedClients.value.add(clientFqn);
  }
};

const isClientExpanded = (clientFqn: string) => {
  return expandedClients.value.has(clientFqn);
};

const expandAll = () => {
  clientNames.value.forEach((name) => expandedClients.value.add(name));
};

const collapseAll = () => {
  expandedClients.value.clear();
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

const getServiceTypeBadgeClass = (serviceType: string): string => {
  switch (serviceType.toLowerCase()) {
    case 'feignclient':
      return 'bg-indigo-500/20 text-indigo-700 dark:text-indigo-400 border-indigo-500/30';
    case 'resttemplate':
      return 'bg-emerald-500/20 text-emerald-700 dark:text-emerald-400 border-emerald-500/30';
    case 'webclient':
      return 'bg-cyan-500/20 text-cyan-700 dark:text-cyan-400 border-cyan-500/30';
    case 'httpclient':
      return 'bg-orange-500/20 text-orange-700 dark:text-orange-400 border-orange-500/30';
    case 'okhttp':
      return 'bg-rose-500/20 text-rose-700 dark:text-rose-400 border-rose-500/30';
    case 'retrofit':
      return 'bg-violet-500/20 text-violet-700 dark:text-violet-400 border-violet-500/30';
    default:
      return 'bg-gray-500/20 text-gray-700 dark:text-gray-400 border-gray-500/30';
  }
};

const fetchServiceCalls = async () => {
  if (!props.workspaceFolderPath || !props.projectPath) return;

  isLoading.value = true;
  error.value = null;

  try {
    const response = await apiClient.fetchJavaServiceCalls(
      props.workspaceFolderPath,
      props.projectPath,
    );
    serviceCalls.value = response.service_calls;
    // Expand all clients by default
    expandAll();
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to fetch service calls';
    serviceCalls.value = [];
  } finally {
    isLoading.value = false;
  }
};

// Fetch service calls when props change
watch(
  () => [props.workspaceFolderPath, props.projectPath],
  () => {
    fetchServiceCalls();
  },
  { immediate: true },
);
</script>

<template>
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Network class="h-5 w-5 text-foreground" />
        <h3 class="text-lg font-medium text-foreground">Java Service Calls</h3>
        <Badge variant="outline" class="ml-2">
          {{ filteredServiceCalls }} / {{ totalServiceCalls }}
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
        placeholder="Search by URL, method, service name, or interface..."
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
    <Card v-else-if="serviceCalls.length === 0">
      <CardHeader>
        <CardTitle class="flex items-center gap-2">
          <Network class="h-5 w-5" />
          No Service Calls Found
        </CardTitle>
        <CardDescription>
          No Java external service calls were found in this project. Supported patterns include:
          FeignClient interfaces, RestTemplate usage, and WebClient usage.
        </CardDescription>
      </CardHeader>
    </Card>

    <!-- Service Calls List -->
    <div v-else class="space-y-2">
      <Collapsible
        v-for="clientFqn in clientNames"
        :key="clientFqn"
        :open="isClientExpanded(clientFqn)"
        class="border rounded-lg"
      >
        <CollapsibleTrigger
          class="flex items-center justify-between w-full p-3 hover:bg-muted/50 transition-colors"
          @click="toggleClient(clientFqn)"
        >
          <div class="flex items-center gap-2">
            <ChevronRight
              v-if="!isClientExpanded(clientFqn)"
              class="h-4 w-4 text-muted-foreground"
            />
            <ChevronDown v-else class="h-4 w-4 text-muted-foreground" />
            <FileCode class="h-4 w-4 text-muted-foreground" />
            <span class="font-medium text-sm">
              {{ groupedServiceCalls[clientFqn][0].class_name }}
            </span>
            <Badge
              variant="outline"
              :class="[
                'text-xs',
                getServiceTypeBadgeClass(groupedServiceCalls[clientFqn][0].service_type),
              ]"
            >
              {{ groupedServiceCalls[clientFqn][0].service_type }}
            </Badge>
            <Badge variant="outline" class="text-xs">
              {{ groupedServiceCalls[clientFqn].length }} calls
            </Badge>
            <span
              v-if="groupedServiceCalls[clientFqn][0].service_name"
              class="text-xs text-muted-foreground"
            >
              → {{ groupedServiceCalls[clientFqn][0].service_name }}
            </span>
          </div>
          <span class="text-xs text-muted-foreground truncate max-w-[300px]">
            {{ clientFqn }}
          </span>
        </CollapsibleTrigger>

        <CollapsibleContent>
          <div class="border-t">
            <div
              v-for="call in groupedServiceCalls[clientFqn]"
              :key="`${call.method_fqn}-${call.http_method}`"
              class="flex items-center gap-3 px-4 py-2 hover:bg-muted/30 border-b last:border-b-0"
            >
              <Badge
                variant="outline"
                :class="[
                  'w-16 justify-center font-mono text-xs',
                  getMethodBadgeClass(call.http_method),
                ]"
              >
                {{ call.http_method }}
              </Badge>

              <code class="flex-1 text-sm font-mono text-foreground">
                {{ call.full_path || call.path || '/' }}
              </code>

              <span class="text-xs text-muted-foreground"> {{ call.method_name }}() </span>

              <span class="text-xs text-muted-foreground"> :{{ call.line_number }} </span>
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  </div>
</template>
